use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use ulid::Ulid;

use super::common::{now, ApiJson};
use super::error::ApiError;
use super::types::{ApiAlign, Quote};
use super::AppState;
use crate::render::{fit_quote, Align, QuoteSpec};

#[derive(Deserialize)]
pub struct QuoteCreateRequest {
    pub text: String,
    #[serde(default)]
    pub attribution: Option<String>,
    pub font: Option<String>,
    pub align: Option<ApiAlign>,
    pub in_rotation: Option<bool>,
}

#[derive(Deserialize, Default)]
pub struct QuotePatchRequest {
    pub text: Option<String>,
    pub attribution: Option<serde_json::Value>,
    pub font: Option<String>,
    pub align: Option<ApiAlign>,
    pub in_rotation: Option<bool>,
}

pub async fn list(State(app): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let mut items = app.store.snapshot().quotes;
    items.sort_by(|a, b| b.added_at.cmp(&a.added_at));
    let total = items.len();
    Json(
        serde_json::json!({"items":items,"total":total,"quote_max_chars":crate::render::QUOTE_MAX_CHARS}),
    )
}

pub async fn get(
    State(app): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Quote>, ApiError> {
    let quote = app
        .store
        .snapshot()
        .quotes
        .into_iter()
        .find(|quote| quote.id == id)
        .ok_or_else(|| ApiError::not_found(format!("Quote '{id}' was not found.")))?;
    Ok(Json(quote))
}

pub async fn create(
    State(app): State<Arc<AppState>>,
    ApiJson(request): ApiJson<QuoteCreateRequest>,
) -> Result<(StatusCode, Json<Quote>), ApiError> {
    let settings = app.store.snapshot().settings;
    let font = request.font.unwrap_or(settings.default_font);
    let align = request.align.unwrap_or(ApiAlign::Left);
    validate_quote(
        &app,
        &request.text,
        request.attribution.as_deref(),
        &font,
        align,
    )?;
    let quote = Quote {
        id: Ulid::new().to_string(),
        text: request.text,
        attribution: request.attribution,
        font,
        align,
        added_at: now(),
        shown_count: 0,
        last_shown_at: None,
        in_rotation: request.in_rotation.unwrap_or(true),
    };
    app.store.update(|state| {
        state.quotes.push(quote.clone());
        Ok(())
    })?;
    Ok((StatusCode::CREATED, Json(quote)))
}

pub async fn patch(
    State(app): State<Arc<AppState>>,
    Path(id): Path<String>,
    ApiJson(request): ApiJson<QuotePatchRequest>,
) -> Result<Json<Quote>, ApiError> {
    let existing = app
        .store
        .snapshot()
        .quotes
        .into_iter()
        .find(|quote| quote.id == id)
        .ok_or_else(|| ApiError::not_found(format!("Quote '{id}' was not found.")))?;
    let text = request.text.unwrap_or(existing.text.clone());
    let attribution = match request.attribution {
        None => existing.attribution.clone(),
        Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::String(value)) => Some(value),
        Some(_) => return Err(ApiError::bad("attribution must be a string or null")),
    };
    let font = request.font.unwrap_or(existing.font.clone());
    let align = request.align.unwrap_or(existing.align);
    validate_quote(&app, &text, attribution.as_deref(), &font, align)?;
    let mut updated = existing;
    updated.text = text;
    updated.attribution = attribution;
    updated.font = font;
    updated.align = align;
    if let Some(in_rotation) = request.in_rotation {
        updated.in_rotation = in_rotation;
    }
    app.store.update(|state| {
        let quote = state
            .quotes
            .iter_mut()
            .find(|quote| quote.id == id)
            .ok_or_else(|| format!("quote '{id}' was removed"))?;
        *quote = updated.clone();
        Ok(())
    })?;
    Ok(Json(updated))
}

pub async fn delete(
    State(app): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let found = app
        .store
        .snapshot()
        .quotes
        .iter()
        .any(|quote| quote.id == id);
    if !found {
        return Err(ApiError::not_found(format!("Quote '{id}' was not found.")));
    }
    app.store.update(|state| {
        state.quotes.retain(|quote| quote.id != id);
        Ok(())
    })?;
    Ok(StatusCode::NO_CONTENT)
}

fn validate_quote(
    app: &AppState,
    text: &str,
    attribution: Option<&str>,
    font: &str,
    align: ApiAlign,
) -> Result<(), ApiError> {
    let face = app
        .fonts
        .by_name(font)
        .ok_or_else(|| ApiError::bad(format!("unknown font: {font:?}")))?;
    let mut spec = QuoteSpec::new(text);
    spec.attribution = attribution.map(str::to_owned);
    spec.font = font.to_owned();
    spec.align = match align {
        ApiAlign::Left => Align::Left,
        ApiAlign::Center => Align::Center,
    };
    fit_quote(&spec, face)?;
    Ok(())
}
