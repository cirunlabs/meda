#!/usr/bin/env bash
#
# Fast Ubuntu CI micro‑VMs with Cloud‑Hypervisor
# Usage:
#   ./ch-vm.sh create  <name> [user‑data.yaml]
#   ./ch-vm.sh list
#   ./ch-vm.sh get     <name>
#   ./ch-vm.sh delete  <name>
#
# Prereqs (run once): sudo apt install cloud-hypervisor genisoimage jq
#   wget https://cloud-images.ubuntu.com/jammy/current/jammy-server-cloudimg-amd64.img
#   qemu-img convert -O raw jammy-server-cloudimg-amd64.img ubuntu-base.raw
#   wget https://github.com/cloud-hypervisor/rust-hypervisor-firmware/releases/latest/download/hypervisor-fw
#
set -euo pipefail
BASE_DIR=${CH_VM_DIR:-/var/lib/ch-vms}        # configurable with env var
BASE_IMG=${BASE_DIR}/ubuntu-base.raw
FW=${BASE_DIR}/hypervisor-fw                 # Rust Hypervisor Firmware
MEM=1024M                                    # 1 GiB – tiny but enough for CI test runners
CPUS=2
: "${BASE_IMG:?missing base image}"
: "${FW:?missing firmware}"

fatal(){ echo >&2 "error: $*"; exit 1; }

# ---------- helpers ----------------------------------------------------------
random_mac()   { printf '52:54:%02x:%02x:%02x:%02x\n' $((RANDOM%256)) $((RANDOM%256)) $((RANDOM%256)) $((RANDOM%256)); }
random_octet() { printf '%d' $(( 16 + RANDOM % 200 )); }  # 16‑215 avoids RFC1918 overlap

create_vm() {
  local NAME=$1; shift
  local USERDATA=${1:-}             # optional user‑data YAML
  local VMDIR=${BASE_DIR}/${NAME}; mkdir -p "${VMDIR}"

  # 1. copy‑on‑write guest disk (reflink is instant on btrfs/xfs)
  cp --reflink=auto "${BASE_IMG}" "${VMDIR}/rootfs.raw"

  # 2. cloud‑init ISO (user‑data + network‑config)
  local SUBNET_OCT=$(random_octet)
  local GUEST_SUBNET="192.168.${SUBNET_OCT}"
  cat >"${VMDIR}/meta-data"<<EOF
instance-id: ${NAME}
local-hostname: ${NAME}
EOF
  # minimal default user‑data if none supplied
  if [[ -z "${USERDATA}" ]]; then
cat >"${VMDIR}/user-data"<<'EOF'
#cloud-config
ssh_pwauth:   false
users:
  - name: cirun
    sudo: ALL=(ALL) NOPASSWD:ALL
    lock_passwd: true
    ssh_authorized_keys:
      - "ssh-ed25519 AAAAC3NzaC1yc2EAAAADAQABAAACAQC..."
package_update: false
EOF
  else
    cp "${USERDATA}" "${VMDIR}/user-data"
  fi

  cat >"${VMDIR}/network-config"<<EOF
version: 2
ethernets:
  eth0:
    dhcp4: false
    addresses: [${GUEST_SUBNET}.2/24]
    gateway4: ${GUEST_SUBNET}.1
    nameservers: {addresses: [8.8.8.8,1.1.1.1]}
EOF

  genisoimage -quiet -output "${VMDIR}/ci.iso" \
              -volid cidata -joliet -rock \
              "${VMDIR}/user-data" "${VMDIR}/meta-data" "${VMDIR}/network-config"

  # 3. per‑VM NAT with its own tap – keeps VMs isolated but gives internet
  local TAP="vmtap-${NAME}"
  sudo ip tuntap add "${TAP}" mode tap
  sudo ip addr add ${GUEST_SUBNET}.1/24 dev "${TAP}"
  sudo ip link set "${TAP}" up
  sudo sysctl -q net.ipv4.ip_forward=1
  sudo iptables -t nat -A POSTROUTING -s ${GUEST_SUBNET}.0/24 ! -o "${TAP}" -j MASQUERADE

  # 4. launch cloud‑hypervisor
  local API_SOCK=${VMDIR}/api.sock
  cloud-hypervisor \
      --api-socket "path=${API_SOCK}" \
      --console off --serial null \
      --cpus "boot=${CPUS}" \
      --memory "size=${MEM}" \
      --kernel "${FW}" \
      --disk "path=${VMDIR}/rootfs.raw" \
      --disk "path=${VMDIR}/ci.iso,readonly=on" \
      --net "tap=${TAP},mac=$(random_mac)" \
      --rng src=/dev/urandom \
      &> "${VMDIR}/ch.log" &
  echo $! > "${VMDIR}/pid"
  echo "VM ${NAME} up → ssh cirun@${GUEST_SUBNET}.2 (key from your user‑data)"
}

list_vms()   { ls -1 "${BASE_DIR}" || true; }
get_vm()     { jq . < <(ch-remote --api-socket "${BASE_DIR}/$1/api.sock" info) | less; }
delete_vm()  {
  local NAME=$1 VMDIR=${BASE_DIR}/${NAME} TAP="vmtap-${NAME}"
  ch-remote --api-socket "${VMDIR}/api.sock" shutdown || true
  sleep 2
  sudo iptables -t nat -D POSTROUTING -s 192.168.*.0/24 ! -o "${TAP}" -j MASQUERADE || true
  sudo ip link del "${TAP}" || true
  pkill -F "${VMDIR}/pid" || true
  rm -rf "${VMDIR}"
  echo "VM ${NAME} removed"
}

case "${1:-}" in
  create)  shift; create_vm "$@";;
  list)    list_vms;;
  get)     shift; get_vm "$@";;
  delete)  shift; delete_vm "$@";;
  *) echo "usage: $0 {create|list|get|delete}"; exit 1;;
esac
