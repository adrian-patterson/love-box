use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Path as AxumPath, State};
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use axum::http::{Method, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;

use crate::api::error::ApiError;
use crate::api::AppState;

#[derive(RustEmbed)]
#[folder = "../web/dist/"]
#[cfg_attr(debug_assertions, allow(dead_code))]
struct WebAssets;

pub async fn font(
    State(app): State<Arc<AppState>>,
    AxumPath(path): AxumPath<String>,
) -> Result<Response, ApiError> {
    let id = path.strip_suffix(".ttf").unwrap_or(&path);
    let face = app
        .fonts
        .faces()
        .iter()
        .find(|face| face.id() == id)
        .ok_or_else(|| ApiError::not_found(format!("Font '{id}' was not found.")))?;
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, "font/ttf")
        .header(CACHE_CONTROL, "public, max-age=31536000, immutable")
        .body(Body::from(face.data()))
        .expect("valid font response"))
}

pub async fn fallback(State(_app): State<Arc<AppState>>, method: Method, uri: Uri) -> Response {
    if method != Method::GET {
        return static_not_found();
    }
    let path = uri.path();
    if path == "/api" || path.starts_with("/api/") {
        return (StatusCode::NOT_FOUND, axum::Json(serde_json::json!({
            "error":{"code":"not_found","message":format!("API route '{}' was not found.", path)}
        }))).into_response();
    }
    let asset_path = path.trim_start_matches('/');
    if asset_path.is_empty() {
        return asset("index.html");
    }
    if safe_asset_path(asset_path) {
        if let Some(response) = find_asset(asset_path) {
            return response;
        }
    }
    // A path with an extension is a static asset request; return a real 404 rather than
    // masking a missing JS/CSS/image file with the SPA shell.
    if Path::new(asset_path).extension().is_some() {
        return static_not_found();
    }
    asset("index.html")
}

fn safe_asset_path(value: &str) -> bool {
    !value.is_empty()
        && Path::new(value)
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn find_asset(path: &str) -> Option<Response> {
    #[cfg(debug_assertions)]
    {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../web/dist");
        let file = root.join(path);
        if file.is_file() {
            return std::fs::read(file)
                .ok()
                .map(|bytes| asset_response(path, bytes));
        }
        None
    }
    #[cfg(not(debug_assertions))]
    {
        WebAssets::get(path).map(|file| asset_response(path, file.data.into_owned()))
    }
}

fn asset(path: &str) -> Response {
    find_asset(path).unwrap_or_else(static_not_found)
}

fn asset_response(path: &str, bytes: Vec<u8>) -> Response {
    let content_type = content_type(path);
    let cache = if path == "index.html" {
        "no-cache"
    } else if path.starts_with("assets/") {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };
    Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, content_type)
        .header(CACHE_CONTROL, cache)
        .body(Body::from(bytes))
        .expect("valid static asset response")
}

fn content_type(path: &str) -> &'static str {
    match PathBuf::from(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("")
    {
        "html" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webmanifest" | "json" => "application/json",
        "ico" => "image/x-icon",
        "ttf" => "font/ttf",
        _ => "application/octet-stream",
    }
}

fn static_not_found() -> Response {
    (StatusCode::NOT_FOUND, "Not found").into_response()
}
