use std::io;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Error, Debug)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("VM {0} already exists")]
    VmAlreadyExists(String),

    #[error("VM {0} does not exist")]
    VmNotFound(String),

    #[error("VM {0} is already running")]
    VmAlreadyRunning(String),

    #[error("VM {0} is not running")]
    VmNotRunning(String),

    #[error("Failed to download {0}: {1}")]
    DownloadFailed(String, String),

    #[error("Failed to execute command: {0}")]
    CommandFailed(String),

    #[error("Network configuration for VM {0} is missing")]
    NetworkConfigMissing(String),

    #[error("Home directory not found")]
    HomeDirNotFound,

    #[error("Failed to parse JSON: {0}")]
    JsonParseFailed(#[from] serde_json::Error),

    #[error("Required dependency {0} not found")]
    DependencyNotFound(String),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("Invalid image name: {0}")]
    InvalidImageName(String),

    #[error("Image not found: {0}")]
    ImageNotFound(String),

    #[error("{0}")]
    Other(String),
}
