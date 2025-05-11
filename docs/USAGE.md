# Meda - Cloud-Hypervisor VM Manager

Meda is a command-line tool for managing Cloud-Hypervisor virtual machines. This document provides a comprehensive guide to all available commands and their output formats.

## Installation

Ensure you have the necessary dependencies installed before using Meda.

## Global Options

The following options apply to all commands:

- `--json`: Output results in JSON format instead of human-readable text

## Commands

### Create a VM

Creates a new virtual machine with the specified name.

```bash
meda create <NAME> [USER_DATA] [--force]
```

**Arguments:**
- `<NAME>`: Name of the VM to create
- `[USER_DATA]`: Optional path to a user-data file for cloud-init
- `--force, -f`: Force creation by deleting any existing VM with the same name

**Output:**
- Standard output: Progress information and success/failure message
- JSON output: 
  ```json
  {
    "success": true|false,
    "message": "Success/error message"
  }
  ```

### List VMs

Lists all available virtual machines.

```bash
meda list
```

**Output:**
- Standard output: Table with columns for NAME, STATE, IP, and PORTS
- JSON output:
  ```json
  [
    {
      "name": "vm-name",
      "state": "running|stopped",
      "ip": "192.168.x.y",
      "ports": "host:guest,host:guest,..."
    },
    ...
  ]
  ```

### Get VM Details

Retrieves detailed information about a specific VM.

```bash
meda get <NAME>
```

**Arguments:**
- `<NAME>`: Name of the VM to get details for

**Output:**
- Standard output: Detailed information about the VM
- JSON output:
  ```json
  {
    "name": "vm-name",
    "state": "running|stopped",
    "ip": "192.168.x.y",
    "details": { ... }
  }
  ```

### Start a VM

Starts a virtual machine.

```bash
meda start <NAME>
```

**Arguments:**
- `<NAME>`: Name of the VM to start

**Output:**
- Standard output: Progress information and success/failure message
- JSON output:
  ```json
  {
    "success": true|false,
    "message": "Success/error message"
  }
  ```

### Stop a VM

Stops a running virtual machine.

```bash
meda stop <NAME>
```

**Arguments:**
- `<NAME>`: Name of the VM to stop

**Output:**
- Standard output: Progress information and success/failure message
- JSON output:
  ```json
  {
    "success": true|false,
    "message": "Success/error message"
  }
  ```

### Delete a VM

Deletes a virtual machine.

```bash
meda delete <NAME>
```

**Arguments:**
- `<NAME>`: Name of the VM to delete

**Output:**
- Standard output: Progress information and success/failure message
- JSON output:
  ```json
  {
    "success": true|false,
    "message": "Success/error message"
  }
  ```

### Port Forwarding

Sets up port forwarding from a host port to a guest port.

```bash
meda port-forward <NAME> <HOST_PORT> <GUEST_PORT>
```

**Arguments:**
- `<NAME>`: Name of the VM to set up port forwarding for
- `<HOST_PORT>`: Port number on the host
- `<GUEST_PORT>`: Port number on the guest VM

**Output:**
- Standard output: Success/failure message
- JSON output:
  ```json
  {
    "success": true|false,
    "message": "Success/error message"
  }
  ```

## Examples

### Creating and Starting a VM

```bash
# Create a new VM named "ubuntu-vm"
meda create ubuntu-vm

# Start the VM
meda start ubuntu-vm

# Check VM status
meda get ubuntu-vm
```

### Setting Up Port Forwarding

```bash
# Forward host port 8080 to guest port 80
meda port-forward ubuntu-vm 8080 80
```

### Cleaning Up

```bash
# Stop the VM
meda stop ubuntu-vm

# Delete the VM
meda delete ubuntu-vm
```
