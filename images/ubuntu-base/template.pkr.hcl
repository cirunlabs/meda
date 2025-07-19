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
  memory            = "1G"
  cpus              = 2
  disk_size         = "10G"

  # Output configuration
  output_image_name = "ubuntu-base"
  output_tag        = var.image_tag
  registry          = var.registry
  organization      = var.organization

  # Push configuration
  push_to_registry  = var.push_enabled
  dry_run           = var.dry_run

  # Use cargo run for development
  meda_binary = "cargo"
}

build {
  name = "ubuntu-base"
  sources = ["source.meda-vm.ubuntu-base"]

  # No provisioning - just create base image
}