//! Photo pipeline tests. Every input image is generated here; none is committed.

use std::io::Cursor;
use std::sync::Arc;
use std::time::Instant;

use image::{DynamicImage, ImageBuffer, ImageFormat, Rgb, RgbImage, Rgba, RgbaImage};
use lovebox::fonts::Fonts;
use lovebox::frame::Frame;
use lovebox::render::photo::{self, cover_crop_rect, ingest, prepare_gray, IngestLimits};
use lovebox::render::{render, Card, Dither, PhotoSpec, RenderError};

/// A smooth colour field with some structure, cheap to generate at any size.
fn scene(w: u32, h: u32) -> RgbImage {
    RgbImage::from_fn(w, h, |x, y| {
        let fx = x * 255 / w;
        let fy = y * 255 / h;
        let ring = (((x / 97) + (y / 89)) % 2) as u32 * 40;
        Rgb([fx as u8, fy as u8, ((fx + fy) / 2 + ring).min(255) as u8])
    })
}

fn encode(img: &DynamicImage, format: ImageFormat) -> Vec<u8> {
    let mut out = Cursor::new(Vec::new());
    img.write_to(&mut out, format).unwrap();
    out.into_inner()
}

fn mean_abs_diff(a: &RgbImage, b: &RgbImage) -> f64 {
    assert_eq!(a.dimensions(), b.dimensions());
    let sum: u64 = a
        .as_raw()
        .iter()
        .zip(b.as_raw())
        .map(|(&x, &y)| u64::from(x.abs_diff(y)))
        .sum();
    sum as f64 / a.as_raw().len() as f64
}

#[test]
fn jpeg_24_megapixels_ingests_under_native_and_pi_limits() {
    let src = DynamicImage::ImageRgb8(scene(6000, 4000));
    let jpeg = {
        let mut out = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 85)
            .encode_image(&src)
            .unwrap();
        out
    };
    drop(src);
    let started = Instant::now();
    let bounded = ingest(jpeg.as_slice(), &IngestLimits::native()).unwrap();
    eprintln!("24 MP JPEG ingest (test profile): {:?}", started.elapsed());
    assert_eq!(bounded.dimensions(), (1440, 960));
    let bounded_pi = ingest(jpeg.as_slice(), &IngestLimits::pi()).unwrap();
    assert_eq!(bounded_pi, bounded);
    // The bounded copy looks like a downscaled scene: top-left dark red, bottom-right bright.
    let tl = bounded.get_pixel(5, 5).0;
    let br = bounded.get_pixel(1430, 950).0;
    assert!(tl[0] < 20 && tl[1] < 20, "{tl:?}");
    assert!(br[0] > 230 && br[1] > 230, "{br:?}");
}

#[test]
fn tiff_16_bit_matches_its_8_bit_twin() {
    let rgb16: ImageBuffer<Rgb<u16>, Vec<u16>> = ImageBuffer::from_fn(3000, 2000, |x, y| {
        Rgb([
            (x * 65535 / 3000) as u16,
            (y * 65535 / 2000) as u16,
            (((x + y) * 37) % 65536) as u16,
        ])
    });
    let deep = DynamicImage::ImageRgb16(rgb16);
    let shallow = DynamicImage::ImageRgb8(deep.to_rgb8());
    let tiff16 = encode(&deep, ImageFormat::Tiff);
    let tiff8 = encode(&shallow, ImageFormat::Tiff);
    drop((deep, shallow));
    let a = ingest(tiff16.as_slice(), &IngestLimits::pi()).unwrap();
    let b = ingest(tiff8.as_slice(), &IngestLimits::pi()).unwrap();
    assert_eq!(a.dimensions(), (1440, 960));
    let diff = mean_abs_diff(&a, &b);
    assert!(diff < 1.0, "mean abs diff {diff}");
}

#[test]
fn over_80_megapixels_is_too_large_before_decoding() {
    // A PNG header claiming 9000x9000 (81 MP) and no pixel data at all: refusing it proves
    // the check happens on the header.
    let mut png = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png, 9000, 9000);
        encoder.set_color(png::ColorType::Rgb);
        let writer = encoder.write_header().unwrap();
        std::mem::forget(writer);
    }
    match ingest(png.as_slice(), &IngestLimits::native()) {
        Err(RenderError::TooLarge(msg)) => assert!(msg.contains("81.0 MP"), "{msg}"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_one_megabyte_limit_is_too_large_not_a_panic() {
    let img = DynamicImage::ImageRgb8(scene(1000, 1000));
    let limits = IngestLimits::native().with_max_alloc(1024 * 1024);
    for format in [ImageFormat::Png, ImageFormat::Jpeg, ImageFormat::Tiff, ImageFormat::Bmp] {
        let bytes = encode(&img, format);
        match ingest(bytes.as_slice(), &limits) {
            Err(RenderError::TooLarge(_)) => {}
            other => panic!("{format:?}: {other:?}"),
        }
    }
}

#[test]
fn png_alpha_composites_on_white() {
    let img = RgbaImage::from_fn(200, 100, |x, _| {
        if x < 100 {
            Rgba([255, 0, 0, 0]) // fully transparent red
        } else {
            Rgba([0, 0, 0, 255]) // opaque black
        }
    });
    let png = encode(&DynamicImage::ImageRgba8(img), ImageFormat::Png);
    let out = ingest(png.as_slice(), &IngestLimits::default()).unwrap();
    assert_eq!(out.dimensions(), (200, 100));
    assert_eq!(out.get_pixel(10, 50).0, [255, 255, 255]);
    assert_eq!(out.get_pixel(190, 50).0, [0, 0, 0]);
}

#[test]
fn webp_and_bmp_decode_losslessly() {
    let img = scene(320, 200);
    for format in [ImageFormat::WebP, ImageFormat::Bmp, ImageFormat::Png] {
        let bytes = encode(&DynamicImage::ImageRgb8(img.clone()), format);
        let out = ingest(bytes.as_slice(), &IngestLimits::default()).unwrap();
        assert_eq!(out, img, "{format:?}");
    }
}

#[test]
fn ingest_reads_files_and_rejects_garbage() {
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let path = dir.join("ingest-file.png");
    std::fs::write(&path, encode(&DynamicImage::ImageRgb8(scene(64, 48)), ImageFormat::Png)).unwrap();
    assert_eq!(ingest(path.as_path(), &IngestLimits::default()).unwrap(), scene(64, 48));
    assert!(matches!(
        ingest(dir.join("missing.png").as_path(), &IngestLimits::default()),
        Err(RenderError::Io(_))
    ));
    assert!(matches!(
        ingest(&b"definitely not an image"[..], &IngestLimits::default()),
        Err(RenderError::UnsupportedFormat(_))
    ));
    let jpeg = encode(&DynamicImage::ImageRgb8(scene(640, 480)), ImageFormat::Jpeg);
    let err = ingest(&jpeg[..jpeg.len() / 3], &IngestLimits::default());
    assert!(err.is_err(), "truncated JPEG decoded");
    assert!(err.unwrap_err().is_client_error());
}

#[test]
fn bounded_copy_round_trips_through_jpeg() {
    let img = scene(1440, 960);
    let jpeg = photo::encode_jpeg(&img, 90).unwrap();
    let back = ingest(jpeg.as_slice(), &IngestLimits::default()).unwrap();
    assert_eq!(back.dimensions(), (1440, 960));
    assert!(mean_abs_diff(&img, &back) < 3.0);
}

fn spec(dither: Dither, focal: Option<(f32, f32)>, enhance: bool) -> PhotoSpec {
    PhotoSpec { dither, focal, enhance }
}

#[test]
fn portrait_crops_top_and_bottom_landscape_crops_sides() {
    // Portrait 600x1000: black top and bottom, white 5:3 band in the middle.
    let portrait = RgbImage::from_fn(600, 1000, |_, y| {
        if (320..680).contains(&y) { Rgb([255; 3]) } else { Rgb([0; 3]) }
    });
    assert_eq!(cover_crop_rect(600, 1000, None), (0, 320, 600, 360));
    let f = photo::frame(&portrait, &spec(Dither::FloydSteinberg, None, false));
    assert!(f.white_fraction() > 0.99, "{}", f.white_fraction());

    // Landscape 2000x600: black sides, white 5:3 middle.
    let landscape = RgbImage::from_fn(2000, 600, |x, _| {
        if (500..1500).contains(&x) { Rgb([255; 3]) } else { Rgb([0; 3]) }
    });
    let f = photo::frame(&landscape, &spec(Dither::FloydSteinberg, None, false));
    assert!(f.white_fraction() > 0.99, "{}", f.white_fraction());
}

#[test]
fn focal_point_shifts_the_crop() {
    // Left half black, right half white, 2000x600: the crop is 1000 wide.
    let img = RgbImage::from_fn(2000, 600, |x, _| if x < 1000 { Rgb([0; 3]) } else { Rgb([255; 3]) });
    let left = photo::frame(&img, &spec(Dither::FloydSteinberg, Some((0.0, 0.5)), false));
    let right = photo::frame(&img, &spec(Dither::FloydSteinberg, Some((1.0, 0.5)), false));
    let centre = photo::frame(&img, &spec(Dither::FloydSteinberg, None, false));
    assert!(left.white_fraction() < 0.01, "{}", left.white_fraction());
    assert!(right.white_fraction() > 0.99, "{}", right.white_fraction());
    assert!((centre.white_fraction() - 0.5).abs() < 0.02, "{}", centre.white_fraction());
}

/// Gradient across, a checkerboard in the lower third, a dark disc: the golden source.
fn gradient_checker() -> RgbImage {
    RgbImage::from_fn(1600, 960, |x, y| {
        let v = if y > 640 && ((x / 40) + (y / 40)) % 2 == 0 {
            if x < 800 { 30 } else { 225 }
        } else {
            (x * 255 / 1599) as u8
        };
        let (dx, dy) = (x as i32 - 1200, y as i32 - 300);
        let v = if dx * dx + dy * dy < 150 * 150 { v / 4 } else { v };
        Rgb([v, v, v])
    })
}

#[test]
fn both_dithers_keep_the_mean_within_5_percent() {
    let img = gradient_checker();
    for method in [Dither::FloydSteinberg, Dither::Atkinson] {
        let s = spec(method, None, false);
        let gray = prepare_gray(&img, &s);
        let mean = gray.pixels().map(|p| f64::from(p.0[0])).sum::<f64>() / (800.0 * 480.0);
        let white = photo::frame(&img, &s).white_fraction();
        assert!((white - mean).abs() < 0.05, "{method:?}: source {mean:.3}, frame {white:.3}");
    }
}

#[test]
fn photo_card_renders_through_render() {
    let card = Card::Photo {
        image: Arc::new(gradient_checker()),
        spec: PhotoSpec::default(),
    };
    let frame = render(&card, &Fonts::bundled()).unwrap();
    assert_ne!(frame, Frame::white());
    assert_eq!(frame, photo::frame(&gradient_checker(), &PhotoSpec::default()));
}
