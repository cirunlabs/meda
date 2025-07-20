use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
};
use log::{error, info};

use super::{models::*, AppState};
use crate::{image, vm};

/// List all VMs
#[utoipa::path(
    get,
    path = "/api/v1/vms",
    responses(
        (status = 200, description = "List of VMs", body = VmListResponse),
        (status = 500, description = "Internal server error", body = ApiError)
    ),
    tag = "VMs"
)]
pub async fn list_vms(
    State(state): State<AppState>,
) -> Result<Json<VmListResponse>, (StatusCode, Json<ApiError>)> {
    match vm::list(&state.config, true).await {
        Ok(_) => {
            // Since vm::list prints JSON, we need to capture it differently
            // For now, let's implement a direct approach
            match get_vm_list(&state.config).await {
                Ok(vms) => Ok(Json(VmListResponse {
                    count: vms.len(),
                    vms,
                })),
                Err(e) => {
                    error!("Failed to list VMs: {}", e);
                    Err((
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ApiError {
                            error: "Failed to list VMs".to_string(),
                            code: "VM_LIST_ERROR".to_string(),
                            details: Some(serde_json::json!({"message": e.to_string()})),
                        }),
                    ))
                }
            }
        }
        Err(e) => {
            error!("Failed to list VMs: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "Failed to list VMs".to_string(),
                    code: "VM_LIST_ERROR".to_string(),
                    details: Some(serde_json::json!({"message": e.to_string()})),
                }),
            ))
        }
    }
}

/// Create a new VM
#[utoipa::path(
    post,
    path = "/api/v1/vms",
    request_body = VmCreateRequest,
    responses(
        (status = 201, description = "VM created successfully", body = VmResponse),
        (status = 400, description = "Bad request", body = ApiError),
        (status = 409, description = "VM already exists", body = ApiError),
        (status = 500, description = "Internal server error", body = ApiError)
    ),
    tag = "VMs"
)]
pub async fn create_vm(
    State(state): State<AppState>,
    Json(request): Json<VmCreateRequest>,
) -> Result<Json<VmResponse>, (StatusCode, Json<ApiError>)> {
    info!("Creating VM: {}", request.name);

    // Handle force delete if VM exists
    if request.force {
        let vm_dir = state.config.vm_dir(&request.name);
        if vm_dir.exists() {
            if vm::check_vm_running(&state.config, &request.name).unwrap_or(false) {
                if let Err(e) = vm::stop(&state.config, &request.name, true).await {
                    error!("Failed to stop existing VM: {}", e);
                    return Err((
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ApiError {
                            error: "Failed to stop existing VM".to_string(),
                            code: "VM_STOP_ERROR".to_string(),
                            details: Some(serde_json::json!({"message": e.to_string()})),
                        }),
                    ));
                }
            }
            if let Err(e) = vm::delete(&state.config, &request.name, true).await {
                error!("Failed to delete existing VM: {}", e);
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiError {
                        error: "Failed to delete existing VM".to_string(),
                        code: "VM_DELETE_ERROR".to_string(),
                        details: Some(serde_json::json!({"message": e.to_string()})),
                    }),
                ));
            }
        }
    }

    // Create VmResources from request
    let resources = vm::VmResources::from_config_with_overrides(
        &state.config,
        request.memory.as_deref(),
        request.cpus,
        request.disk.as_deref(),
    );

    match vm::create(
        &state.config,
        &request.name,
        request.user_data.as_deref(),
        &resources,
        true,
    )
    .await
    {
        Ok(_) => {
            info!("Successfully created VM: {}", request.name);
            Ok(Json(VmResponse {
                success: true,
                message: format!("Successfully created VM: {}", request.name),
                vm: None,
            }))
        }
        Err(e) => {
            error!("Failed to create VM: {}", e);
            let status_code = if e.to_string().contains("already exists") {
                StatusCode::CONFLICT
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };

            Err((
                status_code,
                Json(ApiError {
                    error: "Failed to create VM".to_string(),
                    code: "VM_CREATE_ERROR".to_string(),
                    details: Some(serde_json::json!({"message": e.to_string()})),
                }),
            ))
        }
    }
}

/// Get VM details
#[utoipa::path(
    get,
    path = "/api/v1/vms/{name}",
    params(
        ("name" = String, Path, description = "VM name")
    ),
    responses(
        (status = 200, description = "VM details", body = VmDetailResponse),
        (status = 404, description = "VM not found", body = ApiError),
        (status = 500, description = "Internal server error", body = ApiError)
    ),
    tag = "VMs"
)]
pub async fn get_vm(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<VmDetailResponse>, (StatusCode, Json<ApiError>)> {
    match vm::get(&state.config, &name, true).await {
        Ok(_) => {
            // We need to get the VM details differently since get() prints JSON
            match get_vm_details(&state.config, &name).await {
                Ok(vm_detail) => Ok(Json(vm_detail)),
                Err(e) => {
                    error!("Failed to get VM details: {}", e);
                    Err((
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ApiError {
                            error: "Failed to get VM details".to_string(),
                            code: "VM_GET_ERROR".to_string(),
                            details: Some(serde_json::json!({"message": e.to_string()})),
                        }),
                    ))
                }
            }
        }
        Err(e) => {
            error!("Failed to get VM: {}", e);
            let status_code = if e.to_string().contains("not found") {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };

            Err((
                status_code,
                Json(ApiError {
                    error: "Failed to get VM".to_string(),
                    code: "VM_NOT_FOUND".to_string(),
                    details: Some(serde_json::json!({"message": e.to_string()})),
                }),
            ))
        }
    }
}

/// Delete a VM
#[utoipa::path(
    delete,
    path = "/api/v1/vms/{name}",
    params(
        ("name" = String, Path, description = "VM name")
    ),
    responses(
        (status = 200, description = "VM deleted successfully", body = VmResponse),
        (status = 404, description = "VM not found", body = ApiError),
        (status = 500, description = "Internal server error", body = ApiError)
    ),
    tag = "VMs"
)]
pub async fn delete_vm(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<VmResponse>, (StatusCode, Json<ApiError>)> {
    match vm::delete(&state.config, &name, true).await {
        Ok(_) => {
            info!("Successfully deleted VM: {}", name);
            Ok(Json(VmResponse {
                success: true,
                message: format!("Successfully deleted VM: {}", name),
                vm: None,
            }))
        }
        Err(e) => {
            error!("Failed to delete VM: {}", e);
            let status_code = if e.to_string().contains("not found") {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };

            Err((
                status_code,
                Json(ApiError {
                    error: "Failed to delete VM".to_string(),
                    code: "VM_DELETE_ERROR".to_string(),
                    details: Some(serde_json::json!({"message": e.to_string()})),
                }),
            ))
        }
    }
}

/// Start a VM
#[utoipa::path(
    post,
    path = "/api/v1/vms/{name}/start",
    params(
        ("name" = String, Path, description = "VM name")
    ),
    responses(
        (status = 200, description = "VM started successfully", body = VmResponse),
        (status = 404, description = "VM not found", body = ApiError),
        (status = 409, description = "VM already running", body = ApiError),
        (status = 500, description = "Internal server error", body = ApiError)
    ),
    tag = "VMs"
)]
pub async fn start_vm(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<VmResponse>, (StatusCode, Json<ApiError>)> {
    match vm::start(&state.config, &name, true).await {
        Ok(_) => {
            info!("Successfully started VM: {}", name);
            Ok(Json(VmResponse {
                success: true,
                message: format!("Successfully started VM: {}", name),
                vm: None,
            }))
        }
        Err(e) => {
            error!("Failed to start VM: {}", e);
            let status_code = if e.to_string().contains("not found") {
                StatusCode::NOT_FOUND
            } else if e.to_string().contains("already running") {
                StatusCode::CONFLICT
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };

            Err((
                status_code,
                Json(ApiError {
                    error: "Failed to start VM".to_string(),
                    code: "VM_START_ERROR".to_string(),
                    details: Some(serde_json::json!({"message": e.to_string()})),
                }),
            ))
        }
    }
}

/// Stop a VM
#[utoipa::path(
    post,
    path = "/api/v1/vms/{name}/stop",
    params(
        ("name" = String, Path, description = "VM name")
    ),
    responses(
        (status = 200, description = "VM stopped successfully", body = VmResponse),
        (status = 404, description = "VM not found", body = ApiError),
        (status = 409, description = "VM not running", body = ApiError),
        (status = 500, description = "Internal server error", body = ApiError)
    ),
    tag = "VMs"
)]
pub async fn stop_vm(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<VmResponse>, (StatusCode, Json<ApiError>)> {
    match vm::stop(&state.config, &name, true).await {
        Ok(_) => {
            info!("Successfully stopped VM: {}", name);
            Ok(Json(VmResponse {
                success: true,
                message: format!("Successfully stopped VM: {}", name),
                vm: None,
            }))
        }
        Err(e) => {
            error!("Failed to stop VM: {}", e);
            let status_code = if e.to_string().contains("not found") {
                StatusCode::NOT_FOUND
            } else if e.to_string().contains("not running") {
                StatusCode::CONFLICT
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };

            Err((
                status_code,
                Json(ApiError {
                    error: "Failed to stop VM".to_string(),
                    code: "VM_STOP_ERROR".to_string(),
                    details: Some(serde_json::json!({"message": e.to_string()})),
                }),
            ))
        }
    }
}

/// Get VM IP address
#[utoipa::path(
    get,
    path = "/api/v1/vms/{name}/ip",
    params(
        ("name" = String, Path, description = "VM name")
    ),
    responses(
        (status = 200, description = "VM IP address", body = serde_json::Value),
        (status = 404, description = "VM not found", body = ApiError),
        (status = 500, description = "Internal server error", body = ApiError)
    ),
    tag = "VMs"
)]
pub async fn get_vm_ip(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    match vm::ip(&state.config, &name, true).await {
        Ok(_) => {
            // Get IP directly
            match vm::get_vm_ip(&state.config, &name) {
                Ok(ip) => Ok(Json(serde_json::json!({"vm": name, "ip": ip}))),
                Err(e) => {
                    error!("Failed to get VM IP: {}", e);
                    Err((
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ApiError {
                            error: "Failed to get VM IP".to_string(),
                            code: "VM_IP_ERROR".to_string(),
                            details: Some(serde_json::json!({"message": e.to_string()})),
                        }),
                    ))
                }
            }
        }
        Err(e) => {
            error!("Failed to get VM IP: {}", e);
            let status_code = if e.to_string().contains("not found") {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };

            Err((
                status_code,
                Json(ApiError {
                    error: "Failed to get VM IP".to_string(),
                    code: "VM_IP_ERROR".to_string(),
                    details: Some(serde_json::json!({"message": e.to_string()})),
                }),
            ))
        }
    }
}

/// Set up port forwarding for a VM
#[utoipa::path(
    post,
    path = "/api/v1/vms/{name}/port-forward",
    params(
        ("name" = String, Path, description = "VM name")
    ),
    request_body = PortForwardRequest,
    responses(
        (status = 200, description = "Port forwarding set up successfully", body = VmResponse),
        (status = 404, description = "VM not found", body = ApiError),
        (status = 500, description = "Internal server error", body = ApiError)
    ),
    tag = "VMs"
)]
pub async fn port_forward(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(request): Json<PortForwardRequest>,
) -> Result<Json<VmResponse>, (StatusCode, Json<ApiError>)> {
    match crate::network::port_forward(&state.config, &name, request.host_port, request.guest_port)
        .await
    {
        Ok(_) => {
            info!("Successfully set up port forwarding for VM: {}", name);
            Ok(Json(VmResponse {
                success: true,
                message: format!(
                    "Port forwarding set up: {} -> {}",
                    request.host_port, request.guest_port
                ),
                vm: None,
            }))
        }
        Err(e) => {
            error!("Failed to set up port forwarding: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "Failed to set up port forwarding".to_string(),
                    code: "PORT_FORWARD_ERROR".to_string(),
                    details: Some(serde_json::json!({"message": e.to_string()})),
                }),
            ))
        }
    }
}

// Image management endpoints will be implemented next...

/// List all images
#[utoipa::path(
    get,
    path = "/api/v1/images",
    responses(
        (status = 200, description = "List of images", body = ImageListResponse),
        (status = 500, description = "Internal server error", body = ApiError)
    ),
    tag = "Images"
)]
pub async fn list_images(
    State(state): State<AppState>,
) -> Result<Json<ImageListResponse>, (StatusCode, Json<ApiError>)> {
    match image::list(&state.config, true).await {
        Ok(_) => {
            // For now return empty list - implement proper image listing
            Ok(Json(ImageListResponse {
                images: vec![],
                count: 0,
            }))
        }
        Err(e) => {
            error!("Failed to list images: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "Failed to list images".to_string(),
                    code: "IMAGE_LIST_ERROR".to_string(),
                    details: Some(serde_json::json!({"message": e.to_string()})),
                }),
            ))
        }
    }
}

/// Create a new image
#[utoipa::path(
    post,
    path = "/api/v1/images",
    request_body = ImageCreateRequest,
    responses(
        (status = 201, description = "Image created successfully", body = VmResponse),
        (status = 400, description = "Bad request", body = ApiError),
        (status = 500, description = "Internal server error", body = ApiError)
    ),
    tag = "Images"
)]
pub async fn create_image(
    State(state): State<AppState>,
    Json(request): Json<ImageCreateRequest>,
) -> Result<Json<VmResponse>, (StatusCode, Json<ApiError>)> {
    let default_registry = request.registry.as_deref().unwrap_or("ghcr.io");
    let default_org = request.org.as_deref().unwrap_or("cirunlabs");

    let result = if let Some(vm_name) = request.from_vm {
        image::create_from_vm(
            &state.config,
            &vm_name,
            &request.name,
            &request.tag,
            default_registry,
            default_org,
            true,
        )
        .await
    } else {
        image::create_base_image(
            &state.config,
            &request.name,
            &request.tag,
            default_registry,
            default_org,
            true,
        )
        .await
    };

    match result {
        Ok(_) => {
            info!(
                "Successfully created image: {}:{}",
                request.name, request.tag
            );
            Ok(Json(VmResponse {
                success: true,
                message: format!(
                    "Successfully created image: {}:{}",
                    request.name, request.tag
                ),
                vm: None,
            }))
        }
        Err(e) => {
            error!("Failed to create image: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "Failed to create image".to_string(),
                    code: "IMAGE_CREATE_ERROR".to_string(),
                    details: Some(serde_json::json!({"message": e.to_string()})),
                }),
            ))
        }
    }
}

/// Remove an image
#[utoipa::path(
    delete,
    path = "/api/v1/images/{image}",
    params(
        ("image" = String, Path, description = "Image name and tag")
    ),
    responses(
        (status = 200, description = "Image removed successfully", body = VmResponse),
        (status = 404, description = "Image not found", body = ApiError),
        (status = 500, description = "Internal server error", body = ApiError)
    ),
    tag = "Images"
)]
pub async fn remove_image(
    State(state): State<AppState>,
    Path(image_name): Path<String>,
) -> Result<Json<VmResponse>, (StatusCode, Json<ApiError>)> {
    match image::remove(&state.config, &image_name, None, None, true, true).await {
        Ok(_) => {
            info!("Successfully removed image: {}", image_name);
            Ok(Json(VmResponse {
                success: true,
                message: format!("Successfully removed image: {}", image_name),
                vm: None,
            }))
        }
        Err(e) => {
            error!("Failed to remove image: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "Failed to remove image".to_string(),
                    code: "IMAGE_REMOVE_ERROR".to_string(),
                    details: Some(serde_json::json!({"message": e.to_string()})),
                }),
            ))
        }
    }
}

/// Pull an image from registry
#[utoipa::path(
    post,
    path = "/api/v1/images/pull",
    request_body = ImagePullRequest,
    responses(
        (status = 200, description = "Image pulled successfully", body = VmResponse),
        (status = 400, description = "Bad request", body = ApiError),
        (status = 500, description = "Internal server error", body = ApiError)
    ),
    tag = "Images"
)]
pub async fn pull_image(
    State(state): State<AppState>,
    Json(request): Json<ImagePullRequest>,
) -> Result<Json<VmResponse>, (StatusCode, Json<ApiError>)> {
    match image::pull(
        &state.config,
        &request.image,
        request.registry.as_deref(),
        request.org.as_deref(),
        true,
    )
    .await
    {
        Ok(_) => {
            info!("Successfully pulled image: {}", request.image);
            Ok(Json(VmResponse {
                success: true,
                message: format!("Successfully pulled image: {}", request.image),
                vm: None,
            }))
        }
        Err(e) => {
            error!("Failed to pull image: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "Failed to pull image".to_string(),
                    code: "IMAGE_PULL_ERROR".to_string(),
                    details: Some(serde_json::json!({"message": e.to_string()})),
                }),
            ))
        }
    }
}

/// Push an image to registry
#[utoipa::path(
    post,
    path = "/api/v1/images/push",
    request_body = ImagePushRequest,
    responses(
        (status = 200, description = "Image pushed successfully", body = VmResponse),
        (status = 400, description = "Bad request", body = ApiError),
        (status = 500, description = "Internal server error", body = ApiError)
    ),
    tag = "Images"
)]
pub async fn push_image(
    State(state): State<AppState>,
    Json(request): Json<ImagePushRequest>,
) -> Result<Json<VmResponse>, (StatusCode, Json<ApiError>)> {
    match image::push(
        &state.config,
        &request.name,
        &request.image,
        request.registry.as_deref(),
        request.dry_run,
        true,
    )
    .await
    {
        Ok(_) => {
            info!("Successfully pushed image: {}", request.image);
            Ok(Json(VmResponse {
                success: true,
                message: format!("Successfully pushed image: {}", request.image),
                vm: None,
            }))
        }
        Err(e) => {
            error!("Failed to push image: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "Failed to push image".to_string(),
                    code: "IMAGE_PUSH_ERROR".to_string(),
                    details: Some(serde_json::json!({"message": e.to_string()})),
                }),
            ))
        }
    }
}

/// Prune unused images
#[utoipa::path(
    post,
    path = "/api/v1/images/prune",
    request_body = ImagePruneRequest,
    responses(
        (status = 200, description = "Images pruned successfully", body = VmResponse),
        (status = 500, description = "Internal server error", body = ApiError)
    ),
    tag = "Images"
)]
pub async fn prune_images(
    State(state): State<AppState>,
    Json(request): Json<ImagePruneRequest>,
) -> Result<Json<VmResponse>, (StatusCode, Json<ApiError>)> {
    match image::prune(&state.config, request.all, request.force, true).await {
        Ok(_) => {
            info!("Successfully pruned images");
            Ok(Json(VmResponse {
                success: true,
                message: "Successfully pruned images".to_string(),
                vm: None,
            }))
        }
        Err(e) => {
            error!("Failed to prune images: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "Failed to prune images".to_string(),
                    code: "IMAGE_PRUNE_ERROR".to_string(),
                    details: Some(serde_json::json!({"message": e.to_string()})),
                }),
            ))
        }
    }
}

/// Run VM from image
#[utoipa::path(
    post,
    path = "/api/v1/images/run",
    request_body = ImageRunRequest,
    responses(
        (status = 201, description = "VM created and optionally started from image", body = VmResponse),
        (status = 400, description = "Bad request", body = ApiError),
        (status = 500, description = "Internal server error", body = ApiError)
    ),
    tag = "Images"
)]
pub async fn run_from_image(
    State(state): State<AppState>,
    Json(request): Json<ImageRunRequest>,
) -> Result<Json<VmResponse>, (StatusCode, Json<ApiError>)> {
    let resources = vm::VmResources::from_config_with_overrides(
        &state.config,
        request.memory.as_deref(),
        request.cpus,
        request.disk.as_deref(),
    );

    let options = image::RunOptions {
        vm_name: request.name.as_deref(),
        registry: request.registry.as_deref(),
        org: request.org.as_deref(),
        user_data_path: request.user_data.as_deref(),
        no_start: request.no_start,
        resources,
    };

    match image::run_from_image(&state.config, &request.image, options, true).await {
        Ok(_) => {
            let action = if request.no_start {
                "created"
            } else {
                "created and started"
            };
            info!("Successfully {} VM from image: {}", action, request.image);
            Ok(Json(VmResponse {
                success: true,
                message: format!("Successfully {} VM from image: {}", action, request.image),
                vm: None,
            }))
        }
        Err(e) => {
            error!("Failed to run VM from image: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "Failed to run VM from image".to_string(),
                    code: "IMAGE_RUN_ERROR".to_string(),
                    details: Some(serde_json::json!({"message": e.to_string()})),
                }),
            ))
        }
    }
}

/// Health check endpoint
#[utoipa::path(
    get,
    path = "/api/v1/health",
    responses(
        (status = 200, description = "Service health status", body = HealthResponse)
    ),
    tag = "System"
)]
pub async fn health_check() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "healthy".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        timestamp: chrono::Utc::now(),
    })
}

// Helper functions to get data without JSON printing
async fn get_vm_list(config: &crate::config::Config) -> crate::error::Result<Vec<VmInfo>> {
    use std::fs;

    config.ensure_dirs()?;

    if !config.vm_root.exists() {
        return Ok(vec![]);
    }

    let mut vms = Vec::new();

    for entry in fs::read_dir(&config.vm_root)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            let state = if vm::check_vm_running(config, &name)? {
                "running".to_string()
            } else {
                "stopped".to_string()
            };

            let ip = vm::get_vm_ip(config, &name).unwrap_or_else(|_| "N/A".to_string());

            // Read stored resource information or fall back to defaults
            let vm_dir = config.vm_dir(&name);
            let memory = fs::read_to_string(vm_dir.join("memory"))
                .unwrap_or_else(|_| config.mem.clone())
                .trim()
                .to_string();
            let disk = fs::read_to_string(vm_dir.join("disk_size"))
                .unwrap_or_else(|_| config.disk_size.clone())
                .trim()
                .to_string();

            // Get creation time from directory metadata
            let created = match fs::metadata(&path) {
                Ok(metadata) => {
                    if let Ok(created_time) = metadata.created() {
                        if let Ok(since_epoch) = created_time.duration_since(std::time::UNIX_EPOCH)
                        {
                            crate::util::format_timestamp(since_epoch.as_secs())
                        } else {
                            "unknown".to_string()
                        }
                    } else {
                        "unknown".to_string()
                    }
                }
                Err(_) => "unknown".to_string(),
            };

            vms.push(VmInfo {
                name,
                state,
                ip,
                memory,
                disk,
                created,
            });
        }
    }

    Ok(vms)
}

async fn get_vm_details(
    config: &crate::config::Config,
    name: &str,
) -> crate::error::Result<VmDetailResponse> {
    let vm_dir = config.vm_dir(name);

    if !vm_dir.exists() {
        return Err(crate::error::Error::VmNotFound(name.to_string()));
    }

    let state = if vm::check_vm_running(config, name)? {
        "running".to_string()
    } else {
        "stopped".to_string()
    };

    let ip = vm::get_vm_ip(config, name).ok();

    // Collect additional details
    let mut details = serde_json::Map::new();

    // Add network info
    if let Ok(subnet) = std::fs::read_to_string(vm_dir.join("subnet")) {
        details.insert(
            "subnet".to_string(),
            serde_json::Value::String(subnet.trim().to_string()),
        );
    }

    if let Ok(mac) = std::fs::read_to_string(vm_dir.join("mac")) {
        details.insert(
            "mac".to_string(),
            serde_json::Value::String(mac.trim().to_string()),
        );
    }

    if let Ok(tap) = std::fs::read_to_string(vm_dir.join("tapdev")) {
        details.insert(
            "tap_device".to_string(),
            serde_json::Value::String(tap.trim().to_string()),
        );
    }

    // Add VM resource info
    let memory = std::fs::read_to_string(vm_dir.join("memory"))
        .unwrap_or_else(|_| config.mem.clone())
        .trim()
        .to_string();
    let disk_size = std::fs::read_to_string(vm_dir.join("disk_size"))
        .unwrap_or_else(|_| config.disk_size.clone())
        .trim()
        .to_string();

    details.insert("memory".to_string(), serde_json::Value::String(memory));
    details.insert(
        "disk_size".to_string(),
        serde_json::Value::String(disk_size),
    );

    // Add VM directory path
    details.insert(
        "vm_dir".to_string(),
        serde_json::Value::String(vm_dir.to_string_lossy().to_string()),
    );

    Ok(VmDetailResponse {
        name: name.to_string(),
        state,
        ip,
        details: Some(serde_json::Value::Object(details)),
    })
}
