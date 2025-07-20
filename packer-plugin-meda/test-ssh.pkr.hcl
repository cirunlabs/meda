packer {
  required_plugins {
    meda = {
      version = ">= 1.0.0"
      source = "github.com/cirunlabs/meda"
    }
  }
}

source "meda-vm" "test" {
  vm_name           = "test-ssh"
  base_image        = "ubuntu:latest"
  memory            = "1G"
  cpus              = 2
  disk_size         = "10G"
  output_image_name = "test-ssh"
  user_data_file    = "/home/ubuntu/meda/test-user-data.yaml"
  
  # SSH configuration
  ssh_username = "ubuntu"
  ssh_password = "ubuntu"
  ssh_timeout  = "10m"
  
  # Use cargo run for development
  meda_binary = "cargo"
}

build {
  sources = ["source.meda-vm.test"]
  
  # Simple test to verify SSH connectivity
  provisioner "shell" {
    inline = [
      "echo 'SSH connection successful!'",
      "whoami",
      "pwd",
      "uname -a"
    ]
  }
}