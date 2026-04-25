//! Snapshot / restore support — the fast-create path.
//!
//! Philosophy: rather than cold-booting a kernel + cloud-init + waiting for
//! sshd for every `meda create`, boot the template VM once, let it settle,
//! then `ch-remote snapshot` captures its memory + device state. Subsequent
//! VMs skip all of that and `cloud-hypervisor --restore` into the captured
//! state — measured ~500ms from restore to SSH on this host vs ~27s cold.
//!
//! Iteration 1 scope: snapshot-in-place (VM directory holds both its disks
//! and its `snapshot/` subdir), and restore-in-place (start an existing VM
//! from its own snapshot). Multi-VM-from-one-snapshot is iteration 2.

use crate::config::Config;
use crate::error::{Error, Result};
use crate::util::{run_command, run_command_quietly};
use crate::vm;
use log::info;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

/// Directory under a VM dir that holds the ch-remote snapshot artifacts
/// (config.json, state.json, memory-ranges).
const SNAPSHOT_DIR: &str = "snapshot";

fn snapshot_dir(config: &Config, name: &str) -> PathBuf {
    config.vm_dir(name).join(SNAPSHOT_DIR)
}

fn api_sock(config: &Config, name: &str) -> PathBuf {
    config.vm_dir(name).join("api.sock")
}

/// Pause a running VM and snapshot it to `$VMDIR/snapshot/`. The VM keeps
/// running after the pause; the snapshot is effectively a point-in-time
/// copy that will later be restored. Returns an error if the VM is not
/// running (no api.sock) or ch-remote rejects the snapshot.
pub async fn snapshot(config: &Config, name: &str, json: bool) -> Result<()> {
    let vm_dir = config.vm_dir(name);
    if !vm_dir.exists() {
        return Err(Error::VmNotFound(name.to_string()));
    }
    if !vm::check_vm_running(config, name)? {
        return Err(Error::VmNotRunning(name.to_string()));
    }
    let sock = api_sock(config, name);
    if !sock.exists() {
        return Err(Error::Other(format!(
            "api socket missing for VM '{name}' — snapshot requires a ch-remote-controllable VM"
        )));
    }

    let snap_dir = snapshot_dir(config, name);
    if snap_dir.exists() {
        // Wipe a previous snapshot — ch-remote refuses to write into a
        // non-empty destination, and the caller clearly wants the fresh one.
        fs::remove_dir_all(&snap_dir)?;
    }
    fs::create_dir_all(&snap_dir)?;

    info!("pausing VM {} for snapshot", name);
    run_command(
        &config.cr_bin.to_string_lossy(),
        &["--api-socket", sock.to_str().unwrap(), "pause"],
    )?;

    info!("writing snapshot to {}", snap_dir.display());
    let url = format!("file://{}", snap_dir.display());
    // Resume-on-failure: if ch-remote snapshot errors out we shouldn't
    // leave the VM paused — that would look like a hang to the caller.
    let snap_result = run_command(
        &config.cr_bin.to_string_lossy(),
        &["--api-socket", sock.to_str().unwrap(), "snapshot", &url],
    );
    let resume_result = run_command_quietly(
        &config.cr_bin.to_string_lossy(),
        &["--api-socket", sock.to_str().unwrap(), "resume"],
    );
    snap_result?;
    resume_result?;

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "vm": name,
                "snapshot_dir": snap_dir,
                "size_bytes": dir_size(&snap_dir).unwrap_or(0),
            }))?
        );
    } else {
        info!("snapshot written to {}", snap_dir.display());
    }
    Ok(())
}

/// Restore-in-place: the VM must already exist on disk (snapshot taken
/// previously via `meda snapshot`) and must NOT currently be running.
/// Starts cloud-hypervisor with `--restore` inside the VM's per-VM
/// netns, waits for the api socket, and fires `ch-remote resume`
/// asynchronously. Returns ~120 ms later — sshd-ready follows in
/// 1-3 s once CH finishes paging in the snapshot's memory.
pub async fn restore(config: &Config, name: &str, json: bool) -> Result<()> {
    let vm_dir = config.vm_dir(name);
    if !vm_dir.exists() {
        return Err(Error::VmNotFound(name.to_string()));
    }
    let snap_dir = snapshot_dir(config, name);
    if !snap_dir.exists() {
        return Err(Error::Other(format!(
            "no snapshot for VM '{name}'. Run `meda snapshot {name}` first"
        )));
    }
    if vm::check_vm_running(config, name)? {
        return Err(Error::VmAlreadyRunning(name.to_string()));
    }

    // Per-VM network namespace. Everything — tap, iptables, the CH
    // process itself — lives inside `meda-<hash>` so N concurrent
    // clones of the same template don't collide on the template's
    // static guest IP. Host reaches the guest via the veth pair's
    // netns-side IP (see `src/netns.rs`).
    let _t0 = std::time::Instant::now();
    let subnet = fs::read_to_string(vm_dir.join("subnet"))?;
    let tap_name = fs::read_to_string(vm_dir.join("tapdev"))?;
    let subnet = subnet.trim();
    let tap_name = tap_name.trim();
    let netns_spec = crate::netns::NetnsSpec::load_or_compute(&vm_dir, name);
    netns_spec.save(&vm_dir)?;
    let t_prep = _t0.elapsed();
    crate::netns::create(&netns_spec, subnet, tap_name)?;
    let t_netns = _t0.elapsed();

    let sock = api_sock(config, name);
    // Stale sockets from a crashed prior run confuse ch-remote: it connects
    // to a nonexistent server. Unlink before starting CH.
    let _ = fs::remove_file(&sock);

    let ch_log = vm_dir.join("ch.log");
    let restore_url = format!("file://{}", snap_dir.display());

    info!(
        "restoring VM {} from {} (prep={}ms, netns_create={}ms)",
        name,
        snap_dir.display(),
        t_prep.as_millis(),
        (t_netns - t_prep).as_millis()
    );
    // Run CH inside the per-VM netns. `sudo ip netns exec` wraps a
    // single CH invocation. The child runs as root because entering
    // a netns needs CAP_SYS_ADMIN; that's fine because CH already
    // needs /dev/kvm + tap FD access.
    let mut child = Command::new("sudo")
        .args([
            "ip",
            "netns",
            "exec",
            &netns_spec.netns,
            config.ch_bin.to_str().unwrap(),
            "--api-socket",
            &format!("path={}", sock.display()),
            "--restore",
            // prefault=off: pages are mapped lazily on first guest
            // access instead of being forced into RAM at restore
            // time. Critical for concurrent launches: with 50 VMs
            // booting in parallel, prefault=on would spike to
            // `n_vms * guest_mem_size` of anon-page allocation
            // simultaneously (guaranteed OOM on modest hosts). With
            // prefault off, each VM only pages in what it actually
            // touches (~100-200 MB for an idle runner), so host
            // RSS scales with the *working set*, not the provisioned
            // guest RAM. Single-VM SSH latency is marginally higher
            // on the first access, but it's well under a second.
            &format!("source_url={restore_url}"),
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::from(std::fs::File::create(&ch_log)?))
        .stderr(Stdio::from(std::fs::File::create(vm_dir.join("ch.err"))?))
        .spawn()
        .map_err(|e| Error::CommandFailed(format!("spawn cloud-hypervisor --restore: {e}")))?;

    // `child.id()` here is sudo's pid; CH is sudo's direct child.
    // Linux signals propagate from sudo → CH via sudo's default
    // forwarding behaviour, so killing sudo cleans up CH too.
    fs::write(vm_dir.join("pid"), child.id().to_string())?;

    let t_spawn = _t0.elapsed();
    // Wait for the api socket — comes up in a few ms when CH has loaded
    // enough of the snapshot to serve control requests. 5s ceiling is
    // generous (observed ~3ms on this host).
    let sock_deadline = std::time::Instant::now() + Duration::from_secs(5);
    while !sock.exists() {
        if std::time::Instant::now() > sock_deadline {
            // Collect whatever CH emitted so the user isn't left guessing.
            let log_tail = fs::read_to_string(&ch_log).unwrap_or_default();
            let _ = child.kill();
            return Err(Error::Other(format!(
                "api socket did not appear within 5s — restore likely failed.\nCH log:\n{log_tail}"
            )));
        }
        thread::sleep(Duration::from_millis(1));
    }
    let t_sock = _t0.elapsed();

    // CH ran under `sudo ip netns exec`, so the API socket is owned
    // by root. Relax the perms so ch-remote (and `meda get`) can
    // talk to it from the unprivileged user.
    let _ = run_command("sudo", &["chmod", "0666", sock.to_str().unwrap()]);
    let t_chmod = _t0.elapsed();

    // Resume the VM — CH loads the snapshot paused, and the actual
    // heavy lifting (VCPU resume, memory page-in) happens synchronously
    // inside CH while it services this HTTP PUT. Measured ~370ms on
    // this host for a 512 MiB guest.
    //
    // That's the single biggest chunk of `meda run` latency, and the
    // caller's next action is always to poll sshd anyway (which can't
    // answer until resume has completed), so blocking on ch-remote
    // here just moves wall-clock from our process to theirs. Fire it
    // and detach: our process returns once the resume request is
    // *sent*, not once CH has finished processing it. The user-visible
    // path (ssh to the returned IP) works the instant CH is done.
    let resume_sock = sock.clone();
    let cr_bin = config.cr_bin.clone();
    Command::new(cr_bin)
        .args(["--api-socket", resume_sock.to_str().unwrap(), "resume"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| Error::CommandFailed(format!("spawn ch-remote resume: {e}")))?;
    let t_resume = _t0.elapsed();
    info!(
        "restore phases: spawn={}ms api_sock={}ms chmod={}ms resume_fire={}ms TOTAL={}ms",
        (t_spawn - t_netns).as_millis(),
        (t_sock - t_spawn).as_millis(),
        (t_chmod - t_sock).as_millis(),
        (t_resume - t_chmod).as_millis(),
        t_resume.as_millis()
    );

    if json {
        let out = serde_json::json!({
            "vm": name,
            "restored_from": snap_dir,
            "host": netns_spec.netns_ip,
            "ssh": format!("cirun@{}", netns_spec.netns_ip),
        });
        println!("{}", serde_json::to_string_pretty(&out)?);
    } else {
        info!("restored {} from snapshot", name);
    }

    Ok(())
}

/// Clone a snapshotted VM into a new VM name so the caller can fast-restore
/// a *separate* VM from the template. This is the "create VM from template"
/// path: takes ~100ms of bookkeeping (no cold boot, no cloud-init) and
/// leaves the clone ready to `meda restore <new>`.
///
/// Iteration-1 constraint: the clone inherits the template's in-guest
/// identity (IP, MAC, hostname). Two clones of the same template cannot
/// run *simultaneously* — their guests would both claim 192.168.X.2 and
/// their tap devices would collide on the same subnet. Sequentially,
/// however, any number of clones work: stop one, restore another.
/// Per-clone identity (new MAC + in-guest IP rewrite) is iter-6+ work.
pub async fn clone_template(
    config: &Config,
    template: &str,
    new_name: &str,
    json: bool,
) -> Result<()> {
    let src = config.vm_dir(template);
    let dst = config.vm_dir(new_name);
    if !src.exists() {
        return Err(Error::VmNotFound(template.to_string()));
    }
    if !src.join(SNAPSHOT_DIR).join("config.json").exists() {
        return Err(Error::Other(format!(
            "{template} has no snapshot — run `meda snapshot {template}` first"
        )));
    }
    if dst.exists() {
        return Err(Error::VmAlreadyExists(new_name.to_string()));
    }

    fs::create_dir_all(&dst)?;

    // qcow2 overlay on top of the template's rootfs. The overlay is tiny
    // (~200KB) and writes stay local to this clone — the template's disk
    // is the immutable backing. Backing format is explicitly qcow2
    // because the template's rootfs IS a qcow2 (itself an overlay over
    // base.raw). Passing raw here would make qemu-img read the on-disk
    // size as the virtual size and give the clone a ~60MB disk.
    let src_rootfs = src.join("rootfs.qcow2");
    let dst_rootfs = dst.join("rootfs.qcow2");
    crate::util::create_qcow2_overlay_with_fmt(&src_rootfs, "qcow2", &dst_rootfs, None)?;

    // Cloud-init ISO — reuse the template's so cloud-init sees identical
    // metadata. Copying keeps the clone self-contained (template can be
    // deleted later without breaking this clone's restore).
    if src.join("ci.iso").exists() {
        fs::copy(src.join("ci.iso"), dst.join("ci.iso"))?;
    }

    // Network / resource metadata — plain text files the VM re-reads on
    // stop/start. Subnet, mac, memory, cpus, disk_size stay verbatim so
    // the guest's in-memory TCP/IP state (captured in the snapshot)
    // remains valid. `tapdev` is overwritten below with a per-clone tap
    // name so multiple clones can run side-by-side without fighting
    // over the same kernel interface.
    for f in [
        "subnet",
        "mac",
        "memory",
        "cpus",
        "disk_size",
        "meta-data",
        "user-data",
        "start.sh",
        "devices",
    ] {
        let s = src.join(f);
        if s.exists() {
            fs::copy(&s, dst.join(f))?;
        }
    }
    let template_tap = fs::read_to_string(src.join("tapdev"))?.trim().to_string();
    let clone_tap = unique_tap_name(new_name);
    fs::write(dst.join("tapdev"), &clone_tap)?;

    // Snapshot files — rewrite disk paths + tap name in config.json so
    // CH reads the clone's disks and opens the clone's tap instead of
    // the template's. All other snapshot state (memory image, device
    // registers, MAC address) is identical — that's what makes the
    // restore cheap.
    let src_snap = src.join(SNAPSHOT_DIR);
    let dst_snap = dst.join(SNAPSHOT_DIR);
    fs::create_dir_all(&dst_snap)?;
    // state.json is ~70KB — copy verbatim. memory-ranges is the big one
    // (~=guest RAM, e.g. 512MB). CH mmaps it MAP_PRIVATE on restore, so
    // the guest writes land in per-VM anonymous pages instead of the
    // backing file. Hardlinking template→clone therefore shares storage
    // with zero risk of cross-VM contamination, and cuts ~400ms off the
    // clone cost on ext4 (no reflink support on this filesystem).
    let src_state = src_snap.join("state.json");
    if src_state.exists() {
        fs::copy(&src_state, dst_snap.join("state.json"))?;
    }
    let src_mem = src_snap.join("memory-ranges");
    let dst_mem = dst_snap.join("memory-ranges");
    if src_mem.exists() {
        // Hardlink fails if dst already exists (shouldn't — we just
        // created dst_snap) and silently falls back to a copy on
        // cross-device or unsupported-fs errors.
        match fs::hard_link(&src_mem, &dst_mem) {
            Ok(()) => {}
            Err(e) => {
                log::warn!("hardlink of memory-ranges failed ({e}); falling back to copy");
                fs::copy(&src_mem, &dst_mem)?;
            }
        }
    }
    rewrite_config(
        &src_snap.join("config.json"),
        &dst_snap.join("config.json"),
        &src,
        &dst,
        &template_tap,
        &clone_tap,
    )?;

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "template": template,
                "clone": new_name,
                "dir": dst,
            }))?
        );
    } else {
        info!("cloned {} → {}", template, new_name);
    }
    Ok(())
}

/// Replace references to `src_prefix` with `dst_prefix` (the per-VM
/// directory path — which bubbles up in every disk.path) AND substitute
/// the template's tap name with a fresh per-clone one.
fn rewrite_config(
    src: &Path,
    dst: &Path,
    src_prefix: &Path,
    dst_prefix: &Path,
    template_tap: &str,
    clone_tap: &str,
) -> Result<()> {
    let body = fs::read_to_string(src)?;
    let body = body.replace(
        src_prefix.to_string_lossy().as_ref(),
        dst_prefix.to_string_lossy().as_ref(),
    );
    // Match with quotes so we never accidentally rewrite a substring of
    // some other JSON field (tap names are short hex and could occur
    // elsewhere as part of a hash or MAC).
    let body = body.replace(
        &format!("\"tap\":\"{template_tap}\""),
        &format!("\"tap\":\"{clone_tap}\""),
    );
    fs::write(dst, body)?;
    Ok(())
}

/// Generate a unique tap device name for a clone. Linux caps interface
/// names at 15 chars; `tap-` + 8 hex (total 12) leaves headroom and is
/// deterministic per clone name (memorable across restores).
fn unique_tap_name(vm_name: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    vm_name.hash(&mut h);
    format!("tap-{:08x}", (h.finish() & 0xffff_ffff) as u32)
}

/// List VMs that carry a snapshot — i.e. are ready to be fast-restored.
/// A "template" in this context is any VM directory containing
/// `snapshot/config.json`; no separate template registry exists.
pub fn templates(config: &Config, json: bool) -> Result<()> {
    let mut rows: Vec<(String, u64, bool)> = Vec::new();
    if let Ok(entries) = fs::read_dir(&config.vm_root) {
        for entry in entries.flatten() {
            let vm_dir = entry.path();
            if !vm_dir.is_dir() {
                continue;
            }
            if !vm_dir.join(SNAPSHOT_DIR).join("config.json").exists() {
                continue;
            }
            let Some(name) = vm_dir.file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            let size = dir_size(&vm_dir.join(SNAPSHOT_DIR)).unwrap_or(0);
            let running = vm::check_vm_running(config, name).unwrap_or(false);
            rows.push((name.to_string(), size, running));
        }
    }
    rows.sort_by(|a, b| a.0.cmp(&b.0));

    if json {
        let out: Vec<_> = rows
            .iter()
            .map(|(name, size, running)| {
                serde_json::json!({
                    "name": name,
                    "snapshot_bytes": size,
                    "running": running,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&out)?);
    } else if rows.is_empty() {
        println!("(no templates — run `meda snapshot <vm>` on a running VM first)");
    } else {
        let header_name = "name";
        let header_size = "snap-size";
        let header_state = "state";
        println!("{header_name:<40} {header_size:>10}  {header_state}");
        println!("{}", "-".repeat(60));
        for (name, size, running) in rows {
            let hr = if size >= 1 << 30 {
                format!("{:.1}G", size as f64 / (1u64 << 30) as f64)
            } else if size >= 1 << 20 {
                format!("{:.0}M", size as f64 / (1u64 << 20) as f64)
            } else {
                format!("{}B", size)
            };
            println!(
                "{:<40} {:>10}  {}",
                name,
                hr,
                if running { "running" } else { "stopped" }
            );
        }
    }
    Ok(())
}

fn dir_size(path: &Path) -> std::io::Result<u64> {
    let mut total = 0u64;
    if path.is_dir() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let md = entry.metadata()?;
            total += if md.is_dir() {
                dir_size(&entry.path()).unwrap_or(0)
            } else {
                md.len()
            };
        }
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dir_size_empty() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(dir_size(tmp.path()).unwrap(), 0);
    }

    #[test]
    fn dir_size_counts_files() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("a"), b"hello").unwrap();
        fs::write(tmp.path().join("b"), b"world!").unwrap();
        assert_eq!(dir_size(tmp.path()).unwrap(), 5 + 6);
    }
}
