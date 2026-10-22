//! `lovebox`: the love-box e-paper frame service (binary; the library is `src/lib.rs`).
//!
//! `lovebox serve` (U-101) logs one startup line, then a heartbeat every 300 s until
//! SIGTERM or SIGINT, and exits 0. Later milestones add the web server (docs/PLAN.md).

use std::io::{self, IsTerminal};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use clap::{Args, Parser, Subcommand, ValueEnum};
use lovebox::api::{self, AppState};
use lovebox::config::{Config, ConfigOverrides, PanelKind};
use lovebox::display::DisplayHandle;
use lovebox::fonts::{Fonts, DEFAULT_FONT};
use lovebox::import;
use lovebox::library::Library;
use lovebox::panel::{Panel, PanelError, PngPanel};
use lovebox::render::{
    self, photo, Align, Card, Dither, IngestLimits, MessageSize, PhotoSpec, QuoteSpec, RenderError,
};
use tokio::net::TcpListener;
use tokio::signal::unix::{signal, SignalKind};
use tokio::time::{interval_at, Instant, MissedTickBehavior};
use tracing::info;
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

#[derive(Subcommand, Debug, PartialEq)]
enum Command {
    /// Run the service (the default when no subcommand is given).
    Serve(ServeArgs),
    /// Render a card to an 800x480 1-bit PNG.
    Render(RenderArgs),
    /// List bundled font ids and display names.
    Fonts,
    /// Import one or more photos from this computer to a running frame.
    Import(ImportArgs),
}

#[derive(Args, Debug, PartialEq, Eq)]
struct ImportArgs {
    /// Photo files to send to the frame.
    #[arg(required = true)]
    files: Vec<PathBuf>,
    /// Base URL of the frame service.
    #[arg(long, default_value = "http://lovebox.local")]
    host: String,
    /// Optional caption applied to each imported photo.
    #[arg(long)]
    caption: Option<String>,
    /// Optional dithering method.
    #[arg(long, value_enum)]
    dither: Option<CliDither>,
}

#[derive(Args, Debug, PartialEq)]
struct RenderArgs {
    /// PNG destination.
    #[arg(short, long, global = true, default_value = "out/frame.png")]
    output: PathBuf,
    #[command(subcommand)]
    card: RenderCard,
}

#[derive(Subcommand, Debug, PartialEq)]
enum RenderCard {
    /// Message in one of the four legacy sizes.
    Message {
        text: String,
        #[arg(long, default_value = "48", value_parser = parse_size)]
        size: MessageSize,
        #[arg(long, default_value = DEFAULT_FONT)]
        font: String,
    },
    /// Auto-fitted quote, up to 2,000 normalised characters (14 px floor, 40 px margins).
    Quote {
        #[arg(required_unless_present = "file", conflicts_with = "file")]
        text: Option<String>,
        #[arg(long)]
        file: Option<PathBuf>,
        #[arg(long, default_value = DEFAULT_FONT)]
        font: String,
        #[arg(long)]
        author: Option<String>,
        #[arg(long, value_enum, default_value = "left")]
        align: CliAlign,
    },
    /// Decode, cover-crop and dither a photo.
    Photo {
        input: PathBuf,
        #[arg(long, value_enum, default_value = "floyd-steinberg")]
        dither: CliDither,
        #[arg(long, value_parser = parse_focal)]
        focal: Option<(f32, f32)>,
        #[arg(long)]
        no_enhance: bool,
        /// Save the bounded library copy as JPEG.
        #[arg(long)]
        keep_copy: Option<PathBuf>,
        /// Decode allocation budget in MiB (codec scratch space is additional).
        #[arg(long, default_value_t = 512, value_parser = clap::value_parser!(u64).range(1..=4096))]
        max_alloc_mib: u64,
    },
    /// Blank, all-white frame.
    Clear,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum CliAlign {
    Left,
    Center,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum CliDither {
    FloydSteinberg,
    Atkinson,
}

fn parse_size(value: &str) -> Result<MessageSize, String> {
    value
        .parse::<u32>()
        .ok()
        .and_then(MessageSize::from_pt)
        .ok_or_else(|| "size must be 30, 48, 64 or 96".to_owned())
}

fn parse_focal(value: &str) -> Result<(f32, f32), String> {
    let invalid = || "focal must be two finite fractions in 0..=1, e.g. 0.5,0.3".to_owned();
    let (x, y) = value.split_once(',').ok_or_else(invalid)?;
    let x: f32 = x.parse().map_err(|_| invalid())?;
    let y: f32 = y.parse().map_err(|_| invalid())?;
    if !(0.0..=1.0).contains(&x) || !(0.0..=1.0).contains(&y) {
        return Err(invalid());
    }
    Ok((x, y))
}

fn write_output(path: &std::path::Path, bytes: &[u8]) -> Result<(), RenderError> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, bytes)?;
    Ok(())
}

fn render_command(args: RenderArgs) -> Result<(), RenderError> {
    let card = match args.card {
        RenderCard::Message { text, size, font } => {
            let mut card = Card::message(text, size);
            if let Card::Message { font: name, .. } = &mut card {
                *name = font;
            }
            card
        }
        RenderCard::Quote {
            text,
            file,
            font,
            author,
            align,
        } => {
            let text = match file {
                Some(path) => std::fs::read_to_string(path)?,
                None => text.unwrap_or_default(),
            };
            let mut quote = QuoteSpec::new(text);
            quote.font = font;
            quote.attribution = author;
            quote.align = match align {
                CliAlign::Left => Align::Left,
                CliAlign::Center => Align::Center,
            };
            Card::Quote(quote)
        }
        RenderCard::Photo {
            input,
            dither,
            focal,
            no_enhance,
            keep_copy,
            max_alloc_mib,
        } => {
            let limits = IngestLimits::native().with_max_alloc(max_alloc_mib * 1024 * 1024);
            let image = photo::ingest(input.as_path(), &limits)?;
            if let Some(path) = keep_copy {
                write_output(&path, &photo::encode_jpeg(&image, 90)?)?;
            }
            let spec = PhotoSpec {
                dither: match dither {
                    CliDither::FloydSteinberg => Dither::FloydSteinberg,
                    CliDither::Atkinson => Dither::Atkinson,
                },
                focal,
                enhance: !no_enhance,
            };
            Card::Photo {
                image: Arc::new(image),
                spec,
            }
        }
        RenderCard::Clear => Card::Clear,
    };
    let frame = render::render(&card, &Fonts::bundled())?;
    write_output(&args.output, &frame.to_png())
}

#[derive(Args, Debug, PartialEq, Eq)]
struct ServeArgs {
    /// TOML configuration file (default: /etc/lovebox/config.toml when present).
    #[arg(long)]
    config: Option<PathBuf>,
    /// HTTP bind address.
    #[arg(long)]
    bind: Option<String>,
    /// Local state and photo directory.
    #[arg(long)]
    data_dir: Option<PathBuf>,
    /// Display backend.
    #[arg(long, value_enum)]
    panel: Option<CliPanel>,
    /// Photo decode allocation limit in MiB.
    #[arg(long, value_parser = clap::value_parser!(u64).range(1..=4096))]
    decode_limit_mb: Option<u64>,
    /// Tracing filter (for example info or debug).
    #[arg(long)]
    log: Option<String>,
    /// Seconds between heartbeat log lines.
    #[arg(long, default_value_t = DEFAULT_HEARTBEAT_SECS,
          value_parser = clap::value_parser!(u64).range(1..))]
    heartbeat_secs: u64,
}

impl Default for ServeArgs {
    fn default() -> Self {
        Self {
            config: None,
            bind: None,
            data_dir: None,
            panel: None,
            decode_limit_mb: None,
            log: None,
            heartbeat_secs: DEFAULT_HEARTBEAT_SECS,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum CliPanel {
    Png,
    Waveshare,
}

impl From<CliPanel> for PanelKind {
    fn from(value: CliPanel) -> Self {
        match value {
            CliPanel::Png => Self::Png,
            CliPanel::Waveshare => Self::Waveshare,
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command.unwrap_or(Command::Serve(ServeArgs::default())) {
        Command::Serve(args) => serve(&args).map_err(RenderError::from),
        Command::Render(args) => {
            init_logging(None);
            render_command(args)
        }
        Command::Import(args) => {
            init_logging(None);
            let dither = args.dither.map(|value| match value {
                CliDither::FloydSteinberg => Dither::FloydSteinberg,
                CliDither::Atkinson => Dither::Atkinson,
            });
            import::import_files(&args.files, &args.host, args.caption.as_deref(), dither)
                .map_err(RenderError::InvalidInput)
        }
        Command::Fonts => {
            init_logging(None);
            for face in Fonts::bundled().faces() {
                println!("{}\t{}", face.id(), face.display_name());
            }
            Ok(())
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("fatal: {err}");
            ExitCode::FAILURE
        }
    }
}

/// Plain-text logs on stderr for journald. `RUST_LOG` filters (default `info`); colour only
/// on a terminal; no timestamp when journald is the sink, since it stamps every line.
fn init_logging(configured: Option<&str>) {
    let filter = configured
        .and_then(|value| EnvFilter::try_new(value).ok())
        .or_else(|| EnvFilter::try_from_default_env().ok())
        .unwrap_or_else(|| EnvFilter::new("info"));
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
        .block_on(run(args))
}

async fn run(args: &ServeArgs) -> io::Result<()> {
    let config_path = args.config.clone().or_else(|| {
        let default = PathBuf::from("/etc/lovebox/config.toml");
        default.exists().then_some(default)
    });
    let env: std::collections::HashMap<String, String> = std::env::vars().collect();
    let overrides = ConfigOverrides {
        bind: args.bind.clone(),
        data_dir: args.data_dir.clone(),
        panel: args.panel.map(Into::into),
        decode_limit_mb: args.decode_limit_mb,
        log: args.log.clone(),
    };
    let config = Config::load(config_path.as_deref(), &env, &overrides)
        .map_err(|message| io::Error::new(io::ErrorKind::InvalidInput, message))?;
    init_logging(Some(&config.log));

    std::fs::create_dir_all(&config.data_dir)?;
    let store = std::sync::Arc::new(
        lovebox::store::Store::open(&config.data_dir)
            .map_err(|e| io::Error::other(e.to_string()))?,
    );
    let library = Library::new(&config.data_dir, store.clone(), config.decode_limit_mb);
    let panel_dir = config.data_dir.join("panel");
    let panel_kind = config.panel;
    let display = DisplayHandle::start(
        move || match panel_kind {
            PanelKind::Png => Ok(Box::new(PngPanel::new(panel_dir)) as Box<dyn Panel>),
            PanelKind::Waveshare => Err(PanelError::Device(
                "waveshare support is not built into this binary".to_owned(),
            )),
        },
        Duration::from_secs(10),
        Duration::from_secs(45),
    )
    .map_err(|e| io::Error::other(e.to_string()))?;
    let state = std::sync::Arc::new(AppState::new(config.clone(), store, library, display));
    let app = api::router(state);
    let listener = TcpListener::bind(config.bind).await?;

    // Register handlers before announcing readiness so an early signal is not missed.
    let mut sigterm = signal(SignalKind::terminate())?;
    let mut sigint = signal(SignalKind::interrupt())?;
    info!("{}", startup_line(&hostname()));
    info!(bind = %config.bind, panel = ?config.panel, "HTTP service ready");

    let heartbeat_every = Duration::from_secs(args.heartbeat_secs);
    let started = Instant::now();
    let heartbeat = tokio::spawn(async move {
        let mut heartbeat = interval_at(started + heartbeat_every, heartbeat_every);
        heartbeat.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            heartbeat.tick().await;
            info!(uptime_s = started.elapsed().as_secs(), "heartbeat");
        }
    });
    let server = axum::serve(listener, app).with_graceful_shutdown(async move {
        tokio::select! {
            _ = sigterm.recv() => info!("received SIGTERM, exiting"),
            _ = sigint.recv() => info!("received SIGINT, exiting"),
        }
    });
    let result = server.await;
    heartbeat.abort();
    result
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
