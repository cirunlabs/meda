use crate::config::Config;
use crate::error::{Error, Result};
use crate::network::{generate_random_mac, setup_networking, cleanup_networking};
use crate::util::{check_process_running, download_file, ensure_dependency, run_command, write_string_to_file};
use log::info;
use serde::Serialize;
use std::fs;
use std::process::Command;
use std::thread;
use std::time::Duration;
use std::os::unix::fs::PermissionsExt;

#[derive(Serialize)]
pub struct VmInfo {
    pub name: String,
    pub state: String,
    pub ip: String,
    pub ports: String,
}

#[derive(Serialize)]
pub struct VmResult {
    pub success: bool,
    pub message: String,
}

#[derive(Serialize)]
pub struct VmDetailedInfo {
    pub name: String,
    pub state: String,
    pub ip: Option<String>,
    pub details: Option<serde_json::Value>,
}

pub async fn bootstrap(config: &Config) -> Result<()> {
    info!("Bootstrapping environment");
    info!("Ensuring directories exist");
    config.ensure_dirs()?;
    
    // Download base image if needed
    if !config.base_raw.exists() {
        info!("Downloading Ubuntu image");
        let tmp_file = config.asset_dir.join("img.qcow2");
        download_file(&config.os_url, &tmp_file).await?;
        
        ensure_dependency("qemu-img", "qemu-utils")?;
        
        info!("Converting to raw format");
        run_command(
            "qemu-img",
            &[
                "convert",
                "-f", "qcow2",
                "-O", "raw",
                tmp_file.to_str().unwrap(),
                config.base_raw.to_str().unwrap(),
            ],
        )?;
        
        // Resize image
        run_command(
            "qemu-img",
            &[
                "resize",
                config.base_raw.to_str().unwrap(),
                &config.disk_size,
            ],
        )?;
        
        // Remove temporary file
        fs::remove_file(&tmp_file).ok();
    }
    
    // Download firmware if needed
    if !config.fw_bin.exists() {
        info!("Downloading firmware");
        download_file(&config.fw_url, &config.fw_bin).await?;
        
        // Make firmware executable
        let mut perms = fs::metadata(&config.fw_bin)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&config.fw_bin, perms)?;
    }
    
    // Download cloud-hypervisor if needed
    if !config.ch_bin.exists() {
        info!("Downloading cloud-hypervisor");
        download_file(&config.ch_url, &config.ch_bin).await?;
        
        // Make cloud-hypervisor executable
        let mut perms = fs::metadata(&config.ch_bin)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&config.ch_bin, perms)?;
    }
    
    // Download ch-remote if needed
    if !config.cr_bin.exists() {
        info!("Downloading ch-remote");
        download_file(&config.cr_url, &config.cr_bin).await?;
        
        // Make ch-remote executable
        let mut perms = fs::metadata(&config.cr_bin)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&config.cr_bin, perms)?;
    }
    
    // Ensure other dependencies
    ensure_dependency("genisoimage", "genisoimage")?;
    
    info!("Bootstrap complete");
    Ok(())
}

pub async fn bootstrap_binaries_only(config: &Config) -> Result<()> {
    info!("Bootstrapping hypervisor binaries");
    info!("Ensuring directories exist");
    config.ensure_dirs()?;
    
    // Download firmware if needed
    if !config.fw_bin.exists() {
        info!("Downloading firmware");
        download_file(&config.fw_url, &config.fw_bin).await?;
        
        // Make firmware executable
        let mut perms = fs::metadata(&config.fw_bin)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&config.fw_bin, perms)?;
    }
    
    // Download cloud-hypervisor if needed
    if !config.ch_bin.exists() {
        info!("Downloading cloud-hypervisor");
        download_file(&config.ch_url, &config.ch_bin).await?;
        
        // Make cloud-hypervisor executable
        let mut perms = fs::metadata(&config.ch_bin)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&config.ch_bin, perms)?;
    }
    
    // Download ch-remote if needed
    if !config.cr_bin.exists() {
        info!("Downloading ch-remote");
        download_file(&config.cr_url, &config.cr_bin).await?;
        
        // Make ch-remote executable
        let mut perms = fs::metadata(&config.cr_bin)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&config.cr_bin, perms)?;
    }
    
    // Ensure other dependencies
    ensure_dependency("genisoimage", "genisoimage")?;
    
    info!("Hypervisor binaries bootstrap complete");
    Ok(())
}

pub async fn create(config: &Config, name: &str, user_data_path: Option<&str>, json: bool) -> Result<()> {
    let vm_dir = config.vm_dir(name);
    
    if vm_dir.exists() {
        return Err(Error::VmAlreadyExists(name.to_string()));
    }
    
    if !json {
        info!("Creating VM: {}", name);
    }
    
    // Bootstrap to ensure we have the necessary binaries
    bootstrap(config).await?;
    
    // Create VM directory
    fs::create_dir_all(&vm_dir)?;
    
    // Copy base image
    if !json {
        info!("Copying base image");
    }
    let vm_rootfs = vm_dir.join("rootfs.raw");
    fs::copy(&config.base_raw, &vm_rootfs)?;
    
    // Generate network config with a unique subnet
    let subnet = crate::network::generate_unique_subnet(config).await?;
    // Truncate VM name for tap device to avoid ifname length limits (15 chars max)
    let tap_suffix = if name.len() > 10 {
        &name[..10]
    } else {
        name
    };
    let tap_name = format!("tap-{}", tap_suffix);
    
    // Store network config
    write_string_to_file(&vm_dir.join("subnet"), &subnet)?;
    write_string_to_file(&vm_dir.join("tapdev"), &tap_name)?;
    
    // Create cloud-init files
    let meta_data = format!(
        "instance-id: {}\nlocal-hostname: {}\n",
        name, name
    );
    write_string_to_file(&vm_dir.join("meta-data"), &meta_data)?;
    
    // User data
    if let Some(path) = user_data_path {
        fs::copy(path, vm_dir.join("user-data"))?;
    } else {
        let default_user_data = r#"#cloud-config
users:
  - name: cirun
    sudo: ALL=(ALL) NOPASSWD:ALL
    passwd: $6$ep7LxhhmhQHf.TiY$qPJVJQCnPMnyFdmD0ymP7CH2dos0awET8JlSzDqoiK6AOQwDpx8fCLJ1C5c7nvkVJbIpQCOalC8l2BGkRzogM.
    lock_passwd: false
    inactive: false
    groups: sudo
    shell: /bin/bash
ssh_pwauth: true
"#;
        write_string_to_file(&vm_dir.join("user-data"), default_user_data)?;
    }
    
    // Generate MAC address
    let mac = generate_random_mac();
    write_string_to_file(&vm_dir.join("mac"), &mac)?;
    
    // Create cloud-init ISO
    let ci_dir = vm_dir.join("ci");
    fs::create_dir_all(&ci_dir)?;
    
    // Copy cloud-init files to ci directory
    for file in ["meta-data", "user-data"] {
        let src = vm_dir.join(file);
        let dst = ci_dir.join(file);
        fs::copy(&src, &dst)?;
    }
    
    // Create network-config
    let network_config = format!(
        r#"version: 2
ethernets:
  ens4:
    match:
       macaddress: {}
    addresses: [{}.2/24]
    gateway4: {}.1
    set-name: ens4
    nameservers:
      addresses: [8.8.8.8, 1.1.1.1]
"#,
        mac, subnet, subnet
    );
    write_string_to_file(&ci_dir.join("network-config"), &network_config)?;
    
    // Create cloud-init ISO
    let ci_iso = vm_dir.join("ci.iso");
    run_command(
        "genisoimage",
        &[
            "-output", ci_iso.to_str().unwrap(),
            "-volid", "cidata",
            "-joliet",
            "-rock",
            ci_dir.to_str().unwrap(),
        ],
    )?;
    
    // Setup networking
    if !json {
        info!("Setting up host networking");
    }
    setup_networking(config, name, &tap_name, &subnet).await?;
    
    // Create start script
    let start_script = format!(
        r#"#!/bin/bash
cd "{}"
{} \
  --api-socket path={}/api.sock \
  --console off \
  --serial tty \
  --kernel "{}" \
  --cpus boot={} \
  --memory size={} \
  --disk path={}/rootfs.raw path="{}/ci.iso" \
  --net tap={},mac={} \
  --rng src=/dev/urandom \
  > "{}/ch.log" 2>&1 &
echo $! > "{}/pid"

# Check if command started successfully
sleep 2
if ! ps -p $(cat "{}/pid" 2>/dev/null) &>/dev/null; then
  echo "ERROR: Cloud Hypervisor failed to start. Check log: {}/ch.log" >&2
  exit 1
fi
"#,
        vm_dir.display(),
        config.ch_bin.display(),
        vm_dir.display(),
        config.fw_bin.display(),
        config.cpus,
        config.mem,
        vm_dir.display(),
        vm_dir.display(),
        tap_name,
        mac,
        vm_dir.display(),
        vm_dir.display(),
        vm_dir.display(),
        vm_dir.display()
    );
    
    let start_script_path = vm_dir.join("start.sh");
    write_string_to_file(&start_script_path, &start_script)?;
    
    // Make start script executable
    let mut perms = fs::metadata(&start_script_path)?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&start_script_path, perms)?;
    
    let message = format!("Successfully created VM: {}", name);
    if json {
        let result = VmResult {
            success: true,
            message,
        };
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        info!("{}", message);
    }
    
    Ok(())
}

pub async fn list(config: &Config, json: bool) -> Result<()> {
    config.ensure_dirs()?;
    
    if !config.vm_root.exists() {
        if json {
            println!("[]");
        } else {
            info!("No VMs found");
        }
        return Ok(());
    }
    
    let mut vms = Vec::new();
    
    for entry in fs::read_dir(&config.vm_root)? {
        let entry = entry?;
        let path = entry.path();
        
        if path.is_dir() {
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            let state = if check_vm_running(config, &name)? {
                "running".to_string()
            } else {
                "stopped".to_string()
            };
            
            let ip = get_vm_ip(config, &name).unwrap_or_else(|_| "N/A".to_string());
            let ports = get_vm_ports(config, &name).unwrap_or_else(|_| "N/A".to_string());
            
            vms.push(VmInfo {
                name,
                state,
                ip,
                ports,
            });
        }
    }
    
    if json {
        println!("{}", serde_json::to_string_pretty(&vms)?);
    } else {
        if vms.is_empty() {
            info!("No VMs found");
        } else {
            println!("{:<15} {:<10} {:<15} {:<10}", "NAME", "STATE", "IP", "PORTS");
            println!("{}", "-".repeat(50));
            for vm in vms {
                println!("{:<15} {:<10} {:<15} {:<10}", vm.name, vm.state, vm.ip, vm.ports);
            }
        }
    }
    
    Ok(())
}

pub async fn get(config: &Config, name: &str, json: bool) -> Result<()> {
    let vm_dir = config.vm_dir(name);
    
    if !vm_dir.exists() {
        return Err(Error::VmNotFound(name.to_string()));
    }
    
    let state = if check_vm_running(config, name)? {
        "running".to_string()
    } else {
        "stopped".to_string()
    };
    
    let ip = get_vm_ip(config, name).ok();
    
    // Collect additional details
    let mut details = serde_json::Map::new();
    
    // Add network info
    if let Ok(subnet) = fs::read_to_string(vm_dir.join("subnet")) {
        details.insert("subnet".to_string(), serde_json::Value::String(subnet.trim().to_string()));
    }
    
    if let Ok(mac) = fs::read_to_string(vm_dir.join("mac")) {
        details.insert("mac".to_string(), serde_json::Value::String(mac.trim().to_string()));
    }
    
    if let Ok(tap) = fs::read_to_string(vm_dir.join("tapdev")) {
        details.insert("tap_device".to_string(), serde_json::Value::String(tap.trim().to_string()));
    }
    
    // Add port forwarding info
    if let Ok(ports) = fs::read_to_string(vm_dir.join("ports")) {
        details.insert("port_forwards".to_string(), serde_json::Value::String(ports.trim().to_string()));
    }
    
    // Add VM directory path
    details.insert("vm_dir".to_string(), serde_json::Value::String(vm_dir.to_string_lossy().to_string()));
    
    let vm_info = VmDetailedInfo {
        name: name.to_string(),
        state,
        ip,
        details: Some(serde_json::Value::Object(details)),
    };
    
    if json {
        println!("{}", serde_json::to_string_pretty(&vm_info)?);
    } else {
        println!("VM: {}", vm_info.name);
        println!("State: {}", vm_info.state);
        if let Some(ip) = vm_info.ip {
            println!("IP: {}", ip);
        }
        if let Some(details) = vm_info.details {
            if let serde_json::Value::Object(map) = details {
                for (key, value) in map {
                    println!("{}: {}", key, value.as_str().unwrap_or("N/A"));
                }
            }
        }
    }
    
    Ok(())
}

pub async fn start(config: &Config, name: &str, json: bool) -> Result<()> {
    let vm_dir = config.vm_dir(name);
    
    if !vm_dir.exists() {
        return Err(Error::VmNotFound(name.to_string()));
    }
    
    if check_vm_running(config, name)? {
        let message = format!("VM {} is already running", name);
        if json {
            let result = VmResult {
                success: false,
                message,
            };
            println!("{}", serde_json::to_string_pretty(&result)?);
        } else {
            return Err(Error::VmAlreadyRunning(name.to_string()));
        }
        return Ok(());
    }
    
    if !json {
        info!("Starting VM: {}", name);
    }
    
    let start_script = vm_dir.join("start.sh");
    if !start_script.exists() {
        return Err(Error::Other(format!("Start script not found for VM: {}", name)));
    }
    
    // Run the start script
    run_command("bash", &[start_script.to_str().unwrap()])?;
    
    // Wait a moment for the VM to start
    thread::sleep(Duration::from_secs(3));
    
    // Verify VM is running
    if !check_vm_running(config, name)? {
        return Err(Error::Other(format!("Failed to start VM: {}", name)));
    }
    
    let message = format!("Successfully started VM: {}", name);
    if json {
        let result = VmResult {
            success: true,
            message,
        };
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        info!("{}", message);
    }
    
    Ok(())
}

pub async fn stop(config: &Config, name: &str, json: bool) -> Result<()> {
    let vm_dir = config.vm_dir(name);
    
    if !vm_dir.exists() {
        return Err(Error::VmNotFound(name.to_string()));
    }
    
    if !check_vm_running(config, name)? {
        let message = format!("VM {} is not running", name);
        if json {
            let result = VmResult {
                success: false,
                message,
            };
            println!("{}", serde_json::to_string_pretty(&result)?);
        } else {
            return Err(Error::VmNotRunning(name.to_string()));
        }
        return Ok(());
    }
    
    if !json {
        info!("Stopping VM: {}", name);
    }
    
    let pid_file = vm_dir.join("pid");
    if let Ok(pid_str) = fs::read_to_string(&pid_file) {
        if let Ok(pid) = pid_str.trim().parse::<u32>() {
            // Try graceful shutdown first
            let _ = Command::new("kill").args(["-TERM", &pid.to_string()]).output();
            
            // Wait for graceful shutdown
            for _ in 0..10 {
                if !check_process_running(pid) {
                    break;
                }
                thread::sleep(Duration::from_millis(500));
            }
            
            // Force kill if still running
            if check_process_running(pid) {
                let _ = Command::new("kill").args(["-KILL", &pid.to_string()]).output();
            }
        }
    }
    
    // Clean up PID file
    fs::remove_file(&pid_file).ok();
    
    let message = format!("Successfully stopped VM: {}", name);
    if json {
        let result = VmResult {
            success: true,
            message,
        };
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        info!("{}", message);
    }
    
    Ok(())
}

pub async fn delete(config: &Config, name: &str, json: bool) -> Result<()> {
    let vm_dir = config.vm_dir(name);
    
    if !vm_dir.exists() {
        return Err(Error::VmNotFound(name.to_string()));
    }
    
    // Stop VM if running
    if check_vm_running(config, name)? {
        if !json {
            info!("Stopping VM before deletion");
        }
        stop(config, name, json).await?;
    }
    
    if !json {
        info!("Deleting VM: {}", name);
    }
    
    // Clean up networking
    cleanup_networking(config, name).await?;
    
    // Remove VM directory
    fs::remove_dir_all(&vm_dir)?;
    
    let message = format!("Successfully deleted VM: {}", name);
    if json {
        let result = VmResult {
            success: true,
            message,
        };
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        info!("{}", message);
    }
    
    Ok(())
}

pub fn check_vm_running(config: &Config, name: &str) -> Result<bool> {
    let vm_dir = config.vm_dir(name);
    let pid_file = vm_dir.join("pid");
    
    if !pid_file.exists() {
        return Ok(false);
    }
    
    if let Ok(pid_str) = fs::read_to_string(&pid_file) {
        if let Ok(pid) = pid_str.trim().parse::<u32>() {
            return Ok(check_process_running(pid));
        }
    }
    
    Ok(false)
}

fn get_vm_ip(config: &Config, name: &str) -> Result<String> {
    let vm_dir = config.vm_dir(name);
    let subnet_file = vm_dir.join("subnet");
    
    if !subnet_file.exists() {
        return Err(Error::Other("Subnet file not found".to_string()));
    }
    
    let subnet = fs::read_to_string(subnet_file)?;
    Ok(format!("{}.2", subnet.trim()))
}

fn get_vm_ports(config: &Config, name: &str) -> Result<String> {
    let vm_dir = config.vm_dir(name);
    let ports_file = vm_dir.join("ports");
    
    if !ports_file.exists() {
        return Ok("none".to_string());
    }
    
    let ports = fs::read_to_string(ports_file)?;
    Ok(ports.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::env;

    fn setup_test_config() -> (Config, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        env::set_var("MEDA_ASSET_DIR", temp_dir.path().join("assets").to_str().unwrap());
        env::set_var("MEDA_VM_DIR", temp_dir.path().join("vms").to_str().unwrap());
        let config = Config::new().unwrap();
        env::remove_var("MEDA_ASSET_DIR");
        env::remove_var("MEDA_VM_DIR");
        (config, temp_dir)
    }

    #[test]
    fn test_check_vm_running_no_pid_file() {
        let (config, _temp_dir) = setup_test_config();
        let result = check_vm_running(&config, "nonexistent-vm").unwrap();
        assert!(!result);
    }

    #[test]
    fn test_check_vm_running_invalid_pid() {
        let (config, _temp_dir) = setup_test_config();
        
        // Create VM directory with invalid PID file
        let vm_dir = config.vm_dir("test-vm");
        std::fs::create_dir_all(&vm_dir).unwrap();
        std::fs::write(vm_dir.join("pid"), "invalid_pid").unwrap();
        
        let result = check_vm_running(&config, "test-vm").unwrap();
        assert!(!result);
    }

    #[test]
    fn test_check_vm_running_nonexistent_pid() {
        let (config, _temp_dir) = setup_test_config();
        
        // Create VM directory with nonexistent PID
        let vm_dir = config.vm_dir("test-vm");
        std::fs::create_dir_all(&vm_dir).unwrap();
        std::fs::write(vm_dir.join("pid"), "999999").unwrap();
        
        let result = check_vm_running(&config, "test-vm").unwrap();
        assert!(!result);
    }

    #[test]
    fn test_get_vm_ip() {
        let (config, _temp_dir) = setup_test_config();
        
        // Create VM directory with subnet
        let vm_dir = config.vm_dir("test-vm");
        std::fs::create_dir_all(&vm_dir).unwrap();
        std::fs::write(vm_dir.join("subnet"), "192.168.100").unwrap();
        
        let ip = get_vm_ip(&config, "test-vm").unwrap();
        assert_eq!(ip, "192.168.100.2");
    }

    #[test]
    fn test_get_vm_ip_missing_subnet() {
        let (config, _temp_dir) = setup_test_config();
        
        let result = get_vm_ip(&config, "nonexistent-vm");
        assert!(result.is_err());
    }

    #[test]
    fn test_get_vm_ports_no_file() {
        let (config, _temp_dir) = setup_test_config();
        
        // Create VM directory without ports file
        let vm_dir = config.vm_dir("test-vm");
        std::fs::create_dir_all(&vm_dir).unwrap();
        
        let ports = get_vm_ports(&config, "test-vm").unwrap();
        assert_eq!(ports, "none");
    }

    #[test]
    fn test_get_vm_ports_with_file() {
        let (config, _temp_dir) = setup_test_config();
        
        // Create VM directory with ports file
        let vm_dir = config.vm_dir("test-vm");
        std::fs::create_dir_all(&vm_dir).unwrap();
        std::fs::write(vm_dir.join("ports"), "8080->80").unwrap();
        
        let ports = get_vm_ports(&config, "test-vm").unwrap();
        assert_eq!(ports, "8080->80");
    }

    #[tokio::test]
    async fn test_list_empty_vm_dir() {
        let (config, _temp_dir) = setup_test_config();
        
        // Should not error when VM directory doesn't exist
        let result = list(&config, true).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_get_nonexistent_vm() {
        let (config, _temp_dir) = setup_test_config();
        
        let result = get(&config, "nonexistent-vm", true).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::VmNotFound(_)));
    }

    #[tokio::test]
    async fn test_start_nonexistent_vm() {
        let (config, _temp_dir) = setup_test_config();
        
        let result = start(&config, "nonexistent-vm", true).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::VmNotFound(_)));
    }

    #[tokio::test]
    async fn test_stop_nonexistent_vm() {
        let (config, _temp_dir) = setup_test_config();
        
        let result = stop(&config, "nonexistent-vm", true).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::VmNotFound(_)));
    }

    #[tokio::test]
    async fn test_delete_nonexistent_vm() {
        let (config, _temp_dir) = setup_test_config();
        
        let result = delete(&config, "nonexistent-vm", true).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::VmNotFound(_)));
    }
}