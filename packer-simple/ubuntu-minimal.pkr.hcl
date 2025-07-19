# Simple Packer template using shell provisioner to call Meda CLI
packer {
  required_version = ">= 1.10.0"
}

variable "vm_name" {
  type        = string
  default     = "packer-ubuntu-minimal"
  description = "Name for the VM"
}

variable "output_image_name" {
  type        = string
  default     = "ubuntu-minimal-packer"
  description = "Output image name"
}

variable "base_image" {
  type        = string
  default     = "ubuntu:latest"
  description = "Base image to use"
}

source "null" "meda" {
  communicator = "none"
}

build {
  name = "meda-ubuntu-minimal"
  sources = ["source.null.meda"]

  # Create VM using Meda
  provisioner "shell-local" {
    inline = [
      "echo 'Creating VM ${var.vm_name} from ${var.base_image}...'",
      "meda run ${var.base_image} --name ${var.vm_name} --memory 1G --cpus 2 --disk 10G"
    ]
  }

  # Wait for VM to be ready and get IP
  provisioner "shell-local" {
    inline = [
      "echo 'Waiting for VM to be ready...'",
      "timeout 300 bash -c 'until meda ip ${var.vm_name} >/dev/null 2>&1; do sleep 5; done'",
      "VM_IP=$(meda ip ${var.vm_name})",
      "echo \"VM is ready with IP: $VM_IP\""
    ]
  }

  # Provision the VM via SSH
  provisioner "shell-local" {
    inline = [
      "VM_IP=$(meda ip ${var.vm_name})",
      "echo 'Provisioning VM...'",
      "ssh-keyscan -H $VM_IP >> ~/.ssh/known_hosts",
      "scp -o StrictHostKeyChecking=no provision.sh ubuntu@$VM_IP:/tmp/",
      "ssh -o StrictHostKeyChecking=no ubuntu@$VM_IP 'chmod +x /tmp/provision.sh && /tmp/provision.sh'"
    ]
  }

  # Stop VM and create image
  provisioner "shell-local" {
    inline = [
      "echo 'Stopping VM and creating image...'",
      "meda stop ${var.vm_name}",
      "meda images create --name ${var.output_image_name} --tag latest --from-vm ${var.vm_name}",
      "echo 'Image ${var.output_image_name}:latest created successfully'"
    ]
  }

  # Cleanup VM
  provisioner "shell-local" {
    inline = [
      "echo 'Cleaning up VM...'",
      "meda delete ${var.vm_name}",
      "echo 'VM ${var.vm_name} deleted'"
    ]
  }

  # Show final image
  provisioner "shell-local" {
    inline = [
      "echo 'Build completed successfully!'",
      "echo 'Available images:'",
      "meda images list"
    ]
  }
}