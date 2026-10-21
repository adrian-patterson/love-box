//! `lovebox` library: everything the binary does that can be tested without hardware.
//!
//! - [`frame`]: the 800x480 1-bit [`Frame`](frame::Frame) and its panel byte layout.
//! - [`panel`]: the [`Panel`](panel::Panel) trait and [`PngPanel`](panel::PngPanel).
//!
//! The binary (`src/main.rs`) holds the CLI and the `serve` loop.

pub mod frame;
pub mod panel;
