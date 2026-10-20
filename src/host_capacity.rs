//! Read static host capacity for the admission layer: total RAM, total
//! cores, total disk under `~/.meda`. All values returned in GiB / u32
//! cores so the admission module can compare against request bodies
//! without unit confusion.
//!
//! Detection is best-effort: failure to read `/proc/meminfo` or
//! `statvfs` falls back to a tiny safe value so the admission layer
//! reflexively denies new requests rather than over-accept on a bad
//! probe. The reasoning is the same as the admission module: better
//! 503s than an OOM-kill that drags down the user's systemd session.

use std::fs;
use std::path::Path;

/// Read MemTotal from /proc/meminfo, return as GiB (floor). On failure
/// returns 0 — admission layer will then deny everything, which is the
/// safe direction.
pub fn total_mem_gb() -> u64 {
    let body = match fs::read_to_string("/proc/meminfo") {
        Ok(b) => b,
        Err(_) => return 0,
    };
    for line in body.lines() {
        if let Some(rest) = line.strip_prefix("MemTotal:") {
            // Format: "MemTotal:       102400000 kB"
            let kb: u64 = rest
                .split_whitespace()
                .next()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            return kb / (1024 * 1024); // KiB → GiB, floor
        }
    }
    0
}

/// Number of logical CPUs visible to this process. Uses
/// `num_cpus::get()` via the standard library on Linux (relies on
/// /sys/devices/system/cpu/online). Falls back to 1 if the syscall
/// fails — better to throttle severely than over-accept.
pub fn total_cpu() -> u32 {
    std::thread::available_parallelism()
        .map(|n| n.get() as u32)
        .unwrap_or(1)
}

/// Total filesystem capacity (GiB) of the partition that holds
/// `vm_root`. `statvfs(2)` via libc would be the right primitive,
/// but we already have `nix` in the dep tree and `nix::sys::statvfs`
/// gives the same numbers in a safer wrapper.
pub fn total_disk_gb(vm_root: &Path) -> u64 {
    // statvfs requires an extant entry, and on a fresh install neither
    // ~/.meda/vms nor ~/.meda exists yet: measure the closest ancestor that
    // does, which is on the partition the VMs will be created on.
    let Some(probe) = vm_root.ancestors().find(|p| p.exists()) else {
        return 0;
    };
    match nix::sys::statvfs::statvfs(probe) {
        Ok(st) => {
            // blocks × frsize → bytes. nix exposes these as
            // `fsblkcnt_t` / `fsfilcnt_t`, which is u32 on macOS dev
            // boxes but u64 on Linux CI; `as u64` is a no-op on the
            // wider platform and a safe widening cast on the narrower.
            #[allow(clippy::unnecessary_cast)]
            let total: u64 = (st.blocks() as u64) * (st.fragment_size() as u64);
            total / (1024 * 1024 * 1024)
        }
        Err(_) => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn total_disk_gb_measures_partition_before_meda_dirs_exist() {
        // Fresh install: neither ~/.meda nor ~/.meda/vms exists yet.
        let home = tempfile::tempdir().unwrap();
        let vm_root = home.path().join(".meda").join("vms");

        assert_eq!(total_disk_gb(&vm_root), total_disk_gb(home.path()));
        assert!(total_disk_gb(&vm_root) > 0);
    }
}
