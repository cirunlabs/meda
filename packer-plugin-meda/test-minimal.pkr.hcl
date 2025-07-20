source "meda-vm" "test" {
  vm_name           = "test-minimal"
  base_image        = "ubuntu:latest"
  memory            = "1G"
  cpus              = 2
  output_image_name = "test-minimal"
  
  # SSH configuration
  ssh_username = "cirun"
  ssh_password = "cirun"
  ssh_timeout  = "2m"
  
  # Use meda binary
  meda_binary = "meda"
}

build {
  sources = ["source.meda-vm.test"]
  
  provisioner "shell" {
    inline = ["echo 'SSH works!'"]
  }
}