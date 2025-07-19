packer {
  required_plugins {
    meda = {
      version = ">= 1.0.0"
      source = "github.com/cirunlabs/meda"
    }
  }
}

source "meda-vm" "ubuntu-test" {
  vm_name           = "packer-test-build"
  base_image        = "ubuntu:latest"
  memory            = "1G"
  cpus              = 2
  disk_size         = "10G"
  output_image_name = "ubuntu-test-complete"
  output_tag        = "demo"
  
  # Don't use communicator for this demo
  communicator = "none"
}

build {
  sources = ["source.meda-vm.ubuntu-test"]
  
  # No provisioning - just create and image
}