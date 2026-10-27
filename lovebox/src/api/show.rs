use std::sync::Arc;

use axum::body::Body;
use axum::extract::State;
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE, ETAG};
use axum::http::{HeaderValue, StatusCode};
use axum::response::Response;
use axum::Json;
use serde::Serialize;

use super::cards::render_card;
use super::common::{now, ApiJson};
use super::error::ApiError;
use super::types::{ApiCard, DisplayMode, Shown};
use super::AppState;

#[derive(Serialize)]
pub struct ShowResponse {
    shown_at: String,
    frame_revision: u64,
    last_refresh_duration_ms: u64,
    /// True when this show stopped slideshow or daily mode by pinning the card.
    paused: bool,
}

pub async fn show(
    State(app): State<Arc<AppState>>,
    ApiJson(card): ApiJson<ApiCard>,
) -> Result<Json<ShowResponse>, ApiError> {
    show_by_hand(app, card).await.map(Json)
}

pub async fn clear(State(app): State<Arc<AppState>>) -> Result<Json<ShowResponse>, ApiError> {
    show_by_hand(app, ApiCard::Clear).await.map(Json)
}

/// Shows a card someone chose and pins it, so slideshow or daily mode does not replace
/// it a few minutes later. Re-showing the card already up (Refresh now) keeps the mode.
pub async fn show_by_hand(app: Arc<AppState>, card: ApiCard) -> Result<ShowResponse, ApiError> {
    let before = app.store.snapshot();
    let mut response = show_card(app.clone(), card.clone()).await?;
    if before.settings.mode != DisplayMode::Pinned && before.now_showing.as_ref() != Some(&card) {
        app.store.update(|state| {
            state.settings.mode = DisplayMode::Pinned;
            Ok(())
        })?;
        response.paused = true;
    }
    Ok(response)
}

/// Shows the slideshow's next card now; the interval restarts from here.
pub async fn next(State(app): State<Arc<AppState>>) -> Result<Json<ShowResponse>, ApiError> {
    let state = app.store.snapshot();
    if state.settings.mode != DisplayMode::Slideshow {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "not_slideshow",
            "The frame is not in slideshow mode.",
        ));
    }
    let roll = chrono::Utc::now().timestamp_subsec_nanos() as usize;
    let card = crate::scheduler::next_slideshow_card(&state, roll).ok_or_else(|| {
        ApiError::new(
            StatusCode::CONFLICT,
            "nothing_in_rotation",
            "Add photos or save a quote to start the slideshow.",
        )
    })?;
    show_card(app, card).await.map(Json)
}

/// Renders `card`, shows it on the panel and records it. Shared by the show endpoints and
/// the scheduler; fails with `panel_unavailable` while another update is in progress.
pub async fn show_card(app: Arc<AppState>, card: ApiCard) -> Result<ShowResponse, ApiError> {
    let _permit = app.show_gate.clone().try_acquire_owned().map_err(|_| {
        ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "panel_unavailable",
            "The display is already updating.",
        )
    })?;
    let rendered = render_card(app.clone(), card).await?;
    let shown_at = now();
    let duration_ms = app.display.show(rendered.frame.clone()).await?;
    let frame_png = rendered.frame.to_png();
    let revision = app.store.snapshot().frame_revision.saturating_add(1);
    let result = app.store.update(|state| {
        if let ApiCard::Photo { id } = &rendered.card {
            if let Some(photo) = state.photos.iter_mut().find(|photo| &photo.id == id) {
                photo.shown_count = photo.shown_count.saturating_add(1);
                photo.last_shown_at = Some(shown_at.clone());
            }
        }
        if let ApiCard::Quote {
            text,
            attribution,
            font,
            align,
        } = &rendered.card
        {
            if let Some(quote) = state.quotes.iter_mut().find(|quote| {
                quote.text == *text
                    && quote.attribution == *attribution
                    && Some(quote.font.as_str()) == font.as_deref()
                    && Some(quote.align) == *align
            }) {
                quote.shown_count = quote.shown_count.saturating_add(1);
                quote.last_shown_at = Some(shown_at.clone());
            }
        }
        state.record_shown(
            Shown {
                card: rendered.card.clone(),
                shown_at: shown_at.clone(),
                duration_ms,
                frame_revision: revision,
            },
            frame_png.clone(),
        );
        Ok(())
    });
    if let Err(error) = result {
        return Err(ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "storage_error",
            format!("The panel updated, but saving its state failed: {error}. The actual panel may differ from the saved current card."),
        ));
    }
    Ok(ShowResponse {
        shown_at,
        frame_revision: revision,
        last_refresh_duration_ms: duration_ms,
        paused: false,
    })
}

pub async fn current_frame(State(app): State<Arc<AppState>>) -> Result<Response, ApiError> {
    let state = app.store.snapshot();
    let bytes = state
        .current_frame_png
        .ok_or_else(|| ApiError::not_found("No frame has been shown yet."))?;
    let etag = HeaderValue::from_str(&format!("\"rev-{}\"", state.frame_revision))
        .map_err(|e| ApiError::internal(e.to_string()))?;
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, "image/png")
        .header(CACHE_CONTROL, "no-cache")
        .header(ETAG, etag)
        .body(Body::from(bytes))
        .expect("valid image response"))
}
