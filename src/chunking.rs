use crate::error::{Error, Result};
use log::info;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};

/// Configuration for file chunking
#[derive(Clone, Debug)]
pub struct ChunkingConfig {
    /// Files smaller than this won't be chunked (100MB)
    pub min_chunk_threshold: u64,
    /// Chunk size for files 100MB-2GB (100MB chunks)
    pub small_chunk_size: u64,
    /// Chunk size for files 2GB-10GB (250MB chunks)
    pub medium_chunk_size: u64,
    /// Chunk size for files >10GB (500MB chunks)
    pub large_chunk_size: u64,
    /// Maximum file size for large files (10GB)
    pub large_file_threshold: u64,
    /// Maximum file size for medium files (2GB)
    pub medium_file_threshold: u64,
    /// ORAS concurrency level for push/pull operations
    pub oras_concurrency: u32,
    /// ORAS push concurrency (defaults to oras_concurrency)
    pub oras_push_concurrency: Option<u32>,
    /// ORAS pull concurrency (defaults to oras_concurrency)
    pub oras_pull_concurrency: Option<u32>,
}

impl Default for ChunkingConfig {
    fn default() -> Self {
        Self {
            min_chunk_threshold: 100 * 1024 * 1024,        // 100MB
            small_chunk_size: 100 * 1024 * 1024,           // 100MB chunks
            medium_chunk_size: 250 * 1024 * 1024,          // 250MB chunks
            large_chunk_size: 500 * 1024 * 1024,           // 500MB chunks
            medium_file_threshold: 2 * 1024 * 1024 * 1024, // 2GB
            large_file_threshold: 10 * 1024 * 1024 * 1024, // 10GB
            oras_concurrency: 10,                          // 10 concurrent transfers by default
            oras_push_concurrency: None,                   // Use oras_concurrency
            oras_pull_concurrency: None,                   // Use oras_concurrency
        }
    }
}

impl ChunkingConfig {
    /// Get the effective push concurrency
    pub fn get_push_concurrency(&self) -> u32 {
        self.oras_push_concurrency.unwrap_or(self.oras_concurrency)
    }

    /// Get the effective pull concurrency
    pub fn get_pull_concurrency(&self) -> u32 {
        self.oras_pull_concurrency.unwrap_or(self.oras_concurrency)
    }
}

/// Metadata about a chunked file
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ChunkMetadata {
    pub original_filename: String,
    pub total_chunks: usize,
    pub chunk_size: u64,
    pub total_size: u64,
    pub sha256: Option<String>,
}

/// Information about a single chunk
#[derive(Debug, Clone)]
pub struct ChunkInfo {
    pub chunk_path: PathBuf,
    pub chunk_index: usize,
    pub chunk_size: u64,
}

/// Main file chunker struct
pub struct FileChunker {
    config: ChunkingConfig,
}

impl FileChunker {
    pub fn new() -> Self {
        Self {
            config: ChunkingConfig::default(),
        }
    }

    pub fn with_config(config: ChunkingConfig) -> Self {
        Self { config }
    }

    /// Determine if a file should be chunked based on its size
    pub fn should_chunk_file(&self, file_path: &Path) -> Result<bool> {
        let size = fs::metadata(file_path)?.len();
        Ok(size >= self.config.min_chunk_threshold)
    }

    /// Determine the appropriate chunk size for a file
    fn get_chunk_size(&self, file_size: u64) -> u64 {
        if file_size >= self.config.large_file_threshold {
            self.config.large_chunk_size
        } else if file_size >= self.config.medium_file_threshold {
            self.config.medium_chunk_size
        } else {
            self.config.small_chunk_size
        }
    }

    /// Split a large file into chunks
    pub fn chunk_file(
        &self,
        file_path: &Path,
        output_dir: &Path,
        json: bool,
    ) -> Result<(ChunkMetadata, Vec<ChunkInfo>)> {
        let file_size = fs::metadata(file_path)?.len();

        if !self.should_chunk_file(file_path)? {
            return Err(Error::Other(format!(
                "File {} is below chunking threshold",
                file_path.display()
            )));
        }

        let chunk_size = self.get_chunk_size(file_size);
        let total_chunks = file_size.div_ceil(chunk_size) as usize;
        let filename = file_path.file_name().unwrap().to_string_lossy();

        if !json {
            info!(
                "🔪 Chunking file '{}' ({:.2} MB) into {} chunks of {:.2} MB each",
                filename,
                file_size as f64 / 1024.0 / 1024.0,
                total_chunks,
                chunk_size as f64 / 1024.0 / 1024.0
            );
        }

        // Create output directory if it doesn't exist
        fs::create_dir_all(output_dir)?;

        let mut source_file = File::open(file_path)?;
        let mut chunks = Vec::new();
        let mut buffer = vec![0u8; chunk_size as usize];

        for chunk_index in 0..total_chunks {
            let chunk_filename = format!("{}.chunk.{:03}", filename, chunk_index);
            let chunk_path = output_dir.join(&chunk_filename);

            // Read chunk data
            let bytes_to_read =
                std::cmp::min(chunk_size, file_size - (chunk_index as u64 * chunk_size));
            buffer.resize(bytes_to_read as usize, 0);

            source_file.read_exact(&mut buffer[..bytes_to_read as usize])?;

            // Write chunk file
            let mut chunk_file = File::create(&chunk_path)?;
            chunk_file.write_all(&buffer[..bytes_to_read as usize])?;
            chunk_file.flush()?;

            chunks.push(ChunkInfo {
                chunk_path,
                chunk_index,
                chunk_size: bytes_to_read,
            });

            if !json {
                info!(
                    "📦 Created chunk {}/{}: {} ({:.2} MB)",
                    chunk_index + 1,
                    total_chunks,
                    chunk_filename,
                    bytes_to_read as f64 / 1024.0 / 1024.0
                );
            }
        }

        let metadata = ChunkMetadata {
            original_filename: filename.to_string(),
            total_chunks,
            chunk_size,
            total_size: file_size,
            sha256: None, // TODO: Calculate SHA256 if needed
        };

        Ok((metadata, chunks))
    }

    /// Reassemble chunks back into the original file
    pub fn reassemble_chunks(
        &self,
        chunks: &[ChunkInfo],
        metadata: &ChunkMetadata,
        output_path: &Path,
        json: bool,
    ) -> Result<()> {
        if !json {
            info!(
                "🔧 Reassembling {} chunks into '{}'",
                chunks.len(),
                output_path.display()
            );
        }

        // Sort chunks by index to ensure correct order
        let mut sorted_chunks: Vec<_> = chunks.iter().collect();
        sorted_chunks.sort_by_key(|chunk| chunk.chunk_index);

        // Verify we have all chunks
        if sorted_chunks.len() != metadata.total_chunks {
            return Err(Error::Other(format!(
                "Missing chunks: expected {}, found {}",
                metadata.total_chunks,
                sorted_chunks.len()
            )));
        }

        // Create output file
        let mut output_file = BufWriter::new(File::create(output_path)?);
        let mut total_written = 0u64;

        for (i, chunk_info) in sorted_chunks.iter().enumerate() {
            if chunk_info.chunk_index != i {
                return Err(Error::Other(format!(
                    "Chunk sequence error: expected index {}, found {}",
                    i, chunk_info.chunk_index
                )));
            }

            if !chunk_info.chunk_path.exists() {
                return Err(Error::Other(format!(
                    "Chunk file not found: {}",
                    chunk_info.chunk_path.display()
                )));
            }

            // Copy chunk data to output file
            let mut chunk_file = File::open(&chunk_info.chunk_path)?;
            let mut buffer = vec![0u8; chunk_info.chunk_size as usize];
            chunk_file.read_exact(&mut buffer)?;

            output_file.write_all(&buffer)?;
            total_written += chunk_info.chunk_size;

            if !json {
                info!(
                    "📝 Wrote chunk {}/{} ({:.2} MB)",
                    i + 1,
                    metadata.total_chunks,
                    chunk_info.chunk_size as f64 / 1024.0 / 1024.0
                );
            }
        }

        output_file.flush()?;

        // Verify total size matches
        if total_written != metadata.total_size {
            return Err(Error::Other(format!(
                "Size mismatch after reassembly: expected {}, got {}",
                metadata.total_size, total_written
            )));
        }

        if !json {
            info!(
                "✅ Successfully reassembled file: {:.2} MB",
                total_written as f64 / 1024.0 / 1024.0
            );
        }

        Ok(())
    }

    /// Detect chunk files in a directory and group them by original file
    pub fn detect_chunks(
        &self,
        scan_dir: &Path,
    ) -> Result<HashMap<String, (ChunkMetadata, Vec<ChunkInfo>)>> {
        let mut chunk_groups: HashMap<String, Vec<ChunkInfo>> = HashMap::new();

        // Scan directory for chunk files
        if !scan_dir.exists() {
            return Ok(HashMap::new());
        }

        for entry in fs::read_dir(scan_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_file() {
                let filename = path.file_name().unwrap().to_string_lossy();

                // Check if this looks like a chunk file: "filename.chunk.XXX"
                if let Some(chunk_info) = self.parse_chunk_filename(&filename, &path)? {
                    chunk_groups
                        .entry(chunk_info.0) // original filename
                        .or_default()
                        .push(chunk_info.1); // chunk info
                }
            }
        }

        // Convert to final format with metadata
        let mut result = HashMap::new();
        for (original_filename, mut chunks) in chunk_groups {
            // Sort chunks by index
            chunks.sort_by_key(|c| c.chunk_index);

            // Calculate metadata
            let total_chunks = chunks.len();
            let total_size = chunks.iter().map(|c| c.chunk_size).sum();
            let chunk_size = if !chunks.is_empty() {
                chunks[0].chunk_size // Use first chunk size as reference
            } else {
                0
            };

            let metadata = ChunkMetadata {
                original_filename: original_filename.clone(),
                total_chunks,
                chunk_size,
                total_size,
                sha256: None,
            };

            result.insert(original_filename, (metadata, chunks));
        }

        Ok(result)
    }

    /// Parse a chunk filename to extract original filename and chunk info
    fn parse_chunk_filename(
        &self,
        filename: &str,
        full_path: &Path,
    ) -> Result<Option<(String, ChunkInfo)>> {
        // Look for pattern: "original_filename.chunk.XXX"
        if let Some(chunk_pos) = filename.rfind(".chunk.") {
            let original_name = filename[..chunk_pos].to_string();
            let chunk_suffix = &filename[chunk_pos + 7..]; // Skip ".chunk."

            if let Ok(chunk_index) = chunk_suffix.parse::<usize>() {
                let chunk_size = fs::metadata(full_path)?.len();

                return Ok(Some((
                    original_name,
                    ChunkInfo {
                        chunk_path: full_path.to_path_buf(),
                        chunk_index,
                        chunk_size,
                    },
                )));
            }
        }

        Ok(None)
    }

    /// Clean up temporary chunk files
    pub fn cleanup_chunks(&self, chunks: &[ChunkInfo]) -> Result<()> {
        for chunk in chunks {
            if chunk.chunk_path.exists() {
                fs::remove_file(&chunk.chunk_path)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_chunking_config_default() {
        let config = ChunkingConfig::default();
        assert_eq!(config.min_chunk_threshold, 100 * 1024 * 1024);
        assert_eq!(config.small_chunk_size, 100 * 1024 * 1024);
        assert_eq!(config.oras_concurrency, 10);
        assert_eq!(config.get_push_concurrency(), 10);
        assert_eq!(config.get_pull_concurrency(), 10);
    }

    #[test]
    fn test_chunking_config_concurrency_overrides() {
        let config = ChunkingConfig {
            oras_push_concurrency: Some(15),
            oras_pull_concurrency: Some(25),
            ..Default::default()
        };

        assert_eq!(config.get_push_concurrency(), 15);
        assert_eq!(config.get_pull_concurrency(), 25);
        assert_eq!(config.oras_concurrency, 10); // Base unchanged
    }

    #[test]
    fn test_should_chunk_file() {
        let temp_dir = TempDir::new().unwrap();
        let chunker = FileChunker::new();

        // Create a small file (below threshold)
        let small_file = temp_dir.path().join("small.txt");
        std::fs::write(&small_file, vec![0u8; 50 * 1024 * 1024]).unwrap(); // 50MB

        // Create a large file (above threshold)
        let large_file = temp_dir.path().join("large.txt");
        std::fs::write(&large_file, vec![0u8; 200 * 1024 * 1024]).unwrap(); // 200MB

        assert!(!chunker.should_chunk_file(&small_file).unwrap());
        assert!(chunker.should_chunk_file(&large_file).unwrap());
    }

    #[test]
    fn test_get_chunk_size() {
        let chunker = FileChunker::new();

        // Small file (1GB)
        assert_eq!(
            chunker.get_chunk_size(1024 * 1024 * 1024),
            100 * 1024 * 1024
        );

        // Medium file (5GB)
        assert_eq!(
            chunker.get_chunk_size(5 * 1024 * 1024 * 1024),
            250 * 1024 * 1024
        );

        // Large file (15GB)
        assert_eq!(
            chunker.get_chunk_size(15 * 1024 * 1024 * 1024),
            500 * 1024 * 1024
        );
    }

    #[test]
    fn test_chunk_and_reassemble() {
        let temp_dir = TempDir::new().unwrap();
        let chunker = FileChunker::new();

        // Create test file with known content
        let test_data = vec![0x42u8; 300 * 1024 * 1024]; // 300MB of 0x42
        let source_file = temp_dir.path().join("test.raw");
        std::fs::write(&source_file, &test_data).unwrap();

        // Chunk the file
        let chunk_dir = temp_dir.path().join("chunks");
        let (metadata, chunks) = chunker.chunk_file(&source_file, &chunk_dir, true).unwrap();

        assert_eq!(metadata.total_chunks, 3); // 300MB / 100MB = 3 chunks
        assert_eq!(chunks.len(), 3);

        // Reassemble
        let reassembled_file = temp_dir.path().join("reassembled.raw");
        chunker
            .reassemble_chunks(&chunks, &metadata, &reassembled_file, true)
            .unwrap();

        // Verify content
        let reassembled_data = std::fs::read(&reassembled_file).unwrap();
        assert_eq!(reassembled_data, test_data);
    }

    #[test]
    fn test_parse_chunk_filename() {
        let temp_dir = TempDir::new().unwrap();
        let chunker = FileChunker::new();

        // Create a test chunk file
        let chunk_file = temp_dir.path().join("base.raw.chunk.001");
        std::fs::write(&chunk_file, b"test data").unwrap();

        let result = chunker
            .parse_chunk_filename("base.raw.chunk.001", &chunk_file)
            .unwrap();
        assert!(result.is_some());

        let (original_name, chunk_info) = result.unwrap();
        assert_eq!(original_name, "base.raw");
        assert_eq!(chunk_info.chunk_index, 1);
        assert_eq!(chunk_info.chunk_size, 9); // "test data".len()
    }

    #[test]
    fn test_detect_chunks() {
        let temp_dir = TempDir::new().unwrap();
        let chunker = FileChunker::new();

        // Create test chunk files
        std::fs::write(temp_dir.path().join("base.raw.chunk.000"), b"chunk0").unwrap();
        std::fs::write(temp_dir.path().join("base.raw.chunk.001"), b"chunk1").unwrap();
        std::fs::write(temp_dir.path().join("base.raw.chunk.002"), b"chunk2").unwrap();

        let detected = chunker.detect_chunks(temp_dir.path()).unwrap();

        assert_eq!(detected.len(), 1);
        let (metadata, chunks) = detected.get("base.raw").unwrap();
        assert_eq!(metadata.total_chunks, 3);
        assert_eq!(chunks.len(), 3);
    }
}
