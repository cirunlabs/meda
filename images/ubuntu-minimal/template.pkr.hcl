packer {
  required_plugins {
    meda = {
      version = ">= 1.0.0"
      source = "github.com/cirunlabs/meda"
    }
  }
}

variable "image_tag" {
  type        = string
  default     = "latest"
  description = "Tag for the output image"
}

variable "organization" {
  type        = string
  default     = env("GITHUB_REPOSITORY_OWNER") != "" ? env("GITHUB_REPOSITORY_OWNER") : "cirunlabs"
  description = "Registry organization/namespace"
}

source "meda-vm" "ubuntu-minimal" {
  vm_name           = "ubuntu-minimal-build"
  base_image        = "ubuntu:latest"
  memory            = "1G"
  cpus              = 2
  disk_size         = "10G"

  output_image_name = "ubuntu-minimal"
  output_tag        = var.image_tag

  ssh_username = "ubuntu"
  ssh_timeout  = "5m"
}

build {
  name = "ubuntu-minimal"
  sources = ["source.meda-vm.ubuntu-minimal"]

  provisioner "shell" {
    pause_before = "30s"
    inline = [
      "cloud-init status --wait",
      "sudo apt-get update",
      "sudo apt-get install -y curl wget vim htop",
      "sudo apt-get autoremove -y",
      "sudo apt-get autoclean"
    ]
  }

  post-processor "manifest" {
    output = "manifest.json"
    strip_path = true
  }
}