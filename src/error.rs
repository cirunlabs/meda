use thiserror::Error;
use std::io;
use std::path::PathBuf;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Error, Debug)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    
    #[error("VM {0} already exists")]
    VmAlreadyExists(String),
    
    #[error("VM {0} does not exist")]
    VmNotFound(String),
    
    #[error("VM {0} is not running")]
    VmNotRunning(String),
    
    #[error("VM {0} is already running")]
    VmAlreadyRunning(String),
    
    #[error("Failed to download {0}: {1}")]
    DownloadFailed(String, String),
    
    #[error("Failed to execute command: {0}")]
    CommandFailed(String),
    
    #[error("Network configuration for VM {0} is missing")]
    NetworkConfigMissing(String),
    
    #[error("Failed to set up networking: {0}")]
    NetworkSetupFailed(String),
    
    #[error("Failed to start VM: {0}")]
    VmStartFailed(String),
    
    #[error("Home directory not found")]
    HomeDirNotFound,
    
    #[error("Failed to parse JSON: {0}")]
    JsonParseFailed(#[from] serde_json::Error),
    
    #[error("Required dependency {0} not found")]
    DependencyNotFound(String),
    
    #[error("Failed to convert image: {0}")]
    ImageConversionFailed(String),
    
    #[error("Failed to create cloud-init ISO: {0}")]
    CloudInitFailed(String),
    
    #[error("File not found: {0}")]
    FileNotFound(PathBuf),
    
    #[error("HTTP error: {0}")]
    HttpError(#[from] reqwest::Error),
    
    #[error("{0}")]
    Other(String),
}
