use std::io::Cursor;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::body::{to_bytes, Body};
use axum::http::{header, Method, Request, Response, StatusCode};
use image::{DynamicImage, GenericImageView, ImageFormat, Rgb, RgbImage};
use lovebox::api::{self, AppState};
use lovebox::config::{Config, PanelKind};
use lovebox::display::DisplayHandle;
use lovebox::library::Library;
use lovebox::panel::PanelError;
use lovebox::panel::{Panel, PngPanel};
use lovebox::store::Store;
use tempfile::TempDir;
use tower::ServiceExt;

fn app() -> (TempDir, axum::Router) {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(dir.path()).unwrap());
    let library = Library::new(dir.path(), store.clone(), 160);
    let panel_path = dir.path().join("panel");
    let display = DisplayHandle::start(
        move || Ok(Box::new(PngPanel::new(panel_path)) as Box<dyn Panel>),
        Duration::from_secs(1),
        Duration::from_secs(2),
    )
    .unwrap();
    let config = Config {
        bind: "127.0.0.1:0".parse().unwrap(),
        data_dir: PathBuf::from(dir.path()),
        panel: PanelKind::Png,
        decode_limit_mb: 160,
        log: "info".into(),
    };
    let state = Arc::new(AppState::new(config, store, library, display));
    (dir, api::router(state))
}

async fn request(
    app: &axum::Router,
    method: Method,
    uri: &str,
    content_type: Option<&str>,
    body: Vec<u8>,
) -> Response<Body> {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(content_type) = content_type {
        builder = builder.header(header::CONTENT_TYPE, content_type);
    }
    app.clone()
        .oneshot(builder.body(Body::from(body)).unwrap())
        .await
        .unwrap()
}

async fn json(response: Response<Body>) -> serde_json::Value {
    serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap()
}

fn json_body(value: serde_json::Value) -> Vec<u8> {
    serde_json::to_vec(&value).unwrap()
}

fn png_bytes() -> Vec<u8> {
    let image = RgbImage::from_fn(64, 48, |x, y| {
        Rgb([((x * 3) % 255) as u8, ((y * 5) % 255) as u8, 80])
    });
    let mut bytes = Cursor::new(Vec::new());
    DynamicImage::ImageRgb8(image)
        .write_to(&mut bytes, ImageFormat::Png)
        .unwrap();
    bytes.into_inner()
}

fn multipart(file: &[u8], filename: &str) -> (String, Vec<u8>) {
    let boundary = "test-boundary-301";
    let mut body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\nContent-Type: image/png\r\n\r\n"
    )
    .into_bytes();
    body.extend_from_slice(file);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    (format!("multipart/form-data; boundary={boundary}"), body)
}

#[tokio::test]
async fn local_api_and_web_contract_round_trip() {
    let (dir, app) = app();

    let response = request(&app, Method::GET, "/healthz", None, vec![]).await;
    assert_eq!(response.status(), StatusCode::OK);
    let health = json(response).await;
    assert_eq!(health["panel"]["state"], "ready");
    assert_eq!(health["quote_max_chars"], 2000);

    let response = request(&app, Method::GET, "/api/state", None, vec![]).await;
    assert_eq!(response.status(), StatusCode::OK);
    let state = json(response).await;
    assert_eq!(state["mode"], "pinned");
    assert_eq!(state["capabilities"]["scheduler"], false);
    assert_eq!(state["capabilities"]["daily"], true);
    assert_eq!(state["capabilities"]["slideshow"], true);
    assert_eq!(state["next_change_at"], serde_json::Value::Null);

    let response = request(&app, Method::GET, "/api/fonts", None, vec![]).await;
    assert_eq!(response.status(), StatusCode::OK);
    let fonts = json(response).await;
    let font_id = fonts[0]["id"].as_str().unwrap();
    let response = request(
        &app,
        Method::GET,
        &format!("/fonts/{font_id}.ttf"),
        None,
        vec![],
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CONTENT_TYPE], "font/ttf");
    let response = request(&app, Method::GET, "/fonts/missing.ttf", None, vec![]).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let response = request(&app, Method::GET, "/", None, vec![]).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert!(String::from_utf8(
        to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap()
    .contains("love-box"));
    for asset in ["/manifest.webmanifest", "/apple-touch-icon.png"] {
        let response = request(&app, Method::GET, asset, None, vec![]).await;
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "missing embedded asset {asset}"
        );
    }
    let response = request(&app, Method::GET, "/some/spa/route", None, vec![]).await;
    assert_eq!(response.status(), StatusCode::OK);
    let response = request(&app, Method::GET, "/assets/missing.js", None, vec![]).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    for uri in ["/api", "/api/nope"] {
        let response = request(&app, Method::GET, uri, None, vec![]).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert!(json(response).await["error"].is_object());
    }

    let response = request(&app, Method::GET, "/api/frame/current.png", None, vec![]).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let message = serde_json::json!({"type":"message","text":"Good morning","size":48});
    let response = request(
        &app,
        Method::POST,
        "/api/preview",
        Some("application/json"),
        json_body(message.clone()),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CONTENT_TYPE], "image/png");
    assert_eq!(response.headers()["x-lovebox-truncated"], "false");
    let preview = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let preview = image::load_from_memory(&preview).unwrap();
    assert_eq!(preview.dimensions(), (800, 480));

    let quote_card = serde_json::json!({"type":"quote","text":"A quiet morning","align":"center"});
    let response = request(
        &app,
        Method::POST,
        "/api/preview",
        Some("application/json"),
        json_body(quote_card.clone()),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert!(response.headers().contains_key("x-lovebox-fit-size-px"));
    let response = request(
        &app,
        Method::POST,
        "/api/preview",
        Some("application/json"),
        br#"{"type":"not-a-card"}"#.to_vec(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(json(response).await["error"].is_object());
    let response = request(
        &app,
        Method::POST,
        "/api/preview",
        Some("application/json"),
        json_body(serde_json::json!({"type":"photo","id":"01J00000000000000000000000"})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let response = request(
        &app,
        Method::POST,
        "/api/show",
        Some("application/json"),
        json_body(serde_json::json!({"type":"photo","id":"01J00000000000000000000000"})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let response = request(
        &app,
        Method::POST,
        "/api/show",
        Some("application/json"),
        json_body(message),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let shown = json(response).await;
    assert_eq!(shown["frame_revision"], 1);
    let response = request(&app, Method::GET, "/api/frame/current.png", None, vec![]).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::ETAG], "\"rev-1\"");
    let response = request(&app, Method::POST, "/api/clear", None, vec![]).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(json(response).await["frame_revision"], 2);

    let response = request(
        &app,
        Method::GET,
        "/api/photos?offset=0&limit=50",
        None,
        vec![],
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(json(response).await["total"], 0);
    let upload_defaults = serde_json::json!({
        "default_font":"eb-garamond","default_dither":"atkinson","enhance":false,
        "quiet_hours":{"start":"23:00","end":"07:00","enabled":false},
        "slideshow":{"interval_s":900,"order":"shuffle","set":"all"},"mode":"pinned"
    });
    let response = request(
        &app,
        Method::PUT,
        "/api/settings",
        Some("application/json"),
        json_body(upload_defaults),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let (content_type, body) = multipart(&png_bytes(), "sample.png");
    let response = request(&app, Method::POST, "/api/photos", Some(&content_type), body).await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let photo = json(response).await;
    let photo_id = photo["id"].as_str().unwrap().to_owned();
    assert_eq!(photo["filename_original"], "sample.png");
    assert!(photo["width"].as_u64().unwrap() <= 1600);
    assert_eq!(photo["dither"], "atkinson");
    assert_eq!(photo["enhance"], false);

    let response = request(
        &app,
        Method::GET,
        &format!("/api/photos/{photo_id}"),
        None,
        vec![],
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let response = request(
        &app,
        Method::GET,
        &format!("/api/photos/{photo_id}/thumb.jpg"),
        None,
        vec![],
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CONTENT_TYPE], "image/jpeg");
    let response = request(
        &app,
        Method::GET,
        &format!("/api/photos/{photo_id}/copy.jpg"),
        None,
        vec![],
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let response = request(
        &app,
        Method::GET,
        &format!("/api/photos/{photo_id}/frame.png"),
        None,
        vec![],
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CONTENT_TYPE], "image/png");
    let response = request(
        &app,
        Method::POST,
        "/api/show",
        Some("application/json"),
        json_body(serde_json::json!({"type":"photo","id":photo_id})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let response = request(
        &app,
        Method::PATCH,
        &format!("/api/photos/{photo_id}"),
        Some("application/json"),
        br#"{"caption":"A framed moment","focal":{"x":0.7,"y":0.4},"dither":"atkinson","in_rotation":false}"#.to_vec(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let patched = json(response).await;
    assert_eq!(patched["dither"], "atkinson");
    assert_eq!(patched["in_rotation"], false);
    assert_eq!(patched["caption"], "A framed moment");
    assert_eq!(patched["focal"], serde_json::json!({"x":0.7,"y":0.4}));
    let response = request(
        &app,
        Method::PATCH,
        &format!("/api/photos/{photo_id}"),
        Some("application/json"),
        br#"{"in_rotation":true}"#.to_vec(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let unchanged = json(response).await;
    assert_eq!(unchanged["caption"], "A framed moment");
    assert_eq!(unchanged["focal"], serde_json::json!({"x":0.7,"y":0.4}));
    let response = request(
        &app,
        Method::PATCH,
        &format!("/api/photos/{photo_id}"),
        Some("application/json"),
        br#"{"caption":null,"focal":null}"#.to_vec(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let cleared = json(response).await;
    assert_eq!(cleared["caption"], serde_json::Value::Null);
    assert_eq!(cleared["focal"], serde_json::Value::Null);
    let response = request(&app, Method::GET, "/api/photos?limit=0", None, vec![]).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let response = request(&app, Method::GET, "/api/photos?offset=nope", None, vec![]).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(json(response).await["error"].is_object());

    let response = request(
        &app,
        Method::DELETE,
        &format!("/api/photos/{photo_id}"),
        None,
        vec![],
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let response = request(
        &app,
        Method::GET,
        &format!("/api/photos/{photo_id}"),
        None,
        vec![],
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    for suffix in ["thumb.jpg", "copy.jpg", "frame.png"] {
        let response = request(
            &app,
            Method::GET,
            &format!("/api/photos/01J00000000000000000000000/{suffix}"),
            None,
            vec![],
        )
        .await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
    let (content_type, body) = multipart(&png_bytes(), "oversized-length.png");
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/photos")
                .header(header::CONTENT_TYPE, content_type)
                .header(header::CONTENT_LENGTH, 64 * 1024 * 1024 + 1)
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert!(json(response).await["error"].is_object());

    let response = request(&app, Method::GET, "/api/quotes", None, vec![]).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(json(response).await["total"], 0);
    let response = request(
        &app,
        Method::POST,
        "/api/quotes",
        Some("application/json"),
        json_body(serde_json::json!({"text":"A kind thought","attribution":"A reader"})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let quote = json(response).await;
    let quote_id = quote["id"].as_str().unwrap().to_owned();
    let response = request(
        &app,
        Method::GET,
        &format!("/api/quotes/{quote_id}"),
        None,
        vec![],
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let response = request(
        &app,
        Method::PATCH,
        &format!("/api/quotes/{quote_id}"),
        Some("application/json"),
        br#"{"text":"A kinder thought","attribution":"A different reader","in_rotation":false}"#
            .to_vec(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let updated_quote = json(response).await;
    assert_eq!(updated_quote["in_rotation"], false);
    assert_eq!(updated_quote["attribution"], "A different reader");
    let response = request(
        &app,
        Method::PATCH,
        &format!("/api/quotes/{quote_id}"),
        Some("application/json"),
        br#"{"text":"A thought that remains"}"#.to_vec(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(json(response).await["attribution"], "A different reader");
    let response = request(
        &app,
        Method::PATCH,
        &format!("/api/quotes/{quote_id}"),
        Some("application/json"),
        br#"{"attribution":null}"#.to_vec(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(json(response).await["attribution"], serde_json::Value::Null);
    let long_quote = "a".repeat(2001);
    let response = request(
        &app,
        Method::POST,
        "/api/quotes",
        Some("application/json"),
        json_body(serde_json::json!({"text":long_quote})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(json(response).await["error"]["message"]
        .as_str()
        .unwrap()
        .contains("maximum 2000"));
    let response = request(
        &app,
        Method::DELETE,
        &format!("/api/quotes/{quote_id}"),
        None,
        vec![],
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let response = request(
        &app,
        Method::GET,
        &format!("/api/quotes/{quote_id}"),
        None,
        vec![],
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let response = request(&app, Method::GET, "/api/settings", None, vec![]).await;
    assert_eq!(response.status(), StatusCode::OK);
    let settings: serde_json::Value = serde_json::json!({
        "default_font":"eb-garamond","default_dither":"atkinson","enhance":false,
        "quiet_hours":{"start":"22:30","end":"06:30","enabled":true},
        "slideshow":{"interval_s":60,"order":"upload","set":"all"},"mode":"slideshow"
    });
    let response = request(
        &app,
        Method::PUT,
        "/api/settings",
        Some("application/json"),
        json_body(settings.clone()),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let saved = json(response).await;
    assert_eq!(saved["scheduler"], "daily_slideshow");
    assert_eq!(saved["daily"]["time"], "07:00");
    assert_eq!(saved["mode"], "slideshow");
    let mut invalid_settings = settings;
    invalid_settings["slideshow"]["interval_s"] = serde_json::json!(10);
    let response = request(
        &app,
        Method::PUT,
        "/api/settings",
        Some("application/json"),
        json_body(invalid_settings),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    // A panel update cannot be rolled back if persistence fails afterwards. The API reports
    // that partial outcome and keeps the saved frame revision and now_showing unchanged.
    let panel_png = dir.path().join("panel/frame.png");
    let panel_before = std::fs::read(&panel_png).unwrap();
    let current_before = request(&app, Method::GET, "/api/frame/current.png", None, vec![]).await;
    let current_before = to_bytes(current_before.into_body(), usize::MAX)
        .await
        .unwrap()
        .to_vec();
    std::fs::remove_file(dir.path().join("state.json")).unwrap();
    std::fs::create_dir(dir.path().join("state.json")).unwrap();
    let response = request(
        &app,
        Method::POST,
        "/api/show",
        Some("application/json"),
        json_body(serde_json::json!({"type":"message","text":"New panel content","size":30})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let failure = json(response).await;
    assert!(failure["error"]["message"]
        .as_str()
        .unwrap()
        .contains("panel updated, but saving its state failed"));
    let panel_after = std::fs::read(panel_png).unwrap();
    assert_ne!(panel_after, panel_before);
    let current_after = request(&app, Method::GET, "/api/frame/current.png", None, vec![]).await;
    assert_eq!(
        to_bytes(current_after.into_body(), usize::MAX)
            .await
            .unwrap()
            .as_ref(),
        current_before
    );
    let state_after = request(&app, Method::GET, "/api/state", None, vec![]).await;
    let state_after = json(state_after).await;
    assert_eq!(state_after["frame_revision"], 3);
    assert_eq!(state_after["now_showing"]["type"], "photo");
}

struct StuckPanel;

impl Panel for StuckPanel {
    fn show(&mut self, _: &lovebox::frame::Frame) -> Result<(), PanelError> {
        loop {
            std::thread::sleep(Duration::from_secs(60));
        }
    }

    fn clear(&mut self) -> Result<(), PanelError> {
        loop {
            std::thread::sleep(Duration::from_secs(60));
        }
    }

    fn sleep(&mut self) -> Result<(), PanelError> {
        Ok(())
    }

    fn name(&self) -> &'static str {
        "stuck-test"
    }
}

#[tokio::test]
async fn wedged_display_is_reported_by_health_and_refuses_more_mutations() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(dir.path()).unwrap());
    let library = Library::new(dir.path(), store.clone(), 160);
    let display = DisplayHandle::start(
        || Ok(Box::new(StuckPanel) as Box<dyn Panel>),
        Duration::from_secs(1),
        Duration::from_secs(1),
    )
    .unwrap();
    let config = Config {
        bind: "127.0.0.1:0".parse().unwrap(),
        data_dir: dir.path().to_path_buf(),
        panel: PanelKind::Png,
        decode_limit_mb: 160,
        log: "info".into(),
    };
    let state = Arc::new(AppState::new(config, store.clone(), library, display));
    let app = api::router(state);

    let response = request(&app, Method::POST, "/api/clear", None, vec![]).await;
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert!(json(response).await["error"].is_object());
    assert_eq!(store.snapshot().frame_revision, 0);
    assert!(store.snapshot().now_showing.is_none());

    let response = request(&app, Method::GET, "/healthz", None, vec![]).await;
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(json(response).await["panel"]["state"], "wedged");
    let response = request(
        &app,
        Method::POST,
        "/api/show",
        Some("application/json"),
        br#"{"type":"clear"}"#.to_vec(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}
