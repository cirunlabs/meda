#!/usr/bin/env bash
# Helper functions for VM testing

# Function to check if a VM's network is properly configured
check_vm_network() {
  local vm_name="$1"
  local vm_dir="${HOME}/.ch-vms/vms/$vm_name"
  
  if [[ ! -d "$vm_dir" ]]; then
    echo "VM $vm_name does not exist"
    return 1
  fi
  
  if [[ ! -f "$vm_dir/subnet" || ! -f "$vm_dir/tapdev" ]]; then
    echo "Network configuration files missing for VM $vm_name"
    return 1
  fi
  
  local subnet=$(cat "$vm_dir/subnet")
  local tap=$(cat "$vm_dir/tapdev")
  
  echo "Checking network for VM $vm_name (IP: ${subnet}.2, Tap: $tap)"
  
  # Check if tap device exists
  if ! ip link show "$tap" &>/dev/null; then
    echo "Tap device $tap does not exist, creating it..."
    sudo ip tuntap add "$tap" mode tap
    sudo ip addr add "${subnet}.1/24" dev "$tap"
    sudo ip link set "$tap" up
    echo "Tap device created"
  elif ! ip link show "$tap" | grep -q "UP"; then
    echo "Tap device $tap is DOWN, bringing it up..."
    sudo ip link set "$tap" up
    echo "Tap device brought up"
  fi
  
  # Check if IP is assigned to tap device
  if ! ip addr show "$tap" | grep -q "${subnet}.1/24"; then
    echo "IP address missing on tap device, adding it..."
    sudo ip addr add "${subnet}.1/24" dev "$tap" 2>/dev/null || true
    echo "IP address added"
  fi
  
  # Check if masquerade rule exists
  if ! sudo iptables -t nat -C POSTROUTING -s "${subnet}.0/24" -j MASQUERADE &>/dev/null; then
    echo "Masquerade rule missing, adding it..."
    sudo iptables -t nat -A POSTROUTING -s "${subnet}.0/24" -j MASQUERADE
    echo "Masquerade rule added"
  fi
  
  # Check if IP forwarding is enabled
  if [[ "$(cat /proc/sys/net/ipv4/ip_forward)" != "1" ]]; then
    echo "IP forwarding not enabled, enabling it..."
    sudo sysctl -q net.ipv4.ip_forward=1
    echo "IP forwarding enabled"
  fi
  
  # Check if VM is pingable
  if ping -c1 -W1 "${subnet}.2" &>/dev/null; then
    echo "VM is pingable: SUCCESS"
  else
    echo "VM is not pingable: FAILED"
    return 1
  fi
  
  # Check if SSH port is open
  if nc -z -w2 "${subnet}.2" 22 &>/dev/null; then
    echo "SSH port is open: SUCCESS"
  else
    echo "SSH port is not open: FAILED"
    return 1
  fi
  
  echo "Network check completed successfully"
  return 0
}

# Function to run a command inside a VM
run_in_vm() {
  local vm_name="$1"
  local command="$2"
  local vm_dir="${HOME}/.ch-vms/vms/$vm_name"
  
  if [[ ! -d "$vm_dir" || ! -f "$vm_dir/subnet" ]]; then
    echo "VM $vm_name does not exist or is not properly configured"
    return 1
  fi
  
  local subnet=$(cat "$vm_dir/subnet")
  local vm_ip="${subnet}.2"
  
  # Try SSH with key authentication first
  if ssh -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o ConnectTimeout=5 ubuntu@"$vm_ip" "$command" 2>/dev/null; then
    return 0
  fi
  
  # Try with password authentication
  if command -v sshpass &>/dev/null; then
    if sshpass -p "ubuntu" ssh -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o ConnectTimeout=5 ubuntu@"$vm_ip" "$command" 2>/dev/null; then
      return 0
    fi
  fi
  
  echo "Failed to run command in VM $vm_name"
  return 1
}

# Function to wait for cloud-init to complete in a VM
wait_for_cloud_init() {
  local vm_name="$1"
  local timeout="${2:-300}"  # Default timeout: 5 minutes
  local vm_dir="${HOME}/.ch-vms/vms/$vm_name"
  
  if [[ ! -d "$vm_dir" || ! -f "$vm_dir/subnet" ]]; then
    echo "VM $vm_name does not exist or is not properly configured"
    return 1
  fi
  
  local subnet=$(cat "$vm_dir/subnet")
  local vm_ip="${subnet}.2"
  
  echo "Waiting for cloud-init to complete in VM $vm_name (timeout: ${timeout}s)..."
  
  local start_time=$(date +%s)
  while [[ $(($(date +%s) - start_time)) -lt $timeout ]]; do
    # Check if VM is pingable
    if ! ping -c1 -W1 "$vm_ip" &>/dev/null; then
      echo -n "p"  # p for ping failed
      sleep 5
      continue
    fi
    
    # Check if SSH port is open
    if ! nc -z -w2 "$vm_ip" 22 &>/dev/null; then
      echo -n "s"  # s for SSH port not open
      sleep 5
      continue
    fi
    
    # Try to check cloud-init status
    local status
    status=$(sshpass -p "ubuntu" ssh -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o ConnectTimeout=5 ubuntu@"$vm_ip" "cloud-init status 2>/dev/null || echo 'unknown'" 2>/dev/null)
    
    if [[ "$status" == *"done"* ]]; then
      echo -e "\nCloud-init completed successfully"
      return 0
    elif [[ "$status" == *"running"* ]]; then
      echo -n "r"  # r for running
    else
      echo -n "?"  # unknown status
    fi
    
    sleep 5
  done
  
  echo -e "\nTimeout waiting for cloud-init to complete"
  return 1
}
