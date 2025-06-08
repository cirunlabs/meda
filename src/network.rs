use crate::config::Config;
use crate::error::{Error, Result};
use crate::util::{run_command, run_command_with_output};
use log::{debug, info};
use rand::Rng;
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

pub async fn generate_unique_subnet(config: &Config) -> Result<String> {
    // Get all existing subnets
    let mut used_subnets = Vec::new();

    if let Ok(entries) = fs::read_dir(&config.vm_root) {
        for entry in entries {
            if let Ok(entry) = entry {
                let path = entry.path();
                if path.is_dir() {
                    let subnet_file = path.join("subnet");
                    if subnet_file.exists() {
                        if let Ok(subnet) = fs::read_to_string(subnet_file) {
                            let subnet = subnet.trim();
                            if subnet.starts_with("192.168.") {
                                if let Some(octet_str) = subnet.strip_prefix("192.168.") {
                                    if let Ok(octet) = octet_str.parse::<u8>() {
                                        used_subnets.push(octet);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Generate a unique subnet
    let mut attempts = 0;
    let max_attempts = 200; // Avoid infinite loop

    while attempts < max_attempts {
        let octet = generate_random_octet();
        if !used_subnets.contains(&octet) {
            return Ok(format!("192.168.{}", octet));
        }
        attempts += 1;
    }

    // If we've tried too many times, return an error
    Err(Error::Other(
        "Could not generate a unique subnet after multiple attempts".to_string(),
    ))
}

pub async fn generate_unique_tap_name(config: &Config, vm_name: &str) -> Result<String> {
    // Get all existing TAP device names from VM directories
    let mut used_tap_names = Vec::new();
    
    if let Ok(entries) = fs::read_dir(&config.vm_root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let tapdev_file = path.join("tapdev");
                if let Ok(tap_name) = fs::read_to_string(tapdev_file) {
                    used_tap_names.push(tap_name.trim().to_string());
                }
            }
        }
    }
    
    // Also check currently active TAP devices on the system
    if let Ok(output) = run_command_with_output("ip", &["link", "show"]) {
        if output.status.success() {
            let output_str = String::from_utf8_lossy(&output.stdout);
            for line in output_str.lines() {
                if line.contains("tap-") {
                    if let Some(tap_start) = line.find("tap-") {
                        let tap_part = &line[tap_start..];
                        if let Some(colon_pos) = tap_part.find(':') {
                            let tap_name = tap_part[..colon_pos].to_string();
                            used_tap_names.push(tap_name);
                        }
                    }
                }
            }
        }
    }
    
    // Start with a truncated VM name (max 10 chars for tap- prefix)
    let base_name = if vm_name.len() > 8 {
        &vm_name[..8]
    } else {
        vm_name
    };
    
    // Try the base name first
    let candidate = format!("tap-{}", base_name);
    if !used_tap_names.contains(&candidate) {
        return Ok(candidate);
    }
    
    // If base name is taken, append numbers
    for i in 1..=999 {
        let candidate = format!("tap-{}-{}", base_name, i);
        if !used_tap_names.contains(&candidate) {
            return Ok(candidate);
        }
    }
    
    // If all numeric suffixes are exhausted, use random suffix
    let mut rng = rand::thread_rng();
    for _ in 0..100 {
        let random_suffix: u32 = rng.gen_range(1000..=9999);
        let candidate = format!("tap-{}-{}", base_name, random_suffix);
        if !used_tap_names.contains(&candidate) {
            return Ok(candidate);
        }
    }
    
    Err(Error::Other("Could not generate a unique TAP device name after multiple attempts".to_string()))
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

    // Check if masquerade rule exists
    let check_cmd = format!(
        "sudo iptables -t nat -C POSTROUTING -s {}.0/24 -j MASQUERADE",
        subnet
    );
    let check_result = run_command_with_output("bash", &["-c", &check_cmd]);

    if check_result.is_err() || !check_result.unwrap().status.success() {
        // Add masquerade rule
        run_command(
            "sudo",
            &[
                "iptables",
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

    // Allow traffic from VM to leave host
    let check_forward = format!("sudo iptables -C FORWARD -i {} -j ACCEPT", tap_name);
    let check_result = run_command_with_output("bash", &["-c", &check_forward]);

    if check_result.is_err() || !check_result.unwrap().status.success() {
        run_command(
            "sudo",
            &["iptables", "-A", "FORWARD", "-i", tap_name, "-j", "ACCEPT"],
        )?;
        run_command(
            "sudo",
            &[
                "iptables",
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

pub async fn cleanup_networking(config: &Config, name: &str) -> Result<()> {
    let vm_dir = config.vm_dir(name);

    // Clean up tap device
    if let Ok(tap_name) = fs::read_to_string(vm_dir.join("tapdev")) {
        let tap_name = tap_name.trim();
        let _ = run_command("sudo", &["ip", "link", "del", tap_name]);
    }

    // Clean up iptables rules if this is the last VM using this subnet
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
            // Remove iptables rule
            let _ = run_command(
                "sudo",
                &[
                    "iptables",
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
            assert!(octet >= 16 && octet <= 215);
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
}
