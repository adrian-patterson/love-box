use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Serialize;

use crate::api::common::PanelView;
use crate::api::types::State as DocumentState;
use crate::api::AppState;
use crate::render::QUOTE_MAX_CHARS;

pub async fn health(State(app): State<Arc<AppState>>) -> impl IntoResponse {
    let state = app.store.snapshot();
    let panel = PanelView::from(app.display.status());
    let status = if panel.state == "wedged" {
        StatusCode::SERVICE_UNAVAILABLE
    } else {
        StatusCode::OK
    };
    let response = Json(HealthResponse {
        version: env!("CARGO_PKG_VERSION"),
        git_sha: option_env!("LOVEBOX_GIT_SHA").unwrap_or("unknown"),
        uptime_s: app.started.elapsed().as_secs(),
        panel,
        data_dir: app.config.data_dir.display().to_string(),
        disk_free_bytes: fs2::available_space(&app.config.data_dir).ok(),
        photos: state.photos.len(),
        state_version: state.version,
        cpu_temperature_c: cpu_temperature_c(),
        refresh_count_today: refreshes_today(&state),
        quote_max_chars: QUOTE_MAX_CHARS,
    });
    (status, response)
}

#[derive(Serialize)]
struct HealthResponse {
    version: &'static str,
    git_sha: &'static str,
    uptime_s: u64,
    panel: PanelView,
    data_dir: String,
    disk_free_bytes: Option<u64>,
    photos: usize,
    state_version: u32,
    cpu_temperature_c: Option<f32>,
    refresh_count_today: usize,
    quote_max_chars: usize,
}

#[derive(Serialize)]
pub struct FontInfo {
    id: &'static str,
    display_name: &'static str,
    kind: &'static str,
    italic: bool,
    italic_available: bool,
    file_url: String,
    quote_max_chars: usize,
    message_max_chars: usize,
}

pub async fn fonts(State(app): State<Arc<AppState>>) -> Json<Vec<FontInfo>> {
    let faces = app.fonts.faces();
    let list = faces
        .iter()
        .map(|face| {
            let italic_available = faces
                .iter()
                .any(|other| other.family() == face.family() && other.is_italic());
            FontInfo {
                id: face.id(),
                display_name: face.display_name(),
                kind: if face.family().to_ascii_lowercase().contains("atkinson") {
                    "sans"
                } else {
                    "serif"
                },
                italic: face.is_italic(),
                italic_available,
                file_url: format!("/fonts/{}.ttf", face.id()),
                quote_max_chars: QUOTE_MAX_CHARS,
                message_max_chars: 3000,
            }
        })
        .collect();
    Json(list)
}

pub async fn state(State(app): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let state = app.store.snapshot();
    let panel = PanelView::from(app.display.status());
    let queue: Vec<_> = state.photos.iter().map(|photo| serde_json::json!({"type":"photo","id":photo.id,"in_rotation":photo.in_rotation}))
        .chain(state.quotes.iter().map(|quote| serde_json::json!({"type":"quote","id":quote.id,"in_rotation":quote.in_rotation})))
        .collect();
    let frame_url = if state.current_frame_png.is_some() {
        Some(format!(
            "/api/frame/current.png?rev={}",
            state.frame_revision
        ))
    } else {
        None
    };
    let reason = match state.settings.mode {
        crate::api::types::DisplayMode::Pinned => "pinned",
        crate::api::types::DisplayMode::Daily => "daily",
        crate::api::types::DisplayMode::Slideshow => "slideshow",
    };
    let next_change_at =
        crate::scheduler::next_change_at(&state, chrono::Local::now()).map(|at| at.to_rfc3339());
    Json(serde_json::json!({
        "now_showing": state.now_showing,
        "mode": state.settings.mode,
        "reason": reason,
        "next_change_at": next_change_at,
        "current_frame_url": frame_url,
        "frame_revision": state.frame_revision,
        "settings": state.settings,
        "panel": panel,
        "last_refresh_at": state.last_refresh_at,
        "last_refresh_duration_ms": state.last_refresh_duration_ms,
        "quiet_hours_active": false,
        "capabilities": {"scheduler":false,"daily":true,"slideshow":true,"google":false,"ai":false},
        "sources": [],
        "quote_max_chars": QUOTE_MAX_CHARS,
        "message_max_chars": 3000,
        "queue": queue
    }))
}

fn cpu_temperature_c() -> Option<f32> {
    let text = std::fs::read_to_string("/sys/class/thermal/thermal_zone0/temp").ok()?;
    let milli: i64 = text.trim().parse().ok()?;
    Some(milli as f32 / 1000.0)
}

fn refreshes_today(state: &DocumentState) -> usize {
    use chrono::{DateTime, Local};
    let today = Local::now().date_naive();
    state
        .history
        .iter()
        .filter(|entry| {
            DateTime::parse_from_rfc3339(&entry.shown_at)
                .map(|time| time.with_timezone(&Local).date_naive() == today)
                .unwrap_or(false)
        })
        .count()
}
