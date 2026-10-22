use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use serde_json::Value;
use tracing::{error, warn};

use crate::api::types::State;

#[derive(Debug)]
pub enum StoreError {
    Io(std::io::Error),
    Json(serde_json::Error),
    UnsupportedVersion(u64),
    Update(String),
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "state storage I/O error: {e}"),
            Self::Json(e) => write!(f, "state storage JSON error: {e}"),
            Self::UnsupportedVersion(v) => write!(
                f,
                "state schema version {v} is newer than supported version {}",
                State::VERSION
            ),
            Self::Update(e) => f.write_str(e),
        }
    }
}

impl std::error::Error for StoreError {}

impl From<std::io::Error> for StoreError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}
impl From<serde_json::Error> for StoreError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

pub struct Store {
    dir: PathBuf,
    state_path: PathBuf,
    state: Mutex<State>,
}

impl Store {
    pub fn open(dir: impl AsRef<Path>) -> Result<Self, StoreError> {
        let dir = dir.as_ref().to_path_buf();
        fs::create_dir_all(&dir)?;
        let state_path = dir.join("state.json");
        let state = if state_path.exists() {
            match read_state(&state_path) {
                Ok(state) => state,
                Err(StoreError::UnsupportedVersion(v)) => {
                    return Err(StoreError::UnsupportedVersion(v))
                }
                Err(err) => {
                    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%S%.fZ");
                    let broken = dir.join(format!("state.json.broken-{stamp}"));
                    fs::rename(&state_path, &broken)?;
                    error!(path = %broken.display(), error = %err, "quarantined corrupt state file; starting fresh");
                    State::default()
                }
            }
        } else {
            State::default()
        };
        if !state_path.exists() {
            persist_atomic(&state_path, &state)?;
        }
        Ok(Self {
            dir,
            state_path,
            state: Mutex::new(state),
        })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn snapshot(&self) -> State {
        self.lock().clone()
    }

    pub fn lock(&self) -> MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Apply a candidate update and persist it before publishing it to readers. If the
    /// closure or atomic write fails, the in-memory state remains unchanged.
    pub fn update<T>(
        &self,
        update: impl FnOnce(&mut State) -> Result<T, String>,
    ) -> Result<T, StoreError> {
        let mut guard = self.lock();
        let mut candidate = guard.clone();
        let output = update(&mut candidate).map_err(StoreError::Update)?;
        persist_atomic(&self.state_path, &candidate)?;
        *guard = candidate;
        Ok(output)
    }

    pub fn state_path(&self) -> &Path {
        &self.state_path
    }
}

fn read_state(path: &Path) -> Result<State, StoreError> {
    let bytes = fs::read(path)?;
    let value: Value = serde_json::from_slice(&bytes)?;
    let version = value.get("version").and_then(Value::as_u64).unwrap_or(0);
    if version > State::VERSION as u64 {
        return Err(StoreError::UnsupportedVersion(version));
    }
    let state: State = serde_json::from_value(value)?;
    if state.version != State::VERSION {
        return Err(StoreError::Update(format!(
            "unsupported state schema version {}",
            state.version
        )));
    }
    Ok(state)
}

fn persist_atomic(path: &Path, state: &State) -> Result<(), StoreError> {
    let tmp = path.with_extension("json.tmp");
    let result = (|| {
        let bytes = serde_json::to_vec_pretty(state)?;
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&tmp)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&tmp, path)?;
        if let Some(parent) = path.parent() {
            match File::open(parent).and_then(|dir| dir.sync_all()) {
                Ok(()) => {}
                Err(e) => {
                    warn!(path = %parent.display(), error = %e, "could not sync state directory after atomic rename")
                }
            }
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::types::{ApiCard, Shown};

    #[test]
    fn create_update_reopen_round_trips_and_cleans_temp_file() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        store
            .update(|state| {
                state.settings.mode = crate::api::types::DisplayMode::Daily;
                Ok(())
            })
            .unwrap();
        assert!(!dir.path().join("state.json.tmp").exists());
        drop(store);
        let reopened = Store::open(dir.path()).unwrap();
        assert_eq!(
            reopened.snapshot().settings.mode,
            crate::api::types::DisplayMode::Daily
        );
    }

    #[test]
    fn corrupt_json_is_quarantined_and_fresh_state_starts() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("state.json"), b"{not json").unwrap();
        let store = Store::open(dir.path()).unwrap();
        assert_eq!(store.snapshot().version, State::VERSION);
        assert!(fs::read_dir(dir.path()).unwrap().any(|entry| entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains("state.json.broken-")));
    }

    #[test]
    fn future_state_version_is_rejected_without_quarantine() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("state.json"), br#"{"version":999}"#).unwrap();
        assert!(matches!(
            Store::open(dir.path()),
            Err(StoreError::UnsupportedVersion(999))
        ));
        assert!(dir.path().join("state.json").exists());
    }

    #[test]
    fn history_is_capped_at_five_hundred() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        store
            .update(|state| {
                for idx in 0..510 {
                    state.record_shown(Shown {
                        card: ApiCard::Clear,
                        shown_at: idx.to_string(),
                        duration_ms: 1,
                        frame_revision: idx,
                    });
                }
                Ok(())
            })
            .unwrap();
        assert_eq!(store.snapshot().history.len(), 500);
    }
}
