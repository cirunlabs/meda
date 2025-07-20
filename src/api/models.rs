use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Request to create a new VM
#[derive(Debug, Deserialize, ToSchema)]
pub struct VmCreateRequest {
    /// Name of the VM
    pub name: String,
    /// Path to user-data file (optional)
    pub user_data: Option<String>,
    /// Force create (delete if exists)
    #[serde(default)]
    pub force: bool,
    /// Memory size (e.g., 1G, 2048M, 512M)
    pub memory: Option<String>,
    /// Number of CPUs
    pub cpus: Option<u8>,
    /// Disk size (e.g., 10G, 20G, 5120M)
    pub disk: Option<String>,
}

/// VM response information
#[derive(Debug, Serialize, ToSchema)]
pub struct VmResponse {
    /// Success status
    pub success: bool,
    /// Response message
    pub message: String,
    /// VM details (if applicable)
    pub vm: Option<VmInfo>,
}

/// VM information
#[derive(Debug, Serialize, ToSchema)]
pub struct VmInfo {
    /// VM name
    pub name: String,
    /// VM state (running, stopped)
    pub state: String,
    /// VM IP address
    pub ip: String,
    /// Memory allocation
    pub memory: String,
    /// Disk size
    pub disk: String,
    /// Creation time
    pub created: String,
}

/// VM list response
#[derive(Debug, Serialize, ToSchema)]
pub struct VmListResponse {
    /// List of VMs
    pub vms: Vec<VmInfo>,
    /// Total count
    pub count: usize,
}

/// Detailed VM information
#[derive(Debug, Serialize, ToSchema)]
pub struct VmDetailResponse {
    /// VM name
    pub name: String,
    /// VM state
    pub state: String,
    /// VM IP address (optional)
    pub ip: Option<String>,
    /// Additional VM details
    pub details: Option<serde_json::Value>,
}

/// Port forwarding request
#[derive(Debug, Deserialize, ToSchema)]
pub struct PortForwardRequest {
    /// Host port
    pub host_port: u16,
    /// Guest port
    pub guest_port: u16,
}

/// Image list response
#[derive(Debug, Serialize, ToSchema)]
pub struct ImageListResponse {
    /// List of images
    pub images: Vec<ImageInfo>,
    /// Total count
    pub count: usize,
}

/// Image information
#[derive(Debug, Serialize, ToSchema)]
pub struct ImageInfo {
    /// Image name
    pub name: String,
    /// Image tag
    pub tag: String,
    /// Registry
    pub registry: String,
    /// Image size
    pub size: String,
    /// Creation timestamp
    pub created: String,
}

/// Request to create a new image
#[derive(Debug, Deserialize, ToSchema)]
pub struct ImageCreateRequest {
    /// Image name
    pub name: String,
    /// Image tag (default: latest)
    #[serde(default = "default_tag")]
    pub tag: String,
    /// Registry URL (optional)
    pub registry: Option<String>,
    /// Organization/namespace (optional)
    pub org: Option<String>,
    /// Create from existing VM instead of base image
    pub from_vm: Option<String>,
}

/// Request to pull an image
#[derive(Debug, Deserialize, ToSchema)]
pub struct ImagePullRequest {
    /// Image name with optional tag
    pub image: String,
    /// Registry URL (optional)
    pub registry: Option<String>,
    /// Organization/namespace (optional)
    pub org: Option<String>,
}

/// Request to push an image
#[derive(Debug, Deserialize, ToSchema)]
pub struct ImagePushRequest {
    /// Local image name
    pub name: String,
    /// Target image name with tag
    pub image: String,
    /// Registry URL (optional)
    pub registry: Option<String>,
    /// Dry run - don't actually push
    #[serde(default)]
    pub dry_run: bool,
}

/// Request to prune images
#[derive(Debug, Deserialize, ToSchema)]
pub struct ImagePruneRequest {
    /// Remove all images (not just unused ones)
    #[serde(default)]
    pub all: bool,
    /// Don't prompt for confirmation
    #[serde(default)]
    pub force: bool,
}

/// Request to run VM from image
#[derive(Debug, Deserialize, ToSchema)]
pub struct ImageRunRequest {
    /// Image reference
    pub image: String,
    /// VM name (optional)
    pub name: Option<String>,
    /// Registry URL (optional)
    pub registry: Option<String>,
    /// Organization/namespace (optional)
    pub org: Option<String>,
    /// Path to user-data file (optional)
    pub user_data: Option<String>,
    /// Don't start the VM, just create it
    #[serde(default)]
    pub no_start: bool,
    /// Memory size (optional)
    pub memory: Option<String>,
    /// Number of CPUs (optional)
    pub cpus: Option<u8>,
    /// Disk size (optional)
    pub disk: Option<String>,
}

/// Generic API error response
#[derive(Debug, Serialize, ToSchema)]
pub struct ApiError {
    /// Error message
    pub error: String,
    /// Error code
    pub code: String,
    /// Additional details (optional)
    pub details: Option<serde_json::Value>,
}

/// Health check response
#[derive(Debug, Serialize, ToSchema)]
pub struct HealthResponse {
    /// Service status
    pub status: String,
    /// Service version
    pub version: String,
    /// Current timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

fn default_tag() -> String {
    "latest".to_string()
}

/// Convert VM module types to API types
impl From<crate::vm::VmInfo> for VmInfo {
    fn from(vm_info: crate::vm::VmInfo) -> Self {
        Self {
            name: vm_info.name,
            state: vm_info.state,
            ip: vm_info.ip,
            memory: vm_info.memory,
            disk: vm_info.disk,
            created: vm_info.created,
        }
    }
}

/// Convert image module types to API types
impl From<crate::image::ImageInfo> for ImageInfo {
    fn from(image_info: crate::image::ImageInfo) -> Self {
        Self {
            name: image_info.name,
            tag: image_info.tag,
            registry: image_info.registry,
            size: image_info.size,
            created: image_info.created,
        }
    }
}
