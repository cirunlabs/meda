#!/bin/bash
set -e

echo "Starting VM provisioning..."

# Wait for cloud-init to complete
echo "Waiting for cloud-init to complete..."
sudo cloud-init status --wait

# Update system
echo "Updating system packages..."
sudo apt-get update
sudo apt-get upgrade -y

# Install basic tools
echo "Installing basic tools..."
sudo apt-get install -y curl wget vim htop tree jq git

# Configure user environment
echo "Configuring user environment..."
echo 'export PATH=$PATH:/usr/local/bin' >> ~/.bashrc
echo 'alias ll="ls -la"' >> ~/.bashrc
echo 'alias la="ls -A"' >> ~/.bashrc
echo 'alias l="ls -CF"' >> ~/.bashrc

# Clean up
echo "Cleaning up..."
sudo apt-get autoremove -y
sudo apt-get autoclean
sudo rm -rf /var/lib/apt/lists/*
sudo rm -rf /tmp/*
sudo rm -rf /var/tmp/*

# Clear bash history
history -c

# Clear logs
sudo find /var/log -type f -exec truncate -s 0 {} \;

echo "Provisioning completed successfully!"