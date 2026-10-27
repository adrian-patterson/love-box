//! The display behind a trait, so everything above it runs without hardware.
//!
//! Implementations: [`PngPanel`] (this crate, writes PNG files) and `WavesharePanel`
//! behind the `hardware` feature. Every call is blocking; the server runs
//! the panel on one dedicated thread with its own timeout around each call.

mod png;
mod waveshare;

pub use self::png::PngPanel;
pub use self::waveshare::create_waveshare_panel;

use std::fmt;
use std::time::Duration;

use crate::frame::Frame;

/// Which refresh waveform a [`Panel::show`] uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refresh {
    /// The panel's standard full refresh: about 4 s and several black/white flashes.
    Full,
    /// A shorter full refresh with fewer flashes, at some risk of faint ghosting.
    Fast,
}

/// A display that can show a [`Frame`].
pub trait Panel {
    /// Shows `frame` with a full refresh.
    fn show(&mut self, frame: &Frame) -> Result<(), PanelError>;
    /// Blanks the panel to white.
    fn clear(&mut self) -> Result<(), PanelError>;
    /// Puts the panel into its lowest-power state until the next `show` or `clear`.
    fn sleep(&mut self) -> Result<(), PanelError>;
    /// Sets the refresh used by later `show` and `clear` calls. Panels with one refresh
    /// ignore it.
    fn set_refresh(&mut self, _refresh: Refresh) {}
    /// Short implementation name for logs and the status page, e.g. `"png"`.
    fn name(&self) -> &'static str;
}

/// Why a panel call failed.
#[derive(Debug)]
pub enum PanelError {
    /// Filesystem or device-file I/O failed.
    Io(std::io::Error),
    /// The panel did not become idle within the allowed time.
    Timeout(Duration),
    /// The device reported or caused another failure (SPI, GPIO, protocol).
    Device(String),
}

impl fmt::Display for PanelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(f, "panel I/O error: {err}"),
            Self::Timeout(after) => write!(f, "panel busy for more than {after:?}"),
            Self::Device(msg) => write!(f, "panel device error: {msg}"),
        }
    }
}

impl std::error::Error for PanelError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            _ => None,
        }
    }
}

impl From<std::io::Error> for PanelError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}
