use crate::config::Config;
use crate::error::{Error, Result};
use crate::network::{generate_random_mac, setup_networking, cleanup_networking};
use crate::util::{check_process_running, download_file, ensure_dependency, run_command, run_command_with_output, write_string_to_file};
use log::info;
use serde::Serialize;
use std::fs;
use std::io::Write;
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
            &["convert", "-O", "raw", tmp_file.to_str().unwrap(), config.base_raw.to_str().unwrap()]
        )?;
        
        fs::remove_file(tmp_file)?;
    }
    
    // Download firmware if needed
    if !config.fw_bin.exists() {
        info!("Downloading firmware");
        download_file(&config.fw_url, &config.fw_bin).await?;
        fs::set_permissions(&config.fw_bin, fs::Permissions::from_mode(0o644))?;
    }
    
    // Download cloud-hypervisor if needed
    if !config.ch_bin.exists() {
        info!("Downloading cloud-hypervisor");
        download_file(&config.ch_url, &config.ch_bin).await?;
        fs::set_permissions(&config.ch_bin, fs::Permissions::from_mode(0o755))?;
    }
    
    // Download ch-remote if needed
    if !config.cr_bin.exists() {
        info!("Downloading ch-remote");
        download_file(&config.cr_url, &config.cr_bin).await?;
        fs::set_permissions(&config.cr_bin, fs::Permissions::from_mode(0o755))?;
    }
    
    // Ensure other dependencies
    ensure_dependency("genisoimage", "genisoimage")?;
    ensure_dependency("iptables", "iptables")?;
    ensure_dependency("jq", "jq")?;
    
    Ok(())
}

pub async fn create(config: &Config, name: &str, user_data_path: Option<&str>, json_output: bool) -> Result<()> {
    let vm_dir = config.vm_dir(name);
    
    if !json_output {
        info!("Attempting to create VM: {}", name);
    }
    
    if vm_dir.exists() {
        info!("VM directory already exists at: {}", vm_dir.display());
        return Err(Error::VmAlreadyExists(name.to_string()));
    }
    
    bootstrap(config).await?;
    fs::create_dir_all(&vm_dir)?;
    
    info!("Creating rootfs");
    let rootfs_path = vm_dir.join("rootfs.raw");
    fs::copy(&config.base_raw, &rootfs_path)?;
    
    // Resize disk if needed
    if config.disk_size != "10G" {
        info!("Resizing disk to {}", config.disk_size);
        ensure_dependency("qemu-img", "qemu-utils")?;
        run_command(
            "qemu-img", 
            &["resize", rootfs_path.to_str().unwrap(), &config.disk_size]
        )?;
    }
    
    // Generate network config with a unique subnet
    let subnet = crate::network::generate_unique_subnet(config).await?;
    let tap_name = format!("tap-{}", name);
    
    // Store network config
    write_string_to_file(&vm_dir.join("subnet"), &subnet)?;
    write_string_to_file(&vm_dir.join("tapdev"), &tap_name)?;
    
    // Generate cloud-init files
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
    passwd: $6$rEnJIC81m0vtbMZY$nMsAwJxOwDTyTGfZ1w2.rVssmJbAk0I7hz3T4ufaTcOb5m81Ix9SqPQVnl49.tbXrajEw4lG4qW0g0sVXTZ5X.
    lock_passwd: false
    inactive: false
    groups: sudo
    shell: /bin/bash
ssh_pwauth: true
"#;
        write_string_to_file(&vm_dir.join("user-data"), default_user_data)?;
    }
    
    // Generate MAC address
    let mac_addr = generate_random_mac();
    write_string_to_file(&vm_dir.join("mac"), &mac_addr)?;
    
    // Network config
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
        mac_addr, subnet, subnet
    );
    write_string_to_file(&vm_dir.join("network-config"), &network_config)?;
    
    // Create cloud-init ISO
    info!("Creating cloud-init ISO");
    run_command(
        "genisoimage", 
        &[
            "-quiet", 
            "-output", vm_dir.join("ci.iso").to_str().unwrap(),
            "-volid", "cidata",
            "-joliet",
            "-rock",
            vm_dir.join("user-data").to_str().unwrap(),
            vm_dir.join("meta-data").to_str().unwrap(),
            vm_dir.join("network-config").to_str().unwrap(),
        ]
    )?;
    
    // Setup networking
    info!("Setting up host networking");
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
        mac_addr,
        vm_dir.display(),
        vm_dir.display(),
        vm_dir.display(),
        vm_dir.display()
    );
    
    let start_script_path = vm_dir.join("start.sh");
    write_string_to_file(&start_script_path, &start_script)?;
    fs::set_permissions(&start_script_path, fs::Permissions::from_mode(0o755))?;
    
    // Start the VM
    info!("Booting VM {}", name);
    run_command("bash", &[start_script_path.to_str().unwrap()])?;
    
    // Wait for VM to boot
    info!("Waiting for VM to boot");
    let vm_ip = format!("{}.2", subnet);
    
    for i in 0..60 {
        // Check if process is still running
        if !check_vm_running(config, name)? {
            let error = if let Ok(log) = fs::read_to_string(vm_dir.join("ch.log")) {
                if let Some(line) = log.lines().find(|l| l.contains("error:")) {
                    line.to_string()
                } else {
                    "Process terminated unexpectedly".to_string()
                }
            } else {
                "Process terminated unexpectedly".to_string()
            };
            
            return Err(Error::VmStartFailed(error));
        }
        
        // Try to ping the VM
        let ping_result = run_command_with_output(
            "ping", 
            &["-c1", "-W1", &vm_ip]
        );
        
        if ping_result.is_ok() && ping_result.unwrap().status.success() {
            info!("VM {} is now running at {}", name, vm_ip);
            return Ok(());
        }
        
        if i % 5 == 0 {
            print!(".");
            std::io::stdout().flush().unwrap();
        }
        
        thread::sleep(Duration::from_secs(2));
    }
    
    if check_vm_running(config, name)? {
        if json_output {
            let result = VmResult {
                success: true,
                message: format!("VM {} created and started at {}", name, vm_ip),
            };
            println!("{}", serde_json::to_string_pretty(&result)?);
        } else {
            info!("VM {} appears to be running but not responding to ping yet", name);
            println!("\nWhen ready: ssh ubuntu@{}", vm_ip);
        }
        Ok(())
    } else {
        Err(Error::VmStartFailed("VM failed to start properly".to_string()))
    }
}

pub async fn list(config: &Config, json_output: bool) -> Result<()> {
    bootstrap(config).await?;
    
    let mut vm_list: Vec<VmInfo> = Vec::new();
    
    if !json_output {
        println!("{:<18} {:<8} {:<15} {:<10}", "NAME", "STATE", "IP", "PORTS");
    }
    
    for entry in fs::read_dir(&config.vm_root)? {
        let entry = entry?;
        let path = entry.path();
        
        if path.is_dir() {
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            let mut state = "stopped";
            let mut ip = "-".to_string();
            let mut fwd = "-".to_string();
            
            if check_vm_running(config, &name)? {
                state = "running";
                
                if let Ok(subnet) = fs::read_to_string(path.join("subnet")) {
                    ip = format!("{}.2", subnet.trim());
                }
                
                if let Ok(ports) = fs::read_to_string(path.join("ports")) {
                    fwd = ports.trim().to_string();
                }
            }
            
            if json_output {
                vm_list.push(VmInfo {
                    name: name.clone(),
                    state: state.to_string(),
                    ip: ip.clone(),
                    ports: fwd.clone(),
                });
            } else {
                println!("{:<18} {:<8} {:<15} {:<10}", name, state, ip, fwd);
            }
        }
    }
    
    if json_output {
        println!("{}", serde_json::to_string_pretty(&vm_list)?);
    }
    
    Ok(())
}

pub async fn get(config: &Config, name: &str, json_output: bool) -> Result<()> {
    let vm_dir = config.vm_dir(name);
    
    if !vm_dir.exists() {
        return Err(Error::VmNotFound(name.to_string()));
    }
    
    let is_running = check_vm_running(config, name)?;
    let mut vm_info = VmDetailedInfo {
        name: name.to_string(),
        state: if is_running { "running" } else { "stopped" }.to_string(),
        ip: None,
        details: None,
    };
    
    if is_running {
        if !json_output {
            info!("VM {} is running", name);
        }
        
        let output = run_command_with_output(
            config.cr_bin.to_str().unwrap(),
            &["--api-socket", vm_dir.join("api.sock").to_str().unwrap(), "info"]
        )?;
        
        let stdout = String::from_utf8_lossy(&output.stdout);
        let parsed = serde_json::from_str::<serde_json::Value>(&stdout)?;
        
        if json_output {
            vm_info.details = Some(parsed);
            
            if let Ok(subnet) = fs::read_to_string(vm_dir.join("subnet")) {
                vm_info.ip = Some(format!("{}.2", subnet.trim()));
            }
            
            println!("{}", serde_json::to_string_pretty(&vm_info)?);
        } else {
            println!("{}", serde_json::to_string_pretty(&parsed)?);
        }
    } else {
        if !json_output {
            info!("VM {} is not running", name);
            
            if let Ok(subnet) = fs::read_to_string(vm_dir.join("subnet")) {
                println!("\nTo start VM: meda start {}", name);
                println!("When running: ssh ubuntu@{}.2\n", subnet.trim());
            }
        } else {
            if let Ok(subnet) = fs::read_to_string(vm_dir.join("subnet")) {
                vm_info.ip = Some(format!("{}.2", subnet.trim()));
            }
            
            println!("{}", serde_json::to_string_pretty(&vm_info)?);
        }
    }
    
    Ok(())
}

pub async fn start(config: &Config, name: &str, json_output: bool) -> Result<()> {
    let vm_dir = config.vm_dir(name);
    
    if !vm_dir.exists() {
        return Err(Error::VmNotFound(name.to_string()));
    }
    
    if check_vm_running(config, name)? {
        if json_output {
            let result = VmResult {
                success: true,
                message: format!("VM {} is already running", name),
            };
            println!("{}", serde_json::to_string_pretty(&result)?);
        } else {
            info!("VM {} is already running", name);
        }
        return Ok(());
    }
    
    // Ensure network device is set up
    if vm_dir.join("tapdev").exists() && vm_dir.join("subnet").exists() {
        let tap = fs::read_to_string(vm_dir.join("tapdev"))?.trim().to_string();
        let subnet = fs::read_to_string(vm_dir.join("subnet"))?.trim().to_string();
        
        setup_networking(config, name, &tap, &subnet).await?;
    } else {
        return Err(Error::NetworkConfigMissing(name.to_string()));
    }
    
    info!("Starting VM {}", name);
    let start_script = vm_dir.join("start.sh");
    
    if start_script.exists() {
        run_command("bash", &[start_script.to_str().unwrap()])?;
    } else {
        return Err(Error::Other(format!("Start script for VM {} is missing", name)));
    }
    
    // Wait for VM to boot
    let subnet = fs::read_to_string(vm_dir.join("subnet"))?.trim().to_string();
    let vm_ip = format!("{}.2", subnet);
    
    for _ in 0..20 {
        let ping_result = run_command_with_output(
            "ping", 
            &["-c1", "-W1", &vm_ip]
        );
        
        if ping_result.is_ok() && ping_result.unwrap().status.success() {
            if json_output {
                let result = VmResult {
                    success: true,
                    message: format!("VM {} is now running at {}", name, vm_ip),
                };
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                info!("VM {} is now running at {}", name, vm_ip);
                println!("\nVM {} → ssh ubuntu@{}", name, vm_ip);
            }
            return Ok(());
        }
        
        thread::sleep(Duration::from_secs(1));
    }
    
    if json_output {
        let result = VmResult {
            success: true,
            message: format!("VM {} is starting at {}", name, vm_ip),
        };
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        info!("VM may still be booting. Check status with 'meda list'");
        println!("\nVM {} → ssh -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null ubuntu@{}", name, vm_ip);
        println!("Password authentication is enabled with password: ubuntu");
    }
    
    Ok(())
}

pub async fn stop(config: &Config, name: &str, json_output: bool) -> Result<()> {
    let vm_dir = config.vm_dir(name);
    
    if !vm_dir.exists() {
        return Err(Error::VmNotFound(name.to_string()));
    }
    
    if !check_vm_running(config, name)? {
        if json_output {
            let result = VmResult {
                success: true,
                message: format!("VM {} is not running", name),
            };
            println!("{}", serde_json::to_string_pretty(&result)?);
        } else {
            info!("VM {} is not running", name);
        }
        return Ok(());
    }
    
    info!("Stopping VM {}", name);
    let api_sock = vm_dir.join("api.sock");
    
    if api_sock.exists() {
        // Try graceful shutdown first
        run_command(
            config.cr_bin.to_str().unwrap(),
            &["--api-socket", api_sock.to_str().unwrap(), "power-button"]
        )?;
        
        // Wait for VM to stop
        for _ in 0..15 {
            if !check_vm_running(config, name)? {
                if json_output {
                    let result = VmResult {
                        success: true,
                        message: format!("VM {} stopped", name),
                    };
                    println!("{}", serde_json::to_string_pretty(&result)?);
                } else {
                    info!("VM {} stopped", name);
                }
                return Ok(());
            }
            
            thread::sleep(Duration::from_secs(1));
        }
        
        // Force kill if still running
        if let Ok(pid_str) = fs::read_to_string(vm_dir.join("pid")) {
            if let Ok(pid) = pid_str.trim().parse::<u32>() {
                info!("Force stopping VM {}", name);
                
                // Try SIGTERM first
                let _ = Command::new("kill")
                    .arg("-TERM")
                    .arg(pid.to_string())
                    .status();
                
                thread::sleep(Duration::from_secs(2));
                
                // Then SIGKILL if needed
                let _ = Command::new("kill")
                    .arg("-KILL")
                    .arg(pid.to_string())
                    .status();
                
                fs::remove_file(vm_dir.join("api.sock")).ok();
                fs::remove_file(vm_dir.join("pid")).ok();
            }
        }
    }
    
    if json_output {
        let result = VmResult {
            success: true,
            message: format!("VM {} stopped", name),
        };
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        info!("VM {} stopped", name);
    }
    Ok(())
}

pub async fn delete(config: &Config, name: &str, json_output: bool) -> Result<()> {
    let vm_dir = config.vm_dir(name);
    
    if !vm_dir.exists() {
        return Err(Error::VmNotFound(name.to_string()));
    }
    
    // Stop VM if running
    if check_vm_running(config, name)? {
        stop(config, name, false).await?;
    }
    
    // Clean up networking
    cleanup_networking(config, name).await?;
    
    // Remove VM directory
    fs::remove_dir_all(vm_dir)?;
    
    if json_output {
        let result = VmResult {
            success: true,
            message: format!("VM {} removed", name),
        };
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        info!("VM {} removed", name);
    }
    Ok(())
}

pub fn check_vm_running(config: &Config, name: &str) -> Result<bool> {
    let vm_dir = config.vm_dir(name);
    let pid_file = vm_dir.join("pid");
    let api_sock = vm_dir.join("api.sock");
    
    // First check if the VM directory exists
    if !vm_dir.exists() {
        return Ok(false);
    }
    
    // Check if we have a PID file and the process is running
    if pid_file.exists() {
        if let Ok(pid_str) = fs::read_to_string(&pid_file) {
            if let Ok(pid) = pid_str.trim().parse::<u32>() {
                if check_process_running(pid) {
                    // Process exists
                    return Ok(true);
                } else {
                    // Process doesn't exist, clean up stale files
                    fs::remove_file(&api_sock).ok();
                    fs::remove_file(&pid_file).ok();
                    return Ok(false);
                }
            }
        }
    }
    
    // If we get here, either there's no PID file or it couldn't be parsed
    // Try to find the cloud-hypervisor process by looking for the VM name in the process list
    let output = run_command_with_output(
        "ps", 
        &["aux"]
    )?;
    
    let stdout = String::from_utf8_lossy(&output.stdout);
    let vm_path = vm_dir.to_string_lossy().to_string();
    
    for line in stdout.lines() {
        if line.contains("cloud-hypervisor") && line.contains(&vm_path) && !line.contains("grep") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                if let Ok(pid) = parts[1].parse::<u32>() {
                    // Found running process, create pid file
                    fs::write(&pid_file, pid.to_string())?;
                    return Ok(true);
                }
            }
        }
    }
    
    // If the socket exists but we couldn't find a process, clean it up
    if api_sock.exists() {
        fs::remove_file(&api_sock).ok();
    }
    
    Ok(false)
}
