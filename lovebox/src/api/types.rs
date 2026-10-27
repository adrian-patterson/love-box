use std::collections::VecDeque;

use serde::{Deserialize, Deserializer, Serialize};

use crate::render::{Align, Dither};

#[derive(Clone, Debug, Default, PartialEq)]
pub enum PatchField<T> {
    #[default]
    Missing,
    Null,
    Value(T),
}

pub fn deserialize_patch_field<'de, D, T>(deserializer: D) -> Result<PatchField<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(match Option::<T>::deserialize(deserializer)? {
        Some(value) => PatchField::Value(value),
        None => PatchField::Null,
    })
}

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

/// Serialized with a computed `crop` field (see the `Serialize` impl); deserializing
/// ignores it, so it always reflects the current focal point.
#[derive(Clone, Debug, Deserialize, PartialEq)]
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
    /// Crop zoom, `1.0..=MAX_ZOOM` (see `PhotoSpec::zoom`); absent in state written
    /// before 2026-10-26, which means 1.0.
    #[serde(default = "full_crop")]
    pub zoom: f32,
    pub dither: DitherChoice,
    pub enhance: bool,
    pub shown_count: u64,
    pub last_shown_at: Option<String>,
    pub tags: Vec<String>,
}

/// The part of the stored copy (`width`x`height` pixels) that the panel shows: the
/// renderer's cover crop for the photo's focal point.
#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
pub struct CropRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

fn full_crop() -> f32 {
    1.0
}

impl PhotoMeta {
    pub fn crop(&self) -> CropRect {
        let focal = self.focal.as_ref().map(|f| (f.x, f.y));
        let (x, y, width, height) =
            crate::render::photo::cover_crop_rect(self.width, self.height, focal, self.zoom);
        CropRect {
            x,
            y,
            width,
            height,
        }
    }
}

impl Serialize for PhotoMeta {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("PhotoMeta", 16)?;
        s.serialize_field("id", &self.id)?;
        s.serialize_field("filename_original", &self.filename_original)?;
        s.serialize_field("caption", &self.caption)?;
        s.serialize_field("taken_at", &self.taken_at)?;
        s.serialize_field("added_at", &self.added_at)?;
        s.serialize_field("width", &self.width)?;
        s.serialize_field("height", &self.height)?;
        s.serialize_field("source_id", &self.source_id)?;
        s.serialize_field("focal", &self.focal)?;
        s.serialize_field("zoom", &self.zoom)?;
        s.serialize_field("dither", &self.dither)?;
        s.serialize_field("enhance", &self.enhance)?;
        s.serialize_field("shown_count", &self.shown_count)?;
        s.serialize_field("last_shown_at", &self.last_shown_at)?;
        s.serialize_field("tags", &self.tags)?;
        s.serialize_field("crop", &self.crop())?;
        s.end()
    }
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

/// Daily mode: one new card from the rotation each day at `time` (local, `HH:MM`).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DailySettings {
    pub time: String,
}

impl Default for DailySettings {
    fn default() -> Self {
        Self {
            time: "07:00".to_owned(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Settings {
    pub default_font: String,
    pub default_dither: DitherChoice,
    pub enhance: bool,
    /// Waveshare's shorter refresh: fewer flashes, maybe faint ghosting. Absent in state
    /// written before it existed (2026-10-27), which turns it on.
    #[serde(default = "fast_refresh_default")]
    pub fast_refresh: bool,
    pub quiet_hours: QuietHours,
    pub slideshow: SlideshowSettings,
    /// Absent in state written before daily mode existed (2026-10-26).
    #[serde(default)]
    pub daily: DailySettings,
    pub mode: DisplayMode,
}

fn fast_refresh_default() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            default_font: "eb-garamond".to_owned(),
            default_dither: DitherChoice::FloydSteinberg,
            enhance: true,
            fast_refresh: true,
            quiet_hours: QuietHours::default(),
            slideshow: SlideshowSettings::default(),
            daily: DailySettings::default(),
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
            ("daily.time", &self.daily.time),
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
    /// Local date daily mode last showed (or skipped) its card; it shows at most one a day.
    #[serde(default)]
    pub last_daily_on: Option<chrono::NaiveDate>,
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
            last_daily_on: None,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_saved_before_fast_refresh_existed_load_with_it_on() {
        let mut json = serde_json::to_value(Settings::default()).unwrap();
        json.as_object_mut().unwrap().remove("fast_refresh");
        let settings: Settings = serde_json::from_value(json).unwrap();
        assert!(settings.fast_refresh);
        let off = Settings {
            fast_refresh: false,
            ..Settings::default()
        };
        let back: Settings = serde_json::from_value(serde_json::to_value(&off).unwrap()).unwrap();
        assert!(!back.fast_refresh);
    }

    fn meta(width: u32, height: u32, focal: Option<FocalPoint>) -> PhotoMeta {
        PhotoMeta {
            id: "01J00000000000000000000000".into(),
            filename_original: "a.jpg".into(),
            caption: None,
            taken_at: None,
            added_at: "2026-10-26T00:00:00Z".into(),
            width,
            height,
            source_id: None,
            focal,
            zoom: 1.0,
            dither: DitherChoice::FloydSteinberg,
            enhance: true,
            shown_count: 0,
            last_shown_at: None,
            tags: Vec::new(),
        }
    }

    #[test]
    fn photo_json_carries_the_renderer_crop_and_round_trips() {
        // Portrait 720x960 keeps full width and a 432 px tall band; focal y=1 pins it low.
        let photo = meta(720, 960, Some(FocalPoint { x: 0.5, y: 1.0 }));
        let json = serde_json::to_value(&photo).unwrap();
        assert_eq!(
            json["crop"],
            serde_json::json!({"x": 0, "y": 528, "width": 720, "height": 432})
        );
        let back: PhotoMeta = serde_json::from_value(json).unwrap();
        assert_eq!(back, photo);
    }

    #[test]
    fn zoom_shrinks_the_reported_crop_and_defaults_when_absent() {
        let mut photo = meta(1600, 960, None);
        photo.zoom = 2.0;
        assert_eq!(
            photo.crop(),
            CropRect {
                x: 400,
                y: 240,
                width: 800,
                height: 480
            }
        );
        let mut json = serde_json::to_value(&photo).unwrap();
        json.as_object_mut().unwrap().remove("zoom");
        let old: PhotoMeta = serde_json::from_value(json).unwrap();
        assert_eq!(old.zoom, 1.0);
    }

    #[test]
    fn crop_defaults_to_centre_without_a_focal_point() {
        let crop = meta(2000, 960, None).crop();
        assert_eq!(
            crop,
            CropRect {
                x: 200,
                y: 0,
                width: 1600,
                height: 960
            }
        );
    }
}
