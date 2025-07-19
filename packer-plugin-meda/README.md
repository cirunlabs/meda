# Packer Plugin for Meda

This plugin enables Packer to build VM images using Meda, a Cloud-Hypervisor micro-VM manager.

## Features

- Create VMs with custom resources (memory, CPUs, disk size)
- Support for cloud-init user-data customization
- Integration with Meda CLI or REST API
- Automatic VM lifecycle management (create, start, provision, stop, cleanup)
- Built-in SSH communicator support
- Image creation and management

## Installation

### Pre-built Binaries

Download the latest release from the [releases page](https://github.com/cirunlabs/packer-plugin-meda/releases) and install it using the `packer plugins install` command.

### Building from Source

```bash
git clone https://github.com/cirunlabs/packer-plugin-meda.git
cd packer-plugin-meda
go build -o packer-plugin-meda
packer plugins install --path packer-plugin-meda github.com/cirunlabs/meda
```

## Configuration

### Basic Configuration

```hcl
packer {
  required_plugins {
    meda = {
      version = ">= 1.0.0"
      source = "github.com/cirunlabs/meda"
    }
  }
}

source "meda-vm" "ubuntu" {
  vm_name           = "my-vm"
  base_image        = "ubuntu:latest"
  memory            = "2G"
  cpus              = 4
  disk_size         = "20G"
  output_image_name = "my-custom-image"
  output_tag        = "v1.0"

  ssh_username = "ubuntu"
  ssh_timeout  = "5m"
}

build {
  sources = ["source.meda-vm.ubuntu"]

  provisioner "shell" {
    inline = [
      "sudo apt-get update",
      "sudo apt-get install -y docker.io",
      "sudo systemctl enable docker"
    ]
  }
}
```

### Using Meda REST API

```hcl
source "meda-vm" "ubuntu" {
  use_api     = true
  meda_host   = "localhost"
  meda_port   = 7777

  vm_name           = "api-vm"
  base_image        = "ubuntu:latest"
  output_image_name = "api-built-image"
}
```

### Advanced Configuration

```hcl
source "meda-vm" "ubuntu" {
  vm_name           = "advanced-vm"
  base_image        = "ubuntu:latest"
  memory            = "4G"
  cpus              = 8
  disk_size         = "50G"
  user_data_file    = "cloud-init.yaml"
  output_image_name = "advanced-image"
  output_tag        = "latest"
  registry          = "ghcr.io"
  organization      = "myorg"

  ssh_username     = "ubuntu"
  ssh_timeout     = "10m"
  ssh_port        = 22
}
```

## Configuration Reference

### Required Parameters

- `vm_name` (string) - Name for the VM instance
- `base_image` (string) - Base image to use (e.g., "ubuntu:latest")
- `output_image_name` (string) - Name for the output image

### Optional Parameters

#### Meda Configuration
- `meda_binary` (string) - Path to meda binary (default: "meda")
- `use_api` (bool) - Use REST API instead of CLI (default: false)
- `meda_host` (string) - Meda API host (default: "127.0.0.1")
- `meda_port` (int) - Meda API port (default: 7777)

#### VM Resources
- `memory` (string) - VM memory (default: "1G")
- `cpus` (int) - Number of CPUs (default: 2)
- `disk_size` (string) - Disk size (default: "10G")
- `user_data_file` (string) - Cloud-init user-data file path

#### Image Output
- `output_tag` (string) - Image tag (default: "latest")
- `registry` (string) - Container registry (default: "ghcr.io")
- `organization` (string) - Registry organization

#### SSH Communication
- `ssh_username` (string) - SSH username (default: "ubuntu")
- `ssh_port` (int) - SSH port (default: 22)
- `ssh_timeout` (duration) - SSH timeout (default: "5m")

## Generated Variables

The plugin provides these variables for use in provisioners:

- `{{ .MedaVMName }}` - The generated VM name
- `{{ .MedaVMIP }}` - The VM's IP address

## Examples

See the [examples](examples/) directory for complete Packer templates.

## Troubleshooting

### Common Issues

1. **Meda binary not found**: Ensure meda is installed and in your PATH, or specify the full path using `meda_binary`.

2. **VM creation timeout**: Increase SSH timeout or check Meda logs for VM startup issues.

3. **API connection failed**: Ensure Meda server is running with `meda serve` before using `use_api = true`.

### Debug Mode

Run Packer with debug logging to see detailed plugin output:

```bash
PACKER_LOG=1 packer build template.pkr.hcl
```

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Add tests
5. Submit a pull request

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.