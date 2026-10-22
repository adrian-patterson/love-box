use std::collections::HashMap;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::rejection::QueryRejection;
use axum::extract::{FromRequest, Multipart, Path, Query, Request, State};
use axum::http::header::CONTENT_TYPE;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use tokio::task;

use super::common::ApiJson;
use super::error::ApiError;
use super::types::{DitherChoice, FocalPoint, PhotoMeta};
use super::AppState;
use crate::library::{PhotoOptions, PhotoPatch};

const MAX_UPLOAD_BYTES: usize = 64 * 1024 * 1024;
const MAX_TEXT_FIELD_BYTES: usize = 16 * 1024;

#[derive(Deserialize)]
pub struct PhotoListQuery {
    offset: Option<usize>,
    limit: Option<usize>,
}

#[derive(Deserialize, Default)]
pub struct PhotoPatchRequest {
    pub caption: Option<serde_json::Value>,
    pub focal: Option<serde_json::Value>,
    pub dither: Option<DitherChoice>,
    pub enhance: Option<bool>,
    pub in_rotation: Option<bool>,
}

pub async fn list(
    State(app): State<Arc<AppState>>,
    query: Result<Query<PhotoListQuery>, QueryRejection>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let Query(query) = query.map_err(|_| ApiError::bad("invalid photo pagination query"))?;
    let offset = query.offset.unwrap_or(0);
    let limit = query.limit.unwrap_or(50);
    if limit == 0 || limit > 200 {
        return Err(ApiError::bad("limit must be between 1 and 200"));
    }
    let (items, total) = app.library.list(offset, limit);
    Ok(Json(serde_json::json!({"items":items,"total":total})))
}

pub async fn get(
    State(app): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<PhotoMeta>, ApiError> {
    Ok(Json(app.library.get(&id)?))
}

pub async fn upload(
    State(app): State<Arc<AppState>>,
    request: Request,
) -> Result<impl IntoResponse, ApiError> {
    // Acquire before reading multipart fields so concurrent uploads do not retain bodies while waiting.
    let permit = app.library.acquire_ingest().await?;
    if request
        .headers()
        .get(axum::http::header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .is_some_and(|length| length > MAX_UPLOAD_BYTES as u64)
    {
        return Err(ApiError::too_large(
            "upload exceeds the 64 MiB request limit",
        ));
    }
    let mut multipart = Multipart::from_request(request, &app)
        .await
        .map_err(|error| multipart_error(error.into_response().status()))?;
    let mut file: Option<(String, Vec<u8>)> = None;
    let mut fields = HashMap::<String, String>::new();
    let mut total = 0usize;
    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|error| multipart_error(error.into_response().status()))?
    {
        let name = field.name().unwrap_or_default().to_owned();
        let file_name = field.file_name().map(str::to_owned);
        if name != "file"
            && !matches!(
                name.as_str(),
                "caption"
                    | "focal_x"
                    | "focal_y"
                    | "dither"
                    | "enhance"
                    | "source_id"
                    | "in_rotation"
            )
        {
            return Err(ApiError::bad(format!("unknown multipart field '{name}'")));
        }
        if name == "file" && file.is_some() {
            return Err(ApiError::bad("upload exactly one file per request"));
        }
        let max_for_field = if name == "file" {
            MAX_UPLOAD_BYTES
        } else {
            MAX_TEXT_FIELD_BYTES
        };
        let mut bytes = Vec::new();
        while let Some(chunk) = field
            .chunk()
            .await
            .map_err(|error| multipart_error(error.into_response().status()))?
        {
            total = total.saturating_add(chunk.len());
            if total > MAX_UPLOAD_BYTES {
                return Err(ApiError::too_large(
                    "upload exceeds the 64 MiB request limit",
                ));
            }
            if bytes.len().saturating_add(chunk.len()) > max_for_field {
                return Err(ApiError::too_large(if name == "file" {
                    "file exceeds the 64 MiB upload limit"
                } else {
                    "multipart text fields must be at most 16 KiB"
                }));
            }
            bytes.extend_from_slice(&chunk);
        }
        if name == "file" {
            let filename =
                file_name.ok_or_else(|| ApiError::bad("file part must include a filename"))?;
            file = Some((filename, bytes));
        } else {
            let value = String::from_utf8(bytes)
                .map_err(|_| ApiError::bad(format!("multipart field '{name}' must be UTF-8")))?;
            fields.insert(name, value);
        }
    }
    let (filename, bytes) =
        file.ok_or_else(|| ApiError::bad("multipart field 'file' is required"))?;
    let focal_x = parse_fraction(fields.get("focal_x"), "focal_x")?;
    let focal_y = parse_fraction(fields.get("focal_y"), "focal_y")?;
    let focal = match (focal_x, focal_y) {
        (Some(x), Some(y)) => Some(FocalPoint { x, y }),
        (None, None) => None,
        _ => {
            return Err(ApiError::bad(
                "focal_x and focal_y must be supplied together",
            ))
        }
    };
    let dither = fields
        .get("dither")
        .map(|value| parse_dither(value))
        .transpose()?
        .unwrap_or_default();
    let enhance = fields
        .get("enhance")
        .map(|value| parse_bool(value, "enhance"))
        .transpose()?;
    let in_rotation = fields
        .get("in_rotation")
        .map(|value| parse_bool(value, "in_rotation"))
        .transpose()?;
    let options = PhotoOptions {
        caption: fields.remove("caption"),
        focal,
        dither,
        enhance,
        source_id: fields.remove("source_id"),
        in_rotation,
    };
    let meta = app
        .library
        .add_from_bytes_with_permit(permit, bytes, filename, options)
        .await?;
    Ok((StatusCode::CREATED, Json(meta)))
}

pub async fn thumb(
    State(app): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    file_response(&app, &id, "thumb.jpg", "image/jpeg").await
}

pub async fn copy(
    State(app): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    file_response(&app, &id, "copy.jpg", "image/jpeg").await
}

pub async fn frame(
    State(app): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    file_response(&app, &id, "frame.png", "image/png").await
}

async fn file_response(
    app: &AppState,
    id: &str,
    name: &str,
    content_type: &str,
) -> Result<Response, ApiError> {
    let path = app.library.file_path(id, name)?;
    let bytes = tokio::fs::read(path)
        .await
        .map_err(|e| ApiError::internal(format!("Could not read photo file: {e}")))?;
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, content_type)
        .body(Body::from(bytes))
        .expect("valid photo file response"))
}

pub async fn patch(
    State(app): State<Arc<AppState>>,
    Path(id): Path<String>,
    ApiJson(request): ApiJson<PhotoPatchRequest>,
) -> Result<Json<PhotoMeta>, ApiError> {
    let caption = request.caption.map(parse_nullable_string).transpose()?;
    let focal = request.focal.map(parse_nullable_focal).transpose()?;
    let patch = PhotoPatch {
        caption,
        focal,
        dither: request.dither,
        enhance: request.enhance,
        in_rotation: request.in_rotation,
    };
    Ok(Json(app.library.patch(id, patch).await?))
}

pub async fn delete(
    State(app): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let library = app.library.clone();
    task::spawn_blocking(move || library.remove(&id))
        .await
        .map_err(|e| ApiError::internal(format!("Photo delete worker failed: {e}")))??;
    Ok(StatusCode::NO_CONTENT)
}

fn parse_fraction(value: Option<&String>, name: &str) -> Result<Option<f32>, ApiError> {
    value
        .map(|value| {
            value
                .parse::<f32>()
                .ok()
                .filter(|value| value.is_finite() && (0.0..=1.0).contains(value))
                .ok_or_else(|| ApiError::bad(format!("{name} must be a number in the range 0..=1")))
        })
        .transpose()
}

fn multipart_error(status: StatusCode) -> ApiError {
    if status == StatusCode::PAYLOAD_TOO_LARGE {
        ApiError::too_large("upload exceeds the 64 MiB request limit")
    } else {
        ApiError::bad("Invalid multipart upload.")
    }
}

fn parse_dither(value: &str) -> Result<DitherChoice, ApiError> {
    match value {
        "floyd_steinberg" => Ok(DitherChoice::FloydSteinberg),
        "atkinson" => Ok(DitherChoice::Atkinson),
        _ => Err(ApiError::bad(
            "dither must be 'floyd_steinberg' or 'atkinson'",
        )),
    }
}

fn parse_bool(value: &str, name: &str) -> Result<bool, ApiError> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(ApiError::bad(format!("{name} must be 'true' or 'false'"))),
    }
}

fn parse_nullable_string(value: serde_json::Value) -> Result<Option<String>, ApiError> {
    match value {
        serde_json::Value::Null => Ok(None),
        serde_json::Value::String(value) => Ok(Some(value)),
        _ => Err(ApiError::bad("caption must be a string or null")),
    }
}

fn parse_nullable_focal(value: serde_json::Value) -> Result<Option<FocalPoint>, ApiError> {
    if value.is_null() {
        return Ok(None);
    }
    let focal: FocalPoint = serde_json::from_value(value)
        .map_err(|_| ApiError::bad("focal must be null or an object with numeric x and y"))?;
    if !focal.x.is_finite()
        || !focal.y.is_finite()
        || !(0.0..=1.0).contains(&focal.x)
        || !(0.0..=1.0).contains(&focal.y)
    {
        return Err(ApiError::bad(
            "focal coordinates must be in the range 0..=1",
        ));
    }
    Ok(Some(focal))
}
