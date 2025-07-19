packer {
  required_plugins {
    meda = {
      version = ">= 1.0.0"
      source = "github.com/cirunlabs/meda"
    }
  }
}

# Variables
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

# Data sources
data "http" "cloud_init" {
  url = "https://raw.githubusercontent.com/canonical/cloud-init/main/doc/examples/cloud-config.txt"
}

# Sources
source "meda-vm" "ubuntu-docker" {
  vm_name           = "ubuntu-docker-build"
  base_image        = "ubuntu:latest"
  memory            = "2G"
  cpus              = 4
  disk_size         = "20G"

  output_image_name = "ubuntu-docker"
  output_tag        = var.image_tag
  registry          = var.registry
  organization      = var.organization

  # Use Meda API if available
  use_api    = true
  meda_host  = "127.0.0.1"
  meda_port  = 7777

  # SSH configuration
  ssh_username = "ubuntu"
  ssh_timeout  = "10m"
  ssh_port     = 22
}

# Build
build {
  name = "ubuntu-docker"
  sources = ["source.meda-vm.ubuntu-docker"]

  # Wait for cloud-init to complete
  provisioner "shell" {
    pause_before = "30s"
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
      "sudo apt-get upgrade -y",
      "sudo apt-get autoremove -y",
      "sudo apt-get autoclean"
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
      "sudo usermod -aG docker ubuntu",
      "sudo mkdir -p /etc/docker",
      "echo '{\"log-driver\": \"json-file\", \"log-opts\": {\"max-size\": \"10m\", \"max-file\": \"3\"}}' | sudo tee /etc/docker/daemon.json"
    ]
  }

  # Install additional tools
  provisioner "shell" {
    inline = [
      "echo 'Installing additional tools...'",
      "sudo apt-get install -y curl wget vim htop tree jq git build-essential",
      "curl -fsSL https://get.k8s.io | bash",  # kubectl
      "curl https://raw.githubusercontent.com/helm/helm/main/scripts/get-helm-3 | bash"  # helm
    ]
  }

  # Configure user environment
  provisioner "shell" {
    inline = [
      "echo 'Configuring user environment...'",
      "echo 'export PATH=$PATH:/usr/local/bin' >> ~/.bashrc",
      "echo 'alias ll=\"ls -la\"' >> ~/.bashrc",
      "echo 'alias la=\"ls -A\"' >> ~/.bashrc",
      "echo 'alias l=\"ls -CF\"' >> ~/.bashrc"
    ]
  }

  # Cleanup and preparation for image creation
  provisioner "shell" {
    inline = [
      "echo 'Cleaning up for image creation...'",
      "sudo apt-get autoremove -y",
      "sudo apt-get autoclean",
      "sudo rm -rf /var/lib/apt/lists/*",
      "sudo rm -rf /tmp/*",
      "sudo rm -rf /var/tmp/*",
      "history -c",
      "sudo find /var/log -type f -exec truncate -s 0 {} \\;",
      "echo 'Image preparation completed'"
    ]
  }

  # Final validation
  provisioner "shell" {
    inline = [
      "echo 'Validating installation...'",
      "docker --version",
      "docker compose version",
      "kubectl version --client",
      "helm version --client",
      "echo 'All tools installed successfully'"
    ]
  }

  post-processor "manifest" {
    output = "manifest.json"
    strip_path = true
    custom_data = {
      image_name = "ubuntu-docker"
      image_tag  = var.image_tag
      build_time = timestamp()
      vm_name    = "{{ .MedaVMName }}"
      vm_ip      = "{{ .MedaVMIP }}"
    }
  }
}