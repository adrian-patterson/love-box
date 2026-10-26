use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::{NaiveDateTime, Utc};
use exif::{In, Tag};
use image::{ImageFormat, RgbImage};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::task;
use tracing::warn;
use ulid::Ulid;

use crate::api::types::{DitherChoice, FocalPoint, PhotoMeta};
use crate::frame::{Frame, Threshold};
use crate::render::{self, photo, Dither, IngestLimits, PhotoSpec, RenderError};
use crate::store::{Store, StoreError};

#[derive(Clone, Debug, Default)]
pub struct PhotoOptions {
    pub caption: Option<String>,
    pub focal: Option<FocalPoint>,
    pub dither: DitherChoice,
    pub enhance: Option<bool>,
    pub source_id: Option<String>,
    pub in_rotation: Option<bool>,
}

#[derive(Clone)]
pub struct Library {
    root: PathBuf,
    store: Arc<Store>,
    ingest_limits: IngestLimits,
    ingest_gate: Arc<Semaphore>,
}

impl Library {
    pub fn new(data_dir: impl AsRef<Path>, store: Arc<Store>, decode_limit_mb: u64) -> Self {
        Self {
            root: data_dir.as_ref().join("photos"),
            store,
            ingest_limits: IngestLimits::pi()
                .with_max_alloc(decode_limit_mb.saturating_mul(1024 * 1024)),
            ingest_gate: Arc::new(Semaphore::new(1)),
        }
    }

    pub async fn acquire_ingest(&self) -> Result<OwnedSemaphorePermit, LibraryError> {
        self.ingest_gate
            .clone()
            .acquire_owned()
            .await
            .map_err(|e| LibraryError::Worker(e.to_string()))
    }

    pub fn photo_dir(&self, id: &str) -> Result<PathBuf, LibraryError> {
        validate_id(id)?;
        Ok(self.root.join(id))
    }

    pub async fn add_from_bytes(
        &self,
        bytes: Vec<u8>,
        filename: String,
        options: PhotoOptions,
    ) -> Result<PhotoMeta, LibraryError> {
        let permit = self.acquire_ingest().await?;
        self.add_from_bytes_with_permit(permit, bytes, filename, options)
            .await
    }

    pub async fn add_from_bytes_with_permit(
        &self,
        permit: OwnedSemaphorePermit,
        bytes: Vec<u8>,
        filename: String,
        options: PhotoOptions,
    ) -> Result<PhotoMeta, LibraryError> {
        let this = self.clone();
        task::spawn_blocking(move || {
            let _permit = permit;
            this.add_sync(&bytes, &filename, options)
        })
        .await
        .map_err(|e| LibraryError::Worker(e.to_string()))?
    }

    fn add_sync(
        &self,
        bytes: &[u8],
        filename: &str,
        options: PhotoOptions,
    ) -> Result<PhotoMeta, LibraryError> {
        validate_options(&options)?;
        std::fs::create_dir_all(&self.root)?;
        let image = photo::ingest(bytes, &self.ingest_limits)?;
        let id = Ulid::new().to_string();
        let dir = self.root.join(&id);
        std::fs::create_dir(&dir)?;
        let result = (|| {
            let copy = photo::encode_jpeg(&image, 88)?;
            std::fs::write(dir.join("copy.jpg"), copy)?;
            let thumb = thumbnail(&image);
            std::fs::write(dir.join("thumb.jpg"), photo::encode_jpeg(&thumb, 84)?)?;
            let spec = photo_spec(&options);
            let frame = photo::frame(&image, &spec);
            std::fs::write(dir.join("frame.png"), frame.to_png())?;
            let (width, height) = image.dimensions();
            let meta = PhotoMeta {
                id: id.clone(),
                filename_original: basename(filename),
                caption: options.caption,
                taken_at: exif_taken_at(bytes),
                added_at: Utc::now().to_rfc3339(),
                width,
                height,
                source_id: options.source_id,
                focal: options.focal,
                zoom: 1.0,
                dither: options.dither,
                enhance: options.enhance.unwrap_or(true),
                shown_count: 0,
                last_shown_at: None,
                tags: Vec::new(),
                in_rotation: options.in_rotation.unwrap_or(true),
            };
            self.store.update(|state| {
                state.photos.push(meta.clone());
                state.photos.sort_by(|a, b| b.added_at.cmp(&a.added_at));
                Ok(())
            })?;
            Ok(meta)
        })();
        if result.is_err() {
            let _ = std::fs::remove_dir_all(&dir);
        }
        result
    }

    pub fn get(&self, id: &str) -> Result<PhotoMeta, LibraryError> {
        validate_id(id)?;
        self.store
            .snapshot()
            .photos
            .into_iter()
            .find(|photo| photo.id == id)
            .ok_or_else(|| LibraryError::NotFound(id.to_owned()))
    }

    /// Rebuilds every thumbnail whose shape does not match its photo, from the stored
    /// copy. Returns how many were rebuilt. Blocking; run once at startup.
    pub fn repair_thumbnails(&self) -> Result<usize, LibraryError> {
        let mut repaired = 0;
        for meta in self.store.snapshot().photos {
            let dir = self.photo_dir(&meta.id)?;
            let thumb_path = dir.join("thumb.jpg");
            let misshapen = match image::image_dimensions(&thumb_path) {
                Ok((w, h)) => thumbnail_misshapen(meta.width, meta.height, w, h),
                Err(_) => true,
            };
            if !misshapen {
                continue;
            }
            let copy = image::open(dir.join("copy.jpg"))?.to_rgb8();
            let staged = dir.join("thumb.jpg.new");
            std::fs::write(&staged, photo::encode_jpeg(&thumbnail(&copy), 84)?)?;
            std::fs::rename(&staged, &thumb_path)?;
            repaired += 1;
        }
        Ok(repaired)
    }

    pub fn load_copy(&self, id: &str) -> Result<(PhotoMeta, RgbImage), LibraryError> {
        let meta = self.get(id)?;
        let path = self.file_path(id, "copy.jpg")?;
        let image = image::open(path)?.to_rgb8();
        Ok((meta, image))
    }

    pub fn remove(&self, id: &str) -> Result<(), LibraryError> {
        validate_id(id)?;
        self.get(id)?;
        let photo_dir = self.root.join(id);
        let staged = self.root.join(format!(".delete-{id}-{}", Ulid::new()));
        if photo_dir.exists() {
            std::fs::rename(&photo_dir, &staged)?;
        }
        let update = self.store.update(|state| {
            state.photos.retain(|photo| photo.id != id);
            Ok(())
        });
        if let Err(error) = update {
            if staged.exists() {
                let _ = std::fs::rename(&staged, &photo_dir);
            }
            return Err(error.into());
        }
        if staged.exists() {
            if let Err(e) = std::fs::remove_dir_all(&staged) {
                warn!(id, error = %e, "photo was removed from state but staged files could not be deleted");
            }
        }
        Ok(())
    }

    pub fn frame(&self, id: &str) -> Result<Frame, LibraryError> {
        let path = self.photo_dir(id)?.join("frame.png");
        let image = image::open(&path)?.to_luma8();
        Ok(Frame::from_gray(&image, Threshold(127)))
    }

    pub fn file_path(&self, id: &str, name: &str) -> Result<PathBuf, LibraryError> {
        let allowed = matches!(name, "thumb.jpg" | "copy.jpg" | "frame.png");
        if !allowed {
            return Err(LibraryError::Invalid("unsupported photo file".to_owned()));
        }
        let path = self.photo_dir(id)?.join(name);
        if !path.is_file() {
            return Err(LibraryError::NotFound(id.to_owned()));
        }
        Ok(path)
    }

    pub async fn patch(&self, id: String, patch: PhotoPatch) -> Result<PhotoMeta, LibraryError> {
        let this = self.clone();
        task::spawn_blocking(move || this.patch_sync(&id, patch))
            .await
            .map_err(|e| LibraryError::Worker(e.to_string()))?
    }

    fn patch_sync(&self, id: &str, patch: PhotoPatch) -> Result<PhotoMeta, LibraryError> {
        validate_id(id)?;
        if let Some(Some(focal)) = &patch.focal {
            validate_focal(focal)?;
        }
        if let Some(zoom) = patch.zoom {
            validate_zoom(zoom)?;
        }
        let old = self.get(id)?;
        let mut updated = old.clone();
        if let Some(caption) = patch.caption {
            updated.caption = caption;
        }
        if let Some(focal) = patch.focal {
            updated.focal = focal;
        }
        if let Some(zoom) = patch.zoom {
            updated.zoom = zoom;
        }
        if let Some(dither) = patch.dither {
            updated.dither = dither;
        }
        if let Some(enhance) = patch.enhance {
            updated.enhance = enhance;
        }
        if let Some(in_rotation) = patch.in_rotation {
            updated.in_rotation = in_rotation;
        }
        let dir = self.root.join(id);
        let copy = image::open(dir.join("copy.jpg"))?.to_rgb8();
        let spec = PhotoSpec {
            focal: updated.focal.as_ref().map(|p| (p.x, p.y)),
            zoom: updated.zoom,
            dither: Dither::from(updated.dither),
            enhance: updated.enhance,
        };
        let bytes = photo::frame(&copy, &spec).to_png();
        let frame_path = dir.join("frame.png");
        let old_frame = std::fs::read(&frame_path)?;
        let tmp = dir.join("frame.png.tmp");
        write_synced(&tmp, &bytes)?;
        std::fs::rename(&tmp, &frame_path)?;
        let persisted = self.store.update(|state| {
            let item = state
                .photos
                .iter_mut()
                .find(|item| item.id == id)
                .ok_or_else(|| format!("photo {id} was removed"))?;
            *item = updated.clone();
            Ok(())
        });
        if let Err(error) = persisted {
            let rollback_tmp = dir.join("frame.png.rollback");
            if write_synced(&rollback_tmp, &old_frame).is_ok() {
                let _ = std::fs::rename(&rollback_tmp, &frame_path);
            }
            return Err(error.into());
        }
        Ok(updated)
    }

    pub fn list(&self, offset: usize, limit: usize) -> (Vec<PhotoMeta>, usize) {
        let photos = self.store.snapshot().photos;
        let total = photos.len();
        (photos.into_iter().skip(offset).take(limit).collect(), total)
    }
}

#[derive(Clone, Debug, Default)]
pub struct PhotoPatch {
    pub caption: Option<Option<String>>,
    pub focal: Option<Option<FocalPoint>>,
    pub zoom: Option<f32>,
    pub dither: Option<DitherChoice>,
    pub enhance: Option<bool>,
    pub in_rotation: Option<bool>,
}

#[derive(Debug)]
pub enum LibraryError {
    Io(std::io::Error),
    Image(image::ImageError),
    Render(RenderError),
    Store(StoreError),
    NotFound(String),
    Invalid(String),
    Worker(String),
}

impl std::fmt::Display for LibraryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "photo library I/O error: {e}"),
            Self::Image(e) => write!(f, "photo image I/O error: {e}"),
            Self::Render(e) => write!(f, "{e}"),
            Self::Store(e) => write!(f, "{e}"),
            Self::NotFound(id) => write!(f, "photo '{id}' was not found"),
            Self::Invalid(msg) => f.write_str(msg),
            Self::Worker(msg) => write!(f, "photo worker failed: {msg}"),
        }
    }
}

impl std::error::Error for LibraryError {}
impl From<std::io::Error> for LibraryError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}
impl From<image::ImageError> for LibraryError {
    fn from(e: image::ImageError) -> Self {
        Self::Image(e)
    }
}
impl From<RenderError> for LibraryError {
    fn from(e: RenderError) -> Self {
        Self::Render(e)
    }
}
impl From<StoreError> for LibraryError {
    fn from(e: StoreError) -> Self {
        Self::Store(e)
    }
}

fn photo_spec(options: &PhotoOptions) -> PhotoSpec {
    PhotoSpec {
        focal: options.focal.as_ref().map(|point| (point.x, point.y)),
        zoom: 1.0,
        dither: Dither::from(options.dither),
        enhance: options.enhance.unwrap_or(true),
    }
}

fn validate_options(options: &PhotoOptions) -> Result<(), LibraryError> {
    if let Some(focal) = &options.focal {
        validate_focal(focal)?;
    }
    if let Some(caption) = &options.caption {
        if caption.chars().count() > 3000 {
            return Err(LibraryError::Invalid(
                "caption must be at most 3,000 characters".to_owned(),
            ));
        }
    }
    Ok(())
}

fn validate_zoom(zoom: f32) -> Result<(), LibraryError> {
    if !zoom.is_finite() || !(1.0..=photo::MAX_ZOOM).contains(&zoom) {
        return Err(LibraryError::Invalid(format!(
            "zoom must be a number in the range 1..={}",
            photo::MAX_ZOOM
        )));
    }
    Ok(())
}

fn validate_focal(focal: &FocalPoint) -> Result<(), LibraryError> {
    if !focal.x.is_finite()
        || !focal.y.is_finite()
        || !(0.0..=1.0).contains(&focal.x)
        || !(0.0..=1.0).contains(&focal.y)
    {
        return Err(LibraryError::Invalid(
            "focal coordinates must be in the range 0..=1".to_owned(),
        ));
    }
    Ok(())
}

fn validate_id(id: &str) -> Result<(), LibraryError> {
    if Ulid::from_string(id).is_err() || id.len() != 26 {
        return Err(LibraryError::NotFound(id.to_owned()));
    }
    Ok(())
}

fn basename(filename: &str) -> String {
    Path::new(filename)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .chars()
        .take(255)
        .collect()
}

/// Longest side of a library thumbnail, in pixels.
const THUMB_MAX_SIDE: u32 = 400;

/// Scales `image` to fit in a `THUMB_MAX_SIDE` square, keeping its aspect ratio
/// (`imageops::thumbnail` resizes to exactly the size it is given).
fn thumbnail(image: &RgbImage) -> RgbImage {
    let (width, height) = image.dimensions();
    let scale = (f64::from(THUMB_MAX_SIDE) / f64::from(width.max(height))).min(1.0);
    let scaled = |side: u32| ((f64::from(side) * scale).round() as u32).max(1);
    image::imageops::thumbnail(image, scaled(width), scaled(height))
}

/// True when a `thumb_w`x`thumb_h` thumbnail does not have the photo's aspect ratio
/// (allowing for rounding), as thumbnails written before 2026-10-26 did not.
fn thumbnail_misshapen(photo_w: u32, photo_h: u32, thumb_w: u32, thumb_h: u32) -> bool {
    let photo = f64::from(photo_w) / f64::from(photo_h);
    let thumb = f64::from(thumb_w) / f64::from(thumb_h);
    (photo - thumb).abs() / photo > 0.02
}

fn exif_taken_at(bytes: &[u8]) -> Option<String> {
    let exif = exif::Reader::new()
        .read_from_container(&mut Cursor::new(bytes))
        .ok()?;
    let value = exif
        .get_field(Tag::DateTimeOriginal, In::PRIMARY)?
        .value
        .display_as(Tag::DateTimeOriginal)
        .to_string();
    let naive = NaiveDateTime::parse_from_str(&value, "%Y-%m-%d %H:%M:%S").ok()?;
    Some(naive.and_utc().to_rfc3339())
}

fn write_synced(path: &Path, bytes: &[u8]) -> Result<(), std::io::Error> {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

pub fn render_photo_frame(image: RgbImage, spec: PhotoSpec) -> Frame {
    render::photo::frame(&image, &spec)
}

pub fn decode_bounded_copy(bytes: &[u8], limits: &IngestLimits) -> Result<RgbImage, RenderError> {
    photo::ingest(bytes, limits)
}

pub fn decode_frame_png(bytes: &[u8]) -> Result<Frame, image::ImageError> {
    let image = image::load_from_memory_with_format(bytes, ImageFormat::Png)?.to_luma8();
    Ok(Frame::from_gray(&image, Threshold(127)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgb};

    fn jpeg_bytes(width: u32, height: u32) -> Vec<u8> {
        let image: RgbImage = ImageBuffer::from_fn(width, height, |x, y| {
            Rgb([(x % 256) as u8, (y % 256) as u8, 80])
        });
        photo::encode_jpeg(&image, 90).unwrap()
    }

    fn exif_jpeg() -> Vec<u8> {
        let mut jpeg = jpeg_bytes(64, 48);
        let mut tiff = Vec::new();
        tiff.extend_from_slice(b"II");
        tiff.extend_from_slice(&42u16.to_le_bytes());
        tiff.extend_from_slice(&8u32.to_le_bytes());
        tiff.extend_from_slice(&1u16.to_le_bytes());
        tiff.extend_from_slice(&0x8769u16.to_le_bytes());
        tiff.extend_from_slice(&4u16.to_le_bytes());
        tiff.extend_from_slice(&1u32.to_le_bytes());
        tiff.extend_from_slice(&26u32.to_le_bytes());
        tiff.extend_from_slice(&0u32.to_le_bytes());
        tiff.extend_from_slice(&1u16.to_le_bytes());
        tiff.extend_from_slice(&0x9003u16.to_le_bytes());
        tiff.extend_from_slice(&2u16.to_le_bytes());
        tiff.extend_from_slice(&20u32.to_le_bytes());
        tiff.extend_from_slice(&44u32.to_le_bytes());
        tiff.extend_from_slice(&0u32.to_le_bytes());
        tiff.extend_from_slice(b"2021:07:14 12:34:56\0");
        let mut payload = b"Exif\0\0".to_vec();
        payload.extend_from_slice(&tiff);
        let segment_len = (payload.len() + 2) as u16;
        let mut segment = vec![0xff, 0xe1];
        segment.extend_from_slice(&segment_len.to_be_bytes());
        segment.extend_from_slice(&payload);
        jpeg.splice(2..2, segment);
        jpeg
    }

    #[test]
    fn add_writes_bounded_copy_thumb_and_rendered_frame() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::open(dir.path()).unwrap());
        let lib = Library::new(dir.path(), store, 160);
        let meta = tokio_test_block_on(lib.add_from_bytes(
            jpeg_bytes(2000, 1200),
            "source.jpg".into(),
            PhotoOptions::default(),
        ))
        .unwrap();
        assert_eq!((meta.width, meta.height), (1600, 960));
        let photo_dir = lib.photo_dir(&meta.id).unwrap();
        assert!(photo_dir.join("copy.jpg").is_file());
        assert!(photo_dir.join("thumb.jpg").is_file());
        assert!(photo_dir.join("frame.png").is_file());
        let thumb = image::open(photo_dir.join("thumb.jpg")).unwrap();
        assert_eq!((thumb.width(), thumb.height()), (400, 240));
        assert_eq!((Frame::WIDTH, Frame::HEIGHT), (800, 480));
    }

    #[test]
    fn thumbnails_keep_the_photo_aspect_ratio() {
        let landscape = thumbnail(&RgbImage::new(1600, 960));
        assert_eq!(landscape.dimensions(), (400, 240));
        let portrait = thumbnail(&RgbImage::new(720, 960));
        assert_eq!(portrait.dimensions(), (300, 400));
        let small = thumbnail(&RgbImage::new(120, 80));
        assert_eq!(small.dimensions(), (120, 80));
    }

    #[test]
    fn repair_rebuilds_only_misshapen_thumbnails() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::open(dir.path()).unwrap());
        let lib = Library::new(dir.path(), store, 160);
        let meta = tokio_test_block_on(lib.add_from_bytes(
            jpeg_bytes(2000, 1200),
            "source.jpg".into(),
            PhotoOptions::default(),
        ))
        .unwrap();
        assert_eq!(lib.repair_thumbnails().unwrap(), 0);

        // The pre-fix thumbnail: squashed to exactly 400x960.
        let thumb_path = lib.photo_dir(&meta.id).unwrap().join("thumb.jpg");
        let squashed = image::imageops::thumbnail(&RgbImage::new(1600, 960), 400, 960);
        std::fs::write(&thumb_path, photo::encode_jpeg(&squashed, 84).unwrap()).unwrap();
        assert_eq!(lib.repair_thumbnails().unwrap(), 1);
        assert_eq!(image::image_dimensions(&thumb_path).unwrap(), (400, 240));
        assert_eq!(lib.repair_thumbnails().unwrap(), 0);
    }

    #[test]
    fn patch_zoom_rebuilds_frame_and_rejects_out_of_range() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::open(dir.path()).unwrap());
        let lib = Library::new(dir.path(), store, 160);
        let image: RgbImage = ImageBuffer::from_fn(1000, 600, |x, y| {
            let v = if (x / 25 + y / 25) % 2 == 0 { 0 } else { 255 };
            Rgb([v, v, v])
        });
        let bytes = photo::encode_jpeg(&image, 95).unwrap();
        let meta = tokio_test_block_on(lib.add_from_bytes(
            bytes,
            "source.jpg".into(),
            PhotoOptions::default(),
        ))
        .unwrap();
        assert_eq!(meta.zoom, 1.0);
        let frame_path = lib.photo_dir(&meta.id).unwrap().join("frame.png");
        let before = std::fs::read(&frame_path).unwrap();
        let zoomed = tokio_test_block_on(lib.patch(
            meta.id.clone(),
            PhotoPatch {
                zoom: Some(2.0),
                ..PhotoPatch::default()
            },
        ))
        .unwrap();
        assert_eq!(zoomed.zoom, 2.0);
        assert_eq!(zoomed.crop().width, 500);
        assert_ne!(before, std::fs::read(&frame_path).unwrap());
        for bad in [0.5, photo::MAX_ZOOM + 0.1, f32::NAN] {
            let error = tokio_test_block_on(lib.patch(
                meta.id.clone(),
                PhotoPatch {
                    zoom: Some(bad),
                    ..PhotoPatch::default()
                },
            ))
            .unwrap_err();
            assert!(matches!(error, LibraryError::Invalid(_)), "{bad}");
        }
        assert_eq!(lib.get(&meta.id).unwrap().zoom, 2.0);
    }

    #[test]
    fn patch_focal_rebuilds_frame_and_delete_removes_directory() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::open(dir.path()).unwrap());
        let lib = Library::new(dir.path(), store, 160);
        // Add strong edge detail so shifting the crop changes the 1-bit output.
        let image: RgbImage = ImageBuffer::from_fn(1000, 480, |x, y| {
            let v = if x < 500 { 0 } else { 255 };
            let yv = if y % 7 == 0 { 30 } else { v };
            Rgb([yv, yv, yv])
        });
        let bytes = photo::encode_jpeg(&image, 95).unwrap();
        let meta = tokio_test_block_on(lib.add_from_bytes(
            bytes,
            "source.jpg".into(),
            PhotoOptions::default(),
        ))
        .unwrap();
        let frame_path = lib.photo_dir(&meta.id).unwrap().join("frame.png");
        let before = std::fs::read(&frame_path).unwrap();
        tokio_test_block_on(lib.patch(
            meta.id.clone(),
            PhotoPatch {
                focal: Some(Some(FocalPoint { x: 0.9, y: 0.5 })),
                ..PhotoPatch::default()
            },
        ))
        .unwrap();
        let after = std::fs::read(&frame_path).unwrap();
        assert_ne!(before, after);
        let photo_dir = lib.photo_dir(&meta.id).unwrap();
        lib.remove(&meta.id).unwrap();
        assert!(!photo_dir.exists());
        assert!(matches!(lib.get(&meta.id), Err(LibraryError::NotFound(_))));
    }

    #[test]
    fn exif_datetime_original_is_parsed() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::open(dir.path()).unwrap());
        let lib = Library::new(dir.path(), store, 160);
        let bytes = exif_jpeg();
        let meta = tokio_test_block_on(lib.add_from_bytes(
            bytes,
            "source.jpg".into(),
            PhotoOptions::default(),
        ))
        .unwrap();
        assert_eq!(meta.taken_at.as_deref(), Some("2021-07-14T12:34:56+00:00"));
    }

    fn tokio_test_block_on<F: std::future::Future>(future: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(future)
    }
}
