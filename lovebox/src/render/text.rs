//! Text shared by the message and quote renderers: character-count wrapping (a port of
//! Python's `textwrap.wrap`, which the 2022 version used), pixel-width wrapping, and an
//! anti-aliased rasteriser that is thresholded to 1 bit.

use ab_glyph::{point, Font, FontRef, GlyphId, PxScale, ScaleFont};

use crate::frame::{Frame, Pixel, HEIGHT, WIDTH};

/// ASCII whitespace, the set Python's `textwrap` splits on.
fn is_ws(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r' | '\x0b' | '\x0c')
}

/// Python's `[^\d\W]`: word characters that are not digits.
fn is_letter(c: char) -> bool {
    (c.is_alphanumeric() && !c.is_numeric()) || c == '_'
}

/// Splits text into textwrap chunks: whitespace runs, and words split after a hyphen that
/// joins letters (`well-known` -> `well-`, `known`), as `TextWrapper.wordsep_re` does with
/// `break_on_hyphens=True`.
fn split_chunks(text: &[char]) -> Vec<Vec<char>> {
    let mut chunks = Vec::new();
    let mut i = 0;
    let n = text.len();
    let letter_at = |j: isize| j >= 0 && (j as usize) < n && is_letter(text[j as usize]);
    let char_at = |j: isize| (j >= 0 && (j as usize) < n).then(|| text[j as usize]);
    while i < n {
        let start = i;
        if is_ws(text[i]) {
            while i < n && is_ws(text[i]) {
                i += 1;
            }
        } else {
            while i < n && !is_ws(text[i]) {
                let c = text[i];
                i += 1;
                if c == '-' {
                    let h = (i - 1) as isize;
                    let behind = (letter_at(h - 1) && letter_at(h - 2))
                        || (letter_at(h - 1) && char_at(h - 2) == Some('-') && letter_at(h - 3));
                    let ahead = letter_at(h + 1)
                        && (letter_at(h + 2) || (char_at(h + 2) == Some('-') && letter_at(h + 3)));
                    if behind && ahead && i - 1 > start {
                        break;
                    }
                }
            }
        }
        chunks.push(text[start..i].to_vec());
    }
    chunks
}

/// `str.expandtabs(8)` followed by mapping each whitespace character to a space.
fn munge_whitespace(text: &str) -> Vec<char> {
    let mut out = Vec::with_capacity(text.len());
    let mut column = 0usize;
    for c in text.chars() {
        match c {
            '\t' => {
                let spaces = 8 - column % 8;
                out.extend(std::iter::repeat_n(' ', spaces));
                column += spaces;
            }
            '\n' | '\r' => {
                out.push(' ');
                column = 0;
            }
            c if is_ws(c) => {
                out.push(' ');
                column += 1;
            }
            c => {
                out.push(c);
                column += 1;
            }
        }
    }
    out
}

fn is_blank(chunk: &[char]) -> bool {
    chunk.iter().all(|&c| is_ws(c))
}

/// `textwrap.wrap(text, width=width, max_lines=max_lines, placeholder=placeholder)` with
/// Python's defaults (tabs expanded to 8-column stops, whitespace normalised to spaces,
/// long words broken, break on hyphens, whitespace dropped at line ends). Widths count
/// Unicode scalar values, as Python's `len` does.
pub(crate) fn textwrap(
    text: &str,
    width: usize,
    max_lines: usize,
    placeholder: &str,
) -> Vec<String> {
    assert!(width > 0 && max_lines > 0);
    let munged = munge_whitespace(text);
    let mut chunks = split_chunks(&munged);
    chunks.reverse();
    let placeholder: Vec<char> = placeholder.chars().collect();
    let mut lines: Vec<Vec<char>> = Vec::new();

    while !chunks.is_empty() {
        let mut cur: Vec<Vec<char>> = Vec::new();
        let mut cur_len = 0usize;
        if !lines.is_empty() && is_blank(chunks.last().unwrap()) {
            chunks.pop();
        }
        while let Some(chunk) = chunks.last() {
            if cur_len + chunk.len() <= width {
                cur_len += chunk.len();
                cur.push(chunks.pop().unwrap());
            } else {
                break;
            }
        }
        if chunks.last().is_some_and(|c| c.len() > width) {
            // _handle_long_word with break_long_words and break_on_hyphens.
            let space_left = width
                .saturating_sub(cur_len)
                .max(if width < 1 { 1 } else { 0 });
            let chunk = chunks.last_mut().unwrap();
            let mut end = space_left;
            if chunk.len() > space_left {
                if let Some(h) = chunk[..space_left].iter().rposition(|&c| c == '-') {
                    if h > 0 && chunk[..h].iter().any(|&c| c != '-') {
                        end = h + 1;
                    }
                }
            }
            let rest = chunk.split_off(end);
            cur.push(std::mem::replace(chunk, rest));
            cur_len = cur.iter().map(Vec::len).sum();
        }
        if let Some(blank) = cur.pop_if(|c| is_blank(c)) {
            cur_len -= blank.len();
        }
        if cur.is_empty() {
            continue;
        }
        let last_line_ok =
            (chunks.is_empty() || (chunks.len() == 1 && is_blank(&chunks[0]))) && cur_len <= width;
        if lines.len() + 1 < max_lines || last_line_ok {
            lines.push(cur.concat());
            continue;
        }
        // Too many lines: end this one with the placeholder.
        let mut placed = false;
        while let Some(last) = cur.last() {
            if !is_blank(last) && cur_len + placeholder.len() <= width {
                cur.push(placeholder.clone());
                lines.push(cur.concat());
                placed = true;
                break;
            }
            cur_len -= cur.pop().unwrap().len();
        }
        if !placed {
            if let Some(prev) = lines.last_mut() {
                while prev.last().is_some_and(|&c| is_ws(c)) {
                    prev.pop();
                }
                if prev.len() + placeholder.len() <= width {
                    prev.extend_from_slice(&placeholder);
                    break;
                }
            }
            let trimmed: Vec<char> = placeholder
                .iter()
                .copied()
                .skip_while(|&c| is_ws(c))
                .collect();
            lines.push(trimmed);
        }
        break;
    }
    lines.into_iter().map(|l| l.into_iter().collect()).collect()
}

/// The `PxScale` at which the font's em square is `em_px` pixels, which is what Pillow's
/// `ImageFont.truetype(font, size)` means by `size`. `ab_glyph` scales by ascent-descent.
pub(crate) fn em_scale(font: &FontRef<'_>, em_px: f32) -> PxScale {
    let upem = font.units_per_em().unwrap_or(1000.0);
    PxScale::from(em_px * font.height_unscaled() / upem)
}

/// Ascent and descent in pixels (descent is negative) at `em_px`.
pub(crate) fn vertical_metrics(font: &FontRef<'_>, em_px: f32) -> (f32, f32) {
    let scaled = font.as_scaled(em_scale(font, em_px));
    (scaled.ascent(), scaled.descent())
}

/// Advance width of `text` in pixels at `em_px`, with kerning.
pub(crate) fn text_width(font: &FontRef<'_>, em_px: f32, text: &str) -> f32 {
    let scaled = font.as_scaled(em_scale(font, em_px));
    let mut width = 0.0;
    let mut prev: Option<GlyphId> = None;
    for c in text.chars() {
        let id = scaled.glyph_id(c);
        if let Some(p) = prev {
            width += scaled.kern(p, id);
        }
        width += scaled.h_advance(id);
        prev = Some(id);
    }
    width
}

/// Greedy word wrap of one paragraph (words separated by single spaces) to `max_width`
/// pixels. Returns `None` if any single word is wider than `max_width`.
pub(crate) fn wrap_px(
    font: &FontRef<'_>,
    em_px: f32,
    paragraph: &str,
    max_width: f32,
) -> Option<Vec<String>> {
    let space = text_width(font, em_px, " ");
    let mut lines = Vec::new();
    let mut line = String::new();
    let mut line_width = 0.0;
    for word in paragraph.split(' ').filter(|w| !w.is_empty()) {
        let w = text_width(font, em_px, word);
        if w > max_width {
            return None;
        }
        if line.is_empty() {
            line.push_str(word);
            line_width = w;
        } else if line_width + space + w <= max_width {
            line.push(' ');
            line.push_str(word);
            line_width += space + w;
        } else {
            lines.push(std::mem::take(&mut line));
            line.push_str(word);
            line_width = w;
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    Some(lines)
}

/// An 800x480 coverage buffer (0 = paper, 1 = full ink) that text is drawn into with
/// anti-aliasing and then thresholded to a [`Frame`] at 50%.
pub(crate) struct Canvas {
    coverage: Vec<f32>,
}

impl Canvas {
    pub(crate) fn new() -> Self {
        Self {
            coverage: vec![0.0; (WIDTH * HEIGHT) as usize],
        }
    }

    /// Draws `text` with its left edge at `x` and its baseline at `baseline`, both in
    /// pixels; anything outside the frame is clipped. Returns the advance width.
    pub(crate) fn draw_text(
        &mut self,
        font: &FontRef<'_>,
        em_px: f32,
        x: f32,
        baseline: f32,
        text: &str,
    ) -> f32 {
        let scale = em_scale(font, em_px);
        let scaled = font.as_scaled(scale);
        let mut caret = x;
        let mut prev: Option<GlyphId> = None;
        for c in text.chars() {
            let id = scaled.glyph_id(c);
            if let Some(p) = prev {
                caret += scaled.kern(p, id);
            }
            let glyph = id.with_scale_and_position(scale, point(caret, baseline));
            caret += scaled.h_advance(id);
            prev = Some(id);
            let Some(outline) = font.outline_glyph(glyph) else {
                continue;
            };
            let bounds = outline.px_bounds();
            let (ox, oy) = (bounds.min.x as i64, bounds.min.y as i64);
            let coverage = &mut self.coverage;
            outline.draw(|gx, gy, c| {
                let (px, py) = (ox + i64::from(gx), oy + i64::from(gy));
                if (0..i64::from(WIDTH)).contains(&px) && (0..i64::from(HEIGHT)).contains(&py) {
                    let cell = &mut coverage[(py as usize) * WIDTH as usize + px as usize];
                    *cell = (*cell + c).min(1.0);
                }
            });
        }
        caret - x
    }

    /// Thresholds at 50% coverage: `>= 0.5` is black. No dithering on text.
    pub(crate) fn to_frame(&self) -> Frame {
        let mut frame = Frame::white();
        for (i, &c) in self.coverage.iter().enumerate() {
            if c >= 0.5 {
                frame.set(i as u32 % WIDTH, i as u32 / WIDTH, Pixel::Black);
            }
        }
        frame
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fonts::Fonts;

    fn wrap(text: &str, width: usize, max_lines: usize) -> Vec<String> {
        textwrap(text, width, max_lines, "...")
    }

    // Expected values produced with CPython 3.9.6 `textwrap.wrap(text, width=..,
    // max_lines=.., placeholder="...")`.
    #[test]
    fn textwrap_matches_python() {
        assert_eq!(
            wrap("The quick brown fox jumps over the lazy dog", 10, 99),
            ["The quick", "brown fox", "jumps over", "the lazy", "dog"]
        );
        assert_eq!(
            wrap("The quick brown fox jumps over the lazy dog", 10, 3),
            ["The quick", "brown fox", "jumps..."]
        );
        assert_eq!(
            wrap("a well-known fact", 8, 9),
            ["a well-", "known", "fact"]
        );
        assert_eq!(
            wrap("supercalifragilisticexpialidocious", 10, 9),
            ["supercalif", "ragilistic", "expialidoc", "ious"]
        );
        assert_eq!(
            wrap("  leading\nand\ttabs  ", 20, 9),
            ["  leading and", "tabs"]
        );
        assert_eq!(wrap("", 10, 3), Vec::<String>::new());
        assert_eq!(wrap("aaaa bbbb cccc", 4, 2), ["aaaa", "..."]);
        assert_eq!(wrap("aaaa bbbb cccc", 6, 2), ["aaaa", "..."]);
        assert_eq!(wrap("aaaa bbbb cccc", 7, 2), ["aaaa", "bbbb..."]);
        assert_eq!(
            wrap("one-two-three-four x", 9, 9),
            ["one-two-", "three-", "four x"]
        );
        assert_eq!(wrap("abcdefghij klm", 5, 2), ["abcde", "..."]);
    }

    #[test]
    fn wrap_px_rejects_a_word_wider_than_the_line() {
        let fonts = Fonts::bundled();
        let font = fonts.default_face().font();
        assert!(wrap_px(font, 20.0, &"x".repeat(500), 720.0).is_none());
        let lines = wrap_px(font, 20.0, "one two three", 720.0).unwrap();
        assert_eq!(lines, ["one two three"]);
    }

    #[test]
    fn canvas_draws_ink_inside_the_text_box_only() {
        let fonts = Fonts::bundled();
        let font = fonts.default_face().font();
        let mut canvas = Canvas::new();
        let advance = canvas.draw_text(font, 48.0, 10.0, 60.0, "Hello");
        assert!(advance > 50.0);
        let frame = canvas.to_frame();
        let black = 800 * 480 - frame.count_white();
        assert!(black > 500, "{black} black pixels");
        for y in 0..480 {
            for x in 0..800 {
                if frame.get(x, y) == Pixel::Black {
                    assert!((5..=(15 + advance as u32)).contains(&x) && (10..=75).contains(&y));
                }
            }
        }
    }
}
