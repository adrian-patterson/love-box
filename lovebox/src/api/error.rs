use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

use crate::display::DisplayError;
use crate::library::LibraryError;
use crate::render::RenderError;
use crate::store::StoreError;

#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: String,
}

#[derive(Serialize)]
struct ErrorEnvelope<'a> {
    error: ErrorBody<'a>,
}

#[derive(Serialize)]
struct ErrorBody<'a> {
    code: &'a str,
    message: &'a str,
}

impl ApiError {
    pub fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
        }
    }
    pub fn bad(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "validation_error", message)
    }
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, "not_found", message)
    }
    pub fn too_large(message: impl Into<String>) -> Self {
        Self::new(StatusCode::PAYLOAD_TOO_LARGE, "too_large", message)
    }
    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "internal_error", message)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = ErrorEnvelope {
            error: ErrorBody {
                code: self.code,
                message: &self.message,
            },
        };
        (self.status, axum::Json(body)).into_response()
    }
}

impl From<RenderError> for ApiError {
    fn from(error: RenderError) -> Self {
        let status = match &error {
            RenderError::TooLarge(_) => StatusCode::PAYLOAD_TOO_LARGE,
            RenderError::Io(_) => StatusCode::INTERNAL_SERVER_ERROR,
            _ => StatusCode::BAD_REQUEST,
        };
        let code = if status == StatusCode::PAYLOAD_TOO_LARGE {
            "too_large"
        } else if status == StatusCode::INTERNAL_SERVER_ERROR {
            "internal_error"
        } else {
            "validation_error"
        };
        Self::new(status, code, error.to_string())
    }
}

impl From<LibraryError> for ApiError {
    fn from(error: LibraryError) -> Self {
        match error {
            LibraryError::NotFound(id) => Self::not_found(format!("Photo '{id}' was not found.")),
            LibraryError::Invalid(message) => Self::bad(message),
            LibraryError::Render(error) => error.into(),
            LibraryError::Store(error) => error.into(),
            LibraryError::Io(error) => Self::internal(format!("Photo library I/O failed: {error}")),
            LibraryError::Image(error) => {
                Self::internal(format!("Photo library image read failed: {error}"))
            }
            LibraryError::Worker(error) => Self::internal(format!("Photo worker failed: {error}")),
        }
    }
}

impl From<StoreError> for ApiError {
    fn from(error: StoreError) -> Self {
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "storage_error",
            format!("Could not save local state: {error}"),
        )
    }
}

impl From<DisplayError> for ApiError {
    fn from(error: DisplayError) -> Self {
        Self::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "panel_unavailable",
            error.to_string(),
        )
    }
}
