use crate::error::{Error, Result};
use indicatif::{ProgressBar, ProgressStyle};
use log::debug;
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

    #[test]
    fn test_run_command_success() {
        let result = run_command("echo", &["hello"]);
        assert!(result.is_ok());
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
