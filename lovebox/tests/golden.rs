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
use lovebox::render::{render, Card, MessageSize};

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
