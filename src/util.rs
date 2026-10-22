use crate::error::{Error, Result};
use indicatif::{ProgressBar, ProgressStyle};
use log::{debug, info};
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub fn run_command(program: &str, args: &[&str]) -> Result<()> {
    debug!("Running command: {} {}", program, args.join(" "));

    let status = Command::new(program)
        .args(args)
        .status()
        .map_err(|e| Error::CommandFailed(format!("{} {}: {}", program, args.join(" "), e)))?;

    if !status.success() {
        return Err(Error::CommandFailed(format!(
            "{} {} failed with exit code: {:?}",
            program,
            args.join(" "),
            status.code()
        )));
    }

    Ok(())
}

pub fn run_command_with_output(program: &str, args: &[&str]) -> Result<Output> {
    debug!(
        "Running command with output: {} {}",
        program,
        args.join(" ")
    );

    Command::new(program)
        .args(args)
        .output()
        .map_err(|e| Error::CommandFailed(format!("{} {}: {}", program, args.join(" "), e)))
}

pub fn run_command_quietly(program: &str, args: &[&str]) -> Result<()> {
    debug!("Running command quietly: {} {}", program, args.join(" "));

    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|e| Error::CommandFailed(format!("{} {}: {}", program, args.join(" "), e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::CommandFailed(format!(
            "{} {} failed with exit code: {:?}\nError output: {}",
            program,
            args.join(" "),
            output.status.code(),
            stderr
        )));
    }

    Ok(())
}

pub async fn download_file(url: &str, dest: &Path) -> Result<()> {
    debug!("Downloading {} to {}", url, dest.display());

    let response = reqwest::get(url).await?;

    if !response.status().is_success() {
        return Err(Error::DownloadFailed(
            url.to_string(),
            format!("HTTP status: {}", response.status()),
        ));
    }

    let total_size = response.content_length();
    // Create progress bar if we know the content length and it's a substantial download
    let pb = if let Some(size) = total_size {
        if size > 1_000_000 {
            // Show progress for files > 1MB
            let progress_bar = ProgressBar::new(size);
            progress_bar.set_style(
                ProgressStyle::default_bar()
                    .template("{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}, {eta})")
                    .unwrap()
                    .progress_chars("#>-")
            );

            // Extract filename from path for display
            let filename = dest.file_name().and_then(|n| n.to_str()).unwrap_or("file");
            progress_bar.set_message(format!("Downloading {}", filename));

            println!(
                "📥 Downloading {} ({:.1} MB)...",
                filename,
                size as f64 / 1_000_000.0
            );

            Some(progress_bar)
        } else {
            None
        }
    } else {
        // No content length available, create a spinner for unknown size downloads
        let filename = dest.file_name().and_then(|n| n.to_str()).unwrap_or("file");

        println!("📥 Downloading {}...", filename);

        let progress_bar = ProgressBar::new_spinner();
        progress_bar.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.green} Downloading... {bytes} ({bytes_per_sec})")
                .unwrap(),
        );
        progress_bar.set_message(format!("Downloading {}", filename));

        Some(progress_bar)
    };

    // Stream the download
    let mut file = fs::File::create(dest)?;
    let mut downloaded = 0u64;
    let mut stream = response.bytes_stream();

    use futures_util::StreamExt;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        file.write_all(&chunk)?;

        downloaded += chunk.len() as u64;
        if let Some(ref pb) = pb {
            pb.set_position(downloaded);
        }
    }

    if let Some(pb) = pb {
        pb.finish_with_message("Download complete");
    }

    Ok(())
}

/// Install the executable at `url` to `dest` unless `dest` already holds the
/// file with SHA-256 `sha256`; a binary with any other hash (e.g. one from an
/// older pin) is replaced. The download lands in a sibling temp file and only
/// replaces `dest` once its hash matches, so a wrong or tampered release
/// never ends up where meda runs it.
pub async fn ensure_pinned_binary(url: &str, sha256: &str, dest: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    if file_sha256(dest).ok().as_deref() == Some(sha256) {
        return Ok(());
    }
    info!("Installing {} from {}", dest.display(), url);
    let tmp = dest.with_extension("download");
    download_file(url, &tmp).await?;
    let actual = file_sha256(&tmp)?;
    if actual != sha256 {
        fs::remove_file(&tmp).ok();
        return Err(Error::DownloadFailed(
            url.to_string(),
            format!("sha256 mismatch: expected {sha256}, got {actual}"),
        ));
    }
    let mut perms = fs::metadata(&tmp)?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&tmp, perms)?;
    fs::rename(&tmp, dest)?;
    Ok(())
}

fn file_sha256(path: &Path) -> Result<String> {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    std::io::copy(&mut fs::File::open(path)?, &mut hasher)?;
    Ok(format!("{:x}", hasher.finalize()))
}

pub fn check_dependency(program: &str) -> Result<()> {
    match Command::new("which").arg(program).output() {
        Ok(output) if output.status.success() => Ok(()),
        _ => Err(Error::DependencyNotFound(program.to_string())),
    }
}

pub fn ensure_dependency(program: &str, package: &str) -> Result<()> {
    if check_dependency(program).is_err() {
        return Err(Error::Other(format!(
            "Required dependency '{}' not found. Please install it using your package manager.\n\
            \n\
            For Debian/Ubuntu: sudo apt install {}\n\
            For Fedora/RHEL:   sudo dnf install {}\n\
            For Arch Linux:    sudo pacman -S {}",
            program, package, package, package
        )));
    }
    Ok(())
}

pub fn check_process_running(pid: u32) -> bool {
    match Command::new("ps").args(["-p", &pid.to_string()]).output() {
        Ok(output) => output.status.success(),
        Err(_) => false,
    }
}

/// Resize a raw disk image to the specified size
///
/// Uses `--shrink` to allow both growing and shrinking the disk image.
/// The `--shrink` flag is required by qemu-img when the target size is smaller
/// than the current size, and is safely ignored when growing.
/// See: https://www.qemu.org/docs/master/tools/qemu-img.html
///
/// # Arguments
/// * `disk_path` - Path to the raw disk image
/// * `size` - Target size (e.g., "25G", "1024M")
pub fn resize_raw_disk(disk_path: &Path, size: &str) -> Result<()> {
    run_command(
        "qemu-img",
        &[
            "resize",
            "-f",
            "raw",
            "--shrink",
            disk_path.to_str().unwrap(),
            size,
        ],
    )?;

    // After resizing the raw file, grow the GPT partition table so the
    // largest Linux partition fills the new disk size. Without this,
    // the kernel sees the old (small) partition and the EXT4 superblock
    // extends beyond it, causing "bad geometry" boot failures.
    crate::gpt::grow_largest_partition(disk_path)
}

/// Create a qcow2 overlay image with a raw backing file.
/// This is instant (no data copy) - the overlay stores only written blocks.
/// If size is None, the overlay inherits the backing file's virtual size.
pub fn create_qcow2_overlay(
    backing_file: &Path,
    overlay_path: &Path,
    size: Option<&str>,
) -> Result<()> {
    create_qcow2_overlay_with_fmt(backing_file, "raw", overlay_path, size)
}

/// Create a qcow2 overlay with an explicit backing format. Use `qcow2`
/// when layering over an existing qcow2 (template → clone). Passing
/// `raw` for a qcow2 backing makes qemu-img mis-interpret the backing's
/// on-disk size as its virtual size, which is how clones ended up with
/// ~60MB virtual size instead of inheriting the template's 10G.
pub fn create_qcow2_overlay_with_fmt(
    backing_file: &Path,
    backing_fmt: &str,
    overlay_path: &Path,
    size: Option<&str>,
) -> Result<()> {
    let mut args = vec![
        "create",
        "-f",
        "qcow2",
        "-b",
        backing_file.to_str().unwrap(),
        "-F",
        backing_fmt,
        overlay_path.to_str().unwrap(),
    ];
    if let Some(s) = size {
        args.push(s);
    }
    // qemu-img prints a "Formatting ..." info line to stdout that
    // pollutes `meda run --json` and breaks jq. Capture it quietly —
    // we surface only a real error (stderr + non-zero exit) if the
    // create itself fails.
    run_command_quietly("qemu-img", &args)
}

pub fn write_string_to_file(path: &Path, content: &str) -> Result<()> {
    fs::write(path, content).map_err(Error::Io)
}

/// Build the cloud-hypervisor `--memory` value for a VM.
///
/// VFIO device passthrough requires the guest RAM to be shared (mmap
/// MAP_SHARED) so the IOMMU can DMA-map it; without `shared=on`,
/// `VFIO_IOMMU_MAP_DMA` fails with EINVAL and the VM never boots. Plain VMs
/// keep the default private mapping.
pub fn memory_flag_value(size: &str, has_devices: bool) -> String {
    if has_devices {
        format!("size={size},shared=on")
    } else {
        format!("size={size}")
    }
}

/// Default guest physical-address width to fall back to when the host IOMMU
/// width cannot be read. 39 bits (512 GiB) is the common Intel client/
/// workstation DMAR width and is ample for any CI guest.
const DEFAULT_IOMMU_WIDTH: u8 = 39;

/// Parse the host IOMMU (DMAR) address width from kernel-log text of the form
/// `DMAR: Host address width 39`. Returns None if absent.
pub fn parse_iommu_width(dmesg: &str) -> Option<u8> {
    for line in dmesg.lines() {
        if let Some(idx) = line.find("Host address width") {
            let tail = &line[idx + "Host address width".len()..];
            if let Some(tok) = tail.split_whitespace().next() {
                if let Ok(w) = tok.trim().parse::<u8>() {
                    return Some(w);
                }
            }
        }
    }
    None
}

/// Detect the host IOMMU address width (falls back to DEFAULT_IOMMU_WIDTH).
///
/// cloud-hypervisor sizes the guest's 64-bit MMIO aperture from the CPU's
/// physical-address width, which on many hosts exceeds the IOMMU's addressable
/// range. When a VFIO device's relocated BAR lands above that range, the DMA
/// map of its MMIO region is rejected with EINVAL. Capping the guest phys
/// width to the IOMMU width keeps the aperture mappable.
pub fn detect_iommu_width() -> u8 {
    if let Ok(out) = std::process::Command::new("dmesg").output() {
        if let Some(w) = parse_iommu_width(&String::from_utf8_lossy(&out.stdout)) {
            return w;
        }
    }
    DEFAULT_IOMMU_WIDTH
}

/// Build the cloud-hypervisor `--cpus` value. With devices attached, the guest
/// physical-address width is capped to the host IOMMU width so passthrough BAR
/// MMIO stays within the IOMMU's mappable range.
pub fn cpus_flag_value<T: std::fmt::Display>(boot: T, has_devices: bool) -> String {
    if has_devices {
        format!("boot={boot},max_phys_bits={}", detect_iommu_width())
    } else {
        format!("boot={boot}")
    }
}

/// Build the cloud-hypervisor `--disk` flag for a VM's rootfs and cloud-init
/// ISO. Every disk names its `image_type`: cloud-hypervisor v54+ refuses to
/// start without it. Shared by the VM and image start-script builders.
pub fn disk_flags(vm_dir: &Path) -> String {
    let d = vm_dir.display();
    format!(
        "--disk path={d}/rootfs.qcow2,image_type=qcow2,backing_files=on path=\"{d}/ci.iso\",image_type=raw"
    )
}

/// Build the cloud-hypervisor `--device path=...` VFIO passthrough section for
/// a VM's devices, or an empty string when there are none. Shared by the VM
/// and image start-script builders so the flag format lives in one place.
pub fn device_flags(devices: &[String]) -> String {
    if devices.is_empty() {
        return String::new();
    }
    let args: Vec<String> = devices
        .iter()
        .map(|d| format!("  --device path={d}"))
        .collect();
    format!(" \\\n{}", args.join(" \\\n"))
}

/// Convert a duration to a human-readable format
pub fn format_duration(duration: Duration) -> String {
    let secs = duration.as_secs();

    if secs < 60 {
        format!("{} seconds ago", secs)
    } else if secs < 3600 {
        let mins = secs / 60;
        if mins == 1 {
            "1 minute ago".to_string()
        } else {
            format!("{} minutes ago", mins)
        }
    } else if secs < 86400 {
        let hours = secs / 3600;
        if hours == 1 {
            "1 hour ago".to_string()
        } else {
            format!("{} hours ago", hours)
        }
    } else {
        let days = secs / 86400;
        if days == 1 {
            "1 day ago".to_string()
        } else {
            format!("{} days ago", days)
        }
    }
}

/// Convert a timestamp to a human-readable format
pub fn format_timestamp(timestamp: u64) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    if timestamp > now {
        "in the future".to_string()
    } else {
        let duration = Duration::from_secs(now - timestamp);
        format_duration(duration)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::NamedTempFile;

    const BIN_BODY: &[u8] = b"pinned binary contents";
    // sha256 of BIN_BODY
    const BIN_SHA256: &str = "7a98613920ee544bb51251b9968cfd12ea5bf9cfde4d4da1a1543ea965e10245";

    /// Serve `body` to a single HTTP request on 127.0.0.1; returns its URL.
    async fn serve_once(body: &'static [u8]) -> String {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut req = [0u8; 1024];
            let _ = sock.read(&mut req).await;
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            sock.write_all(head.as_bytes()).await.unwrap();
            sock.write_all(body).await.unwrap();
        });
        format!("http://{addr}/bin")
    }

    #[tokio::test]
    async fn ensure_pinned_binary_installs_matching_download() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("cloud-hypervisor");
        let url = serve_once(BIN_BODY).await;

        ensure_pinned_binary(&url, BIN_SHA256, &dest).await.unwrap();

        assert_eq!(fs::read(&dest).unwrap(), BIN_BODY);
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&dest).unwrap().permissions().mode() & 0o111,
            0o111
        );
    }

    #[tokio::test]
    async fn ensure_pinned_binary_rejects_download_with_wrong_hash() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("cloud-hypervisor");
        let url = serve_once(b"tampered release").await;

        let err = ensure_pinned_binary(&url, BIN_SHA256, &dest)
            .await
            .unwrap_err();

        assert!(err.to_string().contains("sha256 mismatch"), "{err}");
        assert!(!dest.exists());
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    /// A URL on a port nothing listens on, so any download attempt fails.
    fn dead_url() -> String {
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        format!("http://127.0.0.1:{port}/bin")
    }

    #[tokio::test]
    async fn ensure_pinned_binary_keeps_current_binary_without_downloading() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("cloud-hypervisor");
        fs::write(&dest, BIN_BODY).unwrap();

        ensure_pinned_binary(&dead_url(), BIN_SHA256, &dest)
            .await
            .unwrap();

        assert_eq!(fs::read(&dest).unwrap(), BIN_BODY);
    }

    #[tokio::test]
    async fn ensure_pinned_binary_replaces_binary_from_another_pin() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("cloud-hypervisor");
        fs::write(&dest, b"previously pinned release").unwrap();
        let url = serve_once(BIN_BODY).await;

        ensure_pinned_binary(&url, BIN_SHA256, &dest).await.unwrap();

        assert_eq!(fs::read(&dest).unwrap(), BIN_BODY);
    }

    #[test]
    fn test_run_command_success() {
        let result = run_command("echo", &["hello"]);
        assert!(result.is_ok());
    }

    #[test]
    fn memory_flag_adds_shared_only_with_devices() {
        assert_eq!(memory_flag_value("8G", false), "size=8G");
        assert_eq!(memory_flag_value("8G", true), "size=8G,shared=on");
    }

    #[test]
    fn parses_iommu_width_from_dmesg() {
        let d = "foo\n[ 0.5] DMAR: Host address width 39\nbar";
        assert_eq!(parse_iommu_width(d), Some(39));
        assert_eq!(parse_iommu_width("no dmar line here"), None);
    }

    #[test]
    fn cpus_flag_caps_phys_bits_only_with_devices() {
        assert_eq!(cpus_flag_value(4, false), "boot=4");
        let with = cpus_flag_value(4, true);
        assert!(with.starts_with("boot=4,max_phys_bits="), "got {with}");
    }

    #[test]
    fn disk_flags_declare_image_type_for_every_disk() {
        let flags = disk_flags(Path::new("/vms/a"));
        let disks: Vec<&str> = flags
            .strip_prefix("--disk ")
            .expect(&flags)
            .split(' ')
            .collect();
        assert_eq!(disks.len(), 2, "{flags}");
        assert!(disks[0].contains("/vms/a/rootfs.qcow2"), "{flags}");
        assert!(disks[0].contains("image_type=qcow2"), "{flags}");
        assert!(disks[1].contains("/vms/a/ci.iso"), "{flags}");
        assert!(disks[1].contains("image_type=raw"), "{flags}");
    }

    #[test]
    fn device_flags_empty_or_formatted() {
        assert_eq!(device_flags(&[]), "");
        let f = device_flags(&["/sys/bus/pci/devices/0000:01:00.0".to_string()]);
        assert!(
            f.contains("--device path=/sys/bus/pci/devices/0000:01:00.0"),
            "{f}"
        );
        assert_eq!(
            device_flags(&["/a".to_string(), "/b".to_string()])
                .matches("--device path=")
                .count(),
            2
        );
    }

    #[test]
    fn test_run_command_failure() {
        let result = run_command("false", &[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_run_command_with_output_success() {
        let result = run_command_with_output("echo", &["hello"]);
        assert!(result.is_ok());

        let output = result.unwrap();
        assert_eq!(String::from_utf8(output.stdout).unwrap().trim(), "hello");
    }

    #[test]
    fn test_run_command_with_output_failure() {
        let result = run_command_with_output("false", &[]);
        assert!(result.is_ok()); // Command runs but fails

        let output = result.unwrap();
        assert!(!output.status.success());
    }

    #[test]
    fn test_check_dependency_exists() {
        let result = check_dependency("echo");
        assert!(result.is_ok());
    }

    #[test]
    fn test_check_dependency_missing() {
        let result = check_dependency("nonexistent-command-12345");
        assert!(result.is_err());
    }

    #[test]
    fn test_check_process_running() {
        let current_pid = std::process::id();
        assert!(check_process_running(current_pid));

        assert!(!check_process_running(999999));
    }

    #[test]
    fn test_write_string_to_file() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path();

        let content = "test content";
        let result = write_string_to_file(path, content);
        assert!(result.is_ok());

        let read_content = fs::read_to_string(path).unwrap();
        assert_eq!(read_content, content);
    }
}
