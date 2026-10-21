//! [`PngPanel`]: a panel that writes each shown frame to PNG files.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::{Panel, PanelError};
use crate::frame::Frame;

/// A [`Panel`] for laptops and tests. Each `show` writes `<dir>/frame.png` (replaced
/// atomically) and a numbered copy `<dir>/frame-<n>.png`, keeping the newest
/// [`PngPanel::HISTORY`] numbered copies. `clear` shows a white frame; `sleep` does nothing.
/// The directory is created on first use. Numbering continues from the highest `n`
/// already in the directory.
#[derive(Debug)]
pub struct PngPanel {
    dir: PathBuf,
    next: Option<u64>,
}

impl PngPanel {
    /// Numbered frames kept in the directory.
    pub const HISTORY: usize = 20;

    /// A panel writing into `dir`.
    pub fn new(dir: PathBuf) -> Self {
        Self { dir, next: None }
    }

    /// The directory frames are written to.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Path of the current frame, `<dir>/frame.png`.
    pub fn current_path(&self) -> PathBuf {
        self.dir.join("frame.png")
    }

    /// Numbered history files present in the directory, oldest first.
    pub fn history(&self) -> io::Result<Vec<PathBuf>> {
        Ok(numbered(&self.dir)?.into_iter().map(|(_, p)| p).collect())
    }

    fn write(&mut self, frame: &Frame) -> io::Result<()> {
        fs::create_dir_all(&self.dir)?;
        let png = frame.to_png();
        let tmp = self.dir.join(".frame.png.tmp");
        fs::write(&tmp, &png)?;
        fs::rename(&tmp, self.current_path())?;

        let existing = numbered(&self.dir)?;
        let n = match self.next {
            Some(n) => n,
            None => existing.last().map_or(1, |(n, _)| n + 1),
        };
        fs::write(self.dir.join(format!("frame-{n}.png")), &png)?;
        self.next = Some(n + 1);

        let mut all = numbered(&self.dir)?;
        if all.len() > Self::HISTORY {
            let excess = all.len() - Self::HISTORY;
            for (_, path) in all.drain(..excess) {
                fs::remove_file(path)?;
            }
        }
        Ok(())
    }
}

/// `frame-<n>.png` files in `dir`, sorted by `n`.
fn numbered(dir: &Path) -> io::Result<Vec<(u64, PathBuf)>> {
    let mut out = Vec::new();
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(out),
        Err(err) => return Err(err),
    };
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        let n = name
            .strip_prefix("frame-")
            .and_then(|rest| rest.strip_suffix(".png"))
            .and_then(|digits| digits.parse::<u64>().ok());
        if let Some(n) = n {
            out.push((n, entry.path()));
        }
    }
    out.sort_by_key(|(n, _)| *n);
    Ok(out)
}

impl Panel for PngPanel {
    fn show(&mut self, frame: &Frame) -> Result<(), PanelError> {
        Ok(self.write(frame)?)
    }

    fn clear(&mut self) -> Result<(), PanelError> {
        self.show(&Frame::white())
    }

    fn sleep(&mut self) -> Result<(), PanelError> {
        Ok(())
    }

    fn name(&self) -> &'static str {
        "png"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::{Pixel, Threshold};

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("lovebox-pngpanel-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    fn read_frame(path: &Path) -> Frame {
        let img = image::open(path).unwrap().to_luma8();
        Frame::from_gray(&img, Threshold::default())
    }

    #[test]
    fn show_writes_frame_png_and_rotates_history_to_20() {
        let dir = scratch("rotate");
        let mut panel = PngPanel::new(dir.clone());
        assert_eq!(panel.name(), "png");
        let mut last = Frame::white();
        for i in 0..25u32 {
            let mut f = Frame::white();
            f.set(i, 0, Pixel::Black);
            panel.show(&f).unwrap();
            last = f;
        }
        assert_eq!(read_frame(&panel.current_path()), last);
        let history = panel.history().unwrap();
        assert_eq!(history.len(), 20);
        assert_eq!(history.first().unwrap().file_name().unwrap(), "frame-6.png");
        assert_eq!(history.last().unwrap().file_name().unwrap(), "frame-25.png");
        assert_eq!(read_frame(history.last().unwrap()), last);

        // A new panel on the same directory continues the numbering.
        let mut again = PngPanel::new(dir.clone());
        again.show(&Frame::black()).unwrap();
        let history = again.history().unwrap();
        assert_eq!(history.len(), 20);
        assert_eq!(history.last().unwrap().file_name().unwrap(), "frame-26.png");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn clear_writes_a_white_frame_and_sleep_is_a_no_op() {
        let dir = scratch("clear");
        let mut panel = PngPanel::new(dir.clone());
        panel.show(&Frame::black()).unwrap();
        panel.clear().unwrap();
        panel.sleep().unwrap();
        assert_eq!(read_frame(&panel.current_path()), Frame::white());
        fs::remove_dir_all(dir).unwrap();
    }
}
