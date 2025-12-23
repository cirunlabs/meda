<div align="center">
  <img src="meda.png" alt="Meda" width="300"/>

  <img src="meda-command.png" alt="Meda"/>


  **Cloud-Hypervisor VM management**

  [![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg)](https://www.rust-lang.org)
  [![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

  > ⚠️ **Alpha Software**: Meda is currently in alpha. Features and APIs may change.

</div>

---

## What is Meda?

Meda is a wrapper around Cloud-Hypervisor that provides CLI and REST API management for micro-VMs with support for OCI images.

**Features:**
- VM lifecycle management (create, start, stop, delete)
- OCI image support (pull, push, run from container registries)
- REST API with Swagger documentation
- Packer integration for automated builds
- Network management and VM connectivity

## Quick Start

### 🚀 One-Command Install

```bash
curl -fsSL https://raw.githubusercontent.com/cirunlabs/meda/main/scripts/install-release.sh | bash
```

Or with wget:
```bash
wget -qO- https://raw.githubusercontent.com/cirunlabs/meda/main/scripts/install-release.sh | bash
```

### 🔧 Alternative Installations

```bash
# Install specific version
curl -fsSL https://raw.githubusercontent.com/cirunlabs/meda/main/scripts/install-release.sh | bash -s -- --version v0.2.0

# Install to custom directory
curl -fsSL https://raw.githubusercontent.com/cirunlabs/meda/main/scripts/install-release.sh | bash -s -- --install-dir /usr/local/bin

# Build from source
git clone https://github.com/cirunlabs/meda.git && cd meda && cargo install --path .
```

### 🏁 Create Your First VM

```bash
# Create and start your first VM
meda create my-vm --memory 2G --cpus 4
meda start my-vm

# Or run directly from an image
meda run ubuntu:latest --name web-server --memory 1G
```

## Core Features

### 🖥️ VM Lifecycle Management
Complete control over your micro-VMs with intuitive commands:

```bash
# Create VMs with custom resources
meda create web-server --memory 4G --cpus 8 --disk 50G

# List all VMs with status
meda list

# Get detailed VM information
meda get web-server

# VM control
meda start web-server
meda stop web-server
meda delete web-server
```

### 🌐 Network Management
Get VM connectivity information:

```bash
# Get VM IP address
meda ip web-server
```

### 📦 Container-Style Image Management
Work with VM images like container images:

```bash
# Pull images from registries
meda pull ubuntu:latest
meda pull ghcr.io/cirunlabs/ubuntu:22.04

# Run VM from image
meda run ubuntu:latest --name my-ubuntu

# Create custom images from VMs
meda create-image my-custom-image --from-vm configured-vm

# Push images to registries
meda push my-custom-image ghcr.io/myorg/my-image:v1.0

# Clean up unused images
meda prune-images
```

### 🔌 REST API Server
Full-featured HTTP API with Swagger documentation:

```bash
# Start API server (localhost only)
meda serve --port 7777

# Start on all interfaces (accessible from VM's external IP)
meda serve --port 7777 --host 0.0.0.0
```

Access Swagger UI at: `http://your-host:7777/swagger-ui`

#### API Examples

```bash
# Create VM via API
curl -X POST http://localhost:7777/api/v1/vms \
  -H "Content-Type: application/json" \
  -d '{"name": "api-vm", "memory": "2G", "cpus": 4}'

# Pull and run image
curl -X POST http://localhost:7777/api/v1/images/run \
  -H "Content-Type: application/json" \
  -d '{"image": "ubuntu:latest", "name": "api-ubuntu", "memory": "1G"}'

# Get VM IP address
curl http://localhost:7777/api/v1/vms/api-vm/ip
```

### 🏗️ Packer Integration
Automate image building with HashiCorp Packer:

```hcl
# example.pkr.hcl
packer {
  required_plugins {
    meda = {
      version = ">= 1.0.0"
      source = "github.com/cirunlabs/meda"
    }
  }
}

source "meda-vm" "web-server" {
  vm_name           = "nginx-base"
  base_image        = "ubuntu:latest"
  memory            = "2G"
  cpus              = 4
  output_image_name = "nginx-server"
  ssh_username      = "ubuntu"
}

build {
  sources = ["source.meda-vm.web-server"]

  provisioner "shell" {
    inline = [
      "sudo apt-get update",
      "sudo apt-get install -y nginx",
      "sudo systemctl enable nginx"
    ]
  }
}
```

## Installation

### From Source
```bash
git clone https://github.com/cirunlabs/meda.git
cd meda
cargo install --path .
```

### System Requirements
- Linux with KVM support
- iptables
- qemu-utils (`sudo apt install qemu-utils`)
- genisoimage (`sudo apt install genisoimage`)

## Configuration

Customize default VM settings with environment variables:

```bash
export MEDA_CPUS=4              # Default CPU count
export MEDA_MEM=2G              # Default memory
export MEDA_DISK_SIZE=20G       # Default disk size
export MEDA_ASSET_DIR=~/meda    # Asset storage location
export MEDA_VM_DIR=~/meda/vms   # VM storage location
```

## Architecture

Meda is built with modern Rust practices:

- **Async/Await**: Full async runtime with Tokio
- **REST API**: Axum framework with OpenAPI/Swagger docs
- **Error Handling**: Comprehensive error types with `anyhow` and `thiserror`
- **Cloud-Init**: Automated guest configuration
- **Modular Design**: Clean separation between CLI, API, and core VM operations

## Use Cases

### Development Environment
```bash
# Spin up isolated development environments
meda run ubuntu:latest --name dev-env --memory 4G
meda ip dev-env  # Get VM IP for SSH access
```

### Microservices Testing
```bash
# Create multiple service instances
meda run redis:latest --name redis-test
meda run postgres:latest --name db-test
meda run nginx:latest --name web-test
```

### CI/CD Pipelines
```bash
# Build custom images with Packer
packer build web-server.pkr.hcl

# Deploy via API
curl -X POST http://ci-server:7777/api/v1/images/run \
  -d '{"image": "my-app:latest", "name": "production-app"}'
```

## Contributing

We welcome contributions! Run quality checks before submitting:

```bash
# Quick checks (recommended for frequent use)
./scripts/quick-check.sh

# Full quality checks (recommended before pushing)
./scripts/check-quality.sh

# Full quality checks including integration tests
./scripts/check-quality.sh --with-integration
```

## CI/CD Integration

Meda can be easily integrated into CI/CD pipelines. See our [demo GitHub Actions workflow](.github/workflows/demo-meda-usage.yml) for a complete example that:

- Installs meda from the latest release
- Sets up the required system dependencies
- Creates and manages Ubuntu VMs
- Demonstrates the full VM lifecycle

For more details, see [docs/demo-workflow.md](docs/demo-workflow.md).

## License

MIT License - see [LICENSE](LICENSE) for details.