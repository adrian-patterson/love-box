use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::render::{photo, Dither, IngestLimits};

static BOUNDARY_COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn import_files(
    files: &[std::path::PathBuf],
    host: &str,
    caption: Option<&str>,
    dither: Option<Dither>,
) -> Result<(), String> {
    let mut failures = Vec::new();
    for path in files {
        match import_file(path, host, caption, dither) {
            Ok(id) => println!("{id}"),
            Err(error) => {
                eprintln!("{}: {error}", path.display());
                failures.push(format!("{}: {error}", path.display()));
            }
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!("{} import(s) failed", failures.len()))
    }
}

pub fn import_file(
    path: &Path,
    host: &str,
    caption: Option<&str>,
    dither: Option<Dither>,
) -> Result<String, String> {
    let bounded = photo::ingest(path, &IngestLimits::native()).map_err(|e| e.to_string())?;
    let jpeg = photo::encode_jpeg(&bounded, 90).map_err(|e| e.to_string())?;
    let counter = BOUNDARY_COUNTER.fetch_add(1, Ordering::Relaxed);
    let boundary = format!("lovebox-import-{}-{counter}", std::process::id());
    let mut body = Vec::with_capacity(jpeg.len() + 1024);
    let filename = safe_filename(
        path.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .as_ref(),
    );
    append_part(
        &mut body,
        &boundary,
        "file",
        Some(&filename),
        "image/jpeg",
        &jpeg,
    );
    if let Some(caption) = caption {
        append_part(
            &mut body,
            &boundary,
            "caption",
            None,
            "text/plain; charset=utf-8",
            caption.as_bytes(),
        );
    }
    if let Some(dither) = dither {
        let value = match dither {
            Dither::FloydSteinberg => "floyd_steinberg",
            Dither::Atkinson => "atkinson",
        };
        append_part(
            &mut body,
            &boundary,
            "dither",
            None,
            "text/plain",
            value.as_bytes(),
        );
    }
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    let url = format!("{}/api/photos", host.trim_end_matches('/'));
    let response = ureq::post(&url)
        .set(
            "Content-Type",
            &format!("multipart/form-data; boundary={boundary}"),
        )
        .send_bytes(&body)
        .map_err(|e| e.to_string())?;
    let payload = response.into_string().map_err(|e| e.to_string())?;
    let value: serde_json::Value =
        serde_json::from_str(&payload).map_err(|e| format!("server returned invalid JSON: {e}"))?;
    value
        .get("id")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| format!("server response did not include a photo id: {payload}"))
}

fn append_part(
    body: &mut Vec<u8>,
    boundary: &str,
    name: &str,
    filename: Option<&str>,
    mime: &str,
    bytes: &[u8],
) {
    body.extend_from_slice(
        format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"").as_bytes(),
    );
    if let Some(filename) = filename {
        body.extend_from_slice(format!("; filename=\"{filename}\"").as_bytes());
    }
    body.extend_from_slice(format!("\r\nContent-Type: {mime}\r\n\r\n").as_bytes());
    body.extend_from_slice(bytes);
    body.extend_from_slice(b"\r\n");
}

fn safe_filename(value: &str) -> String {
    let cleaned: String = value
        .chars()
        .filter(|c| !matches!(c, '\r' | '\n' | '"' | '\\'))
        .take(200)
        .collect();
    if cleaned.is_empty() {
        "photo.jpg".to_owned()
    } else {
        cleaned
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filenames_cannot_inject_multipart_headers() {
        assert_eq!(safe_filename("../a\"b\r\n.jpg"), "../ab.jpg");
    }
}
