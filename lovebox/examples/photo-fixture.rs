//! Generate synthetic photo inputs, including large acceptance fixtures; no downloads.
use image::{DynamicImage, ImageBuffer, Rgb, RgbImage};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let path = args
        .get(1)
        .ok_or("usage: photo-fixture OUTPUT [WIDTH HEIGHT]")?;
    let width = args
        .get(2)
        .map(|s| s.parse())
        .transpose()?
        .unwrap_or(1600u32);
    let height = args
        .get(3)
        .map(|s| s.parse())
        .transpose()?
        .unwrap_or(960u32);
    if path.ends_with(".tiff") || path.ends_with(".tif") {
        let img: ImageBuffer<Rgb<u16>, Vec<u16>> = ImageBuffer::from_fn(width, height, |x, y| {
            Rgb([
                (u64::from(x) * 65535 / u64::from(width)) as u16,
                (u64::from(y) * 65535 / u64::from(height)) as u16,
                (((x / 80 + y / 80) % 2) * 65535) as u16,
            ])
        });
        DynamicImage::ImageRgb16(img).save(path)?;
    } else {
        let img = RgbImage::from_fn(width, height, |x, y| {
            Rgb([
                (x * 255 / width) as u8,
                (y * 255 / height) as u8,
                if (x / 80 + y / 80) % 2 == 0 { 35 } else { 220 },
            ])
        });
        img.save(path)?;
    }
    Ok(())
}
