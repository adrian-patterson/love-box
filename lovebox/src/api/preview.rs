use std::sync::Arc;

use axum::body::Body;
use axum::extract::State;
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use axum::http::{HeaderName, HeaderValue, StatusCode};
use axum::response::Response;

use super::cards::render_card;
use super::common::ApiJson;
use super::error::ApiError;
use super::types::ApiCard;
use super::AppState;

pub async fn preview(
    State(app): State<Arc<AppState>>,
    ApiJson(card): ApiJson<ApiCard>,
) -> Result<Response, ApiError> {
    let rendered = render_card(app, card).await?;
    let mut response = Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, "image/png")
        .header(CACHE_CONTROL, "no-store");
    if let Some(size) = rendered.metadata.fit_size_px {
        response = response.header("x-lovebox-fit-size-px", HeaderValue::from(size));
    }
    if let Some(truncated) = rendered.metadata.truncated {
        response = response.header(
            HeaderName::from_static("x-lovebox-truncated"),
            if truncated { "true" } else { "false" },
        );
    }
    Ok(response
        .body(Body::from(rendered.frame.to_png()))
        .expect("valid preview response"))
}
