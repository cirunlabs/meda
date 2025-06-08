use crate::error::{Error, Result};
use indicatif::{ProgressBar, ProgressStyle};
use log::debug;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output};

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

pub fn install_dependency(package: &str) -> Result<()> {
    debug!("Installing dependency: {}", package);

    // Update package list
    run_command("sudo apt-get", &["-qq", "update"])?;

    // Install package
    run_command("sudo apt-get", &["-y", "install", package])?;

    Ok(())
}

pub fn ensure_dependency(program: &str, package: &str) -> Result<()> {
    if check_dependency(program).is_err() {
        install_dependency(package)?;
    }
    Ok(())
}

pub fn check_process_running(pid: u32) -> bool {
    match Command::new("ps").args(["-p", &pid.to_string()]).output() {
        Ok(output) => output.status.success(),
        Err(_) => false,
    }
}

pub fn write_string_to_file(path: &Path, content: &str) -> Result<()> {
    fs::write(path, content).map_err(|e| Error::Io(e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::{NamedTempFile, TempDir};

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

    #[tokio::test]
    async fn test_download_file_success() {
        let temp_dir = TempDir::new().unwrap();
        let dest = temp_dir.path().join("test_file");

        let result = download_file("https://httpbin.org/json", &dest).await;
        assert!(result.is_ok());
        assert!(dest.exists());

        let content = fs::read_to_string(&dest).unwrap();
        assert!(content.contains("slideshow"));
    }

    #[tokio::test]
    async fn test_download_file_failure() {
        let temp_dir = TempDir::new().unwrap();
        let dest = temp_dir.path().join("test_file");

        let result = download_file("https://httpbin.org/status/404", &dest).await;
        assert!(result.is_err());
    }
}
