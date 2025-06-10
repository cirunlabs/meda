use axum::{
    routing::{delete, get, post},
    Router,
};
use std::sync::Arc;
use tower::ServiceBuilder;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use utoipa::OpenApi;

use crate::config::Config;

pub mod handlers;
pub mod models;

pub use handlers::*;

/// API application state
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
}

/// Create the main API router with all endpoints
pub fn create_router(config: Arc<Config>, host: &str, port: u16) -> Router {
    // When binding to 0.0.0.0, we want to allow the swagger UI to use the browser's current host
    // This way it will work whether accessed via localhost, VM IP, or any other accessible address
    let base_url = if host == "0.0.0.0" {
        // Use a relative base URL so the browser uses whatever host it's currently on
        String::new() // Empty string will make requests relative to current host
    } else if host == "127.0.0.1" {
        format!("http://localhost:{}", port)
    } else {
        format!("http://{}:{}", host, port)
    };

    let state = AppState { config };

    Router::new()
        // VM management endpoints
        .route("/api/v1/vms", get(list_vms).post(create_vm))
        .route("/api/v1/vms/:name", get(get_vm).delete(delete_vm))
        .route("/api/v1/vms/:name/start", post(start_vm))
        .route("/api/v1/vms/:name/stop", post(stop_vm))
        .route("/api/v1/vms/:name/ip", get(get_vm_ip))
        .route("/api/v1/vms/:name/port-forward", post(port_forward))
        // Image management endpoints
        .route("/api/v1/images", get(list_images).post(create_image))
        .route("/api/v1/images/:image", delete(remove_image))
        .route("/api/v1/images/pull", post(pull_image))
        .route("/api/v1/images/push", post(push_image))
        .route("/api/v1/images/prune", post(prune_images))
        .route("/api/v1/images/run", post(run_from_image))
        // Health check
        .route("/api/v1/health", get(health_check))
        // Swagger UI with dynamic OpenAPI spec
        .merge(create_swagger_ui(&base_url))
        .layer(
            ServiceBuilder::new()
                .layer(TraceLayer::new_for_http())
                .layer(CorsLayer::permissive()),
        )
        .with_state(state)
}

/// OpenAPI documentation
#[derive(OpenApi)]
#[openapi(
    paths(
        handlers::list_vms,
        handlers::create_vm,
        handlers::get_vm,
        handlers::delete_vm,
        handlers::start_vm,
        handlers::stop_vm,
        handlers::get_vm_ip,
        handlers::port_forward,
        handlers::list_images,
        handlers::create_image,
        handlers::remove_image,
        handlers::pull_image,
        handlers::push_image,
        handlers::prune_images,
        handlers::run_from_image,
        handlers::health_check,
    ),
    components(
        schemas(
            models::VmCreateRequest,
            models::VmResponse,
            models::VmListResponse,
            models::VmDetailResponse,
            models::VmInfo,
            models::PortForwardRequest,
            models::ImageListResponse,
            models::ImageCreateRequest,
            models::ImagePullRequest,
            models::ImagePushRequest,
            models::ImagePruneRequest,
            models::ImageRunRequest,
            models::ImageInfo,
            models::ApiError,
            models::HealthResponse,
        )
    ),
    tags(
        (name = "VMs", description = "Virtual Machine management operations"),
        (name = "Images", description = "VM Image management operations"),
        (name = "System", description = "System and health check operations")
    ),
    info(
        title = "Meda API",
        version = "1.0.0",
        description = "REST API for Meda - Cloud-Hypervisor micro-VM manager",
        contact(
            name = "Meda Support",
            email = "support@example.com"
        ),
        license(
            name = "MIT",
            url = "https://opensource.org/licenses/MIT"
        )
    )
)]
pub struct ApiDoc;

/// Create Swagger UI with dynamic OpenAPI spec
fn create_swagger_ui(base_url: &str) -> Router<AppState> {
    let mut openapi = ApiDoc::openapi();

    // When host is 0.0.0.0, use relative URL so it works with any host the browser uses
    let server_url = if base_url.is_empty() {
        "/".to_string() // Relative URL - will use current browser host
    } else {
        base_url.to_string()
    };
    // Update server URL with the actual host/port
    openapi.servers = Some(vec![utoipa::openapi::ServerBuilder::new()
        .url(server_url)
        .description(Some("Meda API Server"))
        .build()]);

    utoipa_swagger_ui::SwaggerUi::new("/swagger-ui")
        .url("/api/v1/openapi.json", openapi)
        .into()
}
