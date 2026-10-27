use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use chrono::Utc;
use tokio::sync::{mpsc, oneshot};
use tracing::{error, info};

use crate::frame::Frame;
use crate::panel::{Panel, PanelError, Refresh};

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
    Show(Frame, Refresh, oneshot::Sender<Result<u64, String>>),
    Clear(oneshot::Sender<Result<u64, String>>),
}

#[derive(Clone)]
pub struct DisplayHandle {
    tx: mpsc::Sender<Command>,
    status: Arc<Mutex<DisplayStatus>>,
    timeout: Duration,
    operation_active: Arc<AtomicBool>,
}

struct ActiveOperationGuard(Arc<AtomicBool>);

impl Drop for ActiveOperationGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
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
                let ready = {
                    let mut state = worker_status
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    if state.state == DisplayState::Wedged {
                        false
                    } else {
                        state.name = name.clone();
                        state.state = DisplayState::Idle;
                        true
                    }
                };
                if !ready {
                    return;
                }
                let _ = init_tx.send(Ok(name.clone()));
                info!(panel = %panel.name(), "display worker ready");
                while let Some(command) = rx.blocking_recv() {
                    if !begin_operation(&worker_status) {
                        let reply = match command {
                            Command::Show(_, _, reply) | Command::Clear(reply) => reply,
                        };
                        let _ = reply.send(Err("display worker is wedged".to_owned()));
                        break;
                    }
                    let (reply, result) = match command {
                        Command::Show(frame, refresh, reply) => {
                            let start = Instant::now();
                            panel.set_refresh(refresh);
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
                            let result = panel.clear().and_then(|_| panel.sleep());
                            (
                                reply,
                                result.map(|_| {
                                    start.elapsed().as_millis().min(u64::MAX as u128) as u64
                                }),
                            )
                        }
                    };
                    let outcome = {
                        // Check and update under one lock. A timed-out caller sets Wedged;
                        // a late panel completion must never turn that back into Idle.
                        let mut state = worker_status
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        match result {
                            Ok(duration) if state.state != DisplayState::Wedged => {
                                state.state = DisplayState::Idle;
                                state.last_error = None;
                                state.last_show_at = Some(Utc::now().to_rfc3339());
                                state.last_refresh_duration_ms = Some(duration);
                                state.shows_total += 1;
                                Ok(duration)
                            }
                            Ok(_) => {
                                Err("operation completed after timeout; display remains wedged"
                                    .to_owned())
                            }
                            Err(err) => {
                                let message = err.to_string();
                                if state.state != DisplayState::Wedged {
                                    state.state = DisplayState::Idle;
                                    state.last_error = Some(message.clone());
                                }
                                Err(message)
                            }
                        }
                    };
                    let remains_wedged = worker_status
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .state
                        == DisplayState::Wedged;
                    let _ = reply.send(outcome);
                    if remains_wedged {
                        break;
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

    pub async fn show(&self, frame: Frame, refresh: Refresh) -> Result<u64, DisplayError> {
        self.execute(|reply| Command::Show(frame, refresh, reply))
            .await
    }

    pub async fn clear(&self) -> Result<u64, DisplayError> {
        self.execute(Command::Clear).await
    }

    async fn execute(
        &self,
        command: impl FnOnce(oneshot::Sender<Result<u64, String>>) -> Command,
    ) -> Result<u64, DisplayError> {
        if self
            .operation_active
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(DisplayError::Busy);
        }
        let _active = ActiveOperationGuard(self.operation_active.clone());
        {
            let mut status = self
                .status
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            match status.state {
                DisplayState::Wedged => {
                    return Err(DisplayError::Wedged(
                        status
                            .last_error
                            .clone()
                            .unwrap_or_else(|| "unknown timeout".to_owned()),
                    ))
                }
                DisplayState::Busy => return Err(DisplayError::Busy),
                DisplayState::Idle => status.state = DisplayState::Busy,
            }
        }
        let (reply, rx) = oneshot::channel();
        match self.tx.try_send(command(reply)) {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Closed(_)) => {
                set_idle_if_not_wedged(&self.status);
                return Err(DisplayError::WorkerStopped);
            }
            Err(mpsc::error::TrySendError::Full(_)) => return Err(DisplayError::Busy),
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
        result
    }

    pub fn status(&self) -> DisplayStatus {
        self.status
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

fn begin_operation(status: &Mutex<DisplayStatus>) -> bool {
    let mut state = status
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if state.state == DisplayState::Wedged {
        false
    } else {
        state.state = DisplayState::Busy;
        true
    }
}

fn set_idle_if_not_wedged(status: &Mutex<DisplayStatus>) {
    let mut state = status
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if state.state != DisplayState::Wedged {
        state.state = DisplayState::Idle;
    }
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
        handle.show(frame.clone(), Refresh::Full).await.unwrap();
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

    struct DelayedPanel {
        show_delay: Duration,
    }

    impl Panel for DelayedPanel {
        fn show(&mut self, _: &Frame) -> Result<(), PanelError> {
            thread::sleep(self.show_delay);
            Ok(())
        }
        fn clear(&mut self) -> Result<(), PanelError> {
            Ok(())
        }
        fn sleep(&mut self) -> Result<(), PanelError> {
            Ok(())
        }
        fn name(&self) -> &'static str {
            "delayed-test"
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
            handle.show(Frame::black(), Refresh::Full).await,
            Err(DisplayError::Timeout(_))
        ));
        assert!(start.elapsed() >= Duration::from_millis(900));
        assert_eq!(handle.status().state, DisplayState::Wedged);
        assert!(matches!(handle.clear().await, Err(DisplayError::Wedged(_))));
    }

    #[tokio::test]
    async fn late_panel_completion_never_clears_wedged_state() {
        let handle = DisplayHandle::start(
            || {
                Ok(Box::new(DelayedPanel {
                    show_delay: Duration::from_millis(200),
                }) as Box<dyn Panel>)
            },
            Duration::from_secs(1),
            Duration::from_millis(40),
        )
        .unwrap();
        assert!(matches!(
            handle.show(Frame::black(), Refresh::Full).await,
            Err(DisplayError::Timeout(_))
        ));
        tokio::time::sleep(Duration::from_millis(240)).await;
        assert_eq!(handle.status().state, DisplayState::Wedged);
        assert_eq!(handle.status().shows_total, 0);
        assert!(matches!(handle.clear().await, Err(DisplayError::Wedged(_))));
    }

    #[tokio::test]
    async fn cancelling_caller_keeps_worker_busy_until_completion_then_releases_gate() {
        let handle = DisplayHandle::start(
            || {
                Ok(Box::new(DelayedPanel {
                    show_delay: Duration::from_millis(200),
                }) as Box<dyn Panel>)
            },
            Duration::from_secs(1),
            Duration::from_secs(1),
        )
        .unwrap();
        let task_handle = handle.clone();
        let task =
            tokio::spawn(async move { task_handle.show(Frame::black(), Refresh::Full).await });
        tokio::time::timeout(Duration::from_secs(1), async {
            while handle.status().state != DisplayState::Busy {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        task.abort();
        let _ = task.await;
        assert!(matches!(handle.clear().await, Err(DisplayError::Busy)));
        tokio::time::sleep(Duration::from_millis(240)).await;
        assert_eq!(handle.status().state, DisplayState::Idle);
        assert_eq!(handle.status().shows_total, 1);
        handle.clear().await.unwrap();
        assert_eq!(handle.status().state, DisplayState::Idle);
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

    #[test]
    fn blocked_panel_constructor_times_out_without_blocking_caller() {
        let started = Instant::now();
        let result = DisplayHandle::start(
            || {
                thread::sleep(Duration::from_millis(150));
                Ok(Box::new(PngPanel::new(std::env::temp_dir())) as Box<dyn Panel>)
            },
            Duration::from_millis(20),
            Duration::from_secs(1),
        );
        assert!(matches!(result, Err(DisplayError::Timeout(_))));
        assert!(started.elapsed() < Duration::from_millis(100));
    }
}
