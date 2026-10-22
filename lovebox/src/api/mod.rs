pub mod cards;
pub mod common;
pub mod error;
pub mod health;
pub mod photos;
pub mod preview;
pub mod quotes;
pub mod settings;
pub mod show;
pub mod types;

use std::sync::Arc;
use std::time::Instant;

use axum::extract::DefaultBodyLimit;
use axum::http::header::{HeaderName, ETAG};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::{DefaultOnResponse, TraceLayer};
use tracing::Level;

use crate::config::Config;
use crate::display::DisplayHandle;
use crate::fonts::Fonts;
use crate::library::Library;
use crate::store::Store;
use tokio::sync::Semaphore;

#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    pub store: Arc<Store>,
    pub library: Library,
    pub display: DisplayHandle,
    pub fonts: Fonts,
    pub started: Instant,
    pub show_gate: Arc<Semaphore>,
}

impl AppState {
    pub fn new(
        config: Config,
        store: Arc<Store>,
        library: Library,
        display: DisplayHandle,
    ) -> Self {
        Self {
            config,
            store,
            library,
            display,
            fonts: Fonts::bundled(),
            started: Instant::now(),
            show_gate: Arc::new(Semaphore::new(1)),
        }
    }
}

pub fn router(state: Arc<AppState>) -> Router {
    let api = Router::new()
        .route("/", get(api_root))
        .route("/state", get(health::state))
        .route("/show", post(show::show))
        .route("/clear", post(show::clear))
        .route("/preview", post(preview::preview))
        .route("/fonts", get(health::fonts))
        .route("/frame/current.png", get(show::current_frame))
        .route(
            "/photos",
            get(photos::list)
                .post(photos::upload)
                .layer(DefaultBodyLimit::max(64 * 1024 * 1024)),
        )
        .route(
            "/photos/{id}",
            get(photos::get).patch(photos::patch).delete(photos::delete),
        )
        .route("/photos/{id}/thumb.jpg", get(photos::thumb))
        .route("/photos/{id}/copy.jpg", get(photos::copy))
        .route("/photos/{id}/frame.png", get(photos::frame))
        .route("/quotes", get(quotes::list).post(quotes::create))
        .route(
            "/quotes/{id}",
            get(quotes::get).patch(quotes::patch).delete(quotes::delete),
        )
        .route("/settings", get(settings::get).put(settings::put))
        .fallback(api_not_found)
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any)
                .expose_headers([
                    ETAG,
                    HeaderName::from_static("x-lovebox-fit-size-px"),
                    HeaderName::from_static("x-lovebox-truncated"),
                ]),
        );

    Router::new()
        .route("/healthz", get(health::health))
        .route("/fonts/{*path}", get(crate::web::font))
        .nest("/api", api)
        .fallback(crate::web::fallback)
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(|request: &axum::http::Request<axum::body::Body>| {
                    tracing::info_span!("http", method = %request.method(), path = %request.uri().path())
                })
                .on_response(DefaultOnResponse::new().level(Level::INFO)),
        )
        .with_state(state)
}

async fn api_root() -> impl IntoResponse {
    (
        StatusCode::NOT_FOUND,
        Json(
            serde_json::json!({"error":{"code":"not_found","message":"API route '/api' was not found."}}),
        ),
    )
}

async fn api_not_found(uri: axum::http::Uri) -> impl IntoResponse {
    (
        StatusCode::NOT_FOUND,
        Json(
            serde_json::json!({"error":{"code":"not_found","message":format!("API route '{}' was not found.", uri.path())}}),
        ),
    )
}
