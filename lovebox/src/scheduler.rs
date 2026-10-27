//! Scheduled modes: daily (one new card each day at a set local time) and slideshow (a new
//! card every interval).
//!
//! A background task checks at least every [`TICK`], sooner when the next slide is due. A
//! due card is shown through the same path as `POST /api/show`.
//!
//! - Daily: once the time has passed and today's card has not been shown, it shows one.
//!   The day is recorded in `State::last_daily_on`, so a reboot after the time catches up
//!   with exactly one card and never bursts through missed days.
//! - Slideshow: the next card is due one interval after the frame last changed, whether
//!   the slideshow or a person changed it, so a card sent by hand stays up for a full
//!   interval and a reboot shows one card, then carries on.
//!
//! Quiet hours are not implemented yet (M5).

use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Local, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use tokio::task::JoinHandle;
use tokio::time::Instant;
use tracing::{info, warn};

use crate::api::show::show_card;
use crate::api::types::{ApiCard, DisplayMode, Settings, SlideshowOrder, State};
use crate::api::AppState;

/// The longest the task sleeps between checks; settings changes take effect within this.
pub const TICK: Duration = Duration::from_secs(30);
/// The shortest sleep, so a due card that cannot be shown yet is not retried in a hot loop.
const MIN_SLEEP: Duration = Duration::from_secs(5);
/// How long to wait after a failed scheduled update before trying again.
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

/// When slideshow mode next changes the card: one interval after the frame last changed,
/// or now if that has passed. `None` outside slideshow mode. A last change in the future
/// (the clock stepped back, as a Pi without a real-time clock can at boot) counts as now.
pub fn next_slide_at(
    settings: &Settings,
    last_refresh_at: Option<&str>,
    now: DateTime<Utc>,
) -> Option<DateTime<Utc>> {
    if settings.mode != DisplayMode::Slideshow {
        return None;
    }
    let interval = chrono::Duration::seconds(
        i64::try_from(settings.slideshow.interval_s).unwrap_or(i64::MAX / 1000),
    );
    let last = last_refresh_at
        .and_then(|text| DateTime::parse_from_rfc3339(text).ok())
        .map(|at| at.with_timezone(&Utc).min(now));
    Some(last.map_or(now, |last| (last + interval).max(now)))
}

/// When the scheduler next changes the card, in local time; `None` when it will not
/// (pinned mode, or a slideshow with nothing in it).
pub fn next_change_at(state: &State, now: DateTime<Local>) -> Option<DateTime<Local>> {
    match state.settings.mode {
        DisplayMode::Pinned => None,
        DisplayMode::Daily => {
            next_daily_at(&state.settings, state.last_daily_on, now.naive_local())
                .and_then(|at| at.and_local_timezone(Local).earliest())
        }
        DisplayMode::Slideshow => {
            if rotation(state, slideshow_tag(&state.settings)).is_empty() {
                return None;
            }
            next_slide_at(
                &state.settings,
                state.last_refresh_at.as_deref(),
                now.with_timezone(&Utc),
            )
            .map(|at| at.with_timezone(&Local))
        }
    }
}

fn slideshow_tag(settings: &Settings) -> Option<&str> {
    settings.slideshow.set.strip_prefix("tag:")
}

/// Every photo and saved quote in the library, least recently shown first
/// (never-shown first, then oldest added). With `tag`, only photos carrying that tag.
fn rotation(state: &State, tag: Option<&str>) -> Vec<ApiCard> {
    let photos = state
        .photos
        .iter()
        .filter(|photo| tag.is_none_or(|tag| photo.tags.iter().any(|t| t == tag)))
        .map(|photo| {
            (
                photo.last_shown_at.clone(),
                photo.added_at.clone(),
                ApiCard::Photo {
                    id: photo.id.clone(),
                },
            )
        });
    let quotes = state.quotes.iter().filter(|_| tag.is_none()).map(|quote| {
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
    let mut items: Vec<_> = photos.chain(quotes).collect();
    // `None < Some(_)`, and RFC 3339 UTC timestamps sort chronologically as text.
    items.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
    items.into_iter().map(|(_, _, card)| card).collect()
}

/// The next card for daily mode: the photo or saved quote in the library that was shown
/// least recently, so everything comes round once before anything repeats.
pub fn next_daily_card(state: &State) -> Option<ApiCard> {
    rotation(state, None).into_iter().next()
}

/// The next card for slideshow mode, from `settings.slideshow.set`. In order: the least
/// recently shown, as in daily mode. Shuffle: pick `roll` (any number) among the less
/// recently shown half, so nothing repeats within half a cycle and nothing waits long.
pub fn next_slideshow_card(state: &State, roll: usize) -> Option<ApiCard> {
    let cards = rotation(state, slideshow_tag(&state.settings));
    if cards.is_empty() {
        return None;
    }
    let pick = match state.settings.slideshow.order {
        SlideshowOrder::Upload => 0,
        SlideshowOrder::Shuffle => roll % cards.len().div_ceil(2),
    };
    cards.into_iter().nth(pick)
}

/// What one scheduler check did.
#[derive(Debug, PartialEq, Eq)]
pub enum Tick {
    /// Pinned mode, nothing due, or the only card in the slideshow is already showing.
    Idle,
    /// Showed a card.
    Shown(ApiCard),
    /// The library is empty. Daily mode marks today done so the check does not repeat.
    NothingInRotation,
    /// Another update is in progress; the next check tries again.
    Busy,
    /// Rendering or the panel failed; retried after [`RETRY_AFTER_FAILURE`].
    Failed(String),
}

/// One scheduler check at `now`.
pub async fn tick(app: &Arc<AppState>, now: DateTime<Local>) -> Tick {
    let state = app.store.snapshot();
    match state.settings.mode {
        DisplayMode::Pinned => Tick::Idle,
        DisplayMode::Daily => daily_tick(app, &state, now.naive_local()).await,
        DisplayMode::Slideshow => slideshow_tick(app, &state, now).await,
    }
}

async fn daily_tick(app: &Arc<AppState>, state: &State, now: NaiveDateTime) -> Tick {
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
    let Some(card) = next_daily_card(state) else {
        if let Err(error) = mark_done() {
            return Tick::Failed(format!("could not record the daily check: {error}"));
        }
        return Tick::NothingInRotation;
    };
    match show(app, card).await {
        Ok(card) => match mark_done() {
            Ok(()) => Tick::Shown(card),
            Err(error) => Tick::Failed(format!("shown, but recording the day failed: {error}")),
        },
        Err(tick) => tick,
    }
}

async fn slideshow_tick(app: &Arc<AppState>, state: &State, now: DateTime<Local>) -> Tick {
    let now = now.with_timezone(&Utc);
    let due = next_slide_at(&state.settings, state.last_refresh_at.as_deref(), now)
        .is_some_and(|at| at <= now);
    if !due {
        return Tick::Idle;
    }
    let roll = now.timestamp_subsec_nanos() as usize;
    let Some(card) = next_slideshow_card(state, roll) else {
        return Tick::NothingInRotation;
    };
    // A one-item slideshow would redraw the same card every interval; skip the flash.
    if state.now_showing.as_ref() == Some(&card) {
        return Tick::Idle;
    }
    show(app, card).await.map_or_else(|tick| tick, Tick::Shown)
}

async fn show(app: &Arc<AppState>, card: ApiCard) -> Result<ApiCard, Tick> {
    match show_card(app.clone(), card.clone()).await {
        Ok(_) => Ok(card),
        Err(error) if error.code == "panel_unavailable" => Err(Tick::Busy),
        Err(error) => Err(Tick::Failed(error.message)),
    }
}

/// How long to sleep before the next check: until the next slide if that comes before
/// [`TICK`], so short slideshow intervals keep time.
fn sleep_before_next_check(app: &AppState) -> Duration {
    let state = app.store.snapshot();
    let now = Utc::now();
    next_slide_at(&state.settings, state.last_refresh_at.as_deref(), now)
        .and_then(|at| (at - now).to_std().ok())
        .map_or(TICK, |wait| wait.clamp(MIN_SLEEP, TICK))
}

/// Starts the background task. It runs until the returned handle is aborted.
pub fn spawn(app: Arc<AppState>) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut retry_at: Option<Instant> = None;
        let mut warned_empty = false;
        loop {
            tokio::time::sleep(sleep_before_next_check(&app)).await;
            if retry_at.is_some_and(|at| Instant::now() < at) {
                continue;
            }
            retry_at = None;
            let result = tick(&app, Local::now()).await;
            if result != Tick::NothingInRotation {
                warned_empty = false;
            }
            match result {
                Tick::Idle | Tick::Busy => {}
                Tick::Shown(card) => info!(?card, "scheduled card shown"),
                Tick::NothingInRotation => {
                    if !warned_empty {
                        warn!("nothing to show; the library has no photos or saved quotes");
                        warned_empty = true;
                    }
                }
                Tick::Failed(message) => {
                    warn!(%message, "scheduled card failed; retrying in 10 minutes");
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

    fn photo(id: &str, added: &str, shown: Option<&str>) -> PhotoMeta {
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
        }
    }

    #[test]
    fn next_card_is_least_recently_shown() {
        let mut state = State {
            photos: vec![
                photo("a", "2026-01-01T00:00:00Z", Some("2026-10-20T07:00:00Z")),
                photo("b", "2026-01-02T00:00:00Z", Some("2026-10-25T07:00:00Z")),
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
        state.photos.push(photo("c", "2026-01-06T00:00:00Z", None));
        assert!(matches!(
            next_daily_card(&state),
            Some(ApiCard::Quote { text, .. }) if text == "new"
        ));
        assert_eq!(next_daily_card(&State::default()), None);
    }

    fn utc(text: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(text)
            .unwrap()
            .with_timezone(&Utc)
    }

    fn slideshow(interval_s: u64, order: SlideshowOrder, set: &str) -> Settings {
        Settings {
            mode: DisplayMode::Slideshow,
            slideshow: crate::api::types::SlideshowSettings {
                interval_s,
                order,
                set: set.into(),
            },
            ..Settings::default()
        }
    }

    #[test]
    fn next_slide_is_one_interval_after_the_last_change_or_now() {
        let s = slideshow(180, SlideshowOrder::Upload, "all");
        let now = utc("2026-10-27T12:00:00Z");
        assert_eq!(
            next_slide_at(&s, Some("2026-10-27T11:59:00Z"), now),
            Some(utc("2026-10-27T12:02:00Z"))
        );
        // Long overdue (the Pi was off): one card now, not one per missed interval.
        assert_eq!(
            next_slide_at(&s, Some("2026-10-20T08:00:00Z"), now),
            Some(now)
        );
        assert_eq!(next_slide_at(&s, None, now), Some(now));
        // A last change in the future (clock stepped back) waits one interval from now.
        assert_eq!(
            next_slide_at(&s, Some("2026-10-27T13:00:00Z"), now),
            Some(utc("2026-10-27T12:03:00Z"))
        );
        assert_eq!(next_slide_at(&Settings::default(), None, now), None);
    }

    fn slideshow_state(order: SlideshowOrder, set: &str) -> State {
        let mut tagged = photo("tagged", "2026-01-03T00:00:00Z", None);
        tagged.tags = vec!["beach".into()];
        State {
            settings: slideshow(900, order, set),
            photos: vec![
                photo("a", "2026-01-01T00:00:00Z", Some("2026-10-25T07:00:00Z")),
                photo("b", "2026-01-02T00:00:00Z", Some("2026-10-20T07:00:00Z")),
                tagged,
            ],
            quotes: vec![quote(
                "q",
                "2026-01-05T00:00:00Z",
                Some("2026-10-26T07:00:00Z"),
            )],
            ..State::default()
        }
    }

    #[test]
    fn slideshow_in_order_takes_the_least_recently_shown() {
        let state = slideshow_state(SlideshowOrder::Upload, "all");
        for roll in [0, 1, 7] {
            assert_eq!(
                next_slideshow_card(&state, roll),
                Some(ApiCard::Photo {
                    id: "tagged".into()
                })
            );
        }
    }

    #[test]
    fn slideshow_shuffle_picks_from_the_older_half() {
        // Order by last shown: tagged (never), b, a, q. The older half is tagged and b.
        let state = slideshow_state(SlideshowOrder::Shuffle, "all");
        let picks: Vec<_> = (0..4)
            .map(|roll| next_slideshow_card(&state, roll).unwrap())
            .collect();
        assert_eq!(
            picks,
            vec![
                ApiCard::Photo {
                    id: "tagged".into()
                },
                ApiCard::Photo { id: "b".into() },
                ApiCard::Photo {
                    id: "tagged".into()
                },
                ApiCard::Photo { id: "b".into() },
            ]
        );
    }

    #[test]
    fn slideshow_tag_set_includes_only_tagged_photos() {
        let state = slideshow_state(SlideshowOrder::Shuffle, "tag:beach");
        assert_eq!(
            next_slideshow_card(&state, 5),
            Some(ApiCard::Photo {
                id: "tagged".into()
            })
        );
        let state = slideshow_state(SlideshowOrder::Upload, "tag:none");
        assert_eq!(next_slideshow_card(&state, 0), None);
        assert_eq!(next_change_at(&state, Local::now()), None);
    }
}
