use axum::extract::{FromRequest, Request};
use axum::http::StatusCode;
use axum::Json;
use serde::de::DeserializeOwned;
use serde::Serialize;

use super::error::ApiError;
use crate::display::{DisplayState, DisplayStatus};

#[derive(Clone, Debug, Serialize)]
pub struct PanelView {
    pub name: String,
    pub state: &'static str,
    pub last_show_at: Option<String>,
    pub last_error: Option<String>,
    pub shows_total: u64,
    pub last_refresh_duration_ms: Option<u64>,
}

impl From<DisplayStatus> for PanelView {
    fn from(value: DisplayStatus) -> Self {
        Self {
            name: value.name,
            state: match value.state {
                DisplayState::Idle => "ready",
                DisplayState::Busy => "busy",
                DisplayState::Wedged => "wedged",
            },
            last_show_at: value.last_show_at,
            last_error: value.last_error,
            shows_total: value.shows_total,
            last_refresh_duration_ms: value.last_refresh_duration_ms,
        }
    }
}

pub struct ApiJson<T>(pub T);

impl<S, T> FromRequest<S> for ApiJson<T>
where
    S: Send + Sync,
    T: DeserializeOwned,
{
    type Rejection = ApiError;

    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        Json::<T>::from_request(request, state)
            .await
            .map(|Json(value)| Self(value))
            .map_err(|error| {
                ApiError::new(StatusCode::BAD_REQUEST, "bad_request", error.body_text())
            })
    }
}

pub fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}
