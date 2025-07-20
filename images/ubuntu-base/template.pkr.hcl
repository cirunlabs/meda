variable "image_tag" {
  type        = string
  default     = "latest"
  description = "Tag for the output image"
}

variable "registry" {
  type        = string
  default     = "ghcr.io"
  description = "Container registry to push to"
}

variable "organization" {
  type        = string
  default     = env("GITHUB_REPOSITORY_OWNER") != "" ? env("GITHUB_REPOSITORY_OWNER") : "cirunlabs"
  description = "Registry organization/namespace"
}

variable "push_enabled" {
  type        = bool
  default     = true
  description = "Whether to push the image to registry"
}

variable "dry_run" {
  type        = bool
  default     = false
  description = "Dry run mode"
}

source "meda-vm" "ubuntu-base" {
  # VM configuration
  vm_name           = "ubuntu-base-build"
  base_image        = "ubuntu:latest"
  memory            = "2G"
  cpus              = 4
  disk_size         = "20G"

  # Output configuration
  output_image_name = "ubuntu-base"
  output_tag        = var.image_tag
  registry          = var.registry
  organization      = var.organization

  # Push configuration
  push_to_registry  = var.push_enabled
  dry_run           = var.dry_run

  # Use meda binary
  meda_binary = "meda"

  # SSH configuration
  ssh_username = "cirun"
  ssh_password = "cirun"
  ssh_timeout  = "10m"
  ssh_port     = 22
}

build {
  name = "ubuntu-base"
  sources = ["source.meda-vm.ubuntu-base"]

  # Wait for cloud-init to complete
  provisioner "shell" {
    inline = [
      "echo 'Waiting for cloud-init to complete...'",
      "cloud-init status --wait",
      "echo 'Cloud-init completed'"
    ]
  }

  # System updates
  provisioner "shell" {
    inline = [
      "echo 'Updating system packages...'",
      "sudo apt-get update",
      "sudo apt-get upgrade -y"
    ]
  }

  # Install Docker
  provisioner "shell" {
    inline = [
      "echo 'Installing Docker...'",
      "sudo apt-get update",
      "sudo apt-get install -y ca-certificates curl gnupg",
      "sudo install -m 0755 -d /etc/apt/keyrings",
      "curl -fsSL https://download.docker.com/linux/ubuntu/gpg | sudo gpg --dearmor -o /etc/apt/keyrings/docker.gpg",
      "sudo chmod a+r /etc/apt/keyrings/docker.gpg",
      "echo \"deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.gpg] https://download.docker.com/linux/ubuntu $(. /etc/os-release && echo $VERSION_CODENAME) stable\" | sudo tee /etc/apt/sources.list.d/docker.list > /dev/null",
      "sudo apt-get update",
      "sudo apt-get install -y docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin"
    ]
  }

  # Configure Docker
  provisioner "shell" {
    inline = [
      "echo 'Configuring Docker...'",
      "sudo systemctl enable docker",
      "sudo systemctl enable containerd",
      "sudo usermod -aG docker cirun",
      "sudo mkdir -p /etc/docker",
      "echo '{\"log-driver\": \"json-file\", \"log-opts\": {\"max-size\": \"10m\", \"max-file\": \"3\"}}' | sudo tee /etc/docker/daemon.json"
    ]
  }

  # Cleanup and preparation for image creation
  provisioner "shell" {
    inline = [
      "echo 'Cleaning up for image creation...'",
      "sudo apt-get autoremove -y",
      "sudo apt-get autoclean",
      "sudo apt-get clean",
      "sudo rm -rf /var/lib/apt/lists/*",
      "sudo rm -rf /tmp/* /var/tmp/*",
      "sudo rm -rf /var/cache/apt/archives/*",
      "sudo rm -rf /usr/share/doc/*",
      "sudo rm -rf /usr/share/man/*",
      "sudo rm -rf /var/cache/debconf/*",
      "sudo rm -rf /home/cirun/.cache/*",
      "sudo rm -rf /root/.cache/*",
      "sudo find /var/log -type f -exec truncate -s 0 {} \\;",
      "sudo rm -rf /var/log/journal/*",
      "echo 'Image preparation completed'"
    ]
  }

  # Final validation
  provisioner "shell" {
    inline = [
      "echo 'Validating installation...'",
      "docker --version",
      "docker compose version",
      "echo 'Docker installed successfully'"
    ]
  }

  post-processor "manifest" {
    output = "manifest.json"
    strip_path = true
    custom_data = {
      image_name = "ubuntu-base"
      image_tag  = var.image_tag
      build_time = timestamp()
      vm_name    = "{{ .MedaVMName }}"
      vm_ip      = "{{ .MedaVMIP }}"
    }
  }
}