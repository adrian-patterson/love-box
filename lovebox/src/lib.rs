//! `lovebox` library: everything the binary does that can be tested without hardware.
//!
//! - [`frame`]: the 800x480 1-bit [`Frame`](frame::Frame) and its panel byte layout.
//! - [`panel`]: the [`Panel`](panel::Panel) trait and [`PngPanel`](panel::PngPanel).
//! - [`render`]: [`Card`](render::Card) -> [`Frame`](frame::Frame) for messages, quotes,
//!   photos and clear.
//! - [`fonts`]: the bundled SIL OFL faces.
//!
//! The binary (`src/main.rs`) holds the CLI and the `serve` loop.

pub mod api;
pub mod config;
pub mod display;
pub mod fonts;
pub mod frame;
pub mod import;
pub mod library;
pub mod panel;
pub mod render;
pub mod scheduler;
pub mod store;
pub mod web;
