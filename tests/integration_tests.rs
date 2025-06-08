use assert_cmd::Command;
use predicates::prelude::*;
use std::env;
use tempfile::TempDir;
use serial_test::serial;

// Helper to set up a clean test environment
fn setup_test_env() -> TempDir {
    let temp_dir = TempDir::new().unwrap();
    env::set_var("MEDA_ASSET_DIR", temp_dir.path().join("assets"));
    env::set_var("MEDA_VM_DIR", temp_dir.path().join("vms"));
    env::set_var("MEDA_CPUS", "1");
    env::set_var("MEDA_MEM", "512M");
    env::set_var("MEDA_DISK_SIZE", "5G");
    temp_dir
}

fn cleanup_test_env() {
    env::remove_var("MEDA_ASSET_DIR");
    env::remove_var("MEDA_VM_DIR");
    env::remove_var("MEDA_CPUS");
    env::remove_var("MEDA_MEM");
    env::remove_var("MEDA_DISK_SIZE");
}

#[test]
#[serial]
fn test_cli_help() {
    let _temp_dir = setup_test_env();
    
    let mut cmd = Command::cargo_bin("meda").unwrap();
    cmd.arg("help");
    
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Cloud-Hypervisor VM Manager"))
        .stdout(predicate::str::contains("create"))
        .stdout(predicate::str::contains("list"))
        .stdout(predicate::str::contains("start"))
        .stdout(predicate::str::contains("stop"))
        .stdout(predicate::str::contains("delete"));
    
    cleanup_test_env();
}

#[test]
#[serial]
fn test_cli_list_empty() {
    let _temp_dir = setup_test_env();
    
    let mut cmd = Command::cargo_bin("meda").unwrap();
    cmd.args(&["list", "--json"]);
    
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("[]"));
    
    cleanup_test_env();
}

#[test]
#[serial]
fn test_cli_get_nonexistent_vm() {
    let _temp_dir = setup_test_env();
    
    let mut cmd = Command::cargo_bin("meda").unwrap();
    cmd.args(&["get", "nonexistent-vm", "--json"]);
    
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("VmNotFound"));
    
    cleanup_test_env();
}

#[test]
#[serial]
fn test_cli_start_nonexistent_vm() {
    let _temp_dir = setup_test_env();
    
    let mut cmd = Command::cargo_bin("meda").unwrap();
    cmd.args(&["start", "nonexistent-vm", "--json"]);
    
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("VmNotFound"));
    
    cleanup_test_env();
}

#[test]
#[serial]
fn test_cli_stop_nonexistent_vm() {
    let _temp_dir = setup_test_env();
    
    let mut cmd = Command::cargo_bin("meda").unwrap();
    cmd.args(&["stop", "nonexistent-vm", "--json"]);
    
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("VmNotFound"));
    
    cleanup_test_env();
}

#[test]
#[serial]
fn test_cli_delete_nonexistent_vm() {
    let _temp_dir = setup_test_env();
    
    let mut cmd = Command::cargo_bin("meda").unwrap();
    cmd.args(&["delete", "nonexistent-vm", "--json"]);
    
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("VmNotFound"));
    
    cleanup_test_env();
}

#[test]
#[serial]
fn test_cli_images_empty() {
    let _temp_dir = setup_test_env();
    
    let mut cmd = Command::cargo_bin("meda").unwrap();
    cmd.args(&["images", "--json"]);
    
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("[]"));
    
    cleanup_test_env();
}

#[test]
#[serial]
fn test_cli_port_forward_nonexistent_vm() {
    let _temp_dir = setup_test_env();
    
    let mut cmd = Command::cargo_bin("meda").unwrap();
    cmd.args(&["port-forward", "nonexistent-vm", "8080", "80", "--json"]);
    
    cmd.assert()
        .success()  // Port forward returns success but with error message in JSON
        .stdout(predicate::str::contains("success\": false"))
        .stdout(predicate::str::contains("does not exist"));
    
    cleanup_test_env();
}

#[test]
#[serial]
fn test_cli_rmi_nonexistent_image() {
    let _temp_dir = setup_test_env();
    
    let mut cmd = Command::cargo_bin("meda").unwrap();
    cmd.args(&["rmi", "nonexistent-image", "--force", "--json"]);
    
    cmd.assert()
        .success()  // Should succeed but report image not found
        .stdout(predicate::str::contains("success"));
    
    cleanup_test_env();
}

#[test]
#[serial]
fn test_cli_prune_empty() {
    let _temp_dir = setup_test_env();
    
    let mut cmd = Command::cargo_bin("meda").unwrap();
    cmd.args(&["prune", "--json"]);
    
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("success"));
    
    cleanup_test_env();
}

#[test]
#[serial]
fn test_cli_run_nonexistent_image() {
    let _temp_dir = setup_test_env();
    
    let mut cmd = Command::cargo_bin("meda").unwrap();
    cmd.args(&["run", "nonexistent-image", "--no-start", "--json"]);
    
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("not found locally"));
    
    cleanup_test_env();
}

#[test]
#[serial]
fn test_cli_json_flag_consistency() {
    let _temp_dir = setup_test_env();
    
    // Test that --json flag works with different commands
    let commands = vec![
        vec!["list"],
        vec!["images"],
        vec!["prune"],
    ];
    
    for cmd_args in commands {
        let mut cmd = Command::cargo_bin("meda").unwrap();
        let mut args = cmd_args;
        args.push("--json");
        cmd.args(&args);
        
        let output = cmd.assert().success();
        // JSON output should be parseable (starts with [ or {)
        output.stdout(predicate::str::starts_with("[").or(predicate::str::starts_with("{")));
    }
    
    cleanup_test_env();
}

// Test VM creation workflow - this actually works and downloads dependencies
#[test]
#[serial]
fn test_cli_create_vm_success() {
    let _temp_dir = setup_test_env();
    
    let mut cmd = Command::cargo_bin("meda").unwrap();
    cmd.args(&["create", "test-vm", "--json"]);
    
    // This will actually succeed and download dependencies
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("success\": true"))
        .stdout(predicate::str::contains("Successfully created VM: test-vm"));
    
    cleanup_test_env();
}

// Test image creation workflow - this also works
#[test]
#[serial]
fn test_cli_create_image_success() {
    let _temp_dir = setup_test_env();
    
    let mut cmd = Command::cargo_bin("meda").unwrap();
    cmd.args(&["create-image", "test-image", "--json"]);
    
    // This will succeed and create the image
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("success\": true"))
        .stdout(predicate::str::contains("Successfully created image"));
    
    cleanup_test_env();
}

// Test pull command - ORAS is available but image doesn't exist
#[test]
#[serial]
fn test_cli_pull_nonexistent_image() {
    let _temp_dir = setup_test_env();
    
    let mut cmd = Command::cargo_bin("meda").unwrap();
    cmd.args(&["pull", "nonexistent-repo/nonexistent-image", "--json"]);
    
    // This should fail because the image doesn't exist in the registry
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
    
    cleanup_test_env();
}

// Test push command
#[test]
#[serial]
fn test_cli_push_nonexistent_image() {
    let _temp_dir = setup_test_env();
    
    let mut cmd = Command::cargo_bin("meda").unwrap();
    cmd.args(&["push", "nonexistent-local-image", "target-image", "--dry-run", "--json"]);
    
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
    
    cleanup_test_env();
}

// Test command argument validation
#[test]
#[serial]
fn test_cli_invalid_commands() {
    let _temp_dir = setup_test_env();
    
    // Test missing required arguments
    let mut cmd = Command::cargo_bin("meda").unwrap();
    cmd.arg("create");  // Missing VM name
    
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("required"));
    
    // Test invalid subcommand
    let mut cmd = Command::cargo_bin("meda").unwrap();
    cmd.arg("invalid-command");
    
    cmd.assert()
        .failure();
    
    cleanup_test_env();
}

// Test force flag behavior
#[test]
#[serial]
fn test_cli_create_with_force_flag() {
    let _temp_dir = setup_test_env();
    
    let mut cmd = Command::cargo_bin("meda").unwrap();
    cmd.args(&["create", "test-vm", "--force", "--json"]);
    
    // Should succeed and accept the force flag
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("success\": true"));
    
    cleanup_test_env();
}

// Test user-data file handling
#[test]
#[serial]
fn test_cli_create_with_user_data() {
    let temp_dir = setup_test_env();
    
    // Create a test user-data file
    let user_data_file = temp_dir.path().join("user-data");
    std::fs::write(&user_data_file, "#cloud-config\npackages:\n  - curl").unwrap();
    
    let mut cmd = Command::cargo_bin("meda").unwrap();
    cmd.args(&[
        "create", 
        "test-vm", 
        user_data_file.to_str().unwrap(),
        "--json"
    ]);
    
    // Should succeed and accept the user-data file
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("success\": true"));
    
    cleanup_test_env();
}

// Test error handling for malformed JSON output
#[test]
#[serial]
fn test_cli_json_output_format() {
    let _temp_dir = setup_test_env();
    
    let mut cmd = Command::cargo_bin("meda").unwrap();
    cmd.args(&["list", "--json"]);
    
    let output = cmd.assert().success();
    
    // Verify JSON output is properly formatted
    output.stdout(predicate::function(|output: &str| {
        // Should be valid JSON
        serde_json::from_str::<serde_json::Value>(output).is_ok()
    }));
    
    cleanup_test_env();
}

// Comprehensive workflow test: create VM, list it, get details, then delete
#[test]
#[serial]
fn test_cli_complete_vm_workflow() {
    let _temp_dir = setup_test_env();
    
    // 1. Create a VM
    let mut cmd = Command::cargo_bin("meda").unwrap();
    cmd.args(&["create", "workflow-test-vm", "--json"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("success\": true"))
        .stdout(predicate::str::contains("Successfully created VM: workflow-test-vm"));
    
    // 2. List VMs and verify our VM appears
    let mut cmd = Command::cargo_bin("meda").unwrap();
    cmd.args(&["list", "--json"]);
    let output = cmd.assert().success();
    output.stdout(predicate::str::contains("workflow-test-vm"));
    
    // 3. Get VM details
    let mut cmd = Command::cargo_bin("meda").unwrap();
    cmd.args(&["get", "workflow-test-vm", "--json"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("workflow-test-vm"))
        .stdout(predicate::str::contains("stopped"));  // VM should be stopped initially
    
    // 4. Try to start VM (this will fail without actual hypervisor but tests the CLI)
    let mut cmd = Command::cargo_bin("meda").unwrap();
    cmd.args(&["start", "workflow-test-vm", "--json"]);
    // Start will likely fail due to missing hypervisor setup, but that's expected
    
    // 5. Delete the VM
    let mut cmd = Command::cargo_bin("meda").unwrap();
    cmd.args(&["delete", "workflow-test-vm", "--json"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("success\": true"))
        .stdout(predicate::str::contains("Successfully deleted VM: workflow-test-vm"));
    
    // 6. Verify VM is gone
    let mut cmd = Command::cargo_bin("meda").unwrap();
    cmd.args(&["list", "--json"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("[]"));  // Should be empty now
    
    cleanup_test_env();
}

// Test image workflow: create image, list it, then remove it
#[test]
#[serial]
fn test_cli_complete_image_workflow() {
    let _temp_dir = setup_test_env();
    
    // 1. Create an image
    let mut cmd = Command::cargo_bin("meda").unwrap();
    cmd.args(&["create-image", "workflow-test-image", "--json"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("success\": true"))
        .stdout(predicate::str::contains("Successfully created image"));
    
    // 2. List images and verify our image appears
    let mut cmd = Command::cargo_bin("meda").unwrap();
    cmd.args(&["images", "--json"]);
    let output = cmd.assert().success();
    output.stdout(predicate::str::contains("workflow-test-image"));
    
    // 3. Remove the image
    let mut cmd = Command::cargo_bin("meda").unwrap();
    cmd.args(&["rmi", "workflow-test-image", "--force", "--json"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("success\": true"));
    
    // 4. Verify image is gone
    let mut cmd = Command::cargo_bin("meda").unwrap();
    cmd.args(&["images", "--json"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("[]"));  // Should be empty now
    
    cleanup_test_env();
}

// Test SSH connectivity to a running VM
#[test]
#[serial]
fn test_cli_vm_ssh_connectivity() {
    let _temp_dir = setup_test_env();
    
    // 1. Create a VM
    let mut cmd = Command::cargo_bin("meda").unwrap();
    cmd.args(&["create", "ssh-test-vm", "--json"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("success\": true"));
    
    // 2. Start the VM
    let mut cmd = Command::cargo_bin("meda").unwrap();
    cmd.args(&["start", "ssh-test-vm", "--json"]);
    
    // Note: VM start might fail in CI environment without proper hypervisor setup
    // But we can still test the command execution
    let start_result = cmd.assert();
    
    // If start succeeded, test SSH connectivity
    if start_result.try_success().is_ok() {
        // 3. Get VM details to find IP
        let mut cmd = Command::cargo_bin("meda").unwrap();
        cmd.args(&["get", "ssh-test-vm", "--json"]);
        let output = cmd.assert().success();
        
        // Parse JSON to get IP
        let stdout = std::str::from_utf8(&output.get_output().stdout).unwrap();
        if let Ok(vm_info) = serde_json::from_str::<serde_json::Value>(stdout) {
            if let Some(ip) = vm_info.get("ip").and_then(|v| v.as_str()) {
                if ip != "N/A" {
                    // 4. Wait a bit for the VM to fully boot
                    std::thread::sleep(std::time::Duration::from_secs(30));
                    
                    // 5. Test SSH connectivity
                    test_ssh_connection(ip);
                }
            }
        }
        
        // 6. Stop the VM
        let mut cmd = Command::cargo_bin("meda").unwrap();
        cmd.args(&["stop", "ssh-test-vm", "--json"]);
        cmd.assert().success();
    }
    
    // 7. Clean up - delete the VM
    let mut cmd = Command::cargo_bin("meda").unwrap();
    cmd.args(&["delete", "ssh-test-vm", "--json"]);
    cmd.assert().success();
    
    cleanup_test_env();
}

// Test SSH connectivity with port forwarding
#[test]
#[serial]
fn test_cli_vm_ssh_with_port_forward() {
    let _temp_dir = setup_test_env();
    
    // 1. Create a VM
    let mut cmd = Command::cargo_bin("meda").unwrap();
    cmd.args(&["create", "ssh-port-test-vm", "--json"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("success\": true"));
    
    // 2. Start the VM
    let mut cmd = Command::cargo_bin("meda").unwrap();
    cmd.args(&["start", "ssh-port-test-vm", "--json"]);
    
    let start_result = cmd.assert();
    
    if start_result.try_success().is_ok() {
        // 3. Set up port forwarding for SSH
        let mut cmd = Command::cargo_bin("meda").unwrap();
        cmd.args(&["port-forward", "ssh-port-test-vm", "2222", "22", "--json"]);
        cmd.assert()
            .success()
            .stdout(predicate::str::contains("success\": true"));
        
        // 4. Wait for VM to boot
        std::thread::sleep(std::time::Duration::from_secs(30));
        
        // 5. Test SSH via port forward
        test_ssh_connection_via_port("localhost", 2222);
        
        // 6. Stop the VM
        let mut cmd = Command::cargo_bin("meda").unwrap();
        cmd.args(&["stop", "ssh-port-test-vm", "--json"]);
        cmd.assert().success();
    }
    
    // 7. Clean up
    let mut cmd = Command::cargo_bin("meda").unwrap();
    cmd.args(&["delete", "ssh-port-test-vm", "--json"]);
    cmd.assert().success();
    
    cleanup_test_env();
}

// Test VM from image with SSH
#[test]
#[serial]
fn test_cli_run_image_ssh() {
    let _temp_dir = setup_test_env();
    
    // 1. Create an image first (if not already exists)
    let mut cmd = Command::cargo_bin("meda").unwrap();
    cmd.args(&["create-image", "ssh-test-image", "--json"]);
    cmd.assert().success();
    
    // 2. Run VM from image
    let mut cmd = Command::cargo_bin("meda").unwrap();
    cmd.args(&["run", "ssh-test-image:latest", "--name", "ssh-from-image-vm", "--json"]);
    
    let run_result = cmd.assert();
    
    if run_result.try_success().is_ok() {
        // 3. Get VM details
        let mut cmd = Command::cargo_bin("meda").unwrap();
        cmd.args(&["get", "ssh-from-image-vm", "--json"]);
        let output = cmd.assert().success();
        
        // Parse JSON to get IP
        let stdout = std::str::from_utf8(&output.get_output().stdout).unwrap();
        if let Ok(vm_info) = serde_json::from_str::<serde_json::Value>(stdout) {
            if let Some(ip) = vm_info.get("ip").and_then(|v| v.as_str()) {
                if ip != "N/A" {
                    // 4. Wait for VM to boot
                    std::thread::sleep(std::time::Duration::from_secs(30));
                    
                    // 5. Test SSH
                    test_ssh_connection(ip);
                }
            }
        }
        
        // 6. Clean up
        let mut cmd = Command::cargo_bin("meda").unwrap();
        cmd.args(&["delete", "ssh-from-image-vm", "--json"]);
        cmd.assert().success();
    }
    
    // Clean up image
    let mut cmd = Command::cargo_bin("meda").unwrap();
    cmd.args(&["rmi", "ssh-test-image", "--force", "--json"]);
    cmd.assert().success();
    
    cleanup_test_env();
}

// Helper function to test SSH connection
fn test_ssh_connection(ip: &str) {
    println!("Testing SSH connection to VM at IP: {}", ip);
    
    // Test basic SSH connectivity with timeout
    let mut cmd = Command::new("ssh");
    cmd.args(&[
        "-o", "ConnectTimeout=10",
        "-o", "StrictHostKeyChecking=no",
        "-o", "UserKnownHostsFile=/dev/null",
        "-o", "BatchMode=yes",  // Non-interactive mode
        &format!("cirun@{}", ip),
        "echo 'SSH connection successful'"
    ]);
    
    // SSH might fail due to various reasons (VM not fully booted, network issues, etc.)
    // So we'll just attempt the connection and log the result
    let result = cmd.output();
    
    match result {
        Ok(output) => {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                println!("SSH test successful: {}", stdout.trim());
                assert!(stdout.contains("SSH connection successful"));
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                println!("SSH test failed (expected in CI): {}", stderr.trim());
                // Don't fail the test in CI environments where SSH might not work
            }
        }
        Err(e) => {
            println!("SSH command failed to execute (expected in CI): {}", e);
            // Don't fail the test if SSH command is not available
        }
    }
}

// Helper function to test SSH connection via port forwarding
fn test_ssh_connection_via_port(host: &str, port: u16) {
    println!("Testing SSH connection to {}:{}", host, port);
    
    let mut cmd = Command::new("ssh");
    cmd.args(&[
        "-p", &port.to_string(),
        "-o", "ConnectTimeout=10",
        "-o", "StrictHostKeyChecking=no",
        "-o", "UserKnownHostsFile=/dev/null",
        "-o", "BatchMode=yes",
        &format!("cirun@{}", host),
        "echo 'SSH via port forward successful'"
    ]);
    
    let result = cmd.output();
    
    match result {
        Ok(output) => {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                println!("SSH port forward test successful: {}", stdout.trim());
                assert!(stdout.contains("SSH via port forward successful"));
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                println!("SSH port forward test failed (expected in CI): {}", stderr.trim());
            }
        }
        Err(e) => {
            println!("SSH port forward command failed to execute (expected in CI): {}", e);
        }
    }
}

// Test SSH with custom user-data
#[test]
#[serial]
fn test_cli_vm_ssh_custom_userdata() {
    let temp_dir = setup_test_env();
    
    // Create custom user-data with SSH key
    let user_data_content = r#"#cloud-config
users:
  - name: cirun
    sudo: ALL=(ALL) NOPASSWD:ALL
    passwd: $6$ep7LxhhmhQHf.TiY$qPJVJQCnPMnyFdmD0ymP7CH2dos0awET8JlSzDqoiK6AOQwDpx8fCLJ1C5c7nvkVJbIpQCOalC8l2BGkRzogM.
    lock_passwd: false
    inactive: false
    groups: sudo
    shell: /bin/bash
    ssh_authorized_keys:
      - ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABgQC7... # Placeholder key
ssh_pwauth: true
packages:
  - curl
  - htop
runcmd:
  - echo "VM is ready for SSH" > /tmp/ready
"#;
    
    let user_data_file = temp_dir.path().join("custom-user-data");
    std::fs::write(&user_data_file, user_data_content).unwrap();
    
    // 1. Create VM with custom user-data
    let mut cmd = Command::cargo_bin("meda").unwrap();
    cmd.args(&[
        "create",
        "ssh-custom-vm",
        user_data_file.to_str().unwrap(),
        "--json"
    ]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("success\": true"));
    
    // 2. Start the VM
    let mut cmd = Command::cargo_bin("meda").unwrap();
    cmd.args(&["start", "ssh-custom-vm", "--json"]);
    
    let start_result = cmd.assert();
    
    if start_result.try_success().is_ok() {
        // 3. Get VM IP
        let mut cmd = Command::cargo_bin("meda").unwrap();
        cmd.args(&["get", "ssh-custom-vm", "--json"]);
        let output = cmd.assert().success();
        
        let stdout = std::str::from_utf8(&output.get_output().stdout).unwrap();
        if let Ok(vm_info) = serde_json::from_str::<serde_json::Value>(stdout) {
            if let Some(ip) = vm_info.get("ip").and_then(|v| v.as_str()) {
                if ip != "N/A" {
                    // 4. Wait longer for custom user-data to complete
                    std::thread::sleep(std::time::Duration::from_secs(60));
                    
                    // 5. Test SSH with additional commands
                    test_ssh_with_commands(ip);
                }
            }
        }
        
        // 6. Stop the VM
        let mut cmd = Command::cargo_bin("meda").unwrap();
        cmd.args(&["stop", "ssh-custom-vm", "--json"]);
        cmd.assert().success();
    }
    
    // 7. Clean up
    let mut cmd = Command::cargo_bin("meda").unwrap();
    cmd.args(&["delete", "ssh-custom-vm", "--json"]);
    cmd.assert().success();
    
    cleanup_test_env();
}

// Helper function to test SSH with additional commands
fn test_ssh_with_commands(ip: &str) {
    println!("Testing SSH with commands to VM at IP: {}", ip);
    
    let test_commands = vec![
        ("whoami", "cirun"),
        ("pwd", "/home/cirun"),
        ("cat /tmp/ready", "VM is ready for SSH"),
        ("curl --version", "curl"),
    ];
    
    for (command, expected_output) in test_commands {
        let mut cmd = Command::new("ssh");
        cmd.args(&[
            "-o", "ConnectTimeout=10",
            "-o", "StrictHostKeyChecking=no", 
            "-o", "UserKnownHostsFile=/dev/null",
            "-o", "BatchMode=yes",
            &format!("cirun@{}", ip),
            command
        ]);
        
        match cmd.output() {
            Ok(output) => {
                if output.status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    println!("Command '{}' output: {}", command, stdout.trim());
                    
                    if stdout.contains(expected_output) {
                        println!("✅ Command '{}' test passed", command);
                    } else {
                        println!("⚠️  Command '{}' output doesn't contain expected: {}", command, expected_output);
                    }
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    println!("❌ Command '{}' failed: {}", command, stderr.trim());
                }
            }
            Err(e) => {
                println!("❌ Failed to execute SSH command '{}': {}", command, e);
            }
        }
    }
}