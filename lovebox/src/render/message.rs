//! Message cards: the 2022 layout (AGENTS.md "Baseline"), preserved exactly.

use super::text::{textwrap, Canvas};
use crate::fonts::Face;
use crate::frame::Frame;

/// Left edge of every message line, in pixels.
pub const MESSAGE_X: f32 = 10.0;

/// Baseline of each line below its top, as a fraction of the font size. 2022 drew each
/// line with Pillow's default "la" anchor, which puts the font's ascender at the line top;
/// its Palatino had an ascender of 0.727 em (`hhea`, checked in `legacy-2022`). The bundled
/// faces have ascenders near 1.0 em, which would push every line about a quarter of an em
/// lower and cut off the fifth 96 px line, so baselines stay where Palatino put them.
pub const BASELINE_EM: f32 = 0.727;

/// The four message sizes of the 2022 version. The size is the em height in pixels
/// (Pillow's `ImageFont.truetype(font, size)`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MessageSize {
    /// 30 px: 58 characters per line, 15 lines, 32 px line advance.
    Pt30,
    /// 48 px: 35 characters per line, 9 lines, 53 px line advance.
    Pt48,
    /// 64 px: 26 characters per line, 7 lines, 68 px line advance.
    Pt64,
    /// 96 px: 16 characters per line, 5 lines, 100 px line advance.
    Pt96,
}

impl MessageSize {
    /// All sizes, smallest first.
    pub const ALL: [MessageSize; 4] = [Self::Pt30, Self::Pt48, Self::Pt64, Self::Pt96];

    /// The size for `30`, `48`, `64` or `96`; `None` for anything else.
    pub fn from_pt(pt: u32) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.pt() == pt)
    }

    /// Font size (em height in pixels).
    pub fn pt(self) -> u32 {
        match self {
            Self::Pt30 => 30,
            Self::Pt48 => 48,
            Self::Pt64 => 64,
            Self::Pt96 => 96,
        }
    }

    /// Characters per line.
    pub fn wrap_width(self) -> usize {
        match self {
            Self::Pt30 => 58,
            Self::Pt48 => 35,
            Self::Pt64 => 26,
            Self::Pt96 => 16,
        }
    }

    /// Maximum lines; text beyond them is truncated with `...`.
    pub fn max_lines(self) -> usize {
        match self {
            Self::Pt30 => 15,
            Self::Pt48 => 9,
            Self::Pt64 => 7,
            Self::Pt96 => 5,
        }
    }

    /// Vertical distance between line tops, in pixels.
    pub fn line_advance(self) -> u32 {
        match self {
            Self::Pt30 => 32,
            Self::Pt48 => 53,
            Self::Pt64 => 68,
            Self::Pt96 => 100,
        }
    }
}

/// The lines a message is drawn as: `textwrap.wrap(text, width, max_lines,
/// placeholder="...")`, as in 2022.
pub fn message_lines(text: &str, size: MessageSize) -> Vec<String> {
    textwrap(text, size.wrap_width(), size.max_lines(), "...")
}

/// Draws a message: lines at x = 10, line `i` with its top at `i * line_advance` from
/// y = 0 and its baseline [`BASELINE_EM`] below that, as in 2022.
pub(crate) fn render_message(text: &str, size: MessageSize, face: &Face) -> Frame {
    let font = face.font();
    let em = size.pt() as f32;
    let mut canvas = Canvas::new();
    for (i, line) in message_lines(text, size).iter().enumerate() {
        let top = (i as u32 * size.line_advance()) as f32;
        canvas.draw_text(font, em, MESSAGE_X, top + BASELINE_EM * em, line);
    }
    canvas.to_frame()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fonts::Fonts;
    use crate::frame::Pixel;

    const LOREM: &str = "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do \
        eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, \
        quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat. \
        Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu \
        fugiat nulla pariatur. Excepteur sint occaecat cupidatat non proident, sunt in culpa \
        qui officia deserunt mollit anim id est laborum.";

    #[test]
    fn sizes_carry_the_2022_values() {
        let table: Vec<_> = MessageSize::ALL
            .iter()
            .map(|s| (s.pt(), s.wrap_width(), s.max_lines(), s.line_advance()))
            .collect();
        assert_eq!(
            table,
            [
                (30, 58, 15, 32),
                (48, 35, 9, 53),
                (64, 26, 7, 68),
                (96, 16, 5, 100)
            ]
        );
        assert_eq!(MessageSize::from_pt(48), Some(MessageSize::Pt48));
        assert_eq!(MessageSize::from_pt(40), None);
    }

    #[test]
    fn each_size_wraps_at_its_width_and_line_count_and_truncates_with_dots() {
        let long = format!("{LOREM} {LOREM} {LOREM}");
        for size in MessageSize::ALL {
            let lines = message_lines(&long, size);
            assert_eq!(lines.len(), size.max_lines(), "{size:?}");
            for line in &lines {
                assert!(
                    line.chars().count() <= size.wrap_width(),
                    "{size:?}: {line:?}"
                );
            }
            assert!(lines.last().unwrap().ends_with("..."), "{size:?}");
            // A line that would have room for the next word is not cut short.
            assert!(
                lines[0].chars().count() > size.wrap_width() - 12,
                "{size:?}"
            );

            let short = message_lines("I love you", size);
            assert_eq!(short, ["I love you"]);
        }
    }

    #[test]
    fn text_starts_at_x10_y0_and_stays_left() {
        let fonts = Fonts::bundled();
        let frame = render_message("HELLO", MessageSize::Pt96, fonts.default_face());
        let mut min_x = u32::MAX;
        let mut min_y = u32::MAX;
        for y in 0..480 {
            for x in 0..800 {
                if frame.get(x, y) == Pixel::Black {
                    min_x = min_x.min(x);
                    min_y = min_y.min(y);
                }
            }
        }
        // Capital H's left side bearing is a few pixels. Its top sits at 0.727 em minus
        // the cap height (EB Garamond: 0.65 em) below y = 0, about 7 px at 96 px.
        assert!((10..20).contains(&min_x), "min_x {min_x}");
        assert!((3..12).contains(&min_y), "min_y {min_y}");
    }
}
