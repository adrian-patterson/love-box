use std::sync::Arc;

use tokio::task;

use crate::api::error::ApiError;
use crate::api::types::{ApiAlign, ApiCard};
use crate::api::AppState;
use crate::fonts::Fonts;
use crate::frame::Frame;
use crate::library::Library;
use crate::render::{
    self, Align, Card, MessageSize, PhotoSpec, QuoteSpec, RenderError, QUOTE_MAX_CHARS,
};

#[derive(Clone, Copy, Debug, Default)]
pub struct RenderMetadata {
    pub fit_size_px: Option<u32>,
    pub truncated: Option<bool>,
}

pub struct RenderedCard {
    pub frame: Frame,
    pub card: ApiCard,
    pub metadata: RenderMetadata,
}

pub async fn render_card(app: Arc<AppState>, card: ApiCard) -> Result<RenderedCard, ApiError> {
    let settings = app.store.snapshot().settings;
    let fonts = app.fonts.clone();
    let library = app.library.clone();
    task::spawn_blocking(move || render_card_sync(&fonts, &library, &settings.default_font, card))
        .await
        .map_err(|e| ApiError::internal(format!("Render worker failed: {e}")))?
}

fn render_card_sync(
    fonts: &Fonts,
    library: &Library,
    default_font: &str,
    card: ApiCard,
) -> Result<RenderedCard, ApiError> {
    let (render_card, canonical, metadata) = match card {
        ApiCard::Message { text, size, font } => {
            if text.chars().count() > 3000 {
                return Err(ApiError::bad(format!(
                    "message is too long: {} characters (maximum 3000)",
                    text.chars().count()
                )));
            }
            if text == "clear" {
                (Card::Clear, ApiCard::Clear, RenderMetadata::default())
            } else {
                let size = MessageSize::from_pt(size)
                    .ok_or_else(|| ApiError::bad("size must be 30, 48, 64 or 96"))?;
                let font = font.unwrap_or_else(|| default_font.to_owned());
                if fonts.by_name(&font).is_none() {
                    return Err(RenderError::UnknownFont(font).into());
                }
                let lines = render::message_lines(&text, size);
                let truncated = lines.last().is_some_and(|line| line.ends_with("..."));
                (
                    Card::Message {
                        text: text.clone(),
                        size,
                        font: font.clone(),
                    },
                    ApiCard::Message {
                        text,
                        size: size.pt(),
                        font: Some(font),
                    },
                    RenderMetadata {
                        fit_size_px: None,
                        truncated: Some(truncated),
                    },
                )
            }
        }
        ApiCard::Quote {
            text,
            attribution,
            font,
            align,
        } => {
            let font = font.unwrap_or_else(|| default_font.to_owned());
            let align = align.unwrap_or(ApiAlign::Left);
            let mut spec = QuoteSpec::new(text.clone());
            spec.font = font.clone();
            spec.attribution = attribution.clone();
            spec.align = Align::from(align);
            spec.max_chars = QUOTE_MAX_CHARS;
            let face = fonts
                .by_name(&font)
                .ok_or_else(|| ApiError::bad(format!("unknown font: {font:?}")))?;
            let layout = render::fit_quote(&spec, face)?;
            (
                Card::Quote(spec),
                ApiCard::Quote {
                    text,
                    attribution,
                    font: Some(font),
                    align: Some(align),
                },
                RenderMetadata {
                    fit_size_px: Some(layout.size_px()),
                    truncated: None,
                },
            )
        }
        ApiCard::Photo { id } => {
            let (meta, image) = library.load_copy(&id)?;
            let spec = PhotoSpec {
                focal: meta.focal.as_ref().map(|p| (p.x, p.y)),
                dither: meta.dither.into(),
                enhance: meta.enhance,
            };
            (
                Card::Photo {
                    image: Arc::new(image),
                    spec,
                },
                ApiCard::Photo { id },
                RenderMetadata::default(),
            )
        }
        ApiCard::Clear => (Card::Clear, ApiCard::Clear, RenderMetadata::default()),
    };
    let frame = render::render(&render_card, fonts)?;
    Ok(RenderedCard {
        frame,
        card: canonical,
        metadata,
    })
}
