//! Daily mode end to end: the scheduler check against a real store and the PNG panel.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use chrono::{NaiveDate, NaiveDateTime};
use lovebox::api::types::{ApiAlign, ApiCard, DisplayMode, Quote};
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
        in_rotation: true,
    }
}

fn at(date: &str, time: &str) -> NaiveDateTime {
    NaiveDateTime::parse_from_str(&format!("{date} {time}"), "%Y-%m-%d %H:%M").unwrap()
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
