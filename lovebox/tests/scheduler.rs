//! Daily and slideshow modes end to end: the scheduler check against a real store and the PNG panel.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Local, NaiveDate, NaiveDateTime};
use lovebox::api::types::{ApiAlign, ApiCard, DisplayMode, Quote, SlideshowOrder};
use lovebox::api::AppState;
use lovebox::config::{Config, PanelKind};
use lovebox::display::DisplayHandle;
use lovebox::library::Library;
use lovebox::panel::{Panel, PngPanel};
use lovebox::scheduler::{tick, Tick};
use lovebox::store::Store;
use tempfile::TempDir;

fn app() -> (TempDir, Arc<AppState>) {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(dir.path()).unwrap());
    let library = Library::new(dir.path(), store.clone(), 160);
    let panel_path = dir.path().join("panel");
    let display = DisplayHandle::start(
        move || Ok(Box::new(PngPanel::new(panel_path)) as Box<dyn Panel>),
        Duration::from_secs(1),
        Duration::from_secs(2),
    )
    .unwrap();
    let config = Config {
        bind: "127.0.0.1:0".parse().unwrap(),
        data_dir: PathBuf::from(dir.path()),
        panel: PanelKind::Png,
        decode_limit_mb: 160,
        log: "info".into(),
    };
    (
        dir,
        Arc::new(AppState::new(config, store, library, display)),
    )
}

fn quote(text: &str, added_at: &str) -> Quote {
    Quote {
        id: text.into(),
        text: text.into(),
        attribution: None,
        font: "eb-garamond".into(),
        align: ApiAlign::Left,
        added_at: added_at.into(),
        shown_count: 0,
        last_shown_at: None,
    }
}

fn at(date: &str, time: &str) -> DateTime<Local> {
    NaiveDateTime::parse_from_str(&format!("{date} {time}"), "%Y-%m-%d %H:%M")
        .unwrap()
        .and_local_timezone(Local)
        .unwrap()
}

fn shown_text(card: &Tick) -> &str {
    match card {
        Tick::Shown(ApiCard::Quote { text, .. }) => text,
        other => panic!("expected a quote to be shown, got {other:?}"),
    }
}

#[tokio::test]
async fn daily_mode_shows_one_card_a_day_and_cycles_through_the_rotation() {
    let (_dir, app) = app();
    app.store
        .update(|state| {
            state.settings.mode = DisplayMode::Daily;
            state.quotes = vec![
                quote("first", "2026-01-01T00:00:00Z"),
                quote("second", "2026-01-02T00:00:00Z"),
            ];
            Ok(())
        })
        .unwrap();

    assert_eq!(tick(&app, at("2026-10-27", "06:59")).await, Tick::Idle);

    let day1 = tick(&app, at("2026-10-27", "07:00")).await;
    assert_eq!(shown_text(&day1), "first");
    let state = app.store.snapshot();
    assert_eq!(state.last_daily_on, NaiveDate::from_ymd_opt(2026, 10, 27));
    assert_eq!(state.frame_revision, 1);
    assert_eq!(state.quotes[0].shown_count, 1);

    // Later the same day: nothing more.
    assert_eq!(tick(&app, at("2026-10-27", "18:00")).await, Tick::Idle);

    // The Pi was off at 07:00 the next day: one catch-up card, the one not yet shown.
    let day2 = tick(&app, at("2026-10-28", "09:30")).await;
    assert_eq!(shown_text(&day2), "second");
    assert_eq!(tick(&app, at("2026-10-28", "09:31")).await, Tick::Idle);

    // Everything has been shown once, so the cycle starts again.
    let day3 = tick(&app, at("2026-10-29", "07:00")).await;
    assert_eq!(shown_text(&day3), "first");
    assert_eq!(app.store.snapshot().frame_revision, 3);
}

#[tokio::test]
async fn empty_rotation_marks_the_day_without_touching_the_panel() {
    let (_dir, app) = app();
    app.store
        .update(|state| {
            state.settings.mode = DisplayMode::Daily;
            Ok(())
        })
        .unwrap();
    assert_eq!(
        tick(&app, at("2026-10-27", "07:00")).await,
        Tick::NothingInRotation
    );
    let state = app.store.snapshot();
    assert_eq!(state.last_daily_on, NaiveDate::from_ymd_opt(2026, 10, 27));
    assert_eq!(state.frame_revision, 0);
    assert_eq!(tick(&app, at("2026-10-27", "08:00")).await, Tick::Idle);
}

#[tokio::test]
async fn pinned_mode_never_changes_the_frame() {
    let (_dir, app) = app();
    app.store
        .update(|state| {
            state.quotes = vec![quote("q", "2026-01-01T00:00:00Z")];
            Ok(())
        })
        .unwrap();
    assert_eq!(tick(&app, at("2026-10-27", "12:00")).await, Tick::Idle);
    assert_eq!(app.store.snapshot().frame_revision, 0);
}

fn slideshow(app: &AppState, interval_s: u64, quotes: &[&str]) {
    app.store
        .update(|state| {
            state.settings.mode = DisplayMode::Slideshow;
            state.settings.slideshow.interval_s = interval_s;
            state.settings.slideshow.order = SlideshowOrder::Upload;
            state.quotes = quotes
                .iter()
                .enumerate()
                .map(|(i, text)| quote(text, &format!("2026-01-0{}T00:00:00Z", i + 1)))
                .collect();
            Ok(())
        })
        .unwrap();
}

// The slideshow times itself from `last_refresh_at`, which the show path stamps with the
// real clock, so these checks run at offsets from the real now.
#[tokio::test]
async fn slideshow_changes_once_per_interval_in_order() {
    let (_dir, app) = app();
    slideshow(&app, 60, &["first", "second", "third"]);
    let start = Local::now();
    let seconds = |s| start + chrono::Duration::seconds(s);

    // Nothing shown yet: the first card goes up straight away.
    assert_eq!(shown_text(&tick(&app, start).await), "first");
    assert_eq!(tick(&app, seconds(30)).await, Tick::Idle);
    assert_eq!(shown_text(&tick(&app, seconds(61)).await), "second");
    // Each show restarts the interval from its (real) time.
    assert_eq!(tick(&app, seconds(59)).await, Tick::Idle);
    // Long after (the Pi was off): one card, then a full interval before the next.
    assert_eq!(shown_text(&tick(&app, seconds(3600)).await), "third");
    assert_eq!(tick(&app, seconds(59)).await, Tick::Idle);
    assert_eq!(app.store.snapshot().frame_revision, 3);
}

#[tokio::test]
async fn a_card_sent_by_hand_pins_it_but_refreshing_the_current_one_does_not() {
    let (_dir, app) = app();
    slideshow(&app, 60, &["first", "second"]);
    let start = Local::now();
    assert_eq!(shown_text(&tick(&app, start).await), "first");

    // Refresh now re-shows the card already up: the slideshow carries on.
    let current = app.store.snapshot().now_showing.unwrap();
    lovebox::api::show::show_by_hand(app.clone(), current)
        .await
        .unwrap();
    assert_eq!(app.store.snapshot().settings.mode, DisplayMode::Slideshow);

    lovebox::api::show::show_by_hand(
        app.clone(),
        ApiCard::Message {
            text: "hello".into(),
            size: 48,
            font: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(app.store.snapshot().settings.mode, DisplayMode::Pinned);
    assert_eq!(
        tick(&app, start + chrono::Duration::seconds(3600)).await,
        Tick::Idle
    );
}

#[tokio::test]
async fn one_card_slideshow_does_not_redraw_and_empty_one_reports_it() {
    let (_dir, app) = app();
    slideshow(&app, 60, &[]);
    let start = Local::now();
    assert_eq!(tick(&app, start).await, Tick::NothingInRotation);

    slideshow(&app, 60, &["only"]);
    assert_eq!(shown_text(&tick(&app, start).await), "only");
    assert_eq!(
        tick(&app, start + chrono::Duration::seconds(120)).await,
        Tick::Idle
    );
    assert_eq!(app.store.snapshot().frame_revision, 1);
}

#[tokio::test]
async fn next_shows_the_following_card_only_in_slideshow_mode() {
    use axum::extract::State;
    let (_dir, app) = app();
    let error = lovebox::api::show::next(State(app.clone()))
        .await
        .err()
        .unwrap();
    assert_eq!(error.code, "not_slideshow");

    slideshow(&app, 900, &["first", "second"]);
    let _ = lovebox::api::show::next(State(app.clone())).await.unwrap();
    let _ = lovebox::api::show::next(State(app.clone())).await.unwrap();
    let state = app.store.snapshot();
    assert!(matches!(
        state.now_showing,
        Some(ApiCard::Quote { ref text, .. }) if text == "second"
    ));
}
