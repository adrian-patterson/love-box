use std::sync::Arc;

use axum::extract::State;
use axum::Json;
use serde::Serialize;

use super::common::ApiJson;
use super::error::ApiError;
use super::types::Settings;
use super::AppState;

#[derive(Serialize)]
pub struct SettingsResponse {
    #[serde(flatten)]
    settings: Settings,
    scheduler: &'static str,
    quote_max_chars: usize,
}

pub async fn get(State(app): State<Arc<AppState>>) -> Json<SettingsResponse> {
    Json(SettingsResponse {
        settings: app.store.snapshot().settings,
        scheduler: "not implemented",
        quote_max_chars: crate::render::QUOTE_MAX_CHARS,
    })
}

pub async fn put(
    State(app): State<Arc<AppState>>,
    ApiJson(settings): ApiJson<Settings>,
) -> Result<Json<SettingsResponse>, ApiError> {
    settings.validate().map_err(ApiError::bad)?;
    if app.fonts.by_name(&settings.default_font).is_none() {
        return Err(ApiError::bad(format!(
            "unknown default font: {:?}",
            settings.default_font
        )));
    }
    app.store.update(|state| {
        state.settings = settings.clone();
        Ok(())
    })?;
    Ok(Json(SettingsResponse {
        settings,
        scheduler: "not implemented",
        quote_max_chars: crate::render::QUOTE_MAX_CHARS,
    }))
}
