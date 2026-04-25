//! Per-VM Linux network namespace — the lever that lets many clones
//! of the same snapshot run concurrently without colliding on the
//! template's baked-in guest IP.
//!
//! Each VM gets its own netns with:
//! - A tap device (`tap-<hash>`) holding the template's `.1` gateway
//!   IP, invisible to the host and to other VMs.
//! - A veth pair (`vmh-*` outside, `vmn-*` inside) that wires the
//!   netns to the host. Host reaches the guest via the netns side
//!   IP (`10.99.N.2`), DNAT'd inside the netns to the guest's
//!   snapshotted IP.
//! - iptables MASQUERADE inside the netns so guest outbound goes
//!   veth → host → internet, with the host's existing
//!   `10.99.0.0/16` MASQUERADE rule completing the path.
//!
//! This means every clone keeps the exact same snapshotted network
//! config — no guest-side reconfig, no shared-bridge ARP collisions,
//! and 50+ VMs can spin up concurrently without stepping on each
//! other.

use crate::error::Result;
use crate::util::run_command;
use log::debug;
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::Path;

/// Per-VM netns + veth wiring. Computed deterministically from the
/// VM name and persisted at `<vmdir>/netns.json` so `meda delete`
/// can reconstruct it for teardown.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NetnsSpec {
    /// Netns name (e.g. `meda-a1b2c3d4`). ≤15 chars for compat with
    /// Linux ifname limits (we also use it as a veth suffix).
    pub netns: String,
    /// Host end of the veth pair. Stays in the host netns.
    pub veth_host: String,
    /// Netns end of the veth pair. Lives inside `netns`.
    pub veth_netns: String,
    /// `10.99.N.1/30` — host side of the veth.
    pub host_ip: String,
    /// `10.99.N.2/30` — netns side of the veth. This is the address
    /// users SSH to; netns iptables DNATs it to the guest's IP.
    pub netns_ip: String,
    /// Low byte of the /30 block (just for cleanup helpers).
    pub subnet_index: u16,
}

impl NetnsSpec {
    /// Compute a deterministic spec from the VM's name. Uses a hash
    /// so the name fits Linux's 15-char ifname cap while staying
    /// unique for ordinary VM names. /30 subnet index is derived
    /// from a separate hash slice so name variations aren't
    /// correlated with subnet collisions.
    pub fn for_vm(vm_name: &str) -> Self {
        let h = {
            let mut d = DefaultHasher::new();
            vm_name.hash(&mut d);
            d.finish()
        };
        // 6 hex chars (24 bits) → ~16M distinct names; collision is
        // possible but astronomically unlikely for realistic CI VM
        // name pools (a few hundred concurrent).
        let short = format!("{:06x}", h & 0xff_ffff);
        // /30 subnets between 10.99.0.0/30 and 10.99.255.252/30 give
        // 64 subnets per third-octet block; use a 14-bit hash slice
        // to pick 1 of 16k. Space is huge vs. realistic concurrency.
        let idx = ((h >> 24) & 0x3fff) as u16;
        let (o3, o4_base) = (((idx >> 6) & 0xff) as u8, ((idx & 0x3f) << 2) as u8);
        Self {
            netns: format!("meda-{short}"),
            veth_host: format!("vmh-{short}"),
            veth_netns: format!("vmn-{short}"),
            host_ip: format!("10.99.{o3}.{}", o4_base + 1),
            netns_ip: format!("10.99.{o3}.{}", o4_base + 2),
            subnet_index: idx,
        }
    }

    /// Persist to `<vmdir>/netns.json` so teardown can find the exact
    /// names/IPs we allocated (in case the hash scheme ever changes).
    pub fn save(&self, vm_dir: &Path) -> Result<()> {
        let j = serde_json::to_string(self)?;
        fs::write(vm_dir.join("netns.json"), j)?;
        Ok(())
    }

    /// Load a previously-persisted spec. Falls back to recomputing
    /// from the VM name if the file doesn't exist (e.g. pre-netns VM
    /// dirs that existed before this module shipped).
    pub fn load_or_compute(vm_dir: &Path, vm_name: &str) -> Self {
        fs::read_to_string(vm_dir.join("netns.json"))
            .ok()
            .and_then(|s| serde_json::from_str::<Self>(&s).ok())
            .unwrap_or_else(|| Self::for_vm(vm_name))
    }
}

/// Bring up a freshly-created netns, wire it to the host via a veth
/// pair, and seed it with the tap + iptables rules needed to serve
/// the guest. Idempotent on the tap/iptables pieces; the netns and
/// veth creation steps check `/sys` / `ip netns list` first.
///
/// All sudo'd work is folded into a single `sudo bash -c` so per-VM
/// fork cost is ~1 sudo round-trip, not ~15.
pub fn create(spec: &NetnsSpec, guest_subnet: &str, tap_name: &str) -> Result<()> {
    // Make sure the shared host-wide rules (ip_forward, MASQUERADE
    // for 10.99.0.0/16) exist before we wire this VM. Idempotent +
    // flock-guarded, so concurrent `meda run`s from a clean host
    // converge on a single MASQUERADE entry instead of N duplicates.
    bootstrap_host()?;

    debug!(
        "netns::create {} veth {}/{} tap {} guest {}.0/24",
        spec.netns, spec.veth_host, spec.veth_netns, tap_name, guest_subnet
    );

    let script = format!(
        r#"set -e

NS={netns}
VETH_H={veth_host}
VETH_N={veth_netns}
HOST_IP={host_ip}
NS_IP={netns_ip}
TAP={tap}
SUBNET={subnet}

# --- Netns ---
if ! ip netns list | awk '{{print $1}}' | grep -qx "$NS"; then
  ip netns add "$NS"
fi

# --- Veth pair ---
# Recreate if either end is missing or misconfigured.
if ! ip link show "$VETH_H" >/dev/null 2>&1; then
  # Both ends must come up together.
  ip -n "$NS" link del "$VETH_N" 2>/dev/null || true
  ip link add "$VETH_H" type veth peer name "$VETH_N"
  ip link set "$VETH_N" netns "$NS"
fi

# Host side of veth.
ip addr replace "$HOST_IP/30" dev "$VETH_H"
ip link set "$VETH_H" up

# Netns side of veth + lo.
ip -n "$NS" link set lo up
ip -n "$NS" addr replace "$NS_IP/30" dev "$VETH_N"
ip -n "$NS" link set "$VETH_N" up
# Default route from netns → host side of veth.
ip -n "$NS" route replace default via "$HOST_IP"

# --- Forwarding inside netns ---
ip netns exec "$NS" sysctl -qw net.ipv4.ip_forward=1

# --- Tap inside netns (owns the guest's subnet gateway IP) ---
if ! ip -n "$NS" link show "$TAP" >/dev/null 2>&1; then
  ip -n "$NS" tuntap add "$TAP" mode tap
  ip -n "$NS" addr add "$SUBNET.1/24" dev "$TAP"
  ip -n "$NS" link set "$TAP" up
fi

# --- iptables inside netns ---
# (a) NAT outbound guest traffic to the veth's netns IP, then again
#     to the host's external IP via the host-level MASQUERADE rule.
ip netns exec "$NS" iptables -w -t nat -C POSTROUTING -s "$SUBNET.0/24" ! -d "$SUBNET.0/24" -j MASQUERADE 2>/dev/null \
  || ip netns exec "$NS" iptables -w -t nat -A POSTROUTING -s "$SUBNET.0/24" ! -d "$SUBNET.0/24" -j MASQUERADE
# (b) DNAT incoming connections that target the netns's veth IP
#     to the guest. This is how the host reaches the guest: `ssh
#     cirun@10.99.N.2` → DNAT → 192.168.X.2:22.
ip netns exec "$NS" iptables -w -t nat -C PREROUTING -d "$NS_IP" -j DNAT --to "$SUBNET.2" 2>/dev/null \
  || ip netns exec "$NS" iptables -w -t nat -A PREROUTING -d "$NS_IP" -j DNAT --to "$SUBNET.2"
# (c) FORWARD accept-rules so the netns's FORWARD policy doesn't
#     drop legitimate traffic in/out of the tap.
ip netns exec "$NS" iptables -w -C FORWARD -i "$TAP" -j ACCEPT 2>/dev/null \
  || ip netns exec "$NS" iptables -w -A FORWARD -i "$TAP" -j ACCEPT
ip netns exec "$NS" iptables -w -C FORWARD -o "$TAP" -m conntrack --ctstate RELATED,ESTABLISHED -j ACCEPT 2>/dev/null \
  || ip netns exec "$NS" iptables -w -A FORWARD -o "$TAP" -m conntrack --ctstate RELATED,ESTABLISHED -j ACCEPT

# --- Host-level plumbing — per-veth only ---
# The shared 10.99.0.0/16 MASQUERADE rule is added once at
# bootstrap (see `bootstrap_host`) — adding it here too races
# under concurrent `meda run`s, where N parallel `iptables -C`
# checks all see "missing" and N parallel `-A`s all succeed,
# leaving N duplicate rules. Per-veth FORWARD rules use a
# unique `$VETH_H` so the -C / -A pair is race-free.
iptables -w -C FORWARD -i "$VETH_H" -j ACCEPT 2>/dev/null || iptables -w -A FORWARD -i "$VETH_H" -j ACCEPT
iptables -w -C FORWARD -o "$VETH_H" -j ACCEPT 2>/dev/null || iptables -w -A FORWARD -o "$VETH_H" -j ACCEPT
"#,
        netns = spec.netns,
        veth_host = spec.veth_host,
        veth_netns = spec.veth_netns,
        host_ip = spec.host_ip,
        netns_ip = spec.netns_ip,
        tap = tap_name,
        subnet = guest_subnet,
    );

    run_command("sudo", &["bash", "-c", &script])?;
    Ok(())
}

/// Tear down the netns, veth pair, and per-VM FORWARD rules. Leaves
/// the shared `10.99.0.0/16` MASQUERADE in place — other VMs still
/// need it. Idempotent: every step ignores "doesn't exist" errors.
pub fn destroy(spec: &NetnsSpec) -> Result<()> {
    let script = format!(
        r#"set +e
iptables -w -D FORWARD -i {veth_host} -j ACCEPT 2>/dev/null
iptables -w -D FORWARD -o {veth_host} -j ACCEPT 2>/dev/null
# Deleting the netns destroys anything inside it (tap, iptables,
# veth-netns end, default route, …), so teardown is just these two
# calls.
ip link del {veth_host} 2>/dev/null
ip netns del {netns} 2>/dev/null
exit 0
"#,
        veth_host = spec.veth_host,
        netns = spec.netns,
    );

    run_command("sudo", &["bash", "-c", &script])?;
    Ok(())
}

/// Idempotent host prep called by every `meda run`. Adds the
/// shared `10.99.0.0/16` MASQUERADE rule (so guest outbound NAT'd
/// to the host's external interface) and sets ip_forward.
///
/// Wrapped in flock so concurrent invocations don't race — without
/// it, N parallel `meda run`s would each do the C / A pair
/// in lock-step and end up with N duplicate rules. iptables's `-w`
/// is a kernel xtables lock, not a check-then-add atomicity
/// guarantee, so the userspace flock is the actual safety belt.
pub fn bootstrap_host() -> Result<()> {
    // Lock file lives in /var/run because anyone running meda
    // already has sudo (we use it for ip/iptables); /tmp is
    // world-writable which would let a hostile local user race us.
    let script = r#"set -e
exec 9>/var/run/meda-bootstrap.lock 2>/dev/null \
  || exec 9>/tmp/meda-bootstrap.lock
flock 9
sysctl -qw net.ipv4.ip_forward=1
iptables -w -t nat -C POSTROUTING -s 10.99.0.0/16 ! -d 10.99.0.0/16 -j MASQUERADE 2>/dev/null \
  || iptables -w -t nat -A POSTROUTING -s 10.99.0.0/16 ! -d 10.99.0.0/16 -j MASQUERADE
"#;
    run_command("sudo", &["bash", "-c", script])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_fits_ifname_limit() {
        let spec = NetnsSpec::for_vm("some-reasonably-long-vm-name-1234");
        assert!(
            spec.netns.len() <= 15,
            "netns name too long: {}",
            spec.netns
        );
        assert!(
            spec.veth_host.len() <= 15,
            "veth host name too long: {}",
            spec.veth_host
        );
        assert!(
            spec.veth_netns.len() <= 15,
            "veth netns name too long: {}",
            spec.veth_netns
        );
    }

    #[test]
    fn spec_is_deterministic() {
        let a = NetnsSpec::for_vm("foo");
        let b = NetnsSpec::for_vm("foo");
        assert_eq!(a.netns, b.netns);
        assert_eq!(a.host_ip, b.host_ip);
        assert_eq!(a.netns_ip, b.netns_ip);
    }

    #[test]
    fn ips_are_in_same_slash30() {
        let spec = NetnsSpec::for_vm("foo");
        // /30: last byte changes by 1 between the two IPs.
        let hb: Vec<u8> = spec
            .host_ip
            .split('.')
            .map(|s| s.parse().unwrap())
            .collect();
        let nb: Vec<u8> = spec
            .netns_ip
            .split('.')
            .map(|s| s.parse().unwrap())
            .collect();
        assert_eq!(hb[..3], nb[..3]);
        assert_eq!(nb[3], hb[3] + 1);
    }
}
