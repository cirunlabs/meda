# create the directory the script looks in
sudo mkdir -p /var/lib/ch-vms
cd /var/lib/ch-vms

# 1. grab the latest Jammy cloud image (≈ 640 MB qcow2)
wget https://cloud-images.ubuntu.com/jammy/current/jammy-server-cloudimg-amd64.img

# 2. convert to raw -- fastest format for Cloud-Hypervisor
sudo apt install -y qemu-utils   # provides qemu-img
qemu-img convert -O raw jammy-server-cloudimg-amd64.img ubuntu-base.raw

# 3. fetch Rust Hypervisor Firmware (direct-kernel boot)
wget -O hypervisor-fw \
  https://github.com/cloud-hypervisor/rust-hypervisor-firmware/releases/latest/download/hypervisor-fw
