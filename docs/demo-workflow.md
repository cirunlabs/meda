# Demo Workflow - Meda Usage with Latest Release

This document explains the GitHub Actions workflow that demonstrates how to install and use meda from a released version.

## Workflow File

The workflow is defined in `.github/workflows/demo-meda-usage.yml` and serves multiple purposes:

1. **Documentation**: Shows how to properly install meda in a CI/CD environment
2. **Testing**: Validates that the release installation process works correctly
3. **Example**: Provides a working reference for users who want to integrate meda into their own workflows

## Workflow Features

### System Setup
- Installs required system dependencies (qemu-utils, genisoimage, iptables)
- Configures KVM access for VM operations
- Checks available disk space

### Meda Installation
- Uses the official `scripts/install-release.sh` script
- Installs to `/usr/local/bin` for global access
- Verifies installation with version and help commands

### VM Demonstration
- Creates an Ubuntu VM with 1GB memory and 2 CPUs
- Starts the VM and waits for initialization
- Retrieves VM status and network information
- Demonstrates proper cleanup (stop and delete VM)

### Error Handling
- Uses JSON output for better automation integration
- Includes cleanup steps that run even if previous steps fail
- Provides clear logging and status information

## Usage

### Manual Trigger
You can manually trigger this workflow from the GitHub Actions tab:
1. Go to the repository's Actions tab
2. Select "Demo - Meda Usage with Latest Release"
3. Click "Run workflow"

### Automatic Trigger
The workflow runs automatically when:
- Changes are pushed to the workflow file itself
- Changes are made to the installation script (`scripts/install-release.sh`)

## Adapting for Your Use Case

To use this workflow as a template for your own projects:

1. **Copy the installation steps**:
   ```yaml
   - name: Install system dependencies
     run: |
       sudo apt-get update
       sudo apt-get install -y qemu-utils genisoimage iptables jq curl

   - name: Give the runner user access to KVM
     run: sudo setfacl -m u:${USER}:rw /dev/kvm

   - name: Install meda using release script
     run: |
       curl -fsSL https://raw.githubusercontent.com/cirunlabs/meda/main/scripts/install-release.sh | sudo bash -s -- --install-dir /usr/local/bin
   ```

2. **Customize VM operations** based on your needs:
   - Change VM specifications (memory, CPUs, disk size)
   - Add your own VM configuration or user-data
   - Integrate with your application deployment

3. **Add your application logic**:
   - Deploy your application to the created VM
   - Run tests against the VM
   - Collect logs or artifacts

## Notes

- The workflow uses a specific version (`v0.3.2`) in CI to avoid GitHub API rate limiting issues
- In normal usage outside CI, you can omit the `--version` parameter to get the latest release automatically
- The workflow includes comprehensive error handling and cleanup
- All VM operations use JSON output for better automation integration

## System Requirements

The workflow runs on `ubuntu-latest` GitHub Actions runners, which provide:
- Linux with KVM support
- Sufficient disk space for VM operations
- Network access for downloading Ubuntu images

## Troubleshooting

If the workflow fails:
1. Check the system dependency installation step
2. Verify KVM access is properly configured
3. Ensure sufficient disk space is available
4. Review the VM creation and startup logs

The workflow includes debug information in case of failures, including disk usage, memory usage, and running processes.