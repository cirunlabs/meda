#!/usr/bin/env bash
# ════════════════════════════════════════════════════════════════════════
#  Cloud-Hypervisor micro-VM helper (Ubuntu Jammy) – create | list | get | delete
# ════════════════════════════════════════════════════════════════════════
set -Eeuo pipefail

# ── CONFIG ───────────────────────────────────────────────────────────────
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CH_HOME="${HOME}/.ch-vms"
ASSET_DIR="${CH_ASSET_DIR:-$CH_HOME/assets}"
VM_ROOT="${CH_VM_DIR:-$CH_HOME/vms}"

OS_URL="https://cloud-images.ubuntu.com/jammy/current/jammy-server-cloudimg-amd64.img"
FW_URL="https://github.com/cloud-hypervisor/rust-hypervisor-firmware/releases/latest/download/hypervisor-fw"
CH_URL="https://github.com/cloud-hypervisor/cloud-hypervisor/releases/latest/download/cloud-hypervisor-static"
CR_URL="https://github.com/cloud-hypervisor/cloud-hypervisor/releases/latest/download/ch-remote-static"

BASE_RAW="$ASSET_DIR/ubuntu-base.raw"
FW_BIN="$ASSET_DIR/hypervisor-fw"
CH_BIN="$ASSET_DIR/cloud-hypervisor"
CR_BIN="$ASSET_DIR/ch-remote"

CPUS=${CH_CPUS:-2}
MEM=${CH_MEM:-1024M}
DISK_SIZE=${CH_DISK_SIZE:-10G}

# ── HELPERS ──────────────────────────────────────────────────────────────
die(){ echo >&2 -e "\033[1;31mfatal: $*\033[0m"; exit 1; }
msg(){ echo -e "\033[1;36m>> $*\033[0m"; }
root(){ command -v sudo &>/dev/null && sudo "$@" || "$@"; }
rand_mac(){ printf '52:54:%02x:%02x:%02x:%02x\n' $((RANDOM % 256)) $((RANDOM % 256)) $((RANDOM % 256)) $((RANDOM % 256)); }
rand_octet(){ printf '%d' $((16 + RANDOM % 200)); }

need(){ 
  if ! command -v "$1" &>/dev/null; then
    msg "Installing dependency: $1"
    root apt-get -qq update
    case "$1" in 
      qemu-img) pkgs=qemu-utils ;;
      genisoimage) pkgs=genisoimage ;;
      iptables) pkgs=iptables ;;
      wget) pkgs=wget ;;
      curl) pkgs=curl ;;
      jq) pkgs=jq ;;
      *) die "missing dep $1";;
    esac
    root apt-get -y install $pkgs
  fi
}

download(){ 
  need wget || need curl
  msg "Downloading $1"
  { command -v wget &>/dev/null && wget -q -O "$2" "$1"; } ||
  { curl -sSL -o "$2" "$1"; }
}

check_vm_running() {
  local dir="$VM_ROOT/$1"
  
  # Check if we have a PID file and the process is running
  if [[ -f "$dir/pid" ]]; then
    local pid=$(cat "$dir/pid" 2>/dev/null)
    if [[ -n "$pid" ]] && ps -p "$pid" &>/dev/null; then
      # Process exists
      if [[ -S "$dir/api.sock" ]]; then
        # Socket exists and process is running
        return 0  # VM is running
      else
        # Process exists but socket is missing or not a socket
        # This is odd but we'll consider it running
        return 0
      fi
    else
      # Process doesn't exist, clean up stale files
      rm -f "$dir/api.sock" "$dir/pid"
      return 1
    fi
  elif [[ -S "$dir/api.sock" ]]; then
    # Socket exists but no PID file
    # Try to find the cloud-hypervisor process that's using this socket
    local found_pid=$(ps aux | grep "$CH_BIN.*$dir/api.sock" | grep -v grep | awk '{print $2}' | head -1)
    if [[ -n "$found_pid" ]]; then
      # Found running process, create pid file
      echo "$found_pid" > "$dir/pid"
      return 0
    else
      # Socket exists but no process using it, clean up
      rm -f "$dir/api.sock"
      return 1
    fi
  fi
  
  return 1  # VM is not running
}

# ── BOOTSTRAP (one-time) ────────────────────────────────────────────────
bootstrap() {
  mkdir -p "$CH_HOME" "$ASSET_DIR" "$VM_ROOT"
  
  if [[ ! -f "$BASE_RAW" ]]; then
    msg "▸ Downloading Ubuntu image"
    tmp="$ASSET_DIR/img.qcow2"
    download "$OS_URL" "$tmp"
    need qemu-img
    msg "▸ Converting to raw format"
    qemu-img convert -O raw "$tmp" "$BASE_RAW"
    rm -f "$tmp"
  fi
  
  [[ -f "$FW_BIN" ]] || { msg "▸ Downloading firmware"; download "$FW_URL" "$FW_BIN"; chmod 644 "$FW_BIN"; }
  [[ -x "$CH_BIN" ]] || { msg "▸ Downloading cloud-hypervisor"; download "$CH_URL" "$CH_BIN"; chmod +x "$CH_BIN"; }
  [[ -x "$CR_BIN" ]] || { msg "▸ Downloading ch-remote"; download "$CR_URL" "$CR_BIN"; chmod +x "$CR_BIN"; }
  
  need genisoimage
  need iptables
  need jq
  
  export PATH="$ASSET_DIR:$PATH"
}

# ── VM OPS ───────────────────────────────────────────────────────────────
create_vm() {
  [[ $# -lt 1 || $# -gt 2 ]] && die "usage: $0 create <name> [user-data]"
  local name="$1" 
  local userdata="${2:-}"
  local dir="$VM_ROOT/$name"
  
  [[ -e "$dir" ]] && die "VM $name already exists"
  
  bootstrap
  mkdir -p "$dir"
  
  msg "▸ Creating rootfs"
  cp --reflink=auto --sparse=auto "$BASE_RAW" "$dir/rootfs.raw"
  
  # Resize disk if needed
  if [[ "$DISK_SIZE" != "10G" ]]; then
    msg "▸ Resizing disk to $DISK_SIZE"
    need qemu-img
    qemu-img resize "$dir/rootfs.raw" "$DISK_SIZE"
  fi
  
  local oct subnet tap
  oct=$(rand_octet)
  subnet="192.168.$oct"
  tap="tap-$name"
  
  # Store network config for later use
  echo "$subnet" > "$dir/subnet"
  echo "$tap" > "$dir/tapdev"
  
  # cloud-init seed
  cat > "$dir/meta-data" << EOF
instance-id: $name
local-hostname: $name
EOF
  
  if [[ -z "$userdata" ]]; then
    cat > "$dir/user-data" << 'EOF'
#cloud-config
users:
  - default
  - name: ubuntu
    sudo: ALL=(ALL) NOPASSWD:ALL
    lock_passwd: False
    inactive: False
    ssh_authorized_keys:
      - ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIJEVWl1nGkztNpYjY0/QHQ0xOTw5hlUbZGxhY0XH7D4h aktech
      - ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIH5/jw+u5VH245tTKooHhKWJ8G2FMms93tF1fsWRo+n+ akhorse@homelab
package_update: false
EOF
  else 
    cp "$userdata" "$dir/user-data"
  fi

    local mac_addr=$(rand_mac)
    echo "$mac_addr" > "$dir/mac"

  # Simplified network config with static IP only
  cat > "$dir/network-config" << EOF
version: 2
ethernets:
  ens4:
    match:
       macaddress: $mac_addr
    addresses: [$subnet.2/24]
    gateway4: $subnet.1
    set-name: ens4
    nameservers:
      addresses: [8.8.8.8, 1.1.1.1]
EOF

  msg "▸ Creating cloud-init ISO"
  genisoimage -quiet -output "$dir/ci.iso" -volid cidata -joliet -rock \
              "$dir/user-data" "$dir/meta-data" "$dir/network-config"
  
  msg "▸ Setting up host networking"
  if ! ip link show "$tap" &>/dev/null; then
    root ip tuntap add "$tap" mode tap
    root ip addr add "$subnet.1/24" dev "$tap"
    root ip link set "$tap" up
  fi
  
  # Enable forwarding
  root sysctl -q net.ipv4.ip_forward=1
  
  # Check if masquerade rule exists before adding
  if ! root iptables -t nat -C POSTROUTING -s "$subnet.0/24" -j MASQUERADE &>/dev/null; then
    root iptables -t nat -A POSTROUTING -s "$subnet.0/24" -j MASQUERADE
  fi

  # Allow traffic from the VM to leave the host and the replies to come back
  if ! root iptables -C FORWARD -i "$tap" -j ACCEPT &>/dev/null; then
    root iptables -A FORWARD -i "$tap" -j ACCEPT
    root iptables -A FORWARD -o "$tap" -m conntrack --ctstate RELATED,ESTABLISHED -j ACCEPT
  fi

  msg "▸ Booting VM $name"
  
  # Create VM start script for easier management
  cat > "$dir/start.sh" << EOF
#!/bin/bash
cd "$dir"
$CH_BIN \\
  --api-socket path=$dir/api.sock \\
  --console off \\
  --serial tty \\
  --kernel "$FW_BIN" \\
  --cpus boot=$CPUS \\
  --memory size=$MEM \\
  --disk path=$dir/rootfs.raw path="$dir/ci.iso" \\
  --net tap=$tap,mac=$mac_addr \\
  --rng src=/dev/urandom \\
  > "$dir/ch.log" 2>&1 &
echo \$! > "$dir/pid"

# Check if command started successfully
sleep 2
if ! ps -p \$(cat "$dir/pid" 2>/dev/null) &>/dev/null; then
  echo "ERROR: Cloud Hypervisor failed to start. Check log: $dir/ch.log" >&2
  exit 1
fi
EOF
  chmod +x "$dir/start.sh"
  
  # Start the VM
  if ! "$dir/start.sh"; then
    # Check the log file for error
    error=$(grep -i "error:" "$dir/ch.log" 2>/dev/null || echo "Unknown error")
    die "Failed to start VM: $error"
  fi
  
  # Wait for VM to boot and get an IP
  msg "▸ Waiting for VM to boot, checkout logs in $dir/ch.log"
  for i in {1..120}; do
    # Check if process is still running
    if ! check_vm_running "$name"; then
      error=$(grep -i "error:" "$dir/ch.log" 2>/dev/null || echo "Process terminated unexpectedly")
      die "VM startup failed: $error"
    fi
    
    if ping -c1 -W1 "$subnet.2" &>/dev/null; then
        msg "▸ VM $name is now running"
        return 0
    fi
    echo -n "." # Show progress indicator
    sleep 2
  done
  
  echo "" # New line after progress dots
  if check_vm_running "$name"; then
    msg "▸ VM appears to be running but not responding to ping yet"
    echo -e "\n\033[1;33mWhen ready: ssh ubuntu@$subnet.2\033[0m"
  else
    tail -n 10 "$dir/ch.log"
    die "VM failed to start properly. See log: $dir/ch.log"
  fi
}

list_vms() {
  bootstrap
  printf "%-18s %-8s %-15s %-10s\n" "NAME" "STATE" "IP" "PORTS"
  
  for d in "$VM_ROOT"/*/; do
    [[ -d "$d" ]] || continue
    
    local name=$(basename "$d")
    local state="stopped"
    local ip="-"
    local fwd="-"
    
    if check_vm_running "$name"; then
      state="running"
      if [[ -f "$d/subnet" ]]; then
        local subnet=$(cat "$d/subnet")
        ip="${subnet}.2"
      fi
      
      if [[ -f "$d/ports" ]]; then
        fwd=$(cat "$d/ports")
      fi
    fi
    
    printf "%-18s %-8s %-15s %-10s\n" "$name" "$state" "$ip" "$fwd"
  done
}

get_vm() {
  [[ $# -ne 1 ]] && die "usage: $0 get <name>"
  local name="$1"
  local dir="$VM_ROOT/$name"
  
  [[ -d "$dir" ]] || die "VM $name does not exist"
  
  if check_vm_running "$name"; then
    msg "▸ VM $name is running"
    "$CR_BIN" --api-socket "$dir/api.sock" info | jq
  else
    msg "▸ VM $name is not running"
    if [[ -f "$dir/subnet" ]]; then
      local subnet=$(cat "$dir/subnet")
      echo -e "\nTo start VM: $0 start $name"
      echo -e "When running: ssh ubuntu@${subnet}.2\n"
    fi
  fi
}

start_vm() {
  [[ $# -ne 1 ]] && die "usage: $0 start <name>"
  local name="$1"
  local dir="$VM_ROOT/$name"
  
  [[ -d "$dir" ]] || die "VM $name does not exist"
  
  if check_vm_running "$name"; then
    msg "▸ VM $name is already running"
    return 0
  fi
  
  # Ensure network device is set up
  if [[ -f "$dir/tapdev" && -f "$dir/subnet" ]]; then
    local tap=$(cat "$dir/tapdev")
    local subnet=$(cat "$dir/subnet")
    
    if ! ip link show "$tap" &>/dev/null; then
      msg "▸ Setting up networking"
      root ip tuntap add "$tap" mode tap
      root ip addr add "$subnet.1/24" dev "$tap"
      root ip link set "$tap" up
    fi
    
    # Enable forwarding
    root sysctl -q net.ipv4.ip_forward=1
    
    # Check if masquerade rule exists before adding
    if ! root iptables -t nat -C POSTROUTING -s "$subnet.0/24" -j MASQUERADE &>/dev/null; then
      root iptables -t nat -A POSTROUTING -s "$subnet.0/24" -j MASQUERADE
    fi
  else
    die "Network configuration for VM $name is missing"
  fi
  
  msg "▸ Starting VM $name"
  if [[ -x "$dir/start.sh" ]]; then
    "$dir/start.sh"
  else
    die "Start script for VM $name is missing"
  fi
  
  # Wait for VM to boot and get an IP
  for _ in {1..20}; do
    if ping -c1 -W1 "${subnet}.2" &>/dev/null; then
      msg "▸ VM $name is now running"
      echo -e "\n\033[1;32mVM $name → ssh ubuntu@${subnet}.2\033[0m"
      return 0
    fi
    sleep 1
  done
  
  msg "▸ VM may still be booting. Check status with '$0 list'"
  echo -e "\n\033[1;32mVM $name → ssh -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null ubuntu@${subnet}.2\033[0m"
  echo -e "Password authentication is enabled with password: ubuntu"
}

stop_vm() {
  [[ $# -ne 1 ]] && die "usage: $0 stop <name>"
  local name="$1"
  local dir="$VM_ROOT/$name"
  
  [[ -d "$dir" ]] || die "VM $name does not exist"
  
  if ! check_vm_running "$name"; then
    msg "▸ VM $name is not running"
    return 0
  fi
  
  msg "▸ Stopping VM $name"
  if [[ -S "$dir/api.sock" ]]; then
    # Try graceful shutdown first
    "$CR_BIN" --api-socket "$dir/api.sock" power-button
    
    # Wait for VM to stop
    for _ in {1..15}; do
      if ! check_vm_running "$name"; then
        msg "▸ VM $name stopped"
        return 0
      fi
      sleep 1
    done
    
    # Force kill if still running
    if [[ -f "$dir/pid" ]]; then
      local pid=$(cat "$dir/pid")
      msg "▸ Force stopping VM $name"
      kill -TERM "$pid" 2>/dev/null || true
      sleep 2
      kill -KILL "$pid" 2>/dev/null || true
      rm -f "$dir/api.sock" "$dir/pid"
    fi
  fi
  
  msg "▸ VM $name stopped"
}

delete_vm() {
  [[ $# -ne 1 ]] && die "usage: $0 delete <name>"
  local name="$1"
  local dir="$VM_ROOT/$name"
  
  [[ -d "$dir" ]] || die "VM $name does not exist"
  
  # Stop VM if running
  if check_vm_running "$name"; then
    stop_vm "$name"
  fi
  
  # Clean up network devices
  if [[ -f "$dir/tapdev" ]]; then
    local tap=$(cat "$dir/tapdev")
    root ip link del "$tap" 2>/dev/null || true
  fi
  
  if [[ -f "$dir/subnet" ]]; then
    local subnet=$(cat "$dir/subnet")
    # Remove iptables rule if this is the last VM using this subnet
    if ! grep -q "$subnet" "$VM_ROOT"/*/subnet 2>/dev/null; then
      root iptables -t nat -D POSTROUTING -s "$subnet.0/24" -j MASQUERADE 2>/dev/null || true
    fi
  fi
  
  rm -rf "$dir"
  msg "▸ VM $name removed"
}

port_forward() {
  [[ $# -ne 3 ]] && die "usage: $0 port-forward <name> <host_port> <guest_port>"
  local name="$1"
  local host_port="$2"
  local guest_port="$3"
  local dir="$VM_ROOT/$name"
  
  [[ -d "$dir" ]] || die "VM $name does not exist"
  [[ -f "$dir/subnet" ]] || die "Network configuration for VM $name is missing"
  
  local subnet=$(cat "$dir/subnet")
  
  # Remove any existing port forward for this host port
  root iptables -t nat -D PREROUTING -p tcp --dport "$host_port" -j DNAT --to "$subnet.2:$guest_port" 2>/dev/null || true
  
  # Add new port forward
  root iptables -t nat -A PREROUTING -p tcp --dport "$host_port" -j DNAT --to "$subnet.2:$guest_port"
  
  # Save port forwarding info
  echo "$host_port->$guest_port" > "$dir/ports"
  
  msg "▸ Port forwarding set up: localhost:$host_port -> $subnet.2:$guest_port"
}

# ── MAIN ──────────────────────────────────────────────────────────────────
case "${1:-}" in
  create)      shift; create_vm "$@" ;;
  list)        list_vms ;;
  get)         shift; get_vm "$@" ;;
  start)       shift; start_vm "$@" ;;
  stop)        shift; stop_vm "$@" ;;
  delete)      shift; delete_vm "$@" ;;
  port-forward) shift; port_forward "$@" ;;
  *)
    echo "Cloud-Hypervisor VM Manager"
    echo "Usage: $0 {create|list|get|start|stop|delete|port-forward}"
    echo
    echo "Commands:"
    echo "  create <name> [user-data]    Create a new VM"
    echo "  list                         List all VMs"
    echo "  get <name>                   Get VM details"
    echo "  start <name>                 Start a VM"
    echo "  stop <name>                  Stop a VM"
    echo "  delete <name>                Delete a VM"
    echo "  port-forward <name> <hp> <gp> Forward host port to guest port"
    echo
    echo "Environment variables:"
    echo "  CH_CPUS      Number of virtual CPUs (default: 2)"
    echo "  CH_MEM       Memory size (default: 1024M)"
    echo "  CH_DISK_SIZE Disk size (default: 10G)"
    echo "  CH_ASSET_DIR Assets directory (default: ~/.ch-vms/assets)"
    echo "  CH_VM_DIR    VM directory (default: ~/.ch-vms/vms)"
    exit 1
    ;;
esac
