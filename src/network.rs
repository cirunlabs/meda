use crate::config::Config;
use crate::error::{Error, Result};
use crate::util::{run_command, run_command_quietly, run_command_with_output};
use log::{debug, info, warn};
use rand::Rng;
use std::collections::HashSet;
use std::fs;

pub fn generate_random_mac() -> String {
    let mut rng = rand::thread_rng();
    format!(
        "52:54:{:02x}:{:02x}:{:02x}:{:02x}",
        rng.gen::<u8>(),
        rng.gen::<u8>(),
        rng.gen::<u8>(),
        rng.gen::<u8>()
    )
}

pub fn generate_random_octet() -> u8 {
    let mut rng = rand::thread_rng();
    16 + rng.gen::<u8>() % 200
}

/// Parse the kernel routing table for `192.168.X.0/24` connected routes and
/// return the set of third-octet values already claimed by the kernel.
///
/// Used as a second source of truth alongside on-disk VM dirs when choosing a
/// subnet for a new VM. A previous `cleanup_networking` that failed to run
/// `ip link del` leaves a stale tap device plus its connected route in the
/// kernel even though the VM dir is gone; reusing that subnet would silently
/// route new traffic via the stale (linkdown) tap.
///
/// Returns an empty set if `ip` is unavailable (macOS dev machines) — the
/// on-disk scan is still consulted by `generate_unique_subnet`.
fn kernel_subnet_octets_in_use() -> HashSet<u8> {
    let mut used = HashSet::new();
    let Ok(output) = run_command_with_output("ip", &["-o", "route", "show"]) else {
        return used;
    };
    if !output.status.success() {
        return used;
    }
    let out = String::from_utf8_lossy(&output.stdout);
    for line in out.lines() {
        // Destination is the first whitespace-separated field, e.g.
        // "192.168.26.0/24 dev tap-66c39bfa proto kernel scope link ..."
        let Some(dest) = line.split_whitespace().next() else {
            continue;
        };
        if let Some(octet) = parse_192_168_slash_24_octet(dest) {
            used.insert(octet);
        }
    }
    used
}

/// Parse a CIDR string of the form "192.168.X.0/24" and return X, or None.
fn parse_192_168_slash_24_octet(dest: &str) -> Option<u8> {
    let net = dest.strip_suffix("/24")?;
    let rest = net.strip_prefix("192.168.")?;
    let third = rest.strip_suffix(".0")?;
    third.parse::<u8>().ok()
}

pub async fn generate_unique_subnet(config: &Config) -> Result<String> {
    // Start with subnets the kernel still has a connected route for. This
    // catches leaks from earlier delete attempts that failed to remove a tap
    // device — the VM dir is gone but the route survives, and picking that
    // subnet would break the new VM's networking.
    let mut used_subnets: HashSet<u8> = kernel_subnet_octets_in_use();

    // Union with subnets claimed by existing VM dirs on disk.
    if let Ok(entries) = fs::read_dir(&config.vm_root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let subnet_file = path.join("subnet");
                if subnet_file.exists() {
                    if let Ok(subnet) = fs::read_to_string(subnet_file) {
                        let subnet = subnet.trim();
                        if let Some(octet_str) = subnet.strip_prefix("192.168.") {
                            if let Ok(octet) = octet_str.parse::<u8>() {
                                used_subnets.insert(octet);
                            }
                        }
                    }
                }
            }
        }
    }

    let mut attempts = 0;
    let max_attempts = 200;

    while attempts < max_attempts {
        let octet = generate_random_octet();
        if !used_subnets.contains(&octet) {
            return Ok(format!("192.168.{}", octet));
        }
        attempts += 1;
    }

    Err(Error::Other(
        "Could not generate a unique subnet after multiple attempts".to_string(),
    ))
}

pub async fn generate_unique_tap_name(_config: &Config, vm_name: &str) -> Result<String> {
    // Get all currently active TAP devices on the system (authoritative source)
    let mut used_tap_names = std::collections::HashSet::new();

    if let Ok(output) = run_command_with_output("ip", &["link", "show"]) {
        if output.status.success() {
            let output_str = String::from_utf8_lossy(&output.stdout);
            for line in output_str.lines() {
                if line.contains("tap-") {
                    if let Some(tap_start) = line.find("tap-") {
                        let tap_part = &line[tap_start..];
                        if let Some(colon_pos) = tap_part.find(':') {
                            let tap_name = tap_part[..colon_pos].to_string();
                            used_tap_names.insert(tap_name);
                        }
                    }
                }
            }
        }
    }

    // Use a deterministic approach: hash of VM name + timestamp for uniqueness
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    vm_name.hash(&mut hasher);
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .hash(&mut hasher);

    let hash = hasher.finish();

    // Create TAP name with strict length limit (Linux max is 15 chars)
    // Format: tap-XXXXXXXX (4 + 8 = 12 chars, well under limit)
    let candidate = format!("tap-{:08x}", (hash % 0xFFFFFFFF) as u32);

    // Double-check it's not in use (extremely unlikely with hash)
    if !used_tap_names.contains(&candidate) {
        return Ok(candidate);
    }

    // Fallback: increment hash until we find unused name
    for i in 1..=1000 {
        let fallback = format!("tap-{:07x}{:x}", (hash % 0xFFFFFFF) as u32, i % 16);
        if !used_tap_names.contains(&fallback) {
            return Ok(fallback);
        }
    }

    Err(Error::Other(
        "Could not generate a unique TAP device name after extensive attempts".to_string(),
    ))
}

/// Clean up orphaned TAP devices (TAP devices with no corresponding VM)
pub async fn cleanup_orphaned_tap_devices(config: &Config) -> Result<Vec<String>> {
    let mut cleaned_up = Vec::new();

    // Get all TAP devices on the system
    let mut system_taps = std::collections::HashSet::new();
    if let Ok(output) = run_command_with_output("ip", &["link", "show"]) {
        if output.status.success() {
            let output_str = String::from_utf8_lossy(&output.stdout);
            for line in output_str.lines() {
                if line.contains("tap-") {
                    if let Some(tap_start) = line.find("tap-") {
                        let tap_part = &line[tap_start..];
                        if let Some(colon_pos) = tap_part.find(':') {
                            let tap_name = tap_part[..colon_pos].to_string();
                            system_taps.insert(tap_name);
                        }
                    }
                }
            }
        }
    }

    // Get all TAP devices referenced by VMs
    let mut vm_taps = std::collections::HashSet::new();
    if let Ok(entries) = fs::read_dir(&config.vm_root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let tapdev_file = path.join("tapdev");
                if let Ok(tap_name) = fs::read_to_string(tapdev_file) {
                    vm_taps.insert(tap_name.trim().to_string());
                }
            }
        }
    }

    // Find orphaned TAP devices (exist on system but not referenced by any VM).
    // Flush the connected route first so a half-failed `ip link del` cannot
    // leave the `192.168.X.0/24` route hanging; then verify the tap is gone.
    for tap_name in system_taps {
        if vm_taps.contains(&tap_name) {
            continue;
        }
        let _ = run_command_quietly("sudo", &["ip", "route", "flush", "dev", &tap_name]);
        if delete_tap_device_verified(&tap_name).is_ok() {
            cleaned_up.push(tap_name);
        }
    }

    Ok(cleaned_up)
}

pub async fn setup_networking(
    _config: &Config,
    name: &str,
    tap_name: &str,
    subnet: &str,
) -> Result<()> {
    debug!("Setting up networking for VM {}", name);

    // Check if tap device exists
    let output = run_command_with_output("sudo", &["ip", "link", "show", tap_name])?;

    if !output.status.success() {
        // Create tap device
        run_command("sudo", &["ip", "tuntap", "add", tap_name, "mode", "tap"])?;
        run_command(
            "sudo",
            &[
                "ip",
                "addr",
                "add",
                &format!("{}.1/24", subnet),
                "dev",
                tap_name,
            ],
        )?;
        run_command("sudo", &["ip", "link", "set", tap_name, "up"])?;
    }

    // Enable forwarding
    run_command("sudo", &["sysctl", "-q", "net.ipv4.ip_forward=1"])?;

    // Check if masquerade rule exists (use -w to wait for xtables lock)
    let check_cmd = format!(
        "sudo iptables -w -t nat -C POSTROUTING -s {}.0/24 -j MASQUERADE",
        subnet
    );
    let check_result = run_command_with_output("bash", &["-c", &check_cmd]);

    if check_result.is_err() || !check_result.unwrap().status.success() {
        // Add masquerade rule
        run_command(
            "sudo",
            &[
                "iptables",
                "-w",
                "-t",
                "nat",
                "-A",
                "POSTROUTING",
                "-s",
                &format!("{}.0/24", subnet),
                "-j",
                "MASQUERADE",
            ],
        )?;
    }

    // Allow traffic from VM to leave host (use -w to wait for xtables lock)
    let check_forward = format!("sudo iptables -w -C FORWARD -i {} -j ACCEPT", tap_name);
    let check_result = run_command_with_output("bash", &["-c", &check_forward]);

    if check_result.is_err() || !check_result.unwrap().status.success() {
        run_command(
            "sudo",
            &[
                "iptables", "-w", "-A", "FORWARD", "-i", tap_name, "-j", "ACCEPT",
            ],
        )?;
        run_command(
            "sudo",
            &[
                "iptables",
                "-w",
                "-A",
                "FORWARD",
                "-o",
                tap_name,
                "-m",
                "conntrack",
                "--ctstate",
                "RELATED,ESTABLISHED",
                "-j",
                "ACCEPT",
            ],
        )?;
    }

    Ok(())
}

pub async fn port_forward(
    config: &Config,
    name: &str,
    host_port: u16,
    guest_port: u16,
) -> Result<()> {
    let vm_dir = config.vm_dir(name);

    if !vm_dir.exists() {
        return Err(Error::VmNotFound(name.to_string()));
    }

    let subnet_file = vm_dir.join("subnet");
    if !subnet_file.exists() {
        return Err(Error::NetworkConfigMissing(name.to_string()));
    }

    let subnet = fs::read_to_string(subnet_file)?;
    let subnet = subnet.trim();

    // Remove any existing port forward for this host port
    let _ = run_command(
        "sudo",
        &[
            "iptables",
            "-w",
            "-t",
            "nat",
            "-D",
            "PREROUTING",
            "-p",
            "tcp",
            "--dport",
            &host_port.to_string(),
            "-j",
            "DNAT",
            "--to",
            &format!("{}.2:{}", subnet, guest_port),
        ],
    );

    // Add new port forward
    run_command(
        "sudo",
        &[
            "iptables",
            "-w",
            "-t",
            "nat",
            "-A",
            "PREROUTING",
            "-p",
            "tcp",
            "--dport",
            &host_port.to_string(),
            "-j",
            "DNAT",
            "--to",
            &format!("{}.2:{}", subnet, guest_port),
        ],
    )?;

    // Save port forwarding info
    fs::write(
        vm_dir.join("ports"),
        format!("{}->{}", host_port, guest_port),
    )?;

    info!(
        "Port forwarding set up: localhost:{} -> {}.2:{}",
        host_port, subnet, guest_port
    );

    Ok(())
}

/// Delete a tap device and verify it is gone from the kernel.
///
/// Treats "already absent" as success regardless of how `ip link del` exited,
/// and retries once after a brief pause to tolerate a race where qemu has not
/// yet released its tun fd.
fn delete_tap_device_verified(tap_name: &str) -> Result<()> {
    if run_command_quietly("sudo", &["ip", "link", "del", tap_name]).is_ok() {
        return Ok(());
    }
    if !tap_exists(tap_name) {
        return Ok(());
    }

    std::thread::sleep(std::time::Duration::from_millis(500));

    if run_command_quietly("sudo", &["ip", "link", "del", tap_name]).is_ok() {
        return Ok(());
    }
    if !tap_exists(tap_name) {
        return Ok(());
    }

    warn!(
        "cleanup_networking: tap device {} still present after delete attempts",
        tap_name
    );
    Err(Error::Other(format!(
        "failed to delete tap device {tap_name}: still present after retry"
    )))
}

/// Report whether a tap device is currently known to the kernel.
fn tap_exists(tap_name: &str) -> bool {
    match run_command_with_output("ip", &["link", "show", tap_name]) {
        Ok(output) => output.status.success(),
        Err(_) => false,
    }
}

pub async fn cleanup_networking(config: &Config, name: &str) -> Result<()> {
    let vm_dir = config.vm_dir(name);

    // Clean up iptables FORWARD rules for this VM's TAP device
    if let Ok(tap_name) = fs::read_to_string(vm_dir.join("tapdev")) {
        let tap_name = tap_name.trim();

        // Remove FORWARD rules referencing this TAP device (inbound and outbound).
        // Best-effort: the rule may have already been reaped by an earlier pass.
        let _ = run_command(
            "sudo",
            &[
                "iptables", "-w", "-D", "FORWARD", "-i", tap_name, "-j", "ACCEPT",
            ],
        );
        let _ = run_command(
            "sudo",
            &[
                "iptables",
                "-w",
                "-D",
                "FORWARD",
                "-o",
                tap_name,
                "-m",
                "conntrack",
                "--ctstate",
                "RELATED,ESTABLISHED",
                "-j",
                "ACCEPT",
            ],
        );

        // Flush connected routes pointing at this tap before deleting the
        // device. `ip link del` normally auto-removes them, but being explicit
        // means a half-successful delete cannot leave a stale route behind.
        let _ = run_command_quietly("sudo", &["ip", "route", "flush", "dev", tap_name]);

        // Delete the tap device and verify it is actually gone. Previously
        // this call was `let _ = run_command(...)`, which silently swallowed
        // failures and let `vm::delete` continue on to `remove_dir_all(vm_dir)`
        // — orphaning the tap + its connected route in the kernel. The next
        // VM that generated the same subnet (disk-only check) would then
        // route via the stale linkdown tap and fail with "No route to host".
        delete_tap_device_verified(tap_name)?;
    }

    // Clean up iptables MASQUERADE rule if this is the last VM using this subnet
    if let Ok(subnet) = fs::read_to_string(vm_dir.join("subnet")) {
        let subnet = subnet.trim();

        // Check if any other VM is using this subnet
        let mut found = false;
        for entry in fs::read_dir(&config.vm_root)? {
            let entry = entry?;
            let path = entry.path();

            if path != vm_dir && path.is_dir() {
                let subnet_file = path.join("subnet");
                if subnet_file.exists() {
                    if let Ok(other_subnet) = fs::read_to_string(subnet_file) {
                        if other_subnet.trim() == subnet {
                            found = true;
                            break;
                        }
                    }
                }
            }
        }

        if !found {
            // Remove MASQUERADE rule
            let _ = run_command(
                "sudo",
                &[
                    "iptables",
                    "-w",
                    "-t",
                    "nat",
                    "-D",
                    "POSTROUTING",
                    "-s",
                    &format!("{}.0/24", subnet),
                    "-j",
                    "MASQUERADE",
                ],
            );
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use tempfile::TempDir;

    #[test]
    fn test_generate_random_mac() {
        let mac = generate_random_mac();
        assert!(mac.starts_with("52:54:"));
        assert_eq!(mac.len(), 17); // XX:XX:XX:XX:XX:XX format
        assert_eq!(mac.chars().filter(|&c| c == ':').count(), 5);
    }

    #[test]
    fn test_generate_random_octet() {
        let octet = generate_random_octet();
        assert!(octet >= 16);
        assert!(octet <= 215); // 16 + 199
    }

    #[tokio::test]
    async fn test_generate_unique_subnet_empty_dir() {
        let temp_dir = TempDir::new().unwrap();

        env::set_var("MEDA_VM_DIR", temp_dir.path().to_str().unwrap());
        let config = Config::new().unwrap();
        env::remove_var("MEDA_VM_DIR");

        let subnet = generate_unique_subnet(&config).await.unwrap();
        assert!(subnet.starts_with("192.168."));

        let parts: Vec<&str> = subnet.split('.').collect();
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0], "192");
        assert_eq!(parts[1], "168");

        let octet: u8 = parts[2].parse().unwrap();
        assert!(octet >= 16);
        assert!(octet <= 215);
    }

    #[tokio::test]
    async fn test_generate_unique_subnet_with_existing() {
        let temp_dir = TempDir::new().unwrap();

        let vm_dir = temp_dir.path().join("test-vm");
        std::fs::create_dir_all(&vm_dir).unwrap();
        std::fs::write(vm_dir.join("subnet"), "192.168.100").unwrap();

        env::set_var("MEDA_VM_DIR", temp_dir.path().to_str().unwrap());
        let config = Config::new().unwrap();
        env::remove_var("MEDA_VM_DIR");

        let subnet = generate_unique_subnet(&config).await.unwrap();
        assert!(subnet.starts_with("192.168."));
        assert_ne!(subnet, "192.168.100");
    }

    #[test]
    fn test_mac_address_uniqueness() {
        let mut macs = std::collections::HashSet::new();

        for _ in 0..100 {
            let mac = generate_random_mac();
            assert!(!macs.contains(&mac), "Generated duplicate MAC: {}", mac);
            macs.insert(mac);
        }
    }

    #[test]
    fn test_octet_range() {
        for _ in 0..1000 {
            let octet = generate_random_octet();
            assert!((16..=215).contains(&octet));
        }
    }

    #[tokio::test]
    async fn test_cleanup_networking_missing_vm() {
        let temp_dir = TempDir::new().unwrap();

        env::set_var("MEDA_VM_DIR", temp_dir.path().to_str().unwrap());
        let config = Config::new().unwrap();
        env::remove_var("MEDA_VM_DIR");

        let result = cleanup_networking(&config, "nonexistent-vm").await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_192_168_slash_24_octet() {
        assert_eq!(parse_192_168_slash_24_octet("192.168.26.0/24"), Some(26));
        assert_eq!(parse_192_168_slash_24_octet("192.168.0.0/24"), Some(0));
        assert_eq!(parse_192_168_slash_24_octet("192.168.255.0/24"), Some(255));

        // Reject non-/24, non-192.168, or non-.0 destinations so we never
        // falsely claim an octet from an unrelated route.
        assert_eq!(parse_192_168_slash_24_octet("10.0.0.0/24"), None);
        assert_eq!(parse_192_168_slash_24_octet("192.168.26.0/16"), None);
        assert_eq!(parse_192_168_slash_24_octet("192.168.26.1/32"), None);
        assert_eq!(parse_192_168_slash_24_octet("192.168.26.0"), None);
        assert_eq!(parse_192_168_slash_24_octet("192.168.999.0/24"), None);
        assert_eq!(parse_192_168_slash_24_octet(""), None);
    }
}
