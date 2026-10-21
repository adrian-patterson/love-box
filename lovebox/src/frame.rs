//! The 800x480 1-bit frame the panel shows, and its wire layout.
//!
//! # Byte layout
//!
//! [`Frame::as_packed_bytes`] returns 48,000 bytes: row-major, top row first, 100 bytes per
//! row, eight pixels per byte with the **most significant bit = leftmost pixel**, and
//! **bit 1 = white, bit 0 = black**. This is the layout of `epd-waveshare`'s
//! `graphics::Display7in5` buffer (the `epd7in5_v2` module, `main` branch):
//!
//! - `src/color.rs`, `impl ColorType for Color`: `bitmask` uses `0x80 >> (pos % 8)` and sets
//!   the bit for `Color::White`; `Color::get_bit_value` is `White => 1`, `Black => 0`.
//! - `src/graphics.rs`, `set_pixel`: byte index `x * 1 / 8 + y * line_bytes(width, 1)`.
//! - `src/epd7in5_v2/mod.rs`, `update_frame`: the buffer goes to the panel unchanged with
//!   `cmd_with_data(spi, Command::DataStartTransmission2, buffer)`.
//!
//! The same bytes are the raw data of a 1-bit grayscale PNG (where 1 is white), which is
//! how [`Frame::to_png`] writes it.

use image::GrayImage;

/// Panel width in pixels.
pub const WIDTH: u32 = 800;
/// Panel height in pixels.
pub const HEIGHT: u32 = 480;
/// Bytes per packed row (`WIDTH / 8`).
pub const ROW_BYTES: usize = (WIDTH / 8) as usize;
/// Bytes in a packed frame (`ROW_BYTES * HEIGHT`) = 48,000.
pub const FRAME_BYTES: usize = ROW_BYTES * HEIGHT as usize;

/// One panel pixel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Pixel {
    /// Ink.
    Black,
    /// Paper.
    White,
}

/// Gray level at or above which a pixel becomes white in [`Frame::from_gray`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Threshold(pub u8);

impl Default for Threshold {
    /// 50%: values `>= 128` are white.
    fn default() -> Self {
        Self(128)
    }
}

/// An 800x480 1-bit image in the panel's packed layout (see the module docs).
#[derive(Clone, PartialEq, Eq)]
pub struct Frame {
    bits: Vec<u8>,
}

impl std::fmt::Debug for Frame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Frame")
            .field("white_pixels", &self.count_white())
            .finish()
    }
}

impl Frame {
    /// Width in pixels (`WIDTH`).
    pub const WIDTH: u32 = WIDTH;
    /// Height in pixels (`HEIGHT`).
    pub const HEIGHT: u32 = HEIGHT;

    /// An all-white frame (every byte `0xFF`).
    pub fn white() -> Self {
        Self {
            bits: vec![0xFF; FRAME_BYTES],
        }
    }

    /// An all-black frame (every byte `0x00`).
    pub fn black() -> Self {
        Self {
            bits: vec![0x00; FRAME_BYTES],
        }
    }

    /// Builds a frame from packed bytes in the layout of [`Frame::as_packed_bytes`].
    /// Returns `None` unless `bytes.len() == FRAME_BYTES`.
    pub fn from_packed_bytes(bytes: &[u8]) -> Option<Self> {
        (bytes.len() == FRAME_BYTES).then(|| Self {
            bits: bytes.to_vec(),
        })
    }

    fn index(x: u32, y: u32) -> (usize, u8) {
        (y as usize * ROW_BYTES + (x / 8) as usize, 0x80 >> (x % 8))
    }

    /// The pixel at (`x`, `y`). Panics if the point is outside the frame.
    pub fn get(&self, x: u32, y: u32) -> Pixel {
        assert!(x < WIDTH && y < HEIGHT, "pixel ({x}, {y}) outside 800x480");
        let (i, mask) = Self::index(x, y);
        if self.bits[i] & mask != 0 {
            Pixel::White
        } else {
            Pixel::Black
        }
    }

    /// Sets the pixel at (`x`, `y`). Points outside the frame are ignored (clipping), so
    /// renderers can draw shapes that overhang the edge.
    pub fn set(&mut self, x: u32, y: u32, pixel: Pixel) {
        if x >= WIDTH || y >= HEIGHT {
            return;
        }
        let (i, mask) = Self::index(x, y);
        match pixel {
            Pixel::White => self.bits[i] |= mask,
            Pixel::Black => self.bits[i] &= !mask,
        }
    }

    /// Thresholds a grayscale image: `value >= threshold` is white. The image is placed at
    /// the top-left corner; any part outside 800x480 is dropped and any uncovered area is
    /// white.
    pub fn from_gray(img: &GrayImage, threshold: Threshold) -> Self {
        let mut frame = Self::white();
        let (w, h) = (img.width().min(WIDTH), img.height().min(HEIGHT));
        for y in 0..h {
            for x in 0..w {
                if img.get_pixel(x, y).0[0] < threshold.0 {
                    frame.set(x, y, Pixel::Black);
                }
            }
        }
        frame
    }

    /// The frame as an 8-bit grayscale image (0 or 255), for previews and comparisons.
    pub fn to_gray(&self) -> GrayImage {
        GrayImage::from_fn(WIDTH, HEIGHT, |x, y| match self.get(x, y) {
            Pixel::White => image::Luma([255]),
            Pixel::Black => image::Luma([0]),
        })
    }

    /// The packed panel bytes: 48,000 bytes, row-major, 100 bytes per row, MSB = leftmost
    /// pixel, 1 = white (the `epd-waveshare` `Display7in5` layout; see the module docs).
    pub fn as_packed_bytes(&self) -> &[u8] {
        &self.bits
    }

    /// Number of white pixels.
    pub fn count_white(&self) -> usize {
        self.bits.iter().map(|b| b.count_ones() as usize).sum()
    }

    /// Fraction of white pixels, `0.0..=1.0`.
    pub fn white_fraction(&self) -> f64 {
        self.count_white() as f64 / (WIDTH as f64 * HEIGHT as f64)
    }

    /// Encodes the frame as a 1-bit grayscale PNG (800x480). The PNG's raw rows are exactly
    /// [`Frame::as_packed_bytes`].
    pub fn to_png(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(8 * 1024);
        let mut encoder = png::Encoder::new(&mut out, WIDTH, HEIGHT);
        encoder.set_color(png::ColorType::Grayscale);
        encoder.set_depth(png::BitDepth::One);
        encoder.set_compression(png::Compression::High);
        // Writing to a Vec cannot fail and the data length is fixed by construction.
        let mut writer = encoder
            .write_header()
            .expect("PNG header to an in-memory buffer");
        writer
            .write_image_data(&self.bits)
            .expect("PNG data of the declared size to an in-memory buffer");
        writer.finish().expect("PNG trailer to an in-memory buffer");
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn white_and_black_constructors() {
        assert_eq!(Frame::white().count_white(), 800 * 480);
        assert_eq!(Frame::black().count_white(), 0);
        assert!(Frame::white().as_packed_bytes().iter().all(|&b| b == 0xFF));
    }

    #[test]
    fn set_get_round_trip() {
        let mut f = Frame::white();
        for &(x, y) in &[
            (0, 0),
            (7, 0),
            (8, 0),
            (799, 0),
            (0, 479),
            (799, 479),
            (123, 321),
        ] {
            assert_eq!(f.get(x, y), Pixel::White);
            f.set(x, y, Pixel::Black);
            assert_eq!(f.get(x, y), Pixel::Black);
        }
        assert_eq!(f.count_white(), 800 * 480 - 7);
        f.set(123, 321, Pixel::White);
        assert_eq!(f.get(123, 321), Pixel::White);
        // Out of range is clipped, not a panic.
        f.set(800, 0, Pixel::Black);
        f.set(0, 480, Pixel::Black);
        assert_eq!(f.count_white(), 800 * 480 - 6);
    }

    #[test]
    fn packed_length_is_48000() {
        assert_eq!(Frame::white().as_packed_bytes().len(), 48_000);
        assert_eq!(FRAME_BYTES, 48_000);
        assert_eq!(ROW_BYTES, 100);
    }

    #[test]
    fn known_pattern_packs_msb_first_white_is_one() {
        let mut f = Frame::white();
        // Row 0: pixel 0 black -> first byte 0b0111_1111.
        f.set(0, 0, Pixel::Black);
        // Row 0: pixels 8..=9 black -> second byte 0b0011_1111.
        f.set(8, 0, Pixel::Black);
        f.set(9, 0, Pixel::Black);
        // Row 1: pixel 7 black -> byte 100 is 0b1111_1110.
        f.set(7, 1, Pixel::Black);
        // Last pixel black -> last byte 0b1111_1110.
        f.set(799, 479, Pixel::Black);
        let b = f.as_packed_bytes();
        assert_eq!(b[0], 0x7F);
        assert_eq!(b[1], 0x3F);
        assert_eq!(b[2], 0xFF);
        assert_eq!(b[100], 0xFE);
        assert_eq!(b[47_999], 0xFE);
        assert_eq!(Frame::from_packed_bytes(b).unwrap(), f);
        assert!(Frame::from_packed_bytes(&b[1..]).is_none());
    }

    #[test]
    fn from_gray_thresholds_at_50_percent() {
        let img = GrayImage::from_fn(800, 480, |x, _| {
            image::Luma([if x < 400 { 127 } else { 128 }])
        });
        let f = Frame::from_gray(&img, Threshold::default());
        assert_eq!(f.get(399, 10), Pixel::Black);
        assert_eq!(f.get(400, 10), Pixel::White);
        assert_eq!(f.count_white(), 400 * 480);
    }

    #[test]
    fn png_round_trips_through_the_image_crate() {
        let mut f = Frame::white();
        for x in 0..800 {
            f.set(x, x % 480, Pixel::Black);
        }
        let png = f.to_png();
        let decoded = image::load_from_memory(&png).unwrap().to_luma8();
        assert_eq!(decoded.dimensions(), (800, 480));
        assert_eq!(Frame::from_gray(&decoded, Threshold::default()), f);
    }
}
