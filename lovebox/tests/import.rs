use std::io::Cursor;
use std::sync::Arc;
use std::time::Duration;

use axum::body::{to_bytes, Body};
use axum::http::{Method, Request};
use image::{DynamicImage, ImageBuffer, ImageFormat, Luma, Rgb, RgbImage};
use lovebox::api::{self, AppState};
use lovebox::config::{Config, PanelKind};
use lovebox::display::DisplayHandle;
use lovebox::import::import_file;
use lovebox::library::Library;
use lovebox::panel::{Panel, PngPanel};
use lovebox::store::Store;
use tempfile::TempDir;
use tower::ServiceExt;

fn app() -> (TempDir, Arc<Store>, axum::Router) {
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
        data_dir: dir.path().to_path_buf(),
        panel: PanelKind::Png,
        decode_limit_mb: 160,
        log: "info".into(),
    };
    let state = Arc::new(AppState::new(config, store.clone(), library, display));
    (dir, store, api::router(state))
}

#[tokio::test]
async fn import_cli_bounds_large_jpeg_and_16_bit_tiff_before_upload() {
    let (dir, store, app) = app();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let host = format!("http://{}", listener.local_addr().unwrap());
    let app_for_test = app.clone();
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let jpeg_path = dir.path().join("large.jpg");
    let rgb = RgbImage::from_pixel(6000, 4000, Rgb([132, 91, 54]));
    let mut jpeg = Cursor::new(Vec::new());
    DynamicImage::ImageRgb8(rgb)
        .write_to(&mut jpeg, ImageFormat::Jpeg)
        .unwrap();
    std::fs::write(&jpeg_path, jpeg.into_inner()).unwrap();
    let jpeg_host = host.clone();
    let jpeg_file = jpeg_path.clone();
    let jpeg_id = tokio::task::spawn_blocking(move || {
        import_file(&jpeg_file, &jpeg_host, Some("large JPEG"), None).unwrap()
    })
    .await
    .unwrap();

    let tiff_path = dir.path().join("large-16-bit.tiff");
    let luma: ImageBuffer<Luma<u16>, Vec<u16>> =
        ImageBuffer::from_pixel(6000, 4000, Luma([32_000]));
    let mut tiff = Cursor::new(Vec::new());
    DynamicImage::ImageLuma16(luma)
        .write_to(&mut tiff, ImageFormat::Tiff)
        .unwrap();
    std::fs::write(&tiff_path, tiff.into_inner()).unwrap();
    let tiff_host = host.clone();
    let tiff_file = tiff_path.clone();
    let tiff_id = tokio::task::spawn_blocking(move || {
        import_file(&tiff_file, &tiff_host, None, None).unwrap()
    })
    .await
    .unwrap();

    let state = store.snapshot();
    assert_eq!(state.photos.len(), 2);
    for id in [&jpeg_id, &tiff_id] {
        let photo = state.photos.iter().find(|photo| photo.id == *id).unwrap();
        assert!(photo.width <= 1600);
        assert!(photo.height <= 960);
    }
    let response = app_for_test
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/photos")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let value: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(value["total"], 2);
    server.abort();
}
