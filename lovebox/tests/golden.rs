//! Golden-frame tests: each card renders to exactly the committed PNG in `tests/golden/`.
//!
//! Regenerate after an intended rendering change with
//! `UPDATE_GOLDENS=1 cargo test --test golden`, then look at the PNGs before committing.
//! On a mismatch the actual frame is written next to the build's temp dir and its path is
//! printed.

use std::fs;
use std::path::PathBuf;

use lovebox::fonts::Fonts;
use lovebox::frame::{Frame, Threshold};
use lovebox::render::{render, Align, Card, MessageSize, QuoteSpec};

fn golden_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(format!("{name}.png"))
}

fn check_golden(name: &str, frame: &Frame) {
    let path = golden_path(name);
    if std::env::var_os("UPDATE_GOLDENS").is_some() {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, frame.to_png()).unwrap();
        eprintln!("updated {}", path.display());
        return;
    }
    let expected = image::open(&path)
        .unwrap_or_else(|e| {
            panic!(
                "{}: {e}; run UPDATE_GOLDENS=1 cargo test --test golden",
                path.display()
            )
        })
        .to_luma8();
    let expected = Frame::from_gray(&expected, Threshold::default());
    if expected != *frame {
        let diff = expected
            .as_packed_bytes()
            .iter()
            .zip(frame.as_packed_bytes())
            .map(|(a, b)| (a ^ b).count_ones())
            .sum::<u32>();
        let actual = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("{name}.actual.png"));
        fs::write(&actual, frame.to_png()).unwrap();
        panic!(
            "{name}: {diff} pixels differ from {}; actual frame written to {}",
            path.display(),
            actual.display()
        );
    }
}

const MESSAGE: &str = "Good morning, love. The coffee is on, the cat has opinions about \
    breakfast again, and I already miss you. See you tonight for dinner by the window, \
    with the good candles and the bad jokes. Every single day, you are my favourite \
    part of it.";

#[test]
fn golden_messages_one_per_size() {
    let fonts = Fonts::bundled();
    for size in MessageSize::ALL {
        let frame = render(&Card::message(MESSAGE, size), &fonts).unwrap();
        check_golden(&format!("message-{}", size.pt()), &frame);
    }
}

#[test]
fn golden_clear() {
    let frame = render(
        &Card::message("clear", MessageSize::Pt48),
        &Fonts::bundled(),
    )
    .unwrap();
    assert_eq!(frame, Frame::white());
    check_golden("clear", &frame);
}

fn lorem(n: usize) -> String {
    const LOREM: &str = "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do \
        eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, \
        quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat. \
        Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu \
        fugiat nulla pariatur. Excepteur sint occaecat cupidatat non proident, sunt in culpa \
        qui officia deserunt mollit anim id est laborum. ";
    let mut s = LOREM.repeat(n / LOREM.len() + 2);
    s.truncate(n);
    let end = s.rfind(' ').unwrap();
    s.truncate(end);
    s
}

#[test]
fn golden_quote_short_centred_with_attribution() {
    let mut spec = QuoteSpec::new("Whatever our souls are made of, his and mine are the same.");
    spec.attribution = Some("Emily Brontë, Wuthering Heights".into());
    spec.align = Align::Center;
    let frame = render(&Card::Quote(spec), &Fonts::bundled()).unwrap();
    check_golden("quote-short", &frame);
}

#[test]
fn golden_quote_long_left_with_paragraphs() {
    let body = format!("{}\n\n{}", lorem(1300), lorem(1300));
    let mut spec = QuoteSpec::new(body);
    spec.attribution = Some("Cicero, De finibus".into());
    let frame = render(&Card::Quote(spec), &Fonts::bundled()).unwrap();
    check_golden("quote-long", &frame);
}

#[test]
fn golden_quote_in_each_bundled_font() {
    let fonts = Fonts::bundled();
    for name in fonts.names() {
        let mut spec = QuoteSpec::new("Hope is the thing with feathers that perches in the soul.");
        spec.attribution = Some("Emily Dickinson".into());
        spec.font = name.to_owned();
        let frame = render(&Card::Quote(spec), &fonts).unwrap();
        check_golden(&format!("quote-font-{name}"), &frame);
    }
}
