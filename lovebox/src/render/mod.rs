//! The renderer: pure functions from a [`Card`] to an 800x480 1-bit [`Frame`].
//!
//! No global state and no filesystem access: fonts come from the caller's [`Fonts`], and
//! photos arrive as already-ingested pixels (see [`photo`]). The same call backs the
//! panel and `/api/preview`, so preview equals panel.

pub mod dither;
mod message;
pub mod photo;
mod quote;
mod text;

pub use self::dither::Dither;
pub use self::photo::{IngestLimits, PhotoSpec};

pub use self::message::{message_lines, MessageSize, BASELINE_EM, MESSAGE_X};
pub use self::quote::{
    fit_quote, normalise_whitespace, Align, QuoteLayout, QuoteSpec, QUOTE_LINE_HEIGHT,
    QUOTE_MARGIN, QUOTE_MAX_CHARS, QUOTE_MAX_PX, QUOTE_MIN_PX,
};

use std::fmt;
use std::sync::Arc;

use image::RgbImage;

use crate::fonts::{Face, Fonts, DEFAULT_FONT};
use crate::frame::Frame;

/// Something the frame can show.
#[derive(Clone, Debug, PartialEq)]
pub enum Card {
    /// A typed message in one of the four 2022 sizes, left-aligned.
    Message {
        /// The message text.
        text: String,
        /// One of the four sizes.
        size: MessageSize,
        /// Font id or display name ([`Fonts::by_name`]); [`DEFAULT_FONT`] by default.
        font: String,
    },
    /// A quote, auto-fitted to the frame.
    Quote(QuoteSpec),
    /// A photo: the bounded copy from [`photo::ingest`] and how to frame it.
    Photo {
        /// The ingested (bounded) image.
        image: Arc<RgbImage>,
        /// Crop, enhancement and dither settings.
        spec: PhotoSpec,
    },
    /// An all-white frame.
    Clear,
}

impl Card {
    /// A message card in the default font. The exact text `clear` becomes [`Card::Clear`],
    /// as in 2022.
    pub fn message(text: impl Into<String>, size: MessageSize) -> Self {
        let text = text.into();
        if text == "clear" {
            Self::Clear
        } else {
            Self::Message {
                text,
                size,
                font: DEFAULT_FONT.to_owned(),
            }
        }
    }
}

/// Why a card could not be rendered. Every variant except [`RenderError::Io`] is the
/// caller's input ([`RenderError::is_client_error`]), which the API maps to HTTP 4xx.
#[derive(Debug)]
pub enum RenderError {
    /// The text is longer than allowed, or does not fit the frame even at the smallest
    /// font size. `chars` is the input length, `max` the limit.
    TooLong {
        /// Characters in the input.
        chars: usize,
        /// The configured maximum.
        max: usize,
    },
    /// The image is too large to decode within the configured limits.
    TooLarge(String),
    /// No bundled font has this name.
    UnknownFont(String),
    /// The image format is not one the renderer decodes.
    UnsupportedFormat(String),
    /// The image data is corrupt or truncated.
    Decode(String),
    /// A parameter is out of range (for example a focal point outside 0..=1).
    InvalidInput(String),
    /// Reading an input file failed.
    Io(std::io::Error),
}

impl RenderError {
    /// True when the input was at fault (HTTP 4xx); false for server-side failures.
    pub fn is_client_error(&self) -> bool {
        !matches!(self, Self::Io(_))
    }
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLong { chars, max } => write!(
                f,
                "text too long: {chars} characters (maximum {max}, and it must fit the \
                 frame at 14 px)"
            ),
            Self::TooLarge(msg) => write!(f, "image too large: {msg}"),
            Self::UnknownFont(name) => write!(f, "unknown font: {name:?}"),
            Self::UnsupportedFormat(msg) => write!(f, "unsupported image format: {msg}"),
            Self::Decode(msg) => write!(f, "could not decode image: {msg}"),
            Self::InvalidInput(msg) => write!(f, "invalid input: {msg}"),
            Self::Io(err) => write!(f, "I/O error: {err}"),
        }
    }
}

impl std::error::Error for RenderError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            _ => None,
        }
    }
}

impl From<std::io::Error> for RenderError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

fn face<'a>(fonts: &'a Fonts, name: &str) -> Result<&'a Face, RenderError> {
    fonts
        .by_name(name)
        .ok_or_else(|| RenderError::UnknownFont(name.to_owned()))
}

/// Renders `card` to a frame.
pub fn render(card: &Card, fonts: &Fonts) -> Result<Frame, RenderError> {
    match card {
        Card::Message { text, size, font } => {
            Ok(message::render_message(text, *size, face(fonts, font)?))
        }
        Card::Quote(spec) => {
            let face = face(fonts, &spec.font)?;
            let layout = quote::fit_quote(spec, face)?;
            Ok(quote::draw_quote(&layout, face, spec.align))
        }
        Card::Photo { image, spec } => Ok(photo::frame(image, spec)),
        Card::Clear => Ok(Frame::white()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clear_text_maps_to_clear_card_and_renders_white() {
        assert_eq!(Card::message("clear", MessageSize::Pt48), Card::Clear);
        assert_ne!(Card::message("clear ", MessageSize::Pt48), Card::Clear);
        assert_ne!(Card::message("Clear", MessageSize::Pt48), Card::Clear);
        let fonts = Fonts::bundled();
        assert_eq!(render(&Card::Clear, &fonts).unwrap(), Frame::white());
    }

    #[test]
    fn unknown_font_is_a_client_error() {
        let card = Card::Message {
            text: "hi".into(),
            size: MessageSize::Pt30,
            font: "Comic Sans".into(),
        };
        let err = render(&card, &Fonts::bundled()).unwrap_err();
        assert!(matches!(err, RenderError::UnknownFont(_)));
        assert!(err.is_client_error());
    }
}
