use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

use crate::render::{Align, Dither};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ApiCard {
    Message {
        text: String,
        size: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        font: Option<String>,
    },
    Quote {
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        attribution: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        font: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        align: Option<ApiAlign>,
    },
    Photo {
        id: String,
    },
    Clear,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ApiAlign {
    #[default]
    Left,
    Center,
}

impl From<ApiAlign> for Align {
    fn from(value: ApiAlign) -> Self {
        match value {
            ApiAlign::Left => Self::Left,
            ApiAlign::Center => Self::Center,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DitherChoice {
    #[default]
    FloydSteinberg,
    Atkinson,
}

impl From<DitherChoice> for Dither {
    fn from(value: DitherChoice) -> Self {
        match value {
            DitherChoice::FloydSteinberg => Self::FloydSteinberg,
            DitherChoice::Atkinson => Self::Atkinson,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct FocalPoint {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PhotoMeta {
    pub id: String,
    pub filename_original: String,
    pub caption: Option<String>,
    pub taken_at: Option<String>,
    pub added_at: String,
    pub width: u32,
    pub height: u32,
    pub source_id: Option<String>,
    pub focal: Option<FocalPoint>,
    pub dither: DitherChoice,
    pub enhance: bool,
    pub shown_count: u64,
    pub last_shown_at: Option<String>,
    pub tags: Vec<String>,
    pub in_rotation: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Quote {
    pub id: String,
    pub text: String,
    pub attribution: Option<String>,
    pub font: String,
    pub align: ApiAlign,
    pub added_at: String,
    pub shown_count: u64,
    pub last_shown_at: Option<String>,
    pub in_rotation: bool,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DisplayMode {
    #[default]
    Pinned,
    Slideshow,
    Daily,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SlideshowOrder {
    #[default]
    Shuffle,
    Upload,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct QuietHours {
    pub start: String,
    pub end: String,
    pub enabled: bool,
}

impl Default for QuietHours {
    fn default() -> Self {
        Self {
            start: "23:00".to_owned(),
            end: "07:00".to_owned(),
            enabled: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SlideshowSettings {
    pub interval_s: u64,
    pub order: SlideshowOrder,
    pub set: String,
}

impl Default for SlideshowSettings {
    fn default() -> Self {
        Self {
            interval_s: 900,
            order: SlideshowOrder::Shuffle,
            set: "all".to_owned(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Settings {
    pub default_font: String,
    pub default_dither: DitherChoice,
    pub enhance: bool,
    pub quiet_hours: QuietHours,
    pub slideshow: SlideshowSettings,
    pub mode: DisplayMode,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            default_font: "eb-garamond".to_owned(),
            default_dither: DitherChoice::FloydSteinberg,
            enhance: true,
            quiet_hours: QuietHours::default(),
            slideshow: SlideshowSettings::default(),
            mode: DisplayMode::Pinned,
        }
    }
}

impl Settings {
    pub fn validate(&self) -> Result<(), String> {
        if self.slideshow.interval_s < 15 {
            return Err("slideshow.interval_s must be at least 15".to_owned());
        }
        for (name, time) in [
            ("quiet_hours.start", &self.quiet_hours.start),
            ("quiet_hours.end", &self.quiet_hours.end),
        ] {
            let parsed = chrono::NaiveTime::parse_from_str(time, "%H:%M")
                .map_err(|_| format!("{name} must be a time in HH:MM format"))?;
            if parsed.format("%H:%M").to_string() != *time {
                return Err(format!("{name} must be a time in HH:MM format"));
            }
        }
        if let Some(tag) = self.slideshow.set.strip_prefix("tag:") {
            if tag.trim().is_empty() {
                return Err("slideshow.set tag name must not be empty".to_owned());
            }
        } else if self.slideshow.set != "all" {
            return Err("slideshow.set must be 'all' or start with 'tag:'".to_owned());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Shown {
    pub card: ApiCard,
    pub shown_at: String,
    pub duration_ms: u64,
    pub frame_revision: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct State {
    pub version: u32,
    pub settings: Settings,
    pub photos: Vec<PhotoMeta>,
    pub quotes: Vec<Quote>,
    pub now_showing: Option<ApiCard>,
    pub history: VecDeque<Shown>,
    pub frame_revision: u64,
    #[serde(default, with = "optional_base64")]
    pub current_frame_png: Option<Vec<u8>>,
    pub last_refresh_at: Option<String>,
    pub last_refresh_duration_ms: Option<u64>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            version: 1,
            settings: Settings::default(),
            photos: Vec::new(),
            quotes: Vec::new(),
            now_showing: None,
            history: VecDeque::new(),
            frame_revision: 0,
            current_frame_png: None,
            last_refresh_at: None,
            last_refresh_duration_ms: None,
        }
    }
}

impl State {
    pub const VERSION: u32 = 1;
    pub const HISTORY_CAP: usize = 500;

    pub fn record_shown(&mut self, shown: Shown, frame_png: Vec<u8>) {
        self.now_showing = Some(shown.card.clone());
        self.frame_revision = shown.frame_revision;
        self.current_frame_png = Some(frame_png);
        self.last_refresh_at = Some(shown.shown_at.clone());
        self.last_refresh_duration_ms = Some(shown.duration_ms);
        self.history.push_front(shown);
        self.history.truncate(Self::HISTORY_CAP);
    }
}

mod optional_base64 {
    use base64::Engine;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(value: &Option<Vec<u8>>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match value {
            Some(bytes) => {
                serializer.serialize_some(&base64::engine::general_purpose::STANDARD.encode(bytes))
            }
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Vec<u8>>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let encoded = Option::<String>::deserialize(deserializer)?;
        encoded
            .map(|value| {
                base64::engine::general_purpose::STANDARD
                    .decode(value)
                    .map_err(serde::de::Error::custom)
            })
            .transpose()
    }
}
