use axum::{
    response::Json,
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
pub fn create_router(config: Arc<Config>) -> Router {
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
        // Health check and docs
        .route("/api/v1/health", get(health_check))
        .route("/api/v1/openapi.json", get(openapi_spec))
        // Swagger UI
        .merge(
            utoipa_swagger_ui::SwaggerUi::new("/swagger-ui")
                .url("/api/v1/openapi.json", ApiDoc::openapi()),
        )
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
            models::PortForwardRequest,
            models::ImageListResponse,
            models::ImageCreateRequest,
            models::ImagePullRequest,
            models::ImagePushRequest,
            models::ImagePruneRequest,
            models::ImageRunRequest,
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
    ),
    servers(
        (url = "http://localhost:7777", description = "Local development server")
    )
)]
pub struct ApiDoc;

/// Get OpenAPI specification
async fn openapi_spec() -> Json<utoipa::openapi::OpenApi> {
    Json(ApiDoc::openapi())
}
