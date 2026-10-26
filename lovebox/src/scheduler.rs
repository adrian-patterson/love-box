//! Daily mode: one new card from the rotation each day at the configured local time.
//!
//! A background task checks every [`TICK`]. Once the time has passed and today's card has
//! not been shown, it shows the next card through the same path as `POST /api/show`. The
//! day is recorded in `State::last_daily_on`, so a reboot after the time catches up with
//! exactly one card and never bursts through missed days. Slideshow mode is not
//! implemented yet (M5).

use std::sync::Arc;
use std::time::Duration;

use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
use tokio::task::JoinHandle;
use tokio::time::Instant;
use tracing::{info, warn};

use crate::api::show::show_card;
use crate::api::types::{ApiCard, DisplayMode, Settings, State};
use crate::api::AppState;

/// How often the task checks whether the daily card is due.
pub const TICK: Duration = Duration::from_secs(30);
/// How long to wait after a failed daily update before trying again.
pub const RETRY_AFTER_FAILURE: Duration = Duration::from_secs(600);

fn daily_time(settings: &Settings) -> NaiveTime {
    // Settings::validate guarantees HH:MM; fall back to the default rather than panic.
    NaiveTime::parse_from_str(&settings.daily.time, "%H:%M")
        .unwrap_or_else(|_| NaiveTime::from_hms_opt(7, 0, 0).expect("valid time"))
}

/// True when daily mode should show a card at local time `now`.
pub fn daily_due(
    settings: &Settings,
    last_daily_on: Option<NaiveDate>,
    now: NaiveDateTime,
) -> bool {
    settings.mode == DisplayMode::Daily
        && now.time() >= daily_time(settings)
        && last_daily_on != Some(now.date())
}

/// When the next daily card will be shown, in local time; `None` outside daily mode.
pub fn next_daily_at(
    settings: &Settings,
    last_daily_on: Option<NaiveDate>,
    now: NaiveDateTime,
) -> Option<NaiveDateTime> {
    if settings.mode != DisplayMode::Daily {
        return None;
    }
    let today = now.date().and_time(daily_time(settings));
    if last_daily_on == Some(now.date()) {
        Some(today + chrono::Duration::days(1))
    } else {
        Some(today.max(now))
    }
}

/// The `last_daily_on` to store when settings change from `old` to `new` at `now`: turning
/// daily mode on (or moving its time) after today's time has passed waits until tomorrow
/// instead of replacing what is on the frame straight away. `None` leaves it unchanged.
pub fn last_daily_after_settings_change(
    old: &Settings,
    new: &Settings,
    now: NaiveDateTime,
) -> Option<NaiveDate> {
    let entering = new.mode == DisplayMode::Daily
        && (old.mode != DisplayMode::Daily || old.daily.time != new.daily.time);
    (entering && now.time() >= daily_time(new)).then(|| now.date())
}

/// The next card for daily mode: the photo or saved quote marked "in rotation" that was
/// shown least recently (never-shown first, then oldest added), so everything comes round
/// once before anything repeats.
pub fn next_daily_card(state: &State) -> Option<ApiCard> {
    let photos = state
        .photos
        .iter()
        .filter(|photo| photo.in_rotation)
        .map(|photo| {
            (
                photo.last_shown_at.clone(),
                photo.added_at.clone(),
                ApiCard::Photo {
                    id: photo.id.clone(),
                },
            )
        });
    let quotes = state
        .quotes
        .iter()
        .filter(|quote| quote.in_rotation)
        .map(|quote| {
            (
                quote.last_shown_at.clone(),
                quote.added_at.clone(),
                ApiCard::Quote {
                    text: quote.text.clone(),
                    attribution: quote.attribution.clone(),
                    font: Some(quote.font.clone()),
                    align: Some(quote.align),
                },
            )
        });
    photos
        .chain(quotes)
        // `None < Some(_)`, and RFC 3339 UTC timestamps sort chronologically as text.
        .min_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)))
        .map(|(_, _, card)| card)
}

/// What one scheduler check did.
#[derive(Debug, PartialEq, Eq)]
pub enum Tick {
    /// Not daily mode, before the time, or today's card is already shown.
    Idle,
    /// Showed a card.
    Shown(ApiCard),
    /// Nothing is in rotation; today is marked done so the check does not repeat.
    NothingInRotation,
    /// Another update is in progress; the next check tries again.
    Busy,
    /// Rendering or the panel failed; retried after [`RETRY_AFTER_FAILURE`].
    Failed(String),
}

/// One scheduler check at local time `now`.
pub async fn tick(app: &Arc<AppState>, now: NaiveDateTime) -> Tick {
    let state = app.store.snapshot();
    if !daily_due(&state.settings, state.last_daily_on, now) {
        return Tick::Idle;
    }
    let today = now.date();
    let mark_done = || {
        app.store.update(|state| {
            state.last_daily_on = Some(today);
            Ok(())
        })
    };
    let Some(card) = next_daily_card(&state) else {
        if let Err(error) = mark_done() {
            return Tick::Failed(format!("could not record the daily check: {error}"));
        }
        return Tick::NothingInRotation;
    };
    match show_card(app.clone(), card.clone()).await {
        Ok(_) => match mark_done() {
            Ok(()) => Tick::Shown(card),
            Err(error) => Tick::Failed(format!("shown, but recording the day failed: {error}")),
        },
        Err(error) if error.code == "panel_unavailable" => Tick::Busy,
        Err(error) => Tick::Failed(error.message),
    }
}

/// Starts the background task. It runs until the returned handle is aborted.
pub fn spawn(app: Arc<AppState>) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut retry_at: Option<Instant> = None;
        let mut interval = tokio::time::interval(TICK);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            interval.tick().await;
            if retry_at.is_some_and(|at| Instant::now() < at) {
                continue;
            }
            retry_at = None;
            match tick(&app, chrono::Local::now().naive_local()).await {
                Tick::Idle | Tick::Busy => {}
                Tick::Shown(card) => info!(?card, "daily card shown"),
                Tick::NothingInRotation => {
                    warn!("daily mode: nothing is in rotation; add photos or saved quotes")
                }
                Tick::Failed(message) => {
                    warn!(%message, "daily card failed; retrying in 10 minutes");
                    retry_at = Some(Instant::now() + RETRY_AFTER_FAILURE);
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::types::{ApiAlign, DitherChoice, PhotoMeta, Quote};

    fn at(date: &str, time: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(&format!("{date} {time}"), "%Y-%m-%d %H:%M").unwrap()
    }

    fn daily(time: &str) -> Settings {
        Settings {
            mode: DisplayMode::Daily,
            daily: crate::api::types::DailySettings {
                time: time.to_owned(),
            },
            ..Settings::default()
        }
    }

    fn day(date: &str) -> Option<NaiveDate> {
        Some(NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap())
    }

    #[test]
    fn due_once_per_day_after_the_time_and_only_in_daily_mode() {
        let s = daily("07:00");
        assert!(!daily_due(&s, None, at("2026-10-27", "06:59")));
        assert!(daily_due(&s, None, at("2026-10-27", "07:00")));
        assert!(daily_due(&s, day("2026-10-26"), at("2026-10-27", "13:00")));
        assert!(!daily_due(&s, day("2026-10-27"), at("2026-10-27", "13:00")));
        assert!(!daily_due(
            &Settings::default(),
            None,
            at("2026-10-27", "13:00")
        ));
    }

    #[test]
    fn next_change_is_today_tomorrow_or_now() {
        let s = daily("07:00");
        assert_eq!(
            next_daily_at(&s, day("2026-10-26"), at("2026-10-27", "06:00")),
            Some(at("2026-10-27", "07:00"))
        );
        assert_eq!(
            next_daily_at(&s, day("2026-10-27"), at("2026-10-27", "09:00")),
            Some(at("2026-10-28", "07:00"))
        );
        // Overdue (the Pi was off at 07:00): due now.
        assert_eq!(
            next_daily_at(&s, day("2026-10-26"), at("2026-10-27", "09:00")),
            Some(at("2026-10-27", "09:00"))
        );
        assert_eq!(
            next_daily_at(&Settings::default(), None, at("2026-10-27", "09:00")),
            None
        );
    }

    #[test]
    fn switching_to_daily_after_the_time_waits_until_tomorrow() {
        let pinned = Settings::default();
        let now = at("2026-10-27", "15:00");
        assert_eq!(
            last_daily_after_settings_change(&pinned, &daily("07:00"), now),
            day("2026-10-27")
        );
        // Before the time: today's card still comes at the set time.
        assert_eq!(
            last_daily_after_settings_change(&pinned, &daily("18:00"), now),
            None
        );
        // Unrelated settings edits while already in daily mode change nothing.
        let mut edited = daily("07:00");
        edited.enhance = false;
        assert_eq!(
            last_daily_after_settings_change(&daily("07:00"), &edited, now),
            None
        );
    }

    fn photo(id: &str, added: &str, shown: Option<&str>, in_rotation: bool) -> PhotoMeta {
        PhotoMeta {
            id: id.into(),
            filename_original: format!("{id}.jpg"),
            caption: None,
            taken_at: None,
            added_at: added.into(),
            width: 1600,
            height: 960,
            source_id: None,
            focal: None,
            zoom: 1.0,
            dither: DitherChoice::FloydSteinberg,
            enhance: true,
            shown_count: 0,
            last_shown_at: shown.map(Into::into),
            tags: Vec::new(),
            in_rotation,
        }
    }

    fn quote(text: &str, added: &str, shown: Option<&str>) -> Quote {
        Quote {
            id: text.into(),
            text: text.into(),
            attribution: None,
            font: "eb-garamond".into(),
            align: ApiAlign::Left,
            added_at: added.into(),
            shown_count: 0,
            last_shown_at: shown.map(Into::into),
            in_rotation: true,
        }
    }

    #[test]
    fn next_card_is_least_recently_shown_in_rotation() {
        let mut state = State {
            photos: vec![
                photo(
                    "a",
                    "2026-01-01T00:00:00Z",
                    Some("2026-10-20T07:00:00Z"),
                    true,
                ),
                photo(
                    "b",
                    "2026-01-02T00:00:00Z",
                    Some("2026-10-25T07:00:00Z"),
                    true,
                ),
                photo("off", "2026-01-03T00:00:00Z", None, false),
            ],
            quotes: vec![quote(
                "q",
                "2026-01-04T00:00:00Z",
                Some("2026-10-22T07:00:00Z"),
            )],
            ..State::default()
        };
        assert_eq!(
            next_daily_card(&state),
            Some(ApiCard::Photo { id: "a".into() })
        );
        // Never shown beats shown; ties break by oldest added.
        state
            .quotes
            .push(quote("new", "2026-01-05T00:00:00Z", None));
        state
            .photos
            .push(photo("c", "2026-01-06T00:00:00Z", None, true));
        assert!(matches!(
            next_daily_card(&state),
            Some(ApiCard::Quote { text, .. }) if text == "new"
        ));
        assert_eq!(next_daily_card(&State::default()), None);
    }
}
