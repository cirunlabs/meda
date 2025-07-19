#!/usr/bin/env bash
# Test script for ch-vm.sh
# This script will:
# 1. Create a VM
# 2. Wait for it to boot
# 3. SSH into it to verify it works
# 4. Delete the VM

set -Eeuo pipefail

# Get the directory of this script
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
VM_NAME="test-vm-$(date +%s)"
SSH_TIMEOUT=180  # seconds to wait for SSH to become available
SSH_RETRY_DELAY=5  # seconds between SSH connection attempts

# Colors for output
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Function to log messages
log() {
  echo -e "${GREEN}[TEST] $1${NC}"
}

error() {
  echo -e "${RED}[ERROR] $1${NC}" >&2
  exit 1
}

warn() {
  echo -e "${YELLOW}[WARN] $1${NC}" >&2
}

info() {
  echo -e "${BLUE}[INFO] $1${NC}"
}

cleanup() {
  if [[ -n "${VM_NAME:-}" ]]; then
    log "Cleaning up VM: $VM_NAME"
    "$SCRIPT_DIR/ch-vm.sh" delete "$VM_NAME" &>/dev/null || true
  fi
}

# Set up trap to clean up VM on script exit
trap cleanup EXIT

# Check dependencies
check_dependencies() {
  local missing=false

  for cmd in nc ssh sshpass; do
    if ! command -v "$cmd" &>/dev/null; then
      warn "Required command '$cmd' not found"
      missing=true
    fi
  done

  if $missing; then
    log "Installing missing dependencies..."
    sudo apt-get update -qq
    sudo apt-get install -y -qq netcat-openbsd openssh-client sshpass
  fi
}

# Check if ch-vm.sh exists and is executable
if [[ ! -x "$SCRIPT_DIR/ch-vm.sh" ]]; then
  error "ch-vm.sh not found or not executable in $SCRIPT_DIR"
fi

# Check and install dependencies
check_dependencies

# Step 1: Create the VM
log "Creating VM: $VM_NAME"
if ! "$SCRIPT_DIR/ch-vm.sh" create "$VM_NAME"; then
  error "Failed to create VM"
fi

# Get the VM's IP address
get_vm_ip() {
  local ip

  # Try multiple methods to get the IP
  ip=$("$SCRIPT_DIR/ch-vm.sh" get "$VM_NAME" | grep -oP '(?<=VM IP: )[0-9.]+' || echo "")

  if [[ -z "$ip" ]]; then
    # Try to extract from list command
    ip=$("$SCRIPT_DIR/ch-vm.sh" list | grep "$VM_NAME" | awk '{print $3}')
  fi

  if [[ -z "$ip" || "$ip" == "-" ]]; then
    # Try to get from VM directory
    local vm_dir="${HOME}/.ch-vms/vms/$VM_NAME"
    if [[ -f "$vm_dir/subnet" ]]; then
      local subnet=$(cat "$vm_dir/subnet")
      ip="${subnet}.2"
    fi
  fi

  echo "$ip"
}

VM_IP=$(get_vm_ip)
if [[ -z "$VM_IP" || "$VM_IP" == "-" ]]; then
  error "Could not determine VM IP address"
fi

log "VM IP address: $VM_IP"

# Step 2: Wait for SSH to become available
log "Waiting for SSH to become available (timeout: ${SSH_TIMEOUT}s)..."
start_time=$(date +%s)
ssh_ready=false

while [[ $(($(date +%s) - start_time)) -lt $SSH_TIMEOUT ]]; do
  # Check if VM is still running
  if ! "$SCRIPT_DIR/ch-vm.sh" get "$VM_NAME" | grep -q "VM .* is running"; then
    warn "VM appears to have stopped running"
    "$SCRIPT_DIR/ch-vm.sh" debug "$VM_NAME"
    error "VM stopped unexpectedly"
  fi

  # Check if we can ping the VM
  if ping -c1 -W1 "$VM_IP" &>/dev/null; then
    info "VM is responding to ping"

    # Check if SSH port is open
    if nc -z -w2 "$VM_IP" 22 &>/dev/null; then
      info "SSH port is open"
      # Wait a bit more to ensure SSH server is fully ready
      sleep 5
      ssh_ready=true
      break
    else
      info "Ping successful but SSH port not open yet"
    fi
  fi

  # Try to fix network if ping fails after some time
  elapsed=$(($(date +%s) - start_time))
  if [[ $elapsed -gt 60 && $((elapsed % 30)) -eq 0 ]]; then
    warn "VM not responding to ping after ${elapsed}s, attempting network fix"
    "$SCRIPT_DIR/ch-vm.sh" debug "$VM_NAME" >/dev/null
  fi

  # Show progress and wait
  echo -n "."
  sleep $SSH_RETRY_DELAY
done
echo ""

if ! $ssh_ready; then
  warn "SSH did not become available within timeout period"
  warn "Running debug to get more information:"
  "$SCRIPT_DIR/ch-vm.sh" debug "$VM_NAME"
  error "SSH connection test failed"
fi

# Step 3: SSH into the VM and run a simple command
log "Testing SSH connection to VM..."
SSH_CMD="ssh -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o ConnectTimeout=10 ubuntu@$VM_IP"

# Try to run a simple command via SSH (multiple attempts)
ssh_success=false
for attempt in {1..5}; do
  info "SSH attempt $attempt/5..."
  if $SSH_CMD "echo 'SSH connection successful' && uname -a" 2>/dev/null; then
    ssh_success=true
    break
  fi

  # If this isn't the first attempt, try to debug what's happening
  if [[ $attempt -gt 1 ]]; then
    info "Checking SSH server status on VM..."
    # Try with verbose output to see what's happening
    $SSH_CMD -v "echo test" 2>&1 | grep -i "connection\|authentication\|debug\|error" || true
  fi

  sleep $SSH_RETRY_DELAY
done

if ! $ssh_success; then
  warn "SSH connection failed. Trying with password authentication..."

  # Check if sshpass is installed
  if ! command -v sshpass &>/dev/null; then
    warn "sshpass not installed. Installing..."
    sudo apt-get update -qq && sudo apt-get install -y -qq sshpass
  fi

  # Try with password authentication (multiple attempts)
  for attempt in {1..5}; do
    info "Password SSH attempt $attempt/5..."
    if sshpass -p "ubuntu" $SSH_CMD "echo 'SSH connection successful' && uname -a"; then
      ssh_success=true
      break
    fi
    sleep $SSH_RETRY_DELAY
  done

  if ! $ssh_success; then
    warn "SSH connection with password also failed"
    warn "Checking if cloud-init has completed on the VM..."

    # Try to check cloud-init status
    if ping -c1 -W1 "$VM_IP" &>/dev/null; then
      # Try with a longer timeout
      if sshpass -p "ubuntu" $SSH_CMD -o ConnectTimeout=30 "sudo cloud-init status" 2>/dev/null; then
        info "Cloud-init status retrieved"
      else
        warn "Could not retrieve cloud-init status"
      fi
    fi

    "$SCRIPT_DIR/ch-vm.sh" debug "$VM_NAME"
    error "SSH test failed"
  fi
fi

log "SSH connection test passed!"

# Step 4: Delete the VM
log "Deleting VM: $VM_NAME"
if ! "$SCRIPT_DIR/ch-vm.sh" delete "$VM_NAME"; then
  error "Failed to delete VM"
fi

log "Test completed successfully!"
