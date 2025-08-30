pub mod api;
pub mod cache;
pub mod handlers;
pub mod state;

use axum::{
    routing::{get, put},
    Router,
};
use std::path::PathBuf;
use std::sync::Arc;
use tower::ServiceBuilder;
use tower_http::{
    cors::CorsLayer,
    services::{ServeDir, ServeFile},
    trace::TraceLayer,
};

use crate::server::state::AppState;

pub async fn run_server(
    database_path: String,
    image_dir: String,
    static_dir: String,
    cache_dir: String,
    host: String,
    port: u16,
) -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    // Create cache directory if it doesn't exist
    std::fs::create_dir_all(&cache_dir)?;

    // Create app state
    let state = match AppState::new(database_path, image_dir, cache_dir.clone()) {
        Ok(state) => Arc::new(state),
        Err(e) => {
            eprintln!("Failed to initialize server: {}", e);
            return Err(e);
        }
    };

    // Create API routes
    let api_routes = Router::new()
        .route("/projects", get(handlers::list_projects))
        .route("/projects/{project_id}/targets", get(handlers::list_targets))
        .route("/images", get(handlers::get_images))
        .route("/images/{image_id}", get(handlers::get_image))
        .route(
            "/images/{image_id}/preview",
            get(handlers::get_image_preview),
        )
        .route("/images/{image_id}/stars", get(handlers::get_image_stars))
        .route(
            "/images/{image_id}/annotated",
            get(handlers::get_annotated_image),
        )
        .route(
            "/images/{image_id}/psf",
            get(handlers::get_psf_visualization),
        )
        .route("/images/{image_id}/grade", put(handlers::update_image_grade))
        .with_state(state);

    // Serve static files with SPA fallback
    let static_path = PathBuf::from(&static_dir);
    let index_path = static_path.join("index.html");

    let serve_dir = ServeDir::new(&static_path).not_found_service(ServeFile::new(&index_path));

    // Create main app
    let app = Router::new()
        .nest("/api", api_routes)
        .fallback_service(serve_dir)
        .layer(
            ServiceBuilder::new()
                .layer(TraceLayer::new_for_http())
                .layer(CorsLayer::permissive()),
        );

    // Create listener
    let listener = tokio::net::TcpListener::bind(format!("{}:{}", host, port)).await?;

    tracing::info!("Server listening on http://{}:{}", host, port);
    tracing::info!("Serving static files from: {}", static_dir);
    tracing::info!("Using cache directory: {}", cache_dir);

    // Run server
    axum::serve(listener, app).await?;

    Ok(())
}
