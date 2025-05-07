# Meda - Cloud-Hypervisor VM Manager

A Rust-based CLI tool for managing Cloud-Hypervisor micro-VMs.

## Features

- Create, list, start, stop, and delete VMs
- Port forwarding
- Network management
- Cloud-init integration

## Installation

### From Releases

Download the latest release from the [Releases page](https://github.com/yourusername/ch-vm/releases).

### From Source

```bash
cargo install --path .
```

## Usage

```bash
# Create a new VM
meda create my-vm

# List all VMs
meda list

# Get VM details
meda get my-vm

# Start a VM
meda start my-vm

# Stop a VM
meda stop my-vm

# Delete a VM
meda delete my-vm

# Forward host port to guest port
meda port-forward my-vm 8080 80
```

## Environment Variables

- `CH_CPUS`: Number of virtual CPUs (default: 2)
- `CH_MEM`: Memory size (default: 1024M)
- `CH_DISK_SIZE`: Disk size (default: 10G)
- `CH_ASSET_DIR`: Assets directory (default: ~/.ch-vms/assets)
- `CH_VM_DIR`: VM directory (default: ~/.ch-vms/vms)

## Requirements

- Linux with iptables
- qemu-utils (for qemu-img)
- genisoimage
- jq

## References

- [Cloud-Hypervisor Networking](https://github.com/cloud-hypervisor/cloud-hypervisor/blob/0dafd47a7ccc64100ecd73a7d31b8540d253c649/docs/networking.md)
- [Cloud-Hypervisor MacVTap Bridge](https://github.com/cloud-hypervisor/cloud-hypervisor/blob/3081d01fc37a05af84ff44aeaebcbb5c96f31da8/docs/macvtap-bridge.md)
