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

pub async fn setup_networking(_config: &Config, name: &str, tap_name: &str, subnet: &str) -> Result<()> {
    debug!("Setting up networking for VM {}", name);
    
    // Check if tap device exists
    let output = run_command_with_output("ip", &["link", "show", tap_name])?;
    
    if !output.status.success() {
        // Create tap device
        run_command("ip", &["tuntap", "add", tap_name, "mode", "tap"])?;
        run_command("ip", &["addr", "add", &format!("{}.1/24", subnet), "dev", tap_name])?;
        run_command("ip", &["link", "set", tap_name, "up"])?;
    }
    
    // Enable forwarding
    run_command("sysctl", &["-q", "net.ipv4.ip_forward=1"])?;
    
    // Check if masquerade rule exists
    let check_cmd = format!("iptables -t nat -C POSTROUTING -s {}.0/24 -j MASQUERADE", subnet);
    let check_result = run_command_with_output("bash", &["-c", &check_cmd]);
    
    if check_result.is_err() || !check_result.unwrap().status.success() {
        // Add masquerade rule
        run_command(
            "iptables", 
            &["-t", "nat", "-A", "POSTROUTING", "-s", &format!("{}.0/24", subnet), "-j", "MASQUERADE"]
        )?;
    }
    
    // Allow traffic from VM to leave host
    let check_forward = format!("iptables -C FORWARD -i {} -j ACCEPT", tap_name);
    let check_result = run_command_with_output("bash", &["-c", &check_forward]);
    
    if check_result.is_err() || !check_result.unwrap().status.success() {
        run_command("iptables", &["-A", "FORWARD", "-i", tap_name, "-j", "ACCEPT"])?;
        run_command(
            "iptables", 
            &["-A", "FORWARD", "-o", tap_name, "-m", "conntrack", "--ctstate", "RELATED,ESTABLISHED", "-j", "ACCEPT"]
        )?;
    }
    
    Ok(())
}

pub async fn port_forward(config: &Config, name: &str, host_port: u16, guest_port: u16) -> Result<()> {
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
        "iptables", 
        &[
            "-t", "nat", "-D", "PREROUTING", 
            "-p", "tcp", "--dport", &host_port.to_string(), 
            "-j", "DNAT", "--to", &format!("{}.2:{}", subnet, guest_port)
        ]
    );
    
    // Add new port forward
    run_command(
        "iptables", 
        &[
            "-t", "nat", "-A", "PREROUTING", 
            "-p", "tcp", "--dport", &host_port.to_string(), 
            "-j", "DNAT", "--to", &format!("{}.2:{}", subnet, guest_port)
        ]
    )?;
    
    // Save port forwarding info
    fs::write(vm_dir.join("ports"), format!("{}->{}", host_port, guest_port))?;
    
    info!("Port forwarding set up: localhost:{} -> {}.2:{}", host_port, subnet, guest_port);
    
    Ok(())
}

pub async fn cleanup_networking(config: &Config, name: &str) -> Result<()> {
    let vm_dir = config.vm_dir(name);
    
    // Clean up tap device
    if let Ok(tap_name) = fs::read_to_string(vm_dir.join("tapdev")) {
        let tap_name = tap_name.trim();
        let _ = run_command("ip", &["link", "del", tap_name]);
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
                "iptables", 
                &["-t", "nat", "-D", "POSTROUTING", "-s", &format!("{}.0/24", subnet), "-j", "MASQUERADE"]
            );
        }
    }
    
    Ok(())
}
