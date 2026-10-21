//! Error-diffusion dithering from grayscale to the 1-bit [`Frame`].

use image::{ImageBuffer, Luma};

use crate::frame::{Frame, Pixel, HEIGHT, WIDTH};

/// A grayscale image with `f32` samples, 0.0 = black, 1.0 = white.
pub type GrayF32 = ImageBuffer<Luma<f32>, Vec<f32>>;

/// Error-diffusion method.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Dither {
    /// Floyd–Steinberg (7/16, 3/16, 5/16, 1/16), serpentine scan. Preserves tone best.
    #[default]
    FloydSteinberg,
    /// Atkinson (1/8 to six neighbours, 2/8 of the error dropped): crisper, more contrast,
    /// blown highlights and blocked shadows.
    Atkinson,
}

/// Dithers `gray` (placed at the top-left; any uncovered area is white) to a frame.
pub fn dither(gray: &GrayF32, method: Dither) -> Frame {
    let (w, h) = (gray.width().min(WIDTH) as usize, gray.height().min(HEIGHT) as usize);
    let mut buf = vec![1.0f32; w * h];
    for y in 0..h {
        for x in 0..w {
            buf[y * w + x] = gray.get_pixel(x as u32, y as u32).0[0];
        }
    }
    match method {
        Dither::FloydSteinberg => floyd_steinberg(&mut buf, w, h),
        Dither::Atkinson => atkinson(&mut buf, w, h),
    }
}

fn quantise(frame: &mut Frame, x: usize, y: usize, value: f32) -> f32 {
    if value >= 0.5 {
        value - 1.0
    } else {
        frame.set(x as u32, y as u32, Pixel::Black);
        value
    }
}

fn floyd_steinberg(buf: &mut [f32], w: usize, h: usize) -> Frame {
    let mut frame = Frame::white();
    for y in 0..h {
        let left_to_right = y % 2 == 0;
        for i in 0..w {
            let x = if left_to_right { i } else { w - 1 - i };
            let err = quantise(&mut frame, x, y, buf[y * w + x]);
            // "Forward" is the scan direction; serpentine mirrors the kernel on odd rows.
            let fwd = |dx: isize| -> Option<usize> {
                let nx = if left_to_right {
                    x as isize + dx
                } else {
                    x as isize - dx
                };
                (0..w as isize).contains(&nx).then_some(nx as usize)
            };
            if let Some(nx) = fwd(1) {
                buf[y * w + nx] += err * 7.0 / 16.0;
            }
            if y + 1 < h {
                let row = (y + 1) * w;
                if let Some(nx) = fwd(-1) {
                    buf[row + nx] += err * 3.0 / 16.0;
                }
                buf[row + x] += err * 5.0 / 16.0;
                if let Some(nx) = fwd(1) {
                    buf[row + nx] += err * 1.0 / 16.0;
                }
            }
        }
    }
    frame
}

fn atkinson(buf: &mut [f32], w: usize, h: usize) -> Frame {
    const NEIGHBOURS: [(isize, usize); 6] = [(1, 0), (2, 0), (-1, 1), (0, 1), (1, 1), (0, 2)];
    let mut frame = Frame::white();
    for y in 0..h {
        for x in 0..w {
            let err = quantise(&mut frame, x, y, buf[y * w + x]) / 8.0;
            for (dx, dy) in NEIGHBOURS {
                let (nx, ny) = (x as isize + dx, y + dy);
                if (0..w as isize).contains(&nx) && ny < h {
                    buf[ny * w + nx as usize] += err;
                }
            }
        }
    }
    frame
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(value: f32) -> GrayF32 {
        GrayF32::from_pixel(WIDTH, HEIGHT, Luma([value]))
    }

    #[test]
    fn extremes_stay_pure() {
        for method in [Dither::FloydSteinberg, Dither::Atkinson] {
            assert_eq!(dither(&flat(1.0), method), Frame::white());
            assert_eq!(dither(&flat(0.0), method), Frame::black());
        }
    }

    #[test]
    fn flat_grays_keep_their_tone() {
        for value in [0.1f32, 0.25, 0.5, 0.75, 0.9] {
            let fs = dither(&flat(value), Dither::FloydSteinberg).white_fraction();
            assert!((fs - value as f64).abs() < 0.01, "FS {value}: {fs}");
        }
        // Atkinson drops a quarter of the error: mid gray holds, other tones are pushed
        // symmetrically towards the ends (more contrast), tones under 1/8 block to black
        // and over 7/8 blow out to white.
        let at = |v: f32| dither(&flat(v), Dither::Atkinson).white_fraction();
        assert!((at(0.5) - 0.5).abs() < 0.02, "Atkinson 0.5: {}", at(0.5));
        let (dark, light) = (at(0.25), at(0.75));
        assert!(dark < 0.25 && light > 0.75, "Atkinson 0.25: {dark}, 0.75: {light}");
        assert!((dark + light - 1.0).abs() < 0.02, "asymmetric: {dark} + {light}");
        assert!(dither(&flat(0.1), Dither::Atkinson).white_fraction() < 0.02);
        assert!(dither(&flat(0.9), Dither::Atkinson).white_fraction() > 0.98);
    }

    #[test]
    fn mid_gray_is_a_fine_pattern_not_a_block() {
        let f = dither(&flat(0.5), Dither::FloydSteinberg);
        // Every 8x8 block has both colours.
        for by in (0..HEIGHT).step_by(8) {
            for bx in (0..WIDTH).step_by(8) {
                let whites = (0..8)
                    .flat_map(|dy| (0..8).map(move |dx| (bx + dx, by + dy)))
                    .filter(|&(x, y)| f.get(x, y) == Pixel::White)
                    .count();
                assert!((8..=56).contains(&whites), "block ({bx}, {by}): {whites}");
            }
        }
    }
}
