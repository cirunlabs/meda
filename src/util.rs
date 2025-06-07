use crate::error::{Error, Result};
use log::debug;
use std::fs;
use std::path::Path;
use std::process::{Command, Output};
use indicatif::{ProgressBar, ProgressStyle};
use std::io::Write;

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
    debug!("Running command with output: {} {}", program, args.join(" "));
    
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
            format!("HTTP status: {}", response.status())
        ));
    }
    
    let total_size = response.content_length();
    // Create progress bar if we know the content length and it's a substantial download
    let pb = if let Some(size) = total_size {
        if size > 1_000_000 { // Show progress for files > 1MB
            let progress_bar = ProgressBar::new(size);
            progress_bar.set_style(
                ProgressStyle::default_bar()
                    .template("{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}, {eta})")
                    .unwrap()
                    .progress_chars("#>-")
            );
            
            // Extract filename from path for display
            let filename = dest.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("file");
            progress_bar.set_message(format!("Downloading {}", filename));
            
            println!("📥 Downloading {} ({:.1} MB)...", filename, size as f64 / 1_000_000.0);
            
            Some(progress_bar)
        } else {
            None
        }
    } else {
        // No content length available, create a spinner for unknown size downloads
        let filename = dest.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file");
        
        println!("📥 Downloading {}...", filename);
        
        let progress_bar = ProgressBar::new_spinner();
        progress_bar.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.green} Downloading... {bytes} ({bytes_per_sec})")
                .unwrap()
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
