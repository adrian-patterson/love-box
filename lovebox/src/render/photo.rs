//! Photo cards, in two stages so the expensive one runs once:
//!
//! 1. [`ingest`] (at upload or import): decode a JPEG, PNG, TIFF (8/16-bit), BMP or WebP
//!    under explicit memory limits and keep a bounded RGB copy, at most 1600x960 with the
//!    aspect ratio preserved, EXIF orientation applied, alpha composited on white.
//! 2. [`frame`] (at display time and for previews): cover-crop the bounded copy to
//!    800x480 around a focal point, convert to grayscale, optionally stretch contrast, and
//!    error-diffuse to 1 bit.

use std::fs::File;
use std::io::{BufRead, BufReader, Cursor, Read, Seek, SeekFrom};
use std::path::Path;

use image::imageops::{self, FilterType};
use image::metadata::Orientation;
use image::{
    DynamicImage, ImageDecoder, ImageError, ImageFormat, ImageReader, Limits, Luma, RgbImage,
};

use super::dither::{dither, Dither, GrayF32};
use super::RenderError;
use crate::frame::{Frame, HEIGHT, WIDTH};

/// Widest bounded copy [`ingest`] keeps.
pub const BOUNDED_WIDTH: u32 = 1600;
/// Tallest bounded copy [`ingest`] keeps.
pub const BOUNDED_HEIGHT: u32 = 960;

/// Memory limits for [`ingest`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IngestLimits {
    /// Largest accepted image, in pixels (width x height), checked from the header before
    /// decoding.
    pub max_pixels: u64,
    /// Budget for decoded pixels, JPEG working buffers and the bounded copy.
    /// Also passed to `image::Limits::max_alloc`. Codec scratch space and allocator
    /// overhead are not covered: this is not a process RSS ceiling.
    pub max_alloc: u64,
}

impl IngestLimits {
    /// 80 megapixels.
    pub const DEFAULT_MAX_PIXELS: u64 = 80_000_000;
    /// 512 MiB: the native (laptop) default.
    pub const NATIVE_MAX_ALLOC: u64 = 512 * 1024 * 1024;
    /// 160 MiB: what the server should use on the 512 MB Pi Zero W.
    pub const PI_MAX_ALLOC: u64 = 160 * 1024 * 1024;

    /// Limits for a laptop: 80 MP, 512 MiB.
    pub fn native() -> Self {
        Self {
            max_pixels: Self::DEFAULT_MAX_PIXELS,
            max_alloc: Self::NATIVE_MAX_ALLOC,
        }
    }

    /// Limits for the Pi Zero W: 80 MP, 160 MiB. Baseline JPEG uses reduced IDCT;
    /// progressive JPEG reserves full-resolution coefficients and may be rejected.
    pub fn pi() -> Self {
        Self {
            max_pixels: Self::DEFAULT_MAX_PIXELS,
            max_alloc: Self::PI_MAX_ALLOC,
        }
    }

    /// These limits with a different `max_alloc`.
    pub fn with_max_alloc(self, max_alloc: u64) -> Self {
        Self { max_alloc, ..self }
    }
}

impl Default for IngestLimits {
    fn default() -> Self {
        Self::native()
    }
}

/// Where [`ingest`] reads an image from.
#[derive(Clone, Copy, Debug)]
pub enum PhotoSource<'a> {
    /// A file; the format is detected from its content, not its extension.
    Path(&'a Path),
    /// Encoded image bytes, e.g. an upload body.
    Bytes(&'a [u8]),
}

impl<'a> From<&'a Path> for PhotoSource<'a> {
    fn from(path: &'a Path) -> Self {
        Self::Path(path)
    }
}

impl<'a> From<&'a [u8]> for PhotoSource<'a> {
    fn from(bytes: &'a [u8]) -> Self {
        Self::Bytes(bytes)
    }
}

fn map_image_error(err: ImageError) -> RenderError {
    match err {
        ImageError::Limits(e) => RenderError::TooLarge(e.to_string()),
        ImageError::Unsupported(e) => RenderError::UnsupportedFormat(e.to_string()),
        ImageError::IoError(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
            RenderError::Decode(format!("truncated image: {e}"))
        }
        ImageError::IoError(e) => RenderError::Io(e),
        other => RenderError::Decode(other.to_string()),
    }
}

/// The largest size within [`BOUNDED_WIDTH`]x[`BOUNDED_HEIGHT`] with the aspect ratio of
/// `w`x`h`; never upscales.
pub fn bounded_size(w: u32, h: u32) -> (u32, u32) {
    let scale = (BOUNDED_WIDTH as f64 / w as f64)
        .min(BOUNDED_HEIGHT as f64 / h as f64)
        .min(1.0);
    (
        ((w as f64 * scale).round() as u32).clamp(1, BOUNDED_WIDTH),
        ((h as f64 * scale).round() as u32).clamp(1, BOUNDED_HEIGHT),
    )
}

fn swaps_axes(orientation: Orientation) -> bool {
    matches!(
        orientation,
        Orientation::Rotate90
            | Orientation::Rotate270
            | Orientation::Rotate90FlipH
            | Orientation::Rotate270FlipH
    )
}

/// Decodes an image and returns its bounded copy (see the module docs).
///
/// The header is read first: more than `limits.max_pixels` pixels, or a decode that would
/// exceed the estimated `limits.max_alloc` working set, is [`RenderError::TooLarge`]
/// before pixel decoding. Other formats also receive `image::Limits::max_alloc`.
/// This is a decode buffer budget, not a hard RSS limit. JPEG uses reduced IDCT, then
/// `image`'s area-averaging `thumbnail`; other formats decode before downscaling.
pub fn ingest<'a>(
    source: impl Into<PhotoSource<'a>>,
    limits: &IngestLimits,
) -> Result<RgbImage, RenderError> {
    match source.into() {
        PhotoSource::Path(path) => {
            let file = BufReader::new(File::open(path)?);
            ingest_reader(ImageReader::new(file), limits)
        }
        PhotoSource::Bytes(bytes) => ingest_reader(ImageReader::new(Cursor::new(bytes)), limits),
    }
}

fn ingest_reader<R: BufRead + Seek>(
    reader: ImageReader<R>,
    limits: &IngestLimits,
) -> Result<RgbImage, RenderError> {
    let reader = reader.with_guessed_format()?;
    if reader.format().is_none() {
        return Err(RenderError::UnsupportedFormat(
            "not a JPEG, PNG, TIFF, BMP or WebP image".to_owned(),
        ));
    }
    let format = reader.format().expect("format checked");
    let mut input = reader.into_inner();
    let start = input.stream_position()?;
    if format == ImageFormat::Png {
        // image::PngDecoder also scans up to IDAT at construction. Read only IHDR
        // first, so a header-only oversized upload is rejected as TooLarge.
        let mut png = png::Decoder::new_with_limits(
            &mut input,
            png::Limits {
                bytes: limits.max_alloc.min(usize::MAX as u64) as usize,
            },
        );
        let header = png.read_header_info().map_err(|e| match e {
            png::DecodingError::LimitsExceeded => RenderError::TooLarge(e.to_string()),
            _ => RenderError::Decode(e.to_string()),
        })?;
        check_pixels(header.width, header.height, limits)?;
        input.seek(SeekFrom::Start(start))?;
    }
    if format == ImageFormat::Jpeg {
        return ingest_jpeg(input, limits);
    }
    let mut reader = ImageReader::with_format(input, format);
    let mut image_limits = Limits::default();
    image_limits.max_alloc = Some(limits.max_alloc);
    reader.limits(image_limits);
    let mut decoder = reader.into_decoder().map_err(map_image_error)?;

    let (w, h) = decoder.dimensions();
    let pixels = u64::from(w) * u64::from(h);
    check_pixels(w, h, limits)?;
    let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
    let (bw, bh) = if swaps_axes(orientation) {
        let (bh, bw) = bounded_size(h, w);
        (bw, bh)
    } else {
        bounded_size(w, h)
    };
    // The decoded image plus the bounded copy at the decoded pixel size.
    let bytes_per_pixel = decoder.total_bytes() / pixels.max(1);
    let needed = decoder.total_bytes() + u64::from(bw) * u64::from(bh) * (bytes_per_pixel + 4);
    if needed > limits.max_alloc {
        return Err(RenderError::TooLarge(format!(
            "decoding {w}x{h} needs about {} MiB; the limit is {} MiB",
            needed.div_ceil(1024 * 1024),
            limits.max_alloc / (1024 * 1024)
        )));
    }

    let decoded = DynamicImage::from_decoder(decoder).map_err(map_image_error)?;
    let mut small = if (bw, bh) == (w, h) {
        decoded
    } else {
        let small = decoded.thumbnail_exact(bw, bh);
        drop(decoded);
        small
    };
    small.apply_orientation(orientation);
    Ok(flatten_on_white(small))
}

fn check_pixels(w: u32, h: u32, limits: &IngestLimits) -> Result<(), RenderError> {
    let pixels = u64::from(w) * u64::from(h);
    if pixels > limits.max_pixels {
        return Err(RenderError::TooLarge(format!(
            "{w}x{h} is {:.1} MP; the limit is {:.0} MP",
            pixels as f64 / 1e6,
            limits.max_pixels as f64 / 1e6
        )));
    }
    Ok(())
}

fn ingest_jpeg<R: Read>(input: R, limits: &IngestLimits) -> Result<RgbImage, RenderError> {
    let decode_error = |e: jpeg_decoder::Error| RenderError::Decode(e.to_string());
    let mut decoder = jpeg_decoder::Decoder::new(input);
    decoder.set_max_decoding_buffer_size(limits.max_alloc.min(usize::MAX as u64) as usize);
    decoder.read_info().map_err(decode_error)?;
    let info = decoder.info().expect("JPEG header read");
    let (w, h) = (u32::from(info.width), u32::from(info.height));
    check_pixels(w, h, limits)?;
    let (bw, bh) = bounded_size(w, h);
    let (sw, sh) = decoder.scale(bw as u16, bh as u16).map_err(decode_error)?;
    let channels = info.pixel_format.pixel_bytes() as u64;
    // Reserve decoded output, component buffers and conservative DCT coefficients
    // (progressive JPEG keeps full-resolution coefficients even when scaled).
    let coefficients = if info.coding_process == jpeg_decoder::CodingProcess::DctProgressive {
        u64::from(w.div_ceil(32) * 32) * u64::from(h.div_ceil(32) * 32) * channels * 2
    } else {
        0
    };
    let needed = coefficients
        + u64::from(w) * channels * 32 * 2
        + u64::from(sw) * u64::from(sh) * channels * 2
        + u64::from(bw) * u64::from(bh) * 6;
    if needed > limits.max_alloc {
        return Err(RenderError::TooLarge(format!(
            "JPEG decode working set needs about {} MiB; budget is {} MiB",
            needed.div_ceil(1024 * 1024),
            limits.max_alloc / (1024 * 1024)
        )));
    }
    let pixels = decoder.decode().map_err(decode_error)?;
    let orientation = decoder
        .exif_data()
        .and_then(Orientation::from_exif_chunk)
        .unwrap_or(Orientation::NoTransforms);
    let (sw, sh) = (u32::from(sw), u32::from(sh));
    let img = match info.pixel_format {
        jpeg_decoder::PixelFormat::RGB24 => RgbImage::from_raw(sw, sh, pixels)
            .ok_or_else(|| RenderError::Decode("unexpected JPEG buffer size".into()))?,
        jpeg_decoder::PixelFormat::L8 => RgbImage::from_fn(sw, sh, |x, y| {
            image::Rgb([pixels[(y * sw + x) as usize]; 3])
        }),
        jpeg_decoder::PixelFormat::CMYK32 => RgbImage::from_fn(sw, sh, |x, y| {
            let i = ((y * sw + x) * 4) as usize;
            let k = u16::from(pixels[i + 3]);
            let channel = |c| ((u16::from(c) * k + 127) / 255) as u8;
            image::Rgb([
                channel(pixels[i]),
                channel(pixels[i + 1]),
                channel(pixels[i + 2]),
            ])
        }),
        jpeg_decoder::PixelFormat::L16 => {
            return Err(RenderError::UnsupportedFormat(
                "16-bit lossless JPEG".into(),
            ))
        }
    };
    let mut small = DynamicImage::ImageRgb8(img).thumbnail_exact(bw, bh);
    small.apply_orientation(orientation);
    let (bw, bh) = bounded_size(small.width(), small.height());
    Ok(small.thumbnail_exact(bw, bh).into_rgb8())
}

/// RGB with any alpha composited over white.
fn flatten_on_white(img: DynamicImage) -> RgbImage {
    if !img.color().has_alpha() {
        return img.into_rgb8();
    }
    let rgba = img.into_rgba8();
    RgbImage::from_fn(rgba.width(), rgba.height(), |x, y| {
        let [r, g, b, a] = rgba.get_pixel(x, y).0;
        let a = u32::from(a);
        let over = |c: u8| ((u32::from(c) * a + 255 * (255 - a) + 127) / 255) as u8;
        image::Rgb([over(r), over(g), over(b)])
    })
}

/// Encodes a bounded copy as JPEG (for the photo library; quality 90 is a good default).
pub fn encode_jpeg(img: &RgbImage, quality: u8) -> Result<Vec<u8>, RenderError> {
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality.clamp(1, 100))
        .encode_image(img)
        .map_err(map_image_error)?;
    Ok(out)
}

/// How a photo becomes a frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PhotoSpec {
    /// Error-diffusion method.
    pub dither: Dither,
    /// Point to keep in view when cropping, as fractions of width and height
    /// (`(0.5, 0.5)` = centre, the default). Values are clamped to `0.0..=1.0`.
    pub focal: Option<(f32, f32)>,
    /// How far to zoom into the cover crop: `1.0` is the largest 5:3 crop (the default),
    /// `2.0` half its width and height. Clamped to `1.0..=MAX_ZOOM`.
    pub zoom: f32,
    /// Stretch contrast (clip 0.5% at each end of the histogram) and lift midtones with a
    /// mild gamma before dithering.
    pub enhance: bool,
}

impl Default for PhotoSpec {
    /// Floyd–Steinberg, centred, enhanced.
    fn default() -> Self {
        Self {
            dither: Dither::FloydSteinberg,
            focal: None,
            zoom: 1.0,
            enhance: true,
        }
    }
}

/// Largest crop zoom: a quarter of the cover crop's width, which for a 1600 px library copy
/// still leaves 400 source pixels across the 800 px panel.
pub const MAX_ZOOM: f32 = 4.0;

/// Gamma applied after the contrast stretch: `out = in^0.9`, so 50% gray becomes 54%.
/// Dithered e-paper reads darker than the source; this lifts it a little.
pub const ENHANCE_GAMMA: f32 = 0.9;
/// Fraction of pixels clipped at each end of the histogram by the contrast stretch.
pub const ENHANCE_CLIP: f64 = 0.005;

/// The source rectangle `(x, y, width, height)` of a `w`x`h` image that cover-crops to
/// the panel's 5:3 aspect, positioned so the focal point is as close to the centre as
/// the image edges allow. Wider-than-5:3 sources lose their sides, taller ones their top
/// and bottom. `zoom` (clamped to `1.0..=MAX_ZOOM`) shrinks that crop about the same centre.
pub fn cover_crop_rect(
    w: u32,
    h: u32,
    focal: Option<(f32, f32)>,
    zoom: f32,
) -> (u32, u32, u32, u32) {
    let (fx, fy) = focal.unwrap_or((0.5, 0.5));
    let clamp01 = |v: f32| if v.is_nan() { 0.5 } else { v.clamp(0.0, 1.0) };
    let (fx, fy) = (clamp01(fx) as f64, clamp01(fy) as f64);
    let (cw, ch) = if u64::from(w) * u64::from(HEIGHT) > u64::from(h) * u64::from(WIDTH) {
        (
            ((h as f64 * WIDTH as f64 / HEIGHT as f64).round() as u32).clamp(1, w),
            h,
        )
    } else {
        (
            w,
            ((w as f64 * HEIGHT as f64 / WIDTH as f64).round() as u32).clamp(1, h),
        )
    };
    let zoom = if zoom.is_nan() {
        1.0
    } else {
        zoom.clamp(1.0, MAX_ZOOM)
    } as f64;
    let shrink = |side: u32| ((side as f64 / zoom).round() as u32).max(1);
    let (cw, ch) = (shrink(cw), shrink(ch));
    let place = |size: u32, crop: u32, f: f64| -> u32 {
        let start = (f * size as f64 - crop as f64 / 2.0).round();
        start.clamp(0.0, (size - crop) as f64) as u32
    };
    (place(w, cw, fx), place(h, ch, fy), cw, ch)
}

/// The 800x480 grayscale image (0.0 = black, 1.0 = white) that [`frame`] dithers:
/// cover-cropped, converted to luma (Rec. 709 weights on the sRGB values, as `image`'s
/// `grayscale` does), resized with Catmull-Rom, and enhanced if `spec.enhance`.
pub fn prepare_gray(img: &RgbImage, spec: &PhotoSpec) -> GrayF32 {
    if img.width() == 0 || img.height() == 0 {
        return GrayF32::from_pixel(WIDTH, HEIGHT, Luma([1.0]));
    }
    let (x, y, cw, ch) = cover_crop_rect(img.width(), img.height(), spec.focal, spec.zoom);
    let luma = GrayF32::from_fn(cw, ch, |px, py| {
        let [r, g, b] = img.get_pixel(x + px, y + py).0;
        Luma([(0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32) / 255.0])
    });
    let mut gray = if (cw, ch) == (WIDTH, HEIGHT) {
        luma
    } else {
        imageops::resize(&luma, WIDTH, HEIGHT, FilterType::CatmullRom)
    };
    if spec.enhance {
        enhance(&mut gray);
    }
    gray
}

/// Contrast stretch between the 0.5% and 99.5% points of the histogram, then
/// [`ENHANCE_GAMMA`]. A flat image is left unstretched.
fn enhance(gray: &mut GrayF32) {
    const BINS: usize = 1024;
    let mut histogram = [0u32; BINS];
    for p in gray.pixels() {
        histogram[((p.0[0].clamp(0.0, 1.0) * (BINS - 1) as f32).round()) as usize] += 1;
    }
    let total = u64::from(gray.width()) * u64::from(gray.height());
    let clip = (total as f64 * ENHANCE_CLIP) as u64;
    let mut seen = 0u64;
    let mut lo = 0usize;
    for (i, &n) in histogram.iter().enumerate() {
        seen += u64::from(n);
        if seen > clip {
            lo = i;
            break;
        }
    }
    seen = 0;
    let mut hi = BINS - 1;
    for (i, &n) in histogram.iter().enumerate().rev() {
        seen += u64::from(n);
        if seen > clip {
            hi = i;
            break;
        }
    }
    let (lo, hi) = (lo as f32 / (BINS - 1) as f32, hi as f32 / (BINS - 1) as f32);
    let stretch = hi - lo > 1.0 / 64.0;
    for p in gray.pixels_mut() {
        let mut v = p.0[0];
        if stretch {
            v = (v - lo) / (hi - lo);
        }
        p.0[0] = v.clamp(0.0, 1.0).powf(ENHANCE_GAMMA);
    }
}

/// Renders a bounded copy (or any RGB image) to a frame: [`prepare_gray`], then
/// [`spec.dither`](PhotoSpec::dither).
pub fn frame(img: &RgbImage, spec: &PhotoSpec) -> Frame {
    dither(&prepare_gray(img, spec), spec.dither)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_size_preserves_aspect_and_never_upscales() {
        assert_eq!(bounded_size(6000, 4000), (1440, 960));
        assert_eq!(bounded_size(4000, 6000), (640, 960));
        assert_eq!(bounded_size(8000, 2000), (1600, 400));
        assert_eq!(bounded_size(800, 480), (800, 480));
        assert_eq!(bounded_size(10, 10), (10, 10));
    }

    #[test]
    fn cover_crop_trims_the_long_axis() {
        // Portrait: full width, a 5:3 band from the middle.
        assert_eq!(cover_crop_rect(600, 1000, None, 1.0), (0, 320, 600, 360));
        // Landscape wider than 5:3: full height, sides trimmed.
        assert_eq!(cover_crop_rect(2000, 600, None, 1.0), (500, 0, 1000, 600));
        // Exactly 5:3: untouched.
        assert_eq!(cover_crop_rect(1600, 960, None, 1.0), (0, 0, 1600, 960));
    }

    #[test]
    fn zoom_shrinks_the_crop_about_the_focal_point_and_clamps() {
        // 1600x960 at zoom 2: an 800x480 crop centred on the default focal point.
        assert_eq!(cover_crop_rect(1600, 960, None, 2.0), (400, 240, 800, 480));
        // The focal point still steers it, clamped to the image.
        assert_eq!(
            cover_crop_rect(1600, 960, Some((1.0, 0.0)), 2.0),
            (800, 0, 800, 480)
        );
        // Below 1, above MAX_ZOOM and NaN are clamped.
        assert_eq!(cover_crop_rect(1600, 960, None, 0.5), (0, 0, 1600, 960));
        assert_eq!(cover_crop_rect(1600, 960, None, 10.0), (600, 360, 400, 240));
        assert_eq!(
            cover_crop_rect(1600, 960, None, f32::NAN),
            (0, 0, 1600, 960)
        );
    }

    #[test]
    fn focal_point_moves_the_crop_and_clamps_at_edges() {
        assert_eq!(
            cover_crop_rect(2000, 600, Some((0.0, 0.5)), 1.0),
            (0, 0, 1000, 600)
        );
        assert_eq!(
            cover_crop_rect(2000, 600, Some((1.0, 0.5)), 1.0),
            (1000, 0, 1000, 600)
        );
        assert_eq!(
            cover_crop_rect(2000, 600, Some((0.4, 0.5)), 1.0),
            (300, 0, 1000, 600)
        );
        assert_eq!(
            cover_crop_rect(600, 1000, Some((0.5, 0.0)), 1.0),
            (0, 0, 600, 360)
        );
        assert_eq!(
            cover_crop_rect(600, 1000, Some((0.5, 2.0)), 1.0),
            (0, 640, 600, 360)
        );
        assert_eq!(
            cover_crop_rect(600, 1000, Some((f32::NAN, 0.5)), 1.0),
            (0, 320, 600, 360)
        );
    }

    #[test]
    fn enhance_stretches_a_low_contrast_image() {
        let mut gray = GrayF32::from_fn(800, 480, |x, _| Luma([0.4 + 0.2 * x as f32 / 799.0]));
        enhance(&mut gray);
        let (min, max) = gray.pixels().fold((1.0f32, 0.0f32), |(lo, hi), p| {
            (lo.min(p.0[0]), hi.max(p.0[0]))
        });
        assert!(min < 0.01 && max > 0.99, "{min}..{max}");
    }
}
