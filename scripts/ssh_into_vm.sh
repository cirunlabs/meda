#!/bin/bash

sep="================================================"

log () {
  echo $sep
  echo $1
  echo $sep
}

ip=$(target/debug/meda list --json | jq -r '.[0].ip')
log "Connecting to IP: $ip"

max_retries=10
delay=3
success=0

# Define SSH function to avoid repetition
ssh_execute() {
    local cmd="$1"
    echo "Executing: $cmd"
    sshpass -p 'cirun' ssh -o StrictHostKeyChecking=no -o ConnectTimeout=5 cirun@$ip "$cmd"
    return $?
}

# Try to establish connection
for i in $(seq 1 $max_retries); do
    log "Attempt $i to SSH into $ip..."
    if ssh_execute "echo 'SSH successful'"; then
    success=1
    break
    fi
    log "SSH attempt $i failed, retrying in $delay seconds..."
    sleep $delay
done

if [ $success -ne 1 ]; then
    log "SSH failed after $max_retries attempts"
    exit 1
fi

# Once connected successfully, collect system information
log "SYSTEM INFORMATION COLLECTION"

log "SYSTEM OVERVIEW"
ssh_execute "hostnamectl"
ssh_execute "uname -a"
ssh_execute "cat /etc/os-release"

log "CPU INFORMATION"
ssh_execute "lscpu"
ssh_execute "cat /proc/cpuinfo | grep 'model name' | head -1"
ssh_execute "nproc --all"

log "MEMORY INFORMATION"
ssh_execute "free -h"
ssh_execute "cat /proc/meminfo | grep -E 'MemTotal|MemFree|MemAvailable'"

log "STORAGE INFORMATION"
ssh_execute "df -h"
ssh_execute "lsblk"
ssh_execute "mount | grep '^/dev'"

log "NETWORK INFORMATION"
ssh_execute "ip addr"
ssh_execute "ip route"
ssh_execute "netstat -tuln"
ssh_execute "cat /etc/hosts"
ssh_execute "cat /etc/resolv.conf"

log "LOAD & RUNNING PROCESSES"
ssh_execute "uptime"
ssh_execute "ps aux | sort -rk 3,3 | head -10"
ssh_execute "systemctl list-units --type=service --state=running"

log "ENVIRONMENT VARIABLES"
ssh_execute "env | sort"

log "USER INFORMATION"
ssh_execute "who"
ssh_execute "id"

log "GPU INFORMATION (IF AVAILABLE)"
ssh_execute "lspci | grep -i 'vga\|3d\|2d'"
ssh_execute "command -v nvidia-smi && nvidia-smi || echo 'nvidia-smi not available'"

log "CONTAINER INFO (IF AVAILABLE)"
ssh_execute "command -v docker && docker ps -a || echo 'docker not available'"

log "System information collection complete."
