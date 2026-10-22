use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use chrono::Utc;
use tokio::sync::{mpsc, oneshot};
use tracing::{error, info};

use crate::frame::Frame;
use crate::panel::{Panel, PanelError};

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DisplayState {
    Idle,
    Busy,
    Wedged,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct DisplayStatus {
    pub name: String,
    pub state: DisplayState,
    pub last_show_at: Option<String>,
    pub last_error: Option<String>,
    pub shows_total: u64,
    pub last_refresh_duration_ms: Option<u64>,
}

#[derive(Debug, Clone)]
pub enum DisplayError {
    Busy,
    Wedged(String),
    Timeout(Duration),
    Panel(String),
    WorkerStopped,
    Initialization(String),
}

impl std::fmt::Display for DisplayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Busy => f.write_str("the display is already updating"),
            Self::Wedged(message) => write!(
                f,
                "display worker is wedged: {message}; restart the service to recover"
            ),
            Self::Timeout(duration) => write!(
                f,
                "display operation timed out after {} seconds; restart the service to recover",
                duration.as_secs()
            ),
            Self::Panel(message) => write!(f, "display operation failed: {message}"),
            Self::WorkerStopped => f.write_str("display worker stopped"),
            Self::Initialization(message) => write!(f, "could not initialize display: {message}"),
        }
    }
}
impl std::error::Error for DisplayError {}

enum Command {
    Show(Frame, oneshot::Sender<Result<u64, String>>),
    Clear(oneshot::Sender<Result<u64, String>>),
}

#[derive(Clone)]
pub struct DisplayHandle {
    tx: mpsc::Sender<Command>,
    status: Arc<Mutex<DisplayStatus>>,
    timeout: Duration,
    operation_active: Arc<AtomicBool>,
}

impl DisplayHandle {
    /// Run the panel factory on the owning thread and bound initialization wait.
    pub fn start<F>(
        factory: F,
        init_timeout: Duration,
        operation_timeout: Duration,
    ) -> Result<Self, DisplayError>
    where
        F: FnOnce() -> Result<Box<dyn Panel>, PanelError> + Send + 'static,
    {
        let (tx, mut rx) = mpsc::channel(1);
        let status = Arc::new(Mutex::new(DisplayStatus {
            name: "initializing".to_owned(),
            state: DisplayState::Busy,
            last_show_at: None,
            last_error: None,
            shows_total: 0,
            last_refresh_duration_ms: None,
        }));
        let worker_status = status.clone();
        let (init_tx, init_rx) = std::sync::mpsc::sync_channel(1);
        thread::Builder::new()
            .name("lovebox-display".to_owned())
            .spawn(move || {
                let mut panel = match factory() {
                    Ok(panel) => panel,
                    Err(err) => {
                        let message = err.to_string();
                        let _ = init_tx.send(Err(message.clone()));
                        error!(error = %message, "display initialization failed");
                        return;
                    }
                };
                let name = panel.name().to_owned();
                let _ = init_tx.send(Ok(name.clone()));
                {
                    let mut state = worker_status
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    state.name = name;
                    state.state = DisplayState::Idle;
                }
                info!(panel = %panel.name(), "display worker ready");
                while let Some(command) = rx.blocking_recv() {
                    if worker_status
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .state
                        == DisplayState::Wedged
                    {
                        break;
                    }
                    let (reply, result) = match command {
                        Command::Show(frame, reply) => {
                            let start = Instant::now();
                            set_busy(&worker_status);
                            let result = panel.show(&frame).and_then(|_| panel.sleep());
                            (
                                reply,
                                result.map(|_| {
                                    start.elapsed().as_millis().min(u64::MAX as u128) as u64
                                }),
                            )
                        }
                        Command::Clear(reply) => {
                            let start = Instant::now();
                            set_busy(&worker_status);
                            let result = panel.clear().and_then(|_| panel.sleep());
                            (
                                reply,
                                result.map(|_| {
                                    start.elapsed().as_millis().min(u64::MAX as u128) as u64
                                }),
                            )
                        }
                    };
                    let wedged = worker_status
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .state
                        == DisplayState::Wedged;
                    match result {
                        Ok(duration) if !wedged => {
                            let mut state = worker_status
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner);
                            state.state = DisplayState::Idle;
                            state.last_error = None;
                            state.last_show_at = Some(Utc::now().to_rfc3339());
                            state.last_refresh_duration_ms = Some(duration);
                            state.shows_total += 1;
                            let _ = reply.send(Ok(duration));
                        }
                        Ok(_) => {
                            let _ = reply.send(Err(
                                "operation completed after timeout; display remains wedged"
                                    .to_owned(),
                            ));
                            break;
                        }
                        Err(err) => {
                            let message = err.to_string();
                            let mut state = worker_status
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner);
                            if !wedged {
                                state.state = DisplayState::Idle;
                                state.last_error = Some(message.clone());
                            }
                            let _ = reply.send(Err(message));
                        }
                    }
                }
            })
            .map_err(|e| DisplayError::Initialization(e.to_string()))?;

        match init_rx.recv_timeout(init_timeout) {
            Ok(Ok(name)) => {
                let mut state = status
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                state.name = name;
                state.state = DisplayState::Idle;
            }
            Ok(Err(message)) => {
                let mut state = status
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                state.state = DisplayState::Wedged;
                state.last_error = Some(message.clone());
                return Err(DisplayError::Initialization(message));
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                let mut state = status
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                state.state = DisplayState::Wedged;
                state.last_error = Some(format!(
                    "display initialization timed out after {} seconds",
                    init_timeout.as_secs()
                ));
                return Err(DisplayError::Timeout(init_timeout));
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                return Err(DisplayError::WorkerStopped)
            }
        }
        Ok(Self {
            tx,
            status,
            timeout: operation_timeout,
            operation_active: Arc::new(AtomicBool::new(false)),
        })
    }

    pub async fn show(&self, frame: Frame) -> Result<u64, DisplayError> {
        self.execute(|reply| Command::Show(frame, reply)).await
    }

    pub async fn clear(&self) -> Result<u64, DisplayError> {
        self.execute(Command::Clear).await
    }

    async fn execute(
        &self,
        command: impl FnOnce(oneshot::Sender<Result<u64, String>>) -> Command,
    ) -> Result<u64, DisplayError> {
        let status = self.status();
        if status.state == DisplayState::Wedged {
            return Err(DisplayError::Wedged(
                status
                    .last_error
                    .unwrap_or_else(|| "unknown timeout".to_owned()),
            ));
        }
        if self
            .operation_active
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(DisplayError::Busy);
        }
        let (reply, rx) = oneshot::channel();
        if self.tx.send(command(reply)).await.is_err() {
            self.operation_active.store(false, Ordering::Release);
            return Err(DisplayError::WorkerStopped);
        }
        let result = match tokio::time::timeout(self.timeout, rx).await {
            Ok(Ok(Ok(duration))) => Ok(duration),
            Ok(Ok(Err(message))) => Err(DisplayError::Panel(message)),
            Ok(Err(_)) => Err(DisplayError::WorkerStopped),
            Err(_) => {
                let message = format!(
                    "operation timed out after {} seconds",
                    self.timeout.as_secs()
                );
                let mut status = self
                    .status
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                status.state = DisplayState::Wedged;
                status.last_error = Some(message);
                error!(
                    timeout_s = self.timeout.as_secs(),
                    "display worker timed out and is wedged"
                );
                Err(DisplayError::Timeout(self.timeout))
            }
        };
        self.operation_active.store(false, Ordering::Release);
        result
    }

    pub fn status(&self) -> DisplayStatus {
        self.status
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

fn set_busy(status: &Mutex<DisplayStatus>) {
    status
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .state = DisplayState::Busy;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panel::{Panel, PngPanel};
    use std::path::PathBuf;

    #[tokio::test]
    async fn png_panel_show_writes_frame_and_reports_idle() {
        let dir = tempfile::tempdir().unwrap();
        let panel_dir = dir.path().join("panel");
        let handle = DisplayHandle::start(
            move || Ok(Box::new(PngPanel::new(panel_dir)) as Box<dyn Panel>),
            Duration::from_secs(1),
            Duration::from_secs(1),
        )
        .unwrap();
        let frame = Frame::black();
        handle.show(frame.clone()).await.unwrap();
        let saved = std::fs::read(PathBuf::from(dir.path()).join("panel/frame.png")).unwrap();
        assert_eq!(crate::library::decode_frame_png(&saved).unwrap(), frame);
        assert_eq!(handle.status().state, DisplayState::Idle);
        assert_eq!(handle.status().shows_total, 1);
    }

    struct StuckPanel;
    impl Panel for StuckPanel {
        fn show(&mut self, _: &Frame) -> Result<(), PanelError> {
            loop {
                thread::sleep(Duration::from_secs(60));
            }
        }
        fn clear(&mut self) -> Result<(), PanelError> {
            loop {
                thread::sleep(Duration::from_secs(60));
            }
        }
        fn sleep(&mut self) -> Result<(), PanelError> {
            Ok(())
        }
        fn name(&self) -> &'static str {
            "stuck-test"
        }
    }

    #[tokio::test]
    async fn blocked_panel_times_out_and_rejects_subsequent_calls() {
        let handle = DisplayHandle::start(
            || Ok(Box::new(StuckPanel) as Box<dyn Panel>),
            Duration::from_secs(1),
            Duration::from_secs(1),
        )
        .unwrap();
        let start = Instant::now();
        assert!(matches!(
            handle.show(Frame::black()).await,
            Err(DisplayError::Timeout(_))
        ));
        assert!(start.elapsed() >= Duration::from_millis(900));
        assert_eq!(handle.status().state, DisplayState::Wedged);
        assert!(matches!(handle.clear().await, Err(DisplayError::Wedged(_))));
    }

    #[test]
    fn bad_panel_constructor_is_returned_as_bounded_initialization_error() {
        let result = DisplayHandle::start(
            || Err(PanelError::Device("no panel".to_owned())),
            Duration::from_secs(1),
            Duration::from_secs(1),
        );
        assert!(matches!(result, Err(DisplayError::Initialization(_))));
    }
}
