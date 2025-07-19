# Packer Integration with Meda

This document describes how to use the Packer plugin for Meda to automate VM image building and deployment.

## Overview

The Meda Packer plugin enables automated image building workflows that integrate with:
- Meda CLI and REST API
- GitHub Actions for CI/CD
- Container registries (GHCR, Docker Hub, etc.)
- Infrastructure as Code practices

## Architecture

```
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│   Packer        │    │      Meda      │    │  Image Registry │
│   Template      │───▶│   VM Manager   │───▶│     (GHCR)     │
│                 │    │                │    │                │
└─────────────────┘    └─────────────────┘    └─────────────────┘
         │                       │                       │
         │                       │                       │
         ▼                       ▼                       ▼
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│ GitHub Actions  │    │ Cloud-Hypervisor│    │   Deployment   │
│    Workflow     │    │      VMs       │    │   Targets      │
└─────────────────┘    └─────────────────┘    └─────────────────┘
```

## Quick Start

### 1. Install the Plugin

```bash
# Install Packer
curl -fsSL https://releases.hashicorp.com/packer/1.10.0/packer_1.10.0_linux_amd64.zip -o packer.zip
unzip packer.zip && sudo mv packer /usr/local/bin/

# Install Meda Packer plugin
packer plugins install github.com/cirunlabs/meda
```

### 2. Create a Packer Template

```hcl
# minimal-example.pkr.hcl
packer {
  required_plugins {
    meda = {
      version = ">= 1.0.0"
      source = "github.com/cirunlabs/meda"
    }
  }
}

source "meda-vm" "ubuntu" {
  vm_name           = "my-image"
  base_image        = "ubuntu:latest"
  memory            = "2G"
  cpus              = 4
  output_image_name = "my-custom-image"
  
  ssh_username = "ubuntu"
}

build {
  sources = ["source.meda-vm.ubuntu"]
  
  provisioner "shell" {
    inline = [
      "sudo apt-get update",
      "sudo apt-get install -y nginx"
    ]
  }
}
```

### 3. Build the Image

```bash
# Validate template
packer validate minimal-example.pkr.hcl

# Build image
packer build minimal-example.pkr.hcl
```

## Configuration Options

### Source Configuration

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `vm_name` | string | Yes | - | Name for the VM instance |
| `base_image` | string | Yes | - | Base image to use |
| `output_image_name` | string | Yes | - | Name for output image |
| `memory` | string | No | "1G" | VM memory allocation |
| `cpus` | int | No | 2 | Number of CPUs |
| `disk_size` | string | No | "10G" | Disk size |
| `use_api` | bool | No | false | Use Meda REST API |
| `meda_host` | string | No | "127.0.0.1" | Meda API host |
| `meda_port` | int | No | 7777 | Meda API port |

### Output Configuration

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `output_tag` | string | No | "latest" | Image tag |
| `registry` | string | No | "ghcr.io" | Container registry |
| `organization` | string | No | - | Registry organization |

## Advanced Usage

### Multi-Stage Builds

```hcl
# Build base image
source "meda-vm" "base" {
  vm_name           = "base-build"
  base_image        = "ubuntu:latest"
  output_image_name = "my-base"
  memory            = "1G"
  cpus              = 2
}

# Build application image from base
source "meda-vm" "app" {
  vm_name           = "app-build"
  base_image        = "my-base:latest"
  output_image_name = "my-app"
  memory            = "2G"
  cpus              = 4
}

build {
  sources = ["source.meda-vm.base"]
  provisioner "shell" {
    inline = ["sudo apt-get update && sudo apt-get install -y build-essential"]
  }
}

build {
  sources = ["source.meda-vm.app"]
  provisioner "file" {
    source = "app/"
    destination = "/tmp/"
  }
  provisioner "shell" {
    inline = ["cd /tmp && make install"]
  }
}
```

### Using Variables and Locals

```hcl
variable "environment" {
  type = string
  default = "dev"
}

variable "version" {
  type = string
  default = "latest"
}

locals {
  image_name = "myapp-${var.environment}"
  full_tag = "${var.version}-${var.environment}"
}

source "meda-vm" "app" {
  vm_name           = local.image_name
  base_image        = "ubuntu:latest"
  output_image_name = local.image_name
  output_tag        = local.full_tag
}
```

### Custom User Data

```hcl
source "meda-vm" "custom" {
  vm_name           = "custom-vm"
  base_image        = "ubuntu:latest"
  user_data_file    = "cloud-init.yaml"
  output_image_name = "custom-image"
}
```

```yaml
# cloud-init.yaml
#cloud-config
users:
  - name: deploy
    sudo: ALL=(ALL) NOPASSWD:ALL
    groups: users, admin
    home: /home/deploy
    shell: /bin/bash
    ssh_authorized_keys:
      - ssh-rsa AAAAB3Nza...

packages:
  - curl
  - wget
  - git

runcmd:
  - systemctl enable ssh
```

## GitHub Actions Integration

### Automated Image Building

The provided GitHub Actions workflow automatically:
1. Detects changes in image directories
2. Builds changed images with Packer
3. Pushes images to GHCR
4. Provides build summaries

### Triggering Builds

```bash
# Build specific image
gh workflow run build-images.yml -f image_name=ubuntu-docker

# Force rebuild all images
gh workflow run build-images.yml -f force_rebuild=true
```

### Custom Workflow

```yaml
name: Custom Image Build

on:
  push:
    paths: ['my-images/**']

jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      
      - name: Setup Meda and Packer
        run: |
          # Install dependencies
          cargo build --release
          sudo cp target/release/meda /usr/local/bin/
          
          # Install Packer
          curl -fsSL https://releases.hashicorp.com/packer/1.10.0/packer_1.10.0_linux_amd64.zip -o packer.zip
          unzip packer.zip && sudo mv packer /usr/local/bin/
          
          # Install plugin
          packer plugins install github.com/cirunlabs/meda
      
      - name: Build images
        run: |
          cd my-images/
          for dir in */; do
            cd "$dir"
            packer build .
            cd ..
          done
```

## Best Practices

### Image Organization

```
images/
├── base/
│   ├── ubuntu-minimal/
│   ├── ubuntu-docker/
│   └── alpine-base/
├── applications/
│   ├── web-server/
│   ├── database/
│   └── api-service/
└── environments/
    ├── development/
    ├── staging/
    └── production/
```

### Security

1. **Use least privilege**: Configure VM users with minimal required permissions
2. **Scan images**: Integrate security scanning in CI/CD pipelines
3. **Sign images**: Use image signing for production deployments
4. **Secrets management**: Use secure methods for handling secrets in builds

### Performance

1. **Layer caching**: Use base images to reduce build times
2. **Parallel builds**: Build independent images in parallel
3. **Resource allocation**: Right-size VM resources for build workloads
4. **Cleanup**: Ensure proper cleanup of temporary resources

### Monitoring

1. **Build metrics**: Track build times and success rates
2. **Resource usage**: Monitor VM resource consumption
3. **Image sizes**: Track image size growth over time
4. **Registry usage**: Monitor registry storage and bandwidth

## Troubleshooting

### Common Issues

1. **Plugin not found**
   ```bash
   packer plugins install github.com/cirunlabs/meda
   ```

2. **Meda API connection failed**
   ```bash
   meda serve --host 127.0.0.1 --port 7777
   ```

3. **VM creation timeout**
   ```hcl
   source "meda-vm" "ubuntu" {
     # Increase timeout
     ssh_timeout = "10m"
   }
   ```

4. **Insufficient resources**
   ```hcl
   source "meda-vm" "ubuntu" {
     memory = "4G"    # Increase memory
     cpus = 8         # Increase CPUs
   }
   ```

### Debug Mode

```bash
# Enable detailed logging
PACKER_LOG=1 packer build template.pkr.hcl

# Check Meda logs
meda logs

# Verify VM status
meda list
meda get vm-name
```

### Getting Help

- [Packer Plugin Repository](https://github.com/cirunlabs/packer-plugin-meda)
- [Meda Documentation](https://github.com/cirunlabs/meda)
- [GitHub Issues](https://github.com/cirunlabs/meda/issues)
- [Community Discussions](https://github.com/cirunlabs/meda/discussions)

## Examples

See the [`images/`](../images/) directory for complete examples:
- [`ubuntu-minimal/`](../images/ubuntu-minimal/) - Basic Ubuntu image
- [`ubuntu-docker/`](../images/ubuntu-docker/) - Ubuntu with Docker and container tools

## Contributing

1. Fork the repository
2. Create feature branch
3. Add tests for new functionality
4. Submit pull request with examples

## Roadmap

- [ ] Support for Windows VMs
- [ ] Integration with Terraform
- [ ] Multi-architecture builds
- [ ] Image vulnerability scanning
- [ ] Build caching improvements