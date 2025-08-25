use crate::chunking::{ChunkInfo, ChunkMetadata, FileChunker};
use crate::config::Config;
use crate::error::{Error, Result};
use crate::util::generate_password_hash;
// Note: download_file will be used when implementing actual registry pulling
use crate::vm;
use log::info;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

pub struct RunOptions<'a> {
    pub vm_name: Option<&'a str>,
    pub registry: Option<&'a str>,
    pub org: Option<&'a str>,
    pub user_data_path: Option<&'a str>,
    pub no_start: bool,
    pub resources: crate::vm::VmResources,
}

#[derive(Serialize)]
pub struct ImageInfo {
    pub name: String,
    pub tag: String,
    pub registry: String,
    pub size: String,
    pub created: String,
}

#[derive(Serialize)]
pub struct ImageResult {
    pub success: bool,
    pub message: String,
}

#[derive(Serialize, Deserialize)]
pub struct ImageManifest {
    pub name: String,
    pub tag: String,
    pub registry: String,
    pub org: String,
    pub artifacts: HashMap<String, String>, // artifact_type -> file_path
    pub metadata: HashMap<String, String>,
    pub created: u64,
}

pub struct ImageRef {
    pub registry: String,
    pub org: String,
    pub name: String,
    pub tag: String,
}

impl ImageRef {
    pub fn parse(image: &str, default_registry: &str, default_org: &str) -> Result<Self> {
        let parts: Vec<&str> = image.split('/').collect();

        let (registry, org, name_tag) = match parts.len() {
            1 => (default_registry, default_org, parts[0]),
            2 => {
                if parts[0].contains('.') || parts[0] == "ghcr.io" {
                    // registry/image:tag
                    (parts[0], default_org, parts[1])
                } else {
                    // org/image:tag
                    (default_registry, parts[0], parts[1])
                }
            }
            3 => (parts[0], parts[1], parts[2]),
            _ => return Err(Error::InvalidImageName(image.to_string())),
        };

        let (name, tag) = if let Some(idx) = name_tag.find(':') {
            (&name_tag[..idx], &name_tag[idx + 1..])
        } else {
            (name_tag, "latest")
        };

        Ok(ImageRef {
            registry: registry.to_string(),
            org: org.to_string(),
            name: name.to_string(),
            tag: tag.to_string(),
        })
    }

    pub fn url(&self) -> String {
        format!("{}/{}/{}:{}", self.registry, self.org, self.name, self.tag)
    }

    pub fn local_dir(&self, config: &Config) -> PathBuf {
        config
            .asset_dir
            .join("images")
            .join(self.registry.replace(".", "_"))
            .join(&self.org)
            .join(&self.name)
            .join(&self.tag)
    }
}

impl ImageManifest {
    pub fn load(image_dir: &Path) -> Result<Self> {
        let manifest_path = image_dir.join("manifest.json");
        if !manifest_path.exists() {
            return Err(Error::ImageNotFound("manifest.json not found".to_string()));
        }

        let content = fs::read_to_string(manifest_path)?;
        let manifest: ImageManifest = serde_json::from_str(&content)?;
        Ok(manifest)
    }

    pub fn save(&self, image_dir: &Path) -> Result<()> {
        fs::create_dir_all(image_dir)?;
        let manifest_path = image_dir.join("manifest.json");
        let content = serde_json::to_string_pretty(self)?;
        fs::write(manifest_path, content)?;
        Ok(())
    }
}

/// Create an image from the current base Ubuntu image + binaries
pub async fn create_base_image(
    config: &Config,
    name: &str,
    tag: &str,
    registry: &str,
    org: &str,
    json: bool,
) -> Result<()> {
    if !json {
        info!("Creating base image: {}/{}:{}:{}", registry, org, name, tag);
    }

    // Ensure we have the base system bootstrapped
    vm::bootstrap(config).await?;

    let image_ref = ImageRef {
        registry: registry.to_string(),
        org: org.to_string(),
        name: name.to_string(),
        tag: tag.to_string(),
    };

    let image_dir = image_ref.local_dir(config);
    fs::create_dir_all(&image_dir)?;

    // Copy base artifacts to image directory
    let mut artifacts = HashMap::new();

    // Copy base raw image
    if config.base_raw.exists() {
        let image_raw = image_dir.join("base.raw");
        fs::copy(&config.base_raw, &image_raw)?;
        artifacts.insert("base_image".to_string(), "base.raw".to_string());
    }

    // Copy firmware
    if config.fw_bin.exists() {
        let fw_copy = image_dir.join("hypervisor-fw");
        fs::copy(&config.fw_bin, &fw_copy)?;
        artifacts.insert("firmware".to_string(), "hypervisor-fw".to_string());
    }

    // Copy cloud-hypervisor binary
    if config.ch_bin.exists() {
        let ch_copy = image_dir.join("cloud-hypervisor");
        fs::copy(&config.ch_bin, &ch_copy)?;
        artifacts.insert("hypervisor".to_string(), "cloud-hypervisor".to_string());
    }

    // Copy ch-remote binary
    if config.cr_bin.exists() {
        let cr_copy = image_dir.join("ch-remote");
        fs::copy(&config.cr_bin, &cr_copy)?;
        artifacts.insert("ch_remote".to_string(), "ch-remote".to_string());
    }

    // Create metadata
    let mut metadata = HashMap::new();
    metadata.insert("os".to_string(), "ubuntu".to_string());
    metadata.insert("arch".to_string(), "amd64".to_string());
    metadata.insert("version".to_string(), "jammy".to_string());
    metadata.insert("created_by".to_string(), "meda".to_string());

    // Create manifest
    let manifest = ImageManifest {
        name: name.to_string(),
        tag: tag.to_string(),
        registry: registry.to_string(),
        org: org.to_string(),
        artifacts,
        metadata,
        created: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    };

    manifest.save(&image_dir)?;

    let message = format!("Successfully created image: {}", image_ref.url());
    if json {
        let result = ImageResult {
            success: true,
            message,
        };
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        info!("{}", message);
    }

    Ok(())
}

/// Pull an image from a registry using ORAS
pub async fn pull(
    config: &Config,
    image: &str,
    registry: Option<&str>,
    org: Option<&str>,
    json: bool,
) -> Result<()> {
    let default_registry = registry.unwrap_or("ghcr.io");
    let default_org = org.unwrap_or("cirunlabs");

    let image_ref = ImageRef::parse(image, default_registry, default_org)?;

    if !json {
        println!("🔧 Using ORAS to pull from registry");
        println!("📥 Pulling image: {}", image_ref.url());
    }

    let image_dir = image_ref.local_dir(config);

    // Check if image already exists locally
    if image_dir.exists() && ImageManifest::load(&image_dir).is_ok() {
        let message = format!("Image {} already exists locally", image_ref.url());
        if json {
            let result = ImageResult {
                success: true,
                message,
            };
            println!("{}", serde_json::to_string_pretty(&result)?);
        } else {
            println!("✅ {}", message);
        }
        return Ok(());
    }

    // Ensure ORAS is available
    let oras_path = ensure_oras_available(config).await?;

    // Create temporary directory for downloaded artifacts
    let temp_dir = std::env::temp_dir().join(format!(
        "meda-pull-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    ));
    fs::create_dir_all(&temp_dir)?;

    let image_ref_str = image_ref.url();

    // Get GitHub token for authentication (optional for public images)
    let github_token = env::var("GITHUB_TOKEN").ok();

    // Use ORAS to pull artifacts to temp directory with enhanced concurrency
    let mut cmd = std::process::Command::new(&oras_path);
    cmd.args([
        "pull",
        &image_ref_str,
        "--output",
        temp_dir.to_str().unwrap(),
        "--allow-path-traversal",
        "--concurrency",
        &config.chunking.get_pull_concurrency().to_string(),
    ]);

    // Set working directory to temp dir to ensure relative downloads
    cmd.current_dir(&temp_dir);

    if !json {
        println!(
            "🔽 ORAS pulling with {}x concurrency to: {}",
            config.chunking.get_pull_concurrency(),
            temp_dir.display()
        );
    }

    // Add authentication if available
    if let Some(ref token) = github_token {
        cmd.args(["--username", "token", "--password", token]);
    }

    // Add progress and performance flags
    if !json {
        cmd.arg("--verbose");
        println!("🔄 Downloading artifacts with ORAS...");

        // Use spawn to show real-time progress
        let mut child = cmd.spawn()?;
        let status = child.wait()?;

        if !status.success() {
            fs::remove_dir_all(&temp_dir).ok();
            return Err(Error::Other("ORAS pull failed".to_string()));
        }
    } else {
        cmd.arg("--no-tty");
        let output = cmd.output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            fs::remove_dir_all(&temp_dir).ok();
            return Err(Error::Other(format!(
                "ORAS pull failed:\nSTDOUT: {}\nSTDERR: {}",
                stdout, stderr
            )));
        }
    }

    // ORAS downloads files to the temp directory, so we need to scan there first
    // If that fails, try scanning the assets images directory as a fallback

    // First try temp directory where ORAS might have downloaded files
    let mut found_artifacts = false;
    if convert_oras_artifacts_to_meda(&temp_dir, &image_dir, &image_ref, json)
        .await
        .is_ok()
    {
        found_artifacts = true;
    } else {
        // ORAS may have restored files to their original absolute paths from push time
        // Look for temporary directories matching the push pattern in /tmp
        if let Ok(tmp_entries) = fs::read_dir("/tmp") {
            for entry in tmp_entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let dir_name = path.file_name().unwrap().to_string_lossy();
                    // Look for directories matching meda-push-chunks-* pattern
                    if dir_name.starts_with("meda-push-chunks-") {
                        if !json {
                            println!("🔍 Found ORAS chunks in temp directory: {}", path.display());
                        }
                        if convert_oras_artifacts_to_meda(&path, &image_dir, &image_ref, json)
                            .await
                            .is_ok()
                        {
                            found_artifacts = true;
                            // Clean up the temp directory after successful processing
                            fs::remove_dir_all(&path).ok();
                            break;
                        }
                    }
                }
            }
        }
    }

    if !found_artifacts {
        // Check if ORAS downloaded directly to the correct tag-based directory structure
        if image_dir.exists() {
            if !json {
                println!(
                    "📁 Found ORAS artifacts in tag directory: {}",
                    image_dir.display()
                );
            }
            // The files are already in the correct location, just create a manifest
            create_manifest_from_tag_directory(&image_dir, &image_ref, json).await?;
            found_artifacts = true;
        } else {
            // ORAS downloads to absolute paths with SHA256 digests, need to find them
            // Check both new and old directory locations for compatibility
            let search_dirs = vec![
                config.asset_dir.join("images"), // New ~/.meda location
                dirs::home_dir()
                    .unwrap_or_default()
                    .join(".ch-vms")
                    .join("assets")
                    .join("images"), // Old location
            ];

            let mut found_source_dir = None;
            for assets_base in search_dirs {
                let registry_dir = assets_base.join(image_ref.registry.replace(".", "_"));
                let org_dir = registry_dir.join(&image_ref.org);

                if !json {
                    println!("🔍 Searching for ORAS downloads in {}", org_dir.display());
                }

                // Look for any directory that contains sha256 (ORAS uses digest-based paths)
                if org_dir.exists() {
                    for entry in fs::read_dir(&org_dir)? {
                        let entry = entry?;
                        let path = entry.path();
                        if path.is_dir() {
                            let dir_name = path.file_name().unwrap().to_string_lossy();
                            if dir_name.contains("@sha256") || dir_name.contains("sha256") {
                                // Found the SHA256 directory, now look for the actual digest subdirectory
                                for subentry in fs::read_dir(&path)? {
                                    let subentry = subentry?;
                                    let subpath = subentry.path();
                                    if subpath.is_dir() {
                                        found_source_dir = Some(subpath);
                                        break;
                                    }
                                }
                                break;
                            }
                        }
                    }
                    if found_source_dir.is_some() {
                        break; // Found artifacts, stop searching
                    }
                }
            }

            if let Some(source_dir) = found_source_dir {
                if !json {
                    println!("📁 Found ORAS artifacts in: {}", source_dir.display());
                }
                // Convert from the SHA256 directory to our tag-based directory
                convert_oras_artifacts_to_meda(&source_dir, &image_dir, &image_ref, json).await?;
                found_artifacts = true;
            } else {
                // No SHA256 directory found, this shouldn't happen with ORAS downloads
                if !json {
                    println!("⚠️  No SHA256 artifact directory found, this may indicate an issue with ORAS download");
                }
                return Err(Error::Other(
                    "ORAS artifacts not found in expected SHA256 directory".to_string(),
                ));
            }
        }
    }

    if !found_artifacts {
        return Err(Error::Other(
            "No artifacts found in any expected location".to_string(),
        ));
    }

    // Clean up temp files
    fs::remove_dir_all(&temp_dir).ok();

    let message = format!("Successfully pulled image {}", image_ref.url());

    if json {
        let result = ImageResult {
            success: true,
            message,
        };
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        println!("✅ {}", message);
    }

    Ok(())
}

/// Push an image to a registry using OCI client
pub async fn push(
    config: &Config,
    name: &str,
    image: &str,
    registry: Option<&str>,
    dry_run: bool,
    json: bool,
) -> Result<()> {
    let default_registry = registry.unwrap_or("ghcr.io");

    // Parse the target image reference
    let target_ref = ImageRef::parse(image, default_registry, "cirunlabs")?;

    if !json {
        info!("Push target: {}", target_ref.url());
        if dry_run {
            info!("Dry run mode - would push to: {}", target_ref.url());
        }
    }

    // Find local image by name
    let images_base_dir = config.asset_dir.join("images");
    let mut found_image = None;

    if images_base_dir.exists() {
        for registry_entry in fs::read_dir(&images_base_dir)? {
            let registry_entry = registry_entry?;
            let registry_path = registry_entry.path();

            if registry_path.is_dir() {
                for org_entry in fs::read_dir(&registry_path)? {
                    let org_entry = org_entry?;
                    let org_path = org_entry.path();

                    if org_path.is_dir() {
                        for name_entry in fs::read_dir(&org_path)? {
                            let name_entry = name_entry?;
                            let name_path = name_entry.path();

                            if name_path.is_dir() && name_path.file_name().unwrap() == name {
                                // Found the image name, now find latest tag or specified tag
                                for tag_entry in fs::read_dir(&name_path)? {
                                    let tag_entry = tag_entry?;
                                    let tag_path = tag_entry.path();

                                    if tag_path.is_dir() {
                                        found_image = Some(tag_path);
                                        break;
                                    }
                                }
                                break;
                            }
                        }
                    }
                }
            }
        }
    }

    let source_dir = found_image
        .ok_or_else(|| Error::ImageNotFound(format!("Local image '{}' not found", name)))?;

    let manifest = ImageManifest::load(&source_dir)?;

    if dry_run {
        let message = format!(
            "Would push image {} (created: {}) to {}",
            name,
            manifest.created,
            target_ref.url()
        );
        if json {
            let result = ImageResult {
                success: true,
                message,
            };
            println!("{}", serde_json::to_string_pretty(&result)?);
        } else {
            info!("{}", message);
        }
        return Ok(());
    }

    // Get GitHub token from environment
    let github_token = env::var("GITHUB_TOKEN").map_err(|_| {
        Error::Other("GITHUB_TOKEN environment variable not set. Please set it with: export GITHUB_TOKEN=your_token".to_string())
    })?;

    if !json {
        info!(
            "Pushing to {} using GitHub token authentication",
            target_ref.url()
        );
    }

    // Push to OCI registry
    match push_to_oci_registry(
        config,
        &source_dir,
        &manifest,
        &target_ref,
        &github_token,
        json,
    )
    .await
    {
        Ok(_) => {
            let message = format!("Successfully pushed image {} to {}", name, target_ref.url());
            if json {
                let result = ImageResult {
                    success: true,
                    message,
                };
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                info!("{}", message);
            }
        }
        Err(e) => {
            let message = format!("Failed to push image {}: {}", name, e);
            if json {
                let result = ImageResult {
                    success: false,
                    message,
                };
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                return Err(e);
            }
        }
    }

    Ok(())
}

/// Push image artifacts to OCI registry using ORAS with chunking support
async fn push_to_oci_registry(
    config: &Config,
    source_dir: &Path,
    manifest: &ImageManifest,
    target_ref: &ImageRef,
    github_token: &str,
    json: bool,
) -> Result<()> {
    if !json {
        println!("🔧 Using ORAS to push to registry with chunking support");
    }

    // Ensure ORAS is available
    let oras_path = ensure_oras_available(config).await?;

    // Target image reference
    let image_ref_str = format!(
        "{}/{}/{}:{}",
        target_ref.registry, target_ref.org, target_ref.name, target_ref.tag
    );

    // Initialize file chunker
    let chunker = FileChunker::with_config(config.chunking.clone());

    // Create temporary directory for chunks
    let temp_dir = std::env::temp_dir().join(format!(
        "meda-push-chunks-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    ));
    fs::create_dir_all(&temp_dir)?;

    // Process artifacts: analyze sizes, create chunks for large files
    let mut files_to_push = Vec::new();
    let mut chunk_metadata = HashMap::new();
    let mut total_size = 0u64;

    if !json {
        println!("🚀 Preparing VM artifacts for {}", image_ref_str);
    }

    for (artifact_type, artifact_file) in &manifest.artifacts {
        let artifact_path = source_dir.join(artifact_file);
        if artifact_path.exists() {
            let size = fs::metadata(&artifact_path)?.len();
            total_size += size;

            if !json {
                println!(
                    "📁 {}: {:.2} MB",
                    artifact_type,
                    size as f64 / 1024.0 / 1024.0
                );
            }

            // Check if file should be chunked
            if chunker.should_chunk_file(&artifact_path)? {
                if !json {
                    println!("🔪 File {} will be chunked", artifact_file);
                }

                // Chunk the file
                let (metadata, chunks) = chunker.chunk_file(&artifact_path, &temp_dir, json)?;

                // Add chunk files to push list with relative paths
                for chunk in &chunks {
                    let relative_path = chunk.chunk_path.strip_prefix(&temp_dir).unwrap();
                    let file_arg = format!(
                        "{}:application/vnd.cirunlabs.meda.{}-chunk.v1",
                        relative_path.to_str().unwrap(),
                        artifact_type.replace("_", "-")
                    );
                    files_to_push.push(file_arg);
                }

                // Store chunk metadata for annotations
                chunk_metadata.insert(artifact_file.clone(), metadata);
            } else {
                // Create symlink in temp directory so it can be pushed with relative path
                let temp_file_path = temp_dir.join(artifact_file);
                if let Some(parent) = temp_file_path.parent() {
                    fs::create_dir_all(parent)?;
                }

                // Remove existing symlink if any
                if temp_file_path.exists() {
                    fs::remove_file(&temp_file_path)?;
                }

                std::os::unix::fs::symlink(&artifact_path, &temp_file_path)?;

                // Add file with relative path
                let file_arg = format!(
                    "{}:application/vnd.cirunlabs.meda.{}.v1",
                    artifact_file,
                    artifact_type.replace("_", "-")
                );
                files_to_push.push(file_arg);
            }
        }
    }

    if !json {
        println!(
            "📊 Total size: {:.2} GB ({} files/chunks to upload)",
            total_size as f64 / 1024.0 / 1024.0 / 1024.0,
            files_to_push.len()
        );
    }

    // Build ORAS push command with all artifacts, chunks, and enhanced concurrency
    let mut cmd = std::process::Command::new(&oras_path);
    cmd.args([
        "push",
        &image_ref_str,
        "--username",
        "token",
        "--password",
        github_token,
        "--artifact-type",
        "application/vnd.cirunlabs.meda.vm.v1",
        "--disable-path-validation",
        "--concurrency",
        &config.chunking.get_push_concurrency().to_string(),
    ]);

    // Set working directory to temp_dir so all file paths are relative
    cmd.current_dir(&temp_dir);

    // Add progress and verbose flags
    if !json {
        cmd.arg("--verbose");
    } else {
        cmd.arg("--no-tty");
    }

    // Add all files (original + chunks)
    for file_arg in &files_to_push {
        cmd.arg(file_arg);
    }

    // Add manifest metadata as annotations
    for (key, value) in &manifest.metadata {
        cmd.args(["--annotation", &format!("meda.metadata.{}={}", key, value)]);
    }

    // Add chunking metadata as annotations
    for filename in chunk_metadata.keys() {
        cmd.args([
            "--annotation",
            &format!("org.cirunlabs.meda.original-filename={}", filename),
        ]);
        cmd.args([
            "--annotation",
            &format!(
                "org.cirunlabs.meda.chunked-files={}",
                chunk_metadata
                    .keys()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(",")
            ),
        ]);
    }

    // Add creation timestamp
    cmd.args([
        "--annotation",
        &format!("meda.created={}", manifest.created),
    ]);
    cmd.args(["--annotation", &format!("meda.name={}", manifest.name)]);
    cmd.args(["--annotation", &format!("meda.tag={}", manifest.tag)]);
    cmd.args([
        "--annotation",
        &format!(
            "org.cirunlabs.meda.upload-time={}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
        ),
    ]);

    if !json {
        println!(
            "🔄 Uploading artifacts with ORAS ({}x concurrency, leveraging concurrent chunk uploads)...",
            config.chunking.get_push_concurrency()
        );

        // Use spawn to show real-time progress
        let mut child = cmd.spawn()?;
        let status = child.wait()?;

        if !status.success() {
            // Clean up temp directory on failure
            fs::remove_dir_all(&temp_dir).ok();
            return Err(Error::Other("ORAS push failed".to_string()));
        }

        println!("✅ Successfully pushed image to registry");
    } else {
        let output = cmd.output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            // Clean up temp directory on failure
            fs::remove_dir_all(&temp_dir).ok();
            return Err(Error::Other(format!(
                "ORAS push failed:\nSTDOUT: {}\nSTDERR: {}",
                stdout, stderr
            )));
        }
    }

    // Clean up temporary chunk files
    fs::remove_dir_all(&temp_dir).ok();

    Ok(())
}

/// Ensure ORAS binary is available, using existing one if present
async fn ensure_oras_available(config: &Config) -> Result<PathBuf> {
    // Bootstrap binaries which will download ORAS if needed
    crate::vm::bootstrap_binaries_only(config).await?;

    // Return the path to the ORAS binary
    Ok(config.oras_bin.clone())
}

/// Convert ORAS downloaded artifacts to Meda image format with chunk reassembly
async fn convert_oras_artifacts_to_meda(
    scan_dir: &Path,
    image_dir: &Path,
    image_ref: &ImageRef,
    json: bool,
) -> Result<()> {
    if !json {
        println!(
            "📦 Converting ORAS artifacts to Meda format with chunk detection from {}",
            scan_dir.display()
        );
    }

    // Create image directory
    fs::create_dir_all(image_dir)?;

    // Check if scan directory exists
    if !scan_dir.exists() {
        return Err(Error::Other(format!(
            "Scan directory does not exist: {}",
            scan_dir.display()
        )));
    }

    // Initialize file chunker for chunk detection
    let chunker = FileChunker::new();

    // First, detect all chunks in the scan directory
    let detected_chunks = chunker.detect_chunks(scan_dir)?;

    if !json && !detected_chunks.is_empty() {
        println!("🔍 Detected {} chunked files", detected_chunks.len());
        for (filename, (metadata, _chunks)) in &detected_chunks {
            println!(
                "📦 {} -> {} chunks ({:.2} MB total)",
                filename,
                metadata.total_chunks,
                metadata.total_size as f64 / 1024.0 / 1024.0
            );
        }
    }

    // Reassemble chunked files
    for (original_filename, (metadata, chunks)) in &detected_chunks {
        let output_path = image_dir.join(original_filename);

        if !json {
            println!("🔧 Reassembling {}", original_filename);
        }

        chunker.reassemble_chunks(chunks, metadata, &output_path, json)?;

        // Clean up chunk files after successful reassembly
        chunker.cleanup_chunks(chunks)?;
    }

    // Scan for regular (non-chunked) files and process them
    let mut artifacts = HashMap::new();
    let mut total_size = 0u64;

    fn scan_directory_for_artifacts(
        dir: &Path,
        artifacts: &mut HashMap<String, String>,
        total_size: &mut u64,
        image_dir: &Path,
        detected_chunks: &HashMap<String, (ChunkMetadata, Vec<ChunkInfo>)>,
        json: bool,
    ) -> Result<()> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_file() {
                let file_name = path.file_name().unwrap().to_string_lossy();

                // Skip chunk files - they've already been processed
                if file_name.contains(".chunk.") {
                    continue;
                }

                let size = fs::metadata(&path)?.len();
                *total_size += size;

                // Try to determine artifact type from file extension or name
                let artifact_type = if file_name.contains("base") || file_name.ends_with(".raw") {
                    "base_image"
                } else if file_name.contains("hypervisor-fw") || file_name.contains("fw") {
                    "firmware"
                } else if file_name.contains("cloud-hypervisor") && !file_name.contains("remote") {
                    "hypervisor"
                } else if file_name.contains("ch-remote") {
                    "ch_remote"
                } else {
                    // Skip manifest files and other non-artifacts
                    if file_name.ends_with(".json") || file_name.starts_with("sha256:") {
                        continue;
                    }
                    // Use filename as artifact type
                    &file_name.replace("-", "_").replace(".", "_")
                };

                // Copy file to image directory with appropriate name
                let dest_file = match artifact_type {
                    "base_image" => "base.raw",
                    "firmware" => "hypervisor-fw",
                    "hypervisor" => "cloud-hypervisor",
                    "ch_remote" => "ch-remote",
                    _ => &file_name,
                };

                let dest_path = image_dir.join(dest_file);

                // Skip if we already processed this artifact type (avoid duplicates)
                // Also skip if this file was reassembled from chunks
                if artifacts.contains_key(artifact_type)
                    || detected_chunks.contains_key(&file_name.to_string())
                {
                    continue;
                }

                fs::copy(&path, &dest_path)?;
                artifacts.insert(artifact_type.to_string(), dest_file.to_string());

                if !json {
                    println!(
                        "📁 Converted artifact: {} → {} ({:.2} MB)",
                        file_name,
                        dest_file,
                        size as f64 / 1024.0 / 1024.0
                    );
                }
            } else if path.is_dir() {
                // Recursively scan subdirectories
                scan_directory_for_artifacts(
                    &path,
                    artifacts,
                    total_size,
                    image_dir,
                    detected_chunks,
                    json,
                )?;
            }
        }
        Ok(())
    }

    scan_directory_for_artifacts(
        scan_dir,
        &mut artifacts,
        &mut total_size,
        image_dir,
        &detected_chunks,
        json,
    )?;

    // Add reassembled files to artifacts
    for (original_filename, (metadata, _)) in &detected_chunks {
        let artifact_type = if original_filename.contains("base")
            || original_filename.ends_with(".raw")
        {
            "base_image"
        } else if original_filename.contains("hypervisor-fw") || original_filename.contains("fw") {
            "firmware"
        } else if original_filename.contains("cloud-hypervisor")
            && !original_filename.contains("remote")
        {
            "hypervisor"
        } else if original_filename.contains("ch-remote") {
            "ch_remote"
        } else {
            &original_filename.replace("-", "_").replace(".", "_")
        };

        let dest_file = match artifact_type {
            "base_image" => "base.raw",
            "firmware" => "hypervisor-fw",
            "hypervisor" => "cloud-hypervisor",
            "ch_remote" => "ch-remote",
            _ => original_filename,
        };

        artifacts.insert(artifact_type.to_string(), dest_file.to_string());
        total_size += metadata.total_size;
    }

    // Check if we found any artifacts
    if artifacts.is_empty() {
        if !json {
            println!(
                "DEBUG: No artifacts found in scan directory: {}",
                scan_dir.display()
            );
            if let Ok(entries) = fs::read_dir(scan_dir) {
                for entry in entries.flatten() {
                    println!("DEBUG: Found in scan dir: {}", entry.path().display());
                }
            }
        }
        return Err(Error::Other(format!(
            "No valid artifacts found in {}",
            scan_dir.display()
        )));
    }

    // Debug: Show what we found
    if !json {
        println!("DEBUG: Scanning directory: {}", scan_dir.display());
        println!(
            "DEBUG: Total artifacts found: {}, total size: {} bytes",
            artifacts.len(),
            total_size
        );
    }

    // Create basic metadata (we'll enhance this when ORAS supports manifest annotations better)
    let mut metadata = HashMap::new();
    metadata.insert("pulled_from".to_string(), image_ref.url());
    metadata.insert(
        "pulled_at".to_string(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            .to_string(),
    );

    // Add chunking metadata if any files were reassembled
    if !detected_chunks.is_empty() {
        let chunked_files: Vec<String> = detected_chunks.keys().cloned().collect();
        metadata.insert("chunked_files".to_string(), chunked_files.join(","));
        metadata.insert("reassembled_from_chunks".to_string(), "true".to_string());
    }

    // Create Meda manifest
    let manifest = ImageManifest {
        name: image_ref.name.clone(),
        tag: image_ref.tag.clone(),
        registry: image_ref.registry.clone(),
        org: image_ref.org.clone(),
        artifacts,
        metadata,
        created: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    };

    // Save manifest
    manifest.save(image_dir)?;

    if !json {
        let chunk_info = if detected_chunks.is_empty() {
            String::new()
        } else {
            format!(" (reassembled {} chunked files)", detected_chunks.len())
        };
        println!(
            "✅ Converted to Meda format ({:.2} MB total){}",
            total_size as f64 / 1024.0 / 1024.0,
            chunk_info
        );
    }

    Ok(())
}

/// Create a manifest from files already in the correct tag-based directory with chunk support
async fn create_manifest_from_tag_directory(
    image_dir: &Path,
    image_ref: &ImageRef,
    json: bool,
) -> Result<()> {
    if !json {
        println!(
            "📝 Creating manifest from tag directory with chunk detection: {}",
            image_dir.display()
        );
    }

    // Initialize file chunker for chunk detection
    let chunker = FileChunker::new();

    // First, detect all chunks in the image directory
    let detected_chunks = chunker.detect_chunks(image_dir)?;

    if !json && !detected_chunks.is_empty() {
        println!(
            "🔍 Detected {} chunked files in tag directory",
            detected_chunks.len()
        );
        for (filename, (metadata, _chunks)) in &detected_chunks {
            println!(
                "📦 {} -> {} chunks ({:.2} MB total)",
                filename,
                metadata.total_chunks,
                metadata.total_size as f64 / 1024.0 / 1024.0
            );
        }
    }

    // Reassemble chunked files
    for (original_filename, (metadata, chunks)) in &detected_chunks {
        let output_path = image_dir.join(original_filename);

        if !json {
            println!("🔧 Reassembling {}", original_filename);
        }

        chunker.reassemble_chunks(chunks, metadata, &output_path, json)?;

        // Clean up chunk files after successful reassembly
        chunker.cleanup_chunks(chunks)?;
    }

    let mut artifacts = HashMap::new();
    let mut total_size = 0u64;

    // Scan the image directory for known artifact files (excluding chunks)
    if let Ok(entries) = fs::read_dir(image_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                let file_name = path.file_name().unwrap().to_string_lossy();

                // Skip chunk files - they've been processed
                if file_name.contains(".chunk.") {
                    continue;
                }

                let size = fs::metadata(&path)?.len();
                total_size += size;

                // Determine artifact type from filename
                let artifact_type = if file_name.contains("base") || file_name.ends_with(".raw") {
                    "base_image"
                } else if file_name.contains("hypervisor-fw") || file_name.contains("fw") {
                    "firmware"
                } else if file_name.contains("cloud-hypervisor") && !file_name.contains("remote") {
                    "hypervisor"
                } else if file_name.contains("ch-remote") {
                    "ch_remote"
                } else if file_name.ends_with(".json") {
                    continue; // Skip manifest files
                } else {
                    // Use filename as artifact type
                    &file_name.replace("-", "_").replace(".", "_")
                };

                artifacts.insert(artifact_type.to_string(), file_name.to_string());

                if !json {
                    println!(
                        "📁 Found artifact: {} → {} ({:.2} MB)",
                        artifact_type,
                        file_name,
                        size as f64 / 1024.0 / 1024.0
                    );
                }
            }
        }
    }

    // Add reassembled files to artifacts (they should already be present from the scan above,
    // but we want to make sure the total_size accounts for reassembled files)
    for (original_filename, (_metadata, _)) in &detected_chunks {
        // The reassembled files should have already been counted in the scan above,
        // but let's make sure the total size is correct
        let artifact_type = if original_filename.contains("base")
            || original_filename.ends_with(".raw")
        {
            "base_image"
        } else if original_filename.contains("hypervisor-fw") || original_filename.contains("fw") {
            "firmware"
        } else if original_filename.contains("cloud-hypervisor")
            && !original_filename.contains("remote")
        {
            "hypervisor"
        } else if original_filename.contains("ch-remote") {
            "ch_remote"
        } else {
            &original_filename.replace("-", "_").replace(".", "_")
        };

        // If the artifact wasn't found in the scan (shouldn't happen), add it
        if !artifacts.contains_key(artifact_type) {
            let dest_file = match artifact_type {
                "base_image" => "base.raw",
                "firmware" => "hypervisor-fw",
                "hypervisor" => "cloud-hypervisor",
                "ch_remote" => "ch-remote",
                _ => original_filename,
            };
            artifacts.insert(artifact_type.to_string(), dest_file.to_string());
        }
    }

    // Create metadata
    let mut metadata = HashMap::new();
    metadata.insert("pulled_from".to_string(), image_ref.url());
    metadata.insert(
        "pulled_at".to_string(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            .to_string(),
    );

    // Add chunking metadata if any files were reassembled
    if !detected_chunks.is_empty() {
        let chunked_files: Vec<String> = detected_chunks.keys().cloned().collect();
        metadata.insert("chunked_files".to_string(), chunked_files.join(","));
        metadata.insert("reassembled_from_chunks".to_string(), "true".to_string());
    }

    // Create Meda manifest
    let manifest = ImageManifest {
        name: image_ref.name.clone(),
        tag: image_ref.tag.clone(),
        registry: image_ref.registry.clone(),
        org: image_ref.org.clone(),
        artifacts,
        metadata,
        created: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    };

    // Save manifest
    manifest.save(image_dir)?;

    if !json {
        let chunk_info = if detected_chunks.is_empty() {
            String::new()
        } else {
            format!(" (reassembled {} chunked files)", detected_chunks.len())
        };
        println!(
            "✅ Created manifest with {} artifacts ({:.2} MB total){}",
            manifest.artifacts.len(),
            total_size as f64 / 1024.0 / 1024.0,
            chunk_info
        );
    }

    Ok(())
}

/// Remove a specific image
pub async fn remove(
    config: &Config,
    image: &str,
    registry: Option<&str>,
    org: Option<&str>,
    force: bool,
    json: bool,
) -> Result<()> {
    let default_registry = registry.unwrap_or("ghcr.io");
    let default_org = org.unwrap_or("cirunlabs");

    let image_ref = ImageRef::parse(image, default_registry, default_org)?;
    let image_dir = image_ref.local_dir(config);

    if !image_dir.exists() {
        let message = format!("Image {} not found locally", image_ref.url());
        if json {
            let result = ImageResult {
                success: false,
                message,
            };
            println!("{}", serde_json::to_string_pretty(&result)?);
        } else {
            return Err(Error::ImageNotFound(message));
        }
        return Ok(());
    }

    // Load manifest to get size info
    let manifest = ImageManifest::load(&image_dir).ok();
    let mut total_size = 0u64;

    if let Some(ref manifest) = manifest {
        for artifact_file in manifest.artifacts.values() {
            let artifact_path = image_dir.join(artifact_file);
            if let Ok(metadata) = fs::metadata(&artifact_path) {
                total_size += metadata.len();
            }
        }
    }

    if !force && !json {
        println!("About to remove image: {}", image_ref.url());
        println!("Size: {:.2} MB", total_size as f64 / 1024.0 / 1024.0);
        print!("Are you sure? [y/N]: ");
        std::io::stdout().flush().ok();

        let mut input = String::new();
        std::io::stdin().read_line(&mut input).ok();
        let input = input.trim().to_lowercase();

        if input != "y" && input != "yes" {
            println!("Cancelled");
            return Ok(());
        }
    }

    // Remove the entire image directory
    fs::remove_dir_all(&image_dir)?;

    let message = format!(
        "Removed image {} ({:.2} MB)",
        image_ref.url(),
        total_size as f64 / 1024.0 / 1024.0
    );

    if json {
        let result = ImageResult {
            success: true,
            message,
        };
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        println!("✅ {}", message);
    }

    Ok(())
}

/// List cached images
pub async fn list(config: &Config, json: bool) -> Result<()> {
    config.ensure_dirs()?;

    let images_dir = config.asset_dir.join("images");

    if !images_dir.exists() {
        if json {
            println!("[]");
        } else {
            info!("No images found");
        }
        return Ok(());
    }

    let mut images = Vec::new();

    // Walk through registry/org/name/tag structure
    for registry_entry in fs::read_dir(&images_dir)? {
        let registry_entry = registry_entry?;
        let registry_path = registry_entry.path();

        if registry_path.is_dir() {
            let registry_name = registry_path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .replace("_", ".");

            for org_entry in fs::read_dir(&registry_path)? {
                let org_entry = org_entry?;
                let org_path = org_entry.path();

                if org_path.is_dir() {
                    for name_entry in fs::read_dir(&org_path)? {
                        let name_entry = name_entry?;
                        let name_path = name_entry.path();

                        if name_path.is_dir() {
                            for tag_entry in fs::read_dir(&name_path)? {
                                let tag_entry = tag_entry?;
                                let tag_path = tag_entry.path();

                                if tag_path.is_dir() {
                                    if let Ok(manifest) = ImageManifest::load(&tag_path) {
                                        // Calculate total size of artifacts
                                        let mut total_size = 0u64;
                                        for artifact_file in manifest.artifacts.values() {
                                            let artifact_path = tag_path.join(artifact_file);
                                            if let Ok(metadata) = fs::metadata(&artifact_path) {
                                                total_size += metadata.len();
                                            }
                                        }

                                        let size = format!(
                                            "{:.2} MB",
                                            total_size as f64 / 1024.0 / 1024.0
                                        );
                                        let created_str =
                                            crate::util::format_timestamp(manifest.created);

                                        images.push(ImageInfo {
                                            name: manifest.name,
                                            tag: manifest.tag,
                                            registry: registry_name.clone(),
                                            size,
                                            created: created_str,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    if json {
        println!("{}", serde_json::to_string_pretty(&images)?);
    } else if images.is_empty() {
        info!("No images found");
    } else {
        println!(
            "{:<20} {:<10} {:<15} {:<12} {:<20}",
            "name", "tag", "registry", "size", "created"
        );
        println!("{}", "-".repeat(85));
        for image in images {
            println!(
                "{:<20} {:<10} {:<15} {:<12} {:<20}",
                image.name, image.tag, image.registry, image.size, image.created
            );
        }
    }

    Ok(())
}

/// Remove unused images
pub async fn prune(config: &Config, all: bool, force: bool, json: bool) -> Result<()> {
    config.ensure_dirs()?;

    let images_dir = config.asset_dir.join("images");

    if !images_dir.exists() {
        let message = "No images directory found".to_string();
        if json {
            let result = ImageResult {
                success: true,
                message,
            };
            println!("{}", serde_json::to_string_pretty(&result)?);
        } else {
            info!("{}", message);
        }
        return Ok(());
    }

    let mut removed_count = 0;
    let mut total_size = 0u64;

    // For now, if --all is specified, remove all images
    // TODO: Implement logic to detect unused images (not referenced by any VM)
    if all {
        if !force && !json {
            info!("Use --force to actually remove all images");
            return Ok(());
        }

        // Remove entire images directory
        if let Ok(_metadata) = fs::metadata(&images_dir) {
            total_size = calculate_directory_size(&images_dir)?;
        }

        fs::remove_dir_all(&images_dir)?;
        removed_count = 1; // Simplified count

        if !json {
            info!("Removed all images");
        }
    }

    let message = format!(
        "Removed {} image(s), freed {:.2} MB",
        removed_count,
        total_size as f64 / 1024.0 / 1024.0
    );

    if json {
        let result = ImageResult {
            success: true,
            message,
        };
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        info!("{}", message);
    }

    Ok(())
}

fn calculate_directory_size(dir: &Path) -> Result<u64> {
    let mut size = 0u64;

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() {
            size += fs::metadata(&path)?.len();
        } else if path.is_dir() {
            size += calculate_directory_size(&path)?;
        }
    }

    Ok(size)
}

/// Create an image from an existing VM
pub async fn create_from_vm(
    config: &Config,
    vm_name: &str,
    image_name: &str,
    tag: &str,
    registry: &str,
    org: &str,
    json: bool,
) -> Result<()> {
    let vm_dir = config.vm_dir(vm_name);
    if !vm_dir.exists() {
        return Err(Error::VmNotFound(vm_name.to_string()));
    }

    let vm_rootfs = vm_dir.join("rootfs.raw");
    if !vm_rootfs.exists() {
        return Err(Error::Other(format!("VM {} rootfs not found", vm_name)));
    }

    // Check if VM is running and stop it if necessary
    if vm::check_vm_running(config, vm_name)? {
        if !json {
            info!("Stopping VM {} before creating image...", vm_name);
        }
        vm::stop(config, vm_name, json).await?;

        // Wait a moment for the VM to fully shut down
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    }

    if !json {
        info!("Creating image from VM: {}", vm_name);
    }

    let image_ref = ImageRef {
        registry: registry.to_string(),
        org: org.to_string(),
        name: image_name.to_string(),
        tag: tag.to_string(),
    };

    let image_dir = image_ref.local_dir(config);
    fs::create_dir_all(&image_dir)?;

    // Copy VM rootfs as base image
    let image_raw = image_dir.join("base.raw");
    fs::copy(&vm_rootfs, &image_raw)?;

    // Note: VM disk is copied as-is to preserve all customizations.
    // Machine-specific data like hostname and network config are handled
    // when creating new VMs from the image.

    let mut artifacts = HashMap::new();
    artifacts.insert("base_image".to_string(), "base.raw".to_string());

    // Copy other VM artifacts if they exist
    if let Ok(entries) = fs::read_dir(&vm_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let file_name = path.file_name().unwrap().to_string_lossy();

            match file_name.as_ref() {
                "user-data" | "meta-data" | "network-config" => {
                    let dest = image_dir.join(&*file_name);
                    fs::copy(&path, &dest)?;
                    artifacts.insert(file_name.to_string(), file_name.to_string());
                }
                _ => {}
            }
        }
    }

    // Create metadata
    let mut metadata = HashMap::new();
    metadata.insert("source_vm".to_string(), vm_name.to_string());
    metadata.insert("created_by".to_string(), "meda".to_string());
    metadata.insert("type".to_string(), "vm_snapshot".to_string());

    let manifest = ImageManifest {
        name: image_name.to_string(),
        tag: tag.to_string(),
        registry: registry.to_string(),
        org: org.to_string(),
        artifacts,
        metadata,
        created: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    };

    manifest.save(&image_dir)?;

    let message = format!(
        "Successfully created image {} from VM {}",
        image_ref.url(),
        vm_name
    );
    if json {
        let result = ImageResult {
            success: true,
            message,
        };
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        info!("{}", message);
    }

    Ok(())
}

/// Run a VM from a local image
pub async fn run_from_image(
    config: &Config,
    image: &str,
    options: RunOptions<'_>,
    json: bool,
) -> Result<()> {
    let default_registry = options.registry.unwrap_or("ghcr.io");
    let default_org = options.org.unwrap_or("cirunlabs");

    let image_ref = ImageRef::parse(image, default_registry, default_org)?;

    if !json {
        info!("🚀 Running VM from image: {}", image_ref.url());
    }

    let image_dir = image_ref.local_dir(config);

    // Check if image exists locally, if not, automatically pull it
    if !image_dir.exists() {
        if !json {
            info!("📥 Image not found locally, pulling: {}", image_ref.url());
        }

        // Attempt to pull the image automatically
        pull(config, image, options.registry, options.org, json).await?;
    }

    // Load image manifest
    let manifest = ImageManifest::load(&image_dir)?;

    // Generate VM name if not provided
    let generated_name = format!(
        "{}-{}",
        image_ref.name,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    );
    let vm_name = options.vm_name.unwrap_or(&generated_name);

    let vm_dir = config.vm_dir(vm_name);

    if vm_dir.exists() {
        return Err(Error::VmAlreadyExists(vm_name.to_string()));
    }

    if !json {
        info!(
            "🔧 Creating VM '{}' from image '{}'",
            vm_name,
            image_ref.url()
        );
    }

    // Bootstrap only the hypervisor binaries (we already have the image)
    vm::bootstrap_binaries_only(config).await?;

    // Create VM directory
    fs::create_dir_all(&vm_dir)?;

    // Copy base image from the cached image
    if let Some(base_image_file) = manifest.artifacts.get("base_image") {
        let source_image = image_dir.join(base_image_file);
        let vm_rootfs = vm_dir.join("rootfs.raw");

        if source_image.exists() {
            if !json {
                info!("📦 Copying base image to VM directory");
            }
            fs::copy(&source_image, &vm_rootfs)?;

            // Resize disk if different from config default
            if options.resources.disk_size != config.disk_size {
                if !json {
                    info!("Resizing disk to {}", options.resources.disk_size);
                }
                crate::util::run_command(
                    "qemu-img",
                    &[
                        "resize",
                        vm_rootfs.to_str().unwrap(),
                        &options.resources.disk_size,
                    ],
                )?;
            }
        } else {
            return Err(Error::Other(format!(
                "Base image artifact '{}' not found in image",
                base_image_file
            )));
        }
    } else {
        return Err(Error::Other(
            "Image manifest missing base_image artifact".to_string(),
        ));
    }

    // Copy user-data from image if it exists, but generate fresh meta-data and network-config
    for (artifact_type, artifact_file) in &manifest.artifacts {
        match artifact_type.as_str() {
            "user-data" => {
                let source = image_dir.join(artifact_file);
                let dest = vm_dir.join(artifact_file);
                if source.exists() {
                    fs::copy(&source, &dest)?;
                }
            }
            // Skip meta-data and network-config - we'll generate fresh ones below
            "meta-data" | "network-config" => {}
            _ => {} // Skip other artifacts like firmware, hypervisor binaries
        }
    }

    // Generate network config with a unique subnet
    let subnet = crate::network::generate_unique_subnet(config).await?;
    // Generate unique TAP device name
    let tap_name = crate::network::generate_unique_tap_name(config, vm_name).await?;

    // Store network config
    crate::util::write_string_to_file(&vm_dir.join("subnet"), &subnet)?;
    crate::util::write_string_to_file(&vm_dir.join("tapdev"), &tap_name)?;

    // Store VM resource configuration
    crate::util::write_string_to_file(&vm_dir.join("memory"), &options.resources.memory)?;
    crate::util::write_string_to_file(&vm_dir.join("cpus"), &options.resources.cpus.to_string())?;
    crate::util::write_string_to_file(&vm_dir.join("disk_size"), &options.resources.disk_size)?;

    // Create or use provided cloud-init files
    if !vm_dir.join("meta-data").exists() {
        let meta_data = format!("instance-id: {}\nlocal-hostname: {}\n", vm_name, vm_name);
        crate::util::write_string_to_file(&vm_dir.join("meta-data"), &meta_data)?;
    }

    // User data - use provided or default
    if let Some(path) = options.user_data_path {
        fs::copy(path, vm_dir.join("user-data"))?;
    } else if !vm_dir.join("user-data").exists() {
        let password_hash = generate_password_hash("meda")?;
        let default_user_data = format!(
            r#"#cloud-config
users:
  - name: meda
    sudo: ALL=(ALL) NOPASSWD:ALL
    passwd: {}
    lock_passwd: false
    inactive: false
    groups: sudo
    shell: /bin/bash
ssh_pwauth: true
"#,
            password_hash
        );
        crate::util::write_string_to_file(&vm_dir.join("user-data"), &default_user_data)?;
    }

    // Generate MAC address
    let mac = crate::network::generate_random_mac();
    crate::util::write_string_to_file(&vm_dir.join("mac"), &mac)?;

    // Create cloud-init ISO
    let ci_dir = vm_dir.join("ci");
    fs::create_dir_all(&ci_dir)?;

    // Copy cloud-init files to ci directory
    for file in ["meta-data", "user-data"] {
        let src = vm_dir.join(file);
        let dst = ci_dir.join(file);
        if src.exists() {
            fs::copy(&src, &dst)?;
        }
    }

    // Add network-config if it doesn't exist
    if !ci_dir.join("network-config").exists() {
        let network_config = format!(
            r#"version: 2
ethernets:
  ens4:
    match:
       macaddress: {}
    addresses: [{}.2/24]
    gateway4: {}.1
    set-name: ens4
    nameservers:
      addresses: [8.8.8.8, 1.1.1.1]
"#,
            mac, subnet, subnet
        );
        crate::util::write_string_to_file(&ci_dir.join("network-config"), &network_config)?;
    }

    // Create cloud-init ISO
    let ci_iso = vm_dir.join("ci.iso");
    if !json {
        info!("Creating cloud-init configuration");
    }
    crate::util::run_command_quietly(
        "genisoimage",
        &[
            "-output",
            ci_iso.to_str().unwrap(),
            "-volid",
            "cidata",
            "-joliet",
            "-rock",
            ci_dir.to_str().unwrap(),
        ],
    )?;

    // Setup networking
    if !json {
        info!("🌐 Setting up host networking");
    }
    crate::network::setup_networking(config, vm_name, &tap_name, &subnet).await?;

    // Create start script
    let start_script = format!(
        r#"#!/bin/bash
cd "{}"
{} \
  --api-socket path={}/api.sock \
  --console off \
  --serial tty \
  --kernel "{}" \
  --cpus boot={} \
  --memory size={} \
  --disk path={}/rootfs.raw path="{}/ci.iso" \
  --net tap={},mac={} \
  --rng src=/dev/urandom \
  > "{}/ch.log" 2>&1 &
echo $! > "{}/pid"

# Check if command started successfully
sleep 2
if ! ps -p $(cat "{}/pid" 2>/dev/null) &>/dev/null; then
  echo "ERROR: Cloud Hypervisor failed to start. Check log: {}/ch.log" >&2
  exit 1
fi
"#,
        vm_dir.display(),
        config.ch_bin.display(),
        vm_dir.display(),
        config.fw_bin.display(),
        options.resources.cpus,
        options.resources.memory,
        vm_dir.display(),
        vm_dir.display(),
        tap_name,
        mac,
        vm_dir.display(),
        vm_dir.display(),
        vm_dir.display(),
        vm_dir.display()
    );

    let start_script_path = vm_dir.join("start.sh");
    crate::util::write_string_to_file(&start_script_path, &start_script)?;

    // Make start script executable
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(&start_script_path)?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&start_script_path, perms)?;

    let message = if options.no_start {
        format!(
            "Successfully created VM '{}' from image '{}' (not started)",
            vm_name,
            image_ref.url()
        )
    } else {
        // Start the VM
        vm::start(config, vm_name, json).await?;
        format!(
            "Successfully created and started VM '{}' from image '{}'",
            vm_name,
            image_ref.url()
        )
    };

    if json {
        let result = crate::vm::VmResult {
            success: true,
            message,
        };
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        info!("✅ {}", message);

        if !options.no_start {
            // Show useful information about the VM
            let ip = crate::vm::get_vm_ip(config, vm_name).unwrap_or_else(|_| "N/A".to_string());
            info!("💡 VM IP address: {}", ip);
            info!("💡 Use 'meda stop {}' to stop the VM", vm_name);
            info!("💡 Use 'meda delete {}' to remove the VM", vm_name);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use tempfile::TempDir;

    #[test]
    fn test_image_ref_parse_simple() {
        let image_ref = ImageRef::parse("ubuntu", "ghcr.io", "cirunlabs").unwrap();
        assert_eq!(image_ref.registry, "ghcr.io");
        assert_eq!(image_ref.org, "cirunlabs");
        assert_eq!(image_ref.name, "ubuntu");
        assert_eq!(image_ref.tag, "latest");
    }

    #[test]
    fn test_image_ref_parse_with_tag() {
        let image_ref = ImageRef::parse("ubuntu:v1.0", "ghcr.io", "cirunlabs").unwrap();
        assert_eq!(image_ref.registry, "ghcr.io");
        assert_eq!(image_ref.org, "cirunlabs");
        assert_eq!(image_ref.name, "ubuntu");
        assert_eq!(image_ref.tag, "v1.0");
    }

    #[test]
    fn test_image_ref_parse_with_org() {
        let image_ref = ImageRef::parse("myorg/ubuntu:v1.0", "ghcr.io", "cirunlabs").unwrap();
        assert_eq!(image_ref.registry, "ghcr.io");
        assert_eq!(image_ref.org, "myorg");
        assert_eq!(image_ref.name, "ubuntu");
        assert_eq!(image_ref.tag, "v1.0");
    }

    #[test]
    fn test_image_ref_parse_with_registry() {
        let image_ref =
            ImageRef::parse("ghcr.io/myorg/ubuntu:v1.0", "registry.com", "defaultorg").unwrap();
        assert_eq!(image_ref.registry, "ghcr.io");
        assert_eq!(image_ref.org, "myorg");
        assert_eq!(image_ref.name, "ubuntu");
        assert_eq!(image_ref.tag, "v1.0");
    }

    #[test]
    fn test_image_ref_parse_registry_detection() {
        let image_ref =
            ImageRef::parse("registry.example.com/ubuntu", "ghcr.io", "cirunlabs").unwrap();
        assert_eq!(image_ref.registry, "registry.example.com");
        assert_eq!(image_ref.org, "cirunlabs");
        assert_eq!(image_ref.name, "ubuntu");
        assert_eq!(image_ref.tag, "latest");
    }

    #[test]
    fn test_image_ref_url() {
        let image_ref = ImageRef {
            registry: "ghcr.io".to_string(),
            org: "cirunlabs".to_string(),
            name: "ubuntu".to_string(),
            tag: "v1.0".to_string(),
        };
        assert_eq!(image_ref.url(), "ghcr.io/cirunlabs/ubuntu:v1.0");
    }

    #[test]
    fn test_image_ref_local_dir() {
        let temp_dir = TempDir::new().unwrap();
        env::set_var("MEDA_ASSET_DIR", temp_dir.path().to_str().unwrap());
        let config = Config::new().unwrap();
        env::remove_var("MEDA_ASSET_DIR");

        let image_ref = ImageRef {
            registry: "ghcr.io".to_string(),
            org: "cirunlabs".to_string(),
            name: "ubuntu".to_string(),
            tag: "v1.0".to_string(),
        };

        let local_dir = image_ref.local_dir(&config);
        assert!(local_dir.to_string_lossy().contains("images"));
        assert!(local_dir.to_string_lossy().contains("ghcr_io"));
        assert!(local_dir.to_string_lossy().contains("cirunlabs"));
        assert!(local_dir.to_string_lossy().contains("ubuntu"));
        assert!(local_dir.to_string_lossy().contains("v1.0"));
    }

    #[test]
    fn test_image_manifest_save_and_load() {
        let temp_dir = TempDir::new().unwrap();

        let mut artifacts = HashMap::new();
        artifacts.insert("base_image".to_string(), "base.raw".to_string());

        let mut metadata = HashMap::new();
        metadata.insert("os".to_string(), "ubuntu".to_string());

        let manifest = ImageManifest {
            name: "test".to_string(),
            tag: "latest".to_string(),
            registry: "ghcr.io".to_string(),
            org: "cirunlabs".to_string(),
            artifacts,
            metadata,
            created: 1234567890,
        };

        // Save manifest
        manifest.save(temp_dir.path()).unwrap();

        // Load manifest
        let loaded = ImageManifest::load(temp_dir.path()).unwrap();
        assert_eq!(loaded.name, "test");
        assert_eq!(loaded.tag, "latest");
        assert_eq!(loaded.registry, "ghcr.io");
        assert_eq!(loaded.org, "cirunlabs");
        assert_eq!(loaded.created, 1234567890);
        assert_eq!(
            loaded.artifacts.get("base_image"),
            Some(&"base.raw".to_string())
        );
        assert_eq!(loaded.metadata.get("os"), Some(&"ubuntu".to_string()));
    }

    #[test]
    fn test_image_manifest_load_missing_file() {
        let temp_dir = TempDir::new().unwrap();
        let result = ImageManifest::load(temp_dir.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_calculate_directory_size() {
        let temp_dir = TempDir::new().unwrap();

        // Create some test files
        std::fs::write(temp_dir.path().join("file1.txt"), "hello").unwrap();
        std::fs::write(temp_dir.path().join("file2.txt"), "world!").unwrap();

        let size = calculate_directory_size(temp_dir.path()).unwrap();
        assert_eq!(size, 11); // "hello" (5) + "world!" (6)
    }

    #[test]
    fn test_calculate_directory_size_with_subdirs() {
        let temp_dir = TempDir::new().unwrap();

        // Create files and subdirectories
        std::fs::write(temp_dir.path().join("file1.txt"), "hello").unwrap();

        let subdir = temp_dir.path().join("subdir");
        std::fs::create_dir(&subdir).unwrap();
        std::fs::write(subdir.join("file2.txt"), "world!").unwrap();

        let size = calculate_directory_size(temp_dir.path()).unwrap();
        assert_eq!(size, 11); // "hello" (5) + "world!" (6)
    }

    #[tokio::test]
    async fn test_list_empty_images_dir() {
        let temp_dir = TempDir::new().unwrap();

        env::set_var("MEDA_ASSET_DIR", temp_dir.path().to_str().unwrap());
        let config = Config::new().unwrap();
        env::remove_var("MEDA_ASSET_DIR");

        // Should not error when images directory doesn't exist
        let result = list(&config, true).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_prune_missing_images_dir() {
        let temp_dir = TempDir::new().unwrap();

        env::set_var("MEDA_ASSET_DIR", temp_dir.path().to_str().unwrap());
        let config = Config::new().unwrap();
        env::remove_var("MEDA_ASSET_DIR");

        // Should not error when images directory doesn't exist
        let result = prune(&config, false, false, true).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_remove_nonexistent_image() {
        let temp_dir = TempDir::new().unwrap();

        env::set_var("MEDA_ASSET_DIR", temp_dir.path().to_str().unwrap());
        let config = Config::new().unwrap();
        env::remove_var("MEDA_ASSET_DIR");

        // Should handle gracefully when image doesn't exist
        let result = remove(&config, "nonexistent", None, None, true, true).await;
        assert!(result.is_ok());
    }
}
