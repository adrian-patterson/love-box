use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PanelKind {
    #[default]
    Png,
    Waveshare,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub bind: SocketAddr,
    pub data_dir: PathBuf,
    pub panel: PanelKind,
    pub decode_limit_mb: u64,
    pub log: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            bind: "0.0.0.0:8080".parse().expect("valid default address"),
            data_dir: PathBuf::from("./data"),
            panel: PanelKind::Png,
            decode_limit_mb: 160,
            log: "info".to_owned(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ConfigOverrides {
    pub bind: Option<String>,
    pub data_dir: Option<PathBuf>,
    pub panel: Option<PanelKind>,
    pub decode_limit_mb: Option<u64>,
    pub log: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct FileConfig {
    bind: Option<String>,
    data_dir: Option<PathBuf>,
    panel: Option<PanelKind>,
    decode_limit_mb: Option<u64>,
    log: Option<String>,
}

impl Config {
    /// Load config with the precedence file < environment < command-line overrides.
    pub fn load(
        path: Option<&Path>,
        env: &HashMap<String, String>,
        flags: &ConfigOverrides,
    ) -> Result<Self, String> {
        let mut config = Self::default();
        if let Some(path) = path {
            let text = std::fs::read_to_string(path)
                .map_err(|e| format!("could not read config {}: {e}", path.display()))?;
            let file: FileConfig = toml::from_str(&text)
                .map_err(|e| format!("could not parse config {}: {e}", path.display()))?;
            if let Some(value) = file.bind {
                config.bind = parse_bind(&value)?;
            }
            if let Some(value) = file.data_dir {
                config.data_dir = value;
            }
            if let Some(value) = file.panel {
                config.panel = value;
            }
            if let Some(value) = file.decode_limit_mb {
                config.decode_limit_mb = value;
            }
            if let Some(value) = file.log {
                config.log = value;
            }
        }
        if let Some(value) = env.get("LOVEBOX_BIND") {
            config.bind = parse_bind(value)?;
        }
        if let Some(value) = env.get("LOVEBOX_DATA_DIR") {
            config.data_dir = PathBuf::from(value);
        }
        if let Some(value) = env.get("LOVEBOX_PANEL") {
            config.panel = parse_panel(value)?;
        }
        if let Some(value) = env.get("LOVEBOX_DECODE_LIMIT_MB") {
            config.decode_limit_mb = value
                .parse()
                .map_err(|_| "LOVEBOX_DECODE_LIMIT_MB must be a positive integer".to_owned())?;
        }
        if let Some(value) = env.get("LOVEBOX_LOG") {
            config.log = value.clone();
        }
        if let Some(value) = &flags.bind {
            config.bind = parse_bind(value)?;
        }
        if let Some(value) = &flags.data_dir {
            config.data_dir = value.clone();
        }
        if let Some(value) = flags.panel {
            config.panel = value;
        }
        if let Some(value) = flags.decode_limit_mb {
            config.decode_limit_mb = value;
        }
        if let Some(value) = &flags.log {
            config.log = value.clone();
        }
        if config.decode_limit_mb == 0 {
            return Err("decode_limit_mb must be at least 1".to_owned());
        }
        Ok(config)
    }
}

fn parse_bind(value: &str) -> Result<SocketAddr, String> {
    value
        .parse()
        .map_err(|_| format!("invalid bind address: {value}"))
}

fn parse_panel(value: &str) -> Result<PanelKind, String> {
    match value {
        "png" => Ok(PanelKind::Png),
        "waveshare" => Ok(PanelKind::Waveshare),
        _ => Err(format!("panel must be 'png' or 'waveshare', got {value:?}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_values_are_used_when_env_and_flags_are_absent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "bind='127.0.0.1:9000'\ndata_dir='/tmp/lovebox'\npanel='png'\ndecode_limit_mb=120\nlog='debug'\n").unwrap();
        let config =
            Config::load(Some(&path), &HashMap::new(), &ConfigOverrides::default()).unwrap();
        assert_eq!(config.bind, "127.0.0.1:9000".parse().unwrap());
        assert_eq!(config.data_dir, PathBuf::from("/tmp/lovebox"));
        assert_eq!(config.decode_limit_mb, 120);
        assert_eq!(config.log, "debug");
    }

    #[test]
    fn environment_overrides_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "bind='127.0.0.1:9000'\npanel='png'\n").unwrap();
        let env = HashMap::from([
            ("LOVEBOX_BIND".into(), "127.0.0.1:9001".into()),
            ("LOVEBOX_PANEL".into(), "waveshare".into()),
        ]);
        let config = Config::load(Some(&path), &env, &ConfigOverrides::default()).unwrap();
        assert_eq!(config.bind, "127.0.0.1:9001".parse().unwrap());
        assert_eq!(config.panel, PanelKind::Waveshare);
    }

    #[test]
    fn command_line_flags_override_environment() {
        let env = HashMap::from([("LOVEBOX_BIND".into(), "127.0.0.1:9001".into())]);
        let flags = ConfigOverrides {
            bind: Some("127.0.0.1:9002".into()),
            ..ConfigOverrides::default()
        };
        let config = Config::load(None, &env, &flags).unwrap();
        assert_eq!(config.bind, "127.0.0.1:9002".parse().unwrap());
    }
}
