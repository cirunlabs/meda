# Ubuntu Docker Image

This Packer template builds an Ubuntu 22.04 LTS image with Docker and container orchestration tools pre-installed.

## Included Software

- **Docker**: Latest Docker CE with Docker Compose plugin
- **kubectl**: Kubernetes command-line tool
- **Helm**: Kubernetes package manager
- **Development Tools**: curl, wget, vim, htop, tree, jq, git, build-essential

## Usage

### Building the Image

```bash
# Build with default settings
packer build template.pkr.hcl

# Build with custom tag
packer build -var "image_tag=v1.0" template.pkr.hcl

# Build with custom registry
packer build \
  -var "image_tag=v1.0" \
  -var "registry=my-registry.com" \
  -var "organization=myorg" \
  template.pkr.hcl
```

### Using the Built Image

```bash
# Run a VM from the built image
meda run ghcr.io/cirunlabs/ubuntu-docker:latest --name my-docker-vm

# Connect to the VM
meda ssh my-docker-vm

# Verify Docker is working
ubuntu@vm:~$ docker run hello-world
ubuntu@vm:~$ docker compose version
ubuntu@vm:~$ kubectl version --client
ubuntu@vm:~$ helm version --client
```

## Configuration

The image is configured with:

- Docker daemon enabled and running on boot
- User `ubuntu` added to docker group (no sudo required for docker commands)
- Docker log rotation configured (10MB max file size, 3 files)
- Useful bash aliases (ll, la, l)
- Clean system state (no cached packages, empty logs)

## Customization

To customize this image:

1. Modify the `template.pkr.hcl` file
2. Add additional provisioner steps
3. Update the metadata.json file
4. Rebuild with `packer build`

## Variables

- `image_tag`: Tag for the output image (default: "latest")
- `registry`: Container registry to push to (default: "ghcr.io")
- `organization`: Registry organization/namespace (default: GITHUB_REPOSITORY_OWNER)

## Build Process

1. Creates a VM from Ubuntu base image
2. Waits for cloud-init to complete
3. Updates system packages
4. Installs Docker from official repository
5. Configures Docker daemon and user permissions
6. Installs kubectl and Helm
7. Installs additional development tools
8. Configures user environment
9. Cleans up system for image creation
10. Validates all tools are working
11. Creates final image artifact