//! `lovebox`: the love-box e-paper frame service.
//!
//! M1 skeleton (U-101): `lovebox serve` logs one startup line, then a heartbeat every
//! 300 s until SIGTERM or SIGINT, and exits 0. Later milestones add the panel, renderer,
//! and web server (docs/PLAN.md).

use std::io::{self, IsTerminal};
use std::process::ExitCode;
use std::time::Duration;

use clap::{Args, Parser, Subcommand};
use tokio::signal::unix::{signal, SignalKind};
use tokio::time::{interval_at, Instant, MissedTickBehavior};
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

/// Crate version from Cargo.toml.
const VERSION: &str = env!("CARGO_PKG_VERSION");
/// Target triple this binary was compiled for (from build.rs).
const TARGET: &str = env!("LOVEBOX_TARGET");
/// Git short SHA at build time, or `unknown` (from build.rs).
const GIT_SHA: &str = env!("LOVEBOX_GIT_SHA");
/// Default seconds between heartbeat log lines.
const DEFAULT_HEARTBEAT_SECS: u64 = 300;

#[derive(Parser, Debug)]
#[command(name = "lovebox", version, about = "love-box e-paper frame service")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Debug, PartialEq, Eq)]
enum Command {
    /// Run the service (the default when no subcommand is given).
    Serve(ServeArgs),
}

#[derive(Args, Debug, PartialEq, Eq)]
struct ServeArgs {
    /// Seconds between heartbeat log lines.
    #[arg(long, default_value_t = DEFAULT_HEARTBEAT_SECS,
          value_parser = clap::value_parser!(u64).range(1..))]
    heartbeat_secs: u64,
}

impl Default for ServeArgs {
    fn default() -> Self {
        Self {
            heartbeat_secs: DEFAULT_HEARTBEAT_SECS,
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    init_logging();
    let result = match cli.command.unwrap_or(Command::Serve(ServeArgs::default())) {
        Command::Serve(args) => serve(&args),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            error!("fatal: {err}");
            ExitCode::FAILURE
        }
    }
}

/// Plain-text logs on stderr for journald. `RUST_LOG` filters (default `info`); colour only
/// on a terminal; no timestamp when journald is the sink, since it stamps every line.
fn init_logging() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let builder = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(io::stderr)
        .with_ansi(io::stderr().is_terminal());
    if std::env::var_os("JOURNAL_STREAM").is_some() {
        builder.without_time().init();
    } else {
        builder.init();
    }
}

/// The one INFO line logged at startup: version, git SHA, target triple, hostname.
fn startup_line(hostname: &str) -> String {
    format!("lovebox {VERSION} starting: git={GIT_SHA} target={TARGET} hostname={hostname}")
}

/// Best-effort hostname without a libc dependency: Linux procfs, then `hostname(1)`.
fn hostname() -> String {
    let from_file = ["/proc/sys/kernel/hostname", "/etc/hostname"]
        .iter()
        .filter_map(|path| std::fs::read_to_string(path).ok())
        .map(|s| s.trim().to_owned())
        .find(|s| !s.is_empty());
    from_file
        .or_else(|| {
            let out = std::process::Command::new("hostname").output().ok()?;
            let s = String::from_utf8(out.stdout).ok()?.trim().to_owned();
            (out.status.success() && !s.is_empty()).then_some(s)
        })
        .unwrap_or_else(|| "unknown".to_owned())
}

fn serve(args: &ServeArgs) -> io::Result<()> {
    // One thread: the Pi Zero W has a single core.
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(run(Duration::from_secs(args.heartbeat_secs)))
}

async fn run(heartbeat_every: Duration) -> io::Result<()> {
    // Register handlers before announcing readiness so an early signal is not missed.
    let mut sigterm = signal(SignalKind::terminate())?;
    let mut sigint = signal(SignalKind::interrupt())?;
    info!("{}", startup_line(&hostname()));

    let started = Instant::now();
    let mut heartbeat = interval_at(started + heartbeat_every, heartbeat_every);
    heartbeat.set_missed_tick_behavior(MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            _ = heartbeat.tick() => {
                info!(uptime_s = started.elapsed().as_secs(), "heartbeat");
            }
            _ = sigterm.recv() => {
                info!("received SIGTERM, exiting");
                return Ok(());
            }
            _ = sigint.recv() => {
                info!("received SIGINT, exiting");
                return Ok(());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn startup_line_contains_version_and_target_triple() {
        let line = startup_line("frame");
        assert!(line.contains(&format!("lovebox {}", env!("CARGO_PKG_VERSION"))));
        assert!(line.contains(&format!("target={TARGET}")));
        assert!(line.contains("hostname=frame"));
        // build.rs passes cargo's TARGET through, so a native test run sees a real triple.
        assert_ne!(TARGET, "unknown");
        assert!(TARGET.split('-').count() >= 3, "not a triple: {TARGET}");
    }

    #[test]
    fn cli_is_consistent() {
        Cli::command().debug_assert();
    }

    #[test]
    fn no_subcommand_means_serve_with_300s_heartbeat() {
        let cli = Cli::try_parse_from(["lovebox"]).unwrap();
        assert_eq!(cli.command, None);
        assert_eq!(ServeArgs::default().heartbeat_secs, 300);
        let cli = Cli::try_parse_from(["lovebox", "serve"]).unwrap();
        assert_eq!(cli.command, Some(Command::Serve(ServeArgs::default())));
    }
}
