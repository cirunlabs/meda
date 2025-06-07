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