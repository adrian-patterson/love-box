//! Quote cards: body text plus optional attribution, auto-fitted to the frame.

use super::text::{text_width, vertical_metrics, wrap_px, Canvas};
use super::RenderError;
use crate::fonts::{Face, DEFAULT_FONT};
use crate::frame::{Frame, HEIGHT, WIDTH};

/// Margin on every side, in pixels.
pub const QUOTE_MARGIN: u32 = 40;
/// Smallest body size the auto-fit tries (em height in pixels).
pub const QUOTE_MIN_PX: u32 = 14;
/// Largest body size the auto-fit tries.
pub const QUOTE_MAX_PX: u32 = 120;
/// Line height as a multiple of the font size.
pub const QUOTE_LINE_HEIGHT: f32 = 1.25;
/// Default maximum body length in characters (after whitespace normalisation).
pub const QUOTE_MAX_CHARS: usize = 2000;

/// Horizontal alignment of the quote body. The attribution is always right-aligned.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Align {
    /// Flush left at the margin.
    #[default]
    Left,
    /// Each line centred between the margins.
    Center,
}

/// What a quote card shows and how.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuoteSpec {
    /// The quote body. Whitespace runs collapse to one space; blank lines separate
    /// paragraphs.
    pub text: String,
    /// Who said it, drawn under the body as `— attribution`.
    pub attribution: Option<String>,
    /// Font id or display name ([`crate::fonts::Fonts::by_name`]).
    pub font: String,
    /// Body alignment.
    pub align: Align,
    /// Optional stricter body limit, in characters; never exceeds [`QUOTE_MAX_CHARS`].
    pub max_chars: usize,
}

impl QuoteSpec {
    /// A left-aligned quote in the default font with no attribution.
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            attribution: None,
            font: DEFAULT_FONT.to_owned(),
            align: Align::Left,
            max_chars: QUOTE_MAX_CHARS,
        }
    }
}

/// Collapses whitespace runs to single spaces and keeps paragraph breaks (one or more
/// blank lines) as exactly one blank line, `"\n\n"`. Trims both ends.
pub fn normalise_whitespace(text: &str) -> String {
    let mut paragraphs: Vec<String> = Vec::new();
    let mut current: Vec<&str> = Vec::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            if !current.is_empty() {
                paragraphs.push(current.join(" "));
                current.clear();
            }
        } else {
            current.extend(line.split_whitespace());
        }
    }
    if !current.is_empty() {
        paragraphs.push(current.join(" "));
    }
    paragraphs.join("\n\n")
}

/// The fitted layout of a quote: the chosen size and the wrapped lines.
#[derive(Clone, Debug, PartialEq)]
pub struct QuoteLayout {
    size_px: u32,
    /// Body lines; `None` is a blank line between paragraphs.
    body: Vec<Option<String>>,
    attribution_px: u32,
    attribution: Vec<String>,
}

impl QuoteLayout {
    /// Body font size (em height in pixels) the auto-fit chose.
    pub fn size_px(&self) -> u32 {
        self.size_px
    }

    /// Attribution font size in pixels.
    pub fn attribution_px(&self) -> u32 {
        self.attribution_px
    }

    /// Number of body lines, including blank lines between paragraphs.
    pub fn body_lines(&self) -> usize {
        self.body.len()
    }

    /// Number of attribution lines (0 without an attribution).
    pub fn attribution_lines(&self) -> usize {
        self.attribution.len()
    }

    fn height(&self) -> f32 {
        block_height(
            self.size_px,
            self.body.len(),
            self.attribution_px,
            self.attribution.len(),
        )
    }
}

/// Attribution size for body size `size`: 60% of it, but never under the 14 px floor nor
/// over the body size.
fn attribution_size(size: u32) -> u32 {
    ((size as f32 * 0.6).round() as u32)
        .max(QUOTE_MIN_PX)
        .min(size)
}

fn block_height(size: u32, body_lines: usize, attr_size: u32, attr_lines: usize) -> f32 {
    let mut h = body_lines as f32 * QUOTE_LINE_HEIGHT * size as f32;
    if attr_lines > 0 {
        // Half a line of space, then the attribution lines.
        h += 0.5 * size as f32 + attr_lines as f32 * QUOTE_LINE_HEIGHT * attr_size as f32;
    }
    h
}

fn text_box() -> (f32, f32) {
    (
        (WIDTH - 2 * QUOTE_MARGIN) as f32,
        (HEIGHT - 2 * QUOTE_MARGIN) as f32,
    )
}

/// Lays the quote out at `size`, or `None` if it does not fit the frame there.
fn layout_at(
    face: &Face,
    size: u32,
    paragraphs: &[&str],
    attribution: Option<&str>,
) -> Option<QuoteLayout> {
    let font = face.font();
    let (max_w, max_h) = text_box();
    let mut body = Vec::new();
    for (i, paragraph) in paragraphs.iter().enumerate() {
        if i > 0 {
            body.push(None);
        }
        body.extend(
            wrap_px(font, size as f32, paragraph, max_w)?
                .into_iter()
                .map(Some),
        );
        if block_height(size, body.len(), 0, 0) > max_h {
            return None;
        }
    }
    let attribution_px = attribution_size(size);
    let attribution = match attribution {
        Some(a) => wrap_px(font, attribution_px as f32, a, max_w)?,
        None => Vec::new(),
    };
    let layout = QuoteLayout {
        size_px: size,
        body,
        attribution_px,
        attribution,
    };
    (layout.height() <= max_h).then_some(layout)
}

/// `— attribution` with whitespace collapsed; no second dash if it already starts with one.
fn attribution_line(attribution: &str) -> Option<String> {
    let a = attribution.split_whitespace().collect::<Vec<_>>().join(" ");
    if a.is_empty() {
        None
    } else if a.starts_with(['—', '–', '-']) {
        Some(a)
    } else {
        Some(format!("— {a}"))
    }
}

/// Finds the largest size in `[QUOTE_MIN_PX, QUOTE_MAX_PX]` at which the greedily wrapped
/// body plus attribution fit inside the margins (binary search).
///
/// Errors: [`RenderError::TooLong`] when the normalised body is over `spec.max_chars`
/// (checked before any layout) or does not fit even at 14 px (for example one
/// unbreakable 2,000-character word); [`RenderError::InvalidInput`] for an empty body.
pub fn fit_quote(spec: &QuoteSpec, face: &Face) -> Result<QuoteLayout, RenderError> {
    let text = normalise_whitespace(&spec.text);
    let chars = text.chars().count();
    if chars > spec.max_chars.min(QUOTE_MAX_CHARS) {
        return Err(RenderError::TooLong {
            chars,
            max: spec.max_chars.min(QUOTE_MAX_CHARS),
        });
    }
    if text.is_empty() {
        return Err(RenderError::InvalidInput("quote text is empty".to_owned()));
    }
    let paragraphs: Vec<&str> = text.split("\n\n").collect();
    let attribution = spec.attribution.as_deref().and_then(attribution_line);
    let attribution = attribution.as_deref();

    let Some(mut best) = layout_at(face, QUOTE_MIN_PX, &paragraphs, attribution) else {
        return Err(RenderError::TooLong {
            chars,
            max: spec.max_chars.min(QUOTE_MAX_CHARS),
        });
    };
    let (mut lo, mut hi) = (QUOTE_MIN_PX, QUOTE_MAX_PX);
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        match layout_at(face, mid, &paragraphs, attribution) {
            Some(layout) => {
                best = layout;
                lo = mid;
            }
            None => hi = mid - 1,
        }
    }
    Ok(best)
}

/// Draws a fitted layout: the block is centred vertically between the margins; body
/// lines are left-aligned or centred; attribution lines are right-aligned.
pub(crate) fn draw_quote(layout: &QuoteLayout, face: &Face, align: Align) -> Frame {
    let font = face.font();
    let (max_w, max_h) = text_box();
    let left = QUOTE_MARGIN as f32;
    let mut canvas = Canvas::new();
    let mut top = QUOTE_MARGIN as f32 + ((max_h - layout.height()) / 2.0).max(0.0);

    let size = layout.size_px as f32;
    let line_h = QUOTE_LINE_HEIGHT * size;
    let (ascent, descent) = vertical_metrics(font, size);
    // Centre the font's ascent..descent box in each line box.
    let baseline_offset = (line_h - (ascent - descent)) / 2.0 + ascent;
    for line in &layout.body {
        if let Some(line) = line {
            let x = match align {
                Align::Left => left,
                Align::Center => left + ((max_w - text_width(font, size, line)) / 2.0).max(0.0),
            };
            canvas.draw_text(font, size, x, top + baseline_offset, line);
        }
        top += line_h;
    }

    if !layout.attribution.is_empty() {
        top += 0.5 * size;
        let a_size = layout.attribution_px as f32;
        let a_line_h = QUOTE_LINE_HEIGHT * a_size;
        let (a_ascent, a_descent) = vertical_metrics(font, a_size);
        let a_offset = (a_line_h - (a_ascent - a_descent)) / 2.0 + a_ascent;
        for line in &layout.attribution {
            let x = left + max_w - text_width(font, a_size, line);
            canvas.draw_text(font, a_size, x, top + a_offset, line);
            top += a_line_h;
        }
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
        qui officia deserunt mollit anim id est laborum. ";

    /// Lorem ipsum cut at a word boundary to at most `n` characters.
    pub(crate) fn lorem(n: usize) -> String {
        let mut s = LOREM.repeat(n / LOREM.len() + 2);
        s.truncate(n);
        let end = s.rfind(' ').unwrap_or(n);
        s.truncate(end);
        s
    }

    fn ink_bounds(frame: &Frame) -> (u32, u32, u32, u32) {
        let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
        for y in 0..480 {
            for x in 0..800 {
                if frame.get(x, y) == Pixel::Black {
                    (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
                }
            }
        }
        (x0, y0, x1, y1)
    }

    #[test]
    #[ignore = "measurement: run with --ignored --nocapture"]
    fn measure_capacity_at_the_floor() {
        let fonts = Fonts::bundled();
        for face in fonts.faces() {
            let (mut lo, mut hi) = (100usize, 6000usize);
            while lo < hi {
                let mid = (lo + hi).div_ceil(2);
                let mut spec = QuoteSpec::new(lorem(mid));
                spec.max_chars = usize::MAX;
                if layout_at(
                    face,
                    QUOTE_MIN_PX,
                    &[&normalise_whitespace(&spec.text)],
                    None,
                )
                .is_some()
                {
                    lo = mid;
                } else {
                    hi = mid - 1;
                }
            }
            println!("{:28} fits {lo} chars of lorem at 14 px", face.id());
        }
    }

    #[test]
    fn whitespace_collapses_and_paragraphs_survive() {
        assert_eq!(
            normalise_whitespace("  one   two\nthree\n\n\n  four\t five \n"),
            "one two three\n\nfour five"
        );
        assert_eq!(normalise_whitespace(" \n \n"), "");
    }

    #[test]
    fn twelve_words_render_large() {
        let fonts = Fonts::bundled();
        let spec =
            QuoteSpec::new("Love is not love which alters when it alteration finds, or bends.");
        let layout = fit_quote(&spec, fonts.default_face()).unwrap();
        assert!(layout.size_px() >= 60, "size {}", layout.size_px());
    }

    #[test]
    fn two_thousand_characters_fit_in_every_bundled_face() {
        let fonts = Fonts::bundled();
        let mut text = lorem(2000);
        while text.len() + 2 <= 2000 {
            text.push_str(" a");
        }
        if text.len() < 2000 {
            text.push('a');
        }
        assert_eq!(text.chars().count(), 2000);
        for face in fonts.faces() {
            let mut spec = QuoteSpec::new(text.clone());
            spec.attribution = Some("A reader".into());
            let layout = fit_quote(&spec, face).unwrap();
            assert!(layout.size_px() >= QUOTE_MIN_PX, "{}", face.id());
        }
    }

    #[test]
    fn over_max_chars_is_too_long_before_layout() {
        let fonts = Fonts::bundled();
        let text = "a".repeat(2001);
        match fit_quote(&QuoteSpec::new(text), fonts.default_face()) {
            Err(RenderError::TooLong {
                chars: 2001,
                max: 2000,
            }) => {}
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn caller_can_lower_but_never_raise_the_character_cap() {
        let fonts = Fonts::bundled();
        let mut spec = QuoteSpec::new("a".repeat(2001));
        spec.max_chars = usize::MAX;
        assert!(matches!(
            fit_quote(&spec, fonts.default_face()),
            Err(RenderError::TooLong { max: 2000, .. })
        ));
        spec.text = "One short quote".into();
        spec.max_chars = 5;
        assert!(matches!(
            fit_quote(&spec, fonts.default_face()),
            Err(RenderError::TooLong { max: 5, .. })
        ));
    }

    #[test]
    fn one_unbreakable_word_is_too_long_not_a_panic() {
        let fonts = Fonts::bundled();
        let text = "x".repeat(2000);
        match fit_quote(&QuoteSpec::new(text), fonts.default_face()) {
            Err(RenderError::TooLong {
                chars: 2000,
                max: 2000,
            }) => {}
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn empty_quote_is_invalid() {
        let fonts = Fonts::bundled();
        assert!(matches!(
            fit_quote(&QuoteSpec::new(" \n "), fonts.default_face()),
            Err(RenderError::InvalidInput(_))
        ));
    }

    #[test]
    fn attribution_takes_space_and_is_right_aligned() {
        let fonts = Fonts::bundled();
        let face = fonts.default_face();
        let text = "Whatever our souls are made of, his and mine are the same.";
        let bare = fit_quote(&QuoteSpec::new(text), face).unwrap();
        assert_eq!(bare.attribution_lines(), 0);
        let mut spec = QuoteSpec::new(text);
        spec.attribution = Some("Emily Brontë".into());
        let with = fit_quote(&spec, face).unwrap();
        assert_eq!(with.attribution_lines(), 1);
        assert!(with.size_px() <= bare.size_px());
        assert!(with.attribution_px() < with.size_px());

        let frame = draw_quote(&with, face, Align::Left);
        let (x0, y0, x1, y1) = ink_bounds(&frame);
        assert!(x0 >= 38 && y0 >= 30, "ink starts at ({x0}, {y0})");
        // The attribution's right edge sits at the right margin.
        assert!((750..=762).contains(&x1), "ink ends at x = {x1}");
        assert!(y1 <= 450, "ink ends at y = {y1}");
    }

    #[test]
    fn centre_alignment_centres_short_lines() {
        let fonts = Fonts::bundled();
        let face = fonts.default_face();
        let mut spec = QuoteSpec::new("Short.\n\nAnd a somewhat longer second paragraph here.");
        spec.align = Align::Center;
        let layout = fit_quote(&spec, face).unwrap();
        let frame = draw_quote(&layout, face, Align::Center);
        let (x0, _, x1, _) = ink_bounds(&frame);
        let centre = (x0 + x1) / 2;
        assert!((390..=410).contains(&centre), "centre {centre}");
        let left = draw_quote(&layout, face, Align::Left);
        assert_ne!(left, frame);
    }

    #[test]
    fn every_bundled_font_fits_a_short_and_a_long_quote() {
        let fonts = Fonts::bundled();
        // Libre Baskerville, the widest face, holds ~2,150 characters at 14 px.
        let long = lorem(2000);
        for face in fonts.faces() {
            let short = fit_quote(&QuoteSpec::new("Hold fast to dreams."), face).unwrap();
            assert!(short.size_px() >= 60, "{}: {}", face.id(), short.size_px());
            let frame = draw_quote(&short, face, Align::Left);
            assert!(
                frame.count_white() < 800 * 480,
                "{} drew nothing",
                face.id()
            );
            let long = fit_quote(&QuoteSpec::new(long.clone()), face).unwrap();
            assert!(long.size_px() >= QUOTE_MIN_PX, "{}", face.id());
        }
    }
}
