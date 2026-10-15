use anyhow::Context as _;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Target {
    Display,
    Region,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Quality {
    Standard,
    High,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct Settings {
    /// Serialized form (may contain `~`); expand at use site with [`expand_tilde`].
    pub output_directory: String,
    pub default_target: Target,
    pub default_fps: u32,
    pub quality: Quality,
    pub preferred_backend: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            output_directory: "~/Videos/Screen Recordings".into(),
            default_target: Target::Display,
            default_fps: 30,
            quality: Quality::Standard,
            preferred_backend: "auto".into(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("invalid default_fps {0}; must be 24, 30, or 60")]
    InvalidFps(u32),
    #[error("failed to parse config: {0}")]
    Parse(#[from] toml::de::Error),
}

impl Settings {
    /// Parse and validate settings; `Err` on malformed TOML or bad fps.
    pub fn from_toml_str(s: &str) -> Result<Self, ConfigError> {
        let settings: Self = toml::from_str(s)?;
        match settings.default_fps {
            24 | 30 | 60 => Ok(settings),
            fps => Err(ConfigError::InvalidFps(fps)),
        }
    }
}

/// Like [`Settings::from_toml_str`] but never fails: warns and returns defaults.
pub fn load_from_str(s: &str) -> Settings {
    match Settings::from_toml_str(s) {
        Ok(settings) => settings,
        Err(e) => {
            tracing::warn!(error = %e, "invalid config; using defaults");
            Settings::default()
        }
    }
}

pub(crate) fn config_dir() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "niri-recorder").map(|p| p.config_dir().to_path_buf())
}

/// Load settings from the XDG config path. Never fails: a missing or
/// unreadable/malformed `config.toml` yields defaults (with a warning
/// for anything other than a missing file).
pub fn load() -> Settings {
    let Some(path) = config_dir().map(|d| d.join("config.toml")) else {
        tracing::warn!("could not determine config directory; using defaults");
        return Settings::default();
    };
    match std::fs::read_to_string(&path) {
        Ok(contents) => load_from_str(&contents),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Settings::default(),
        Err(e) => {
            tracing::warn!(error = %e, path = %path.display(), "failed to read config; using defaults");
            Settings::default()
        }
    }
}

/// Write settings to `<config_dir>/config.toml`, creating directories.
pub fn save(s: &Settings) -> anyhow::Result<()> {
    let dir = config_dir().context("could not determine config directory")?;
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join("config.toml"), toml::to_string_pretty(s)?)?;
    Ok(())
}

/// Expand a leading `~` (or `~/`) to the user's home directory.
/// Non-tilde and `~other` paths pass through unchanged.
pub fn expand_tilde(p: &str) -> PathBuf {
    if let Some(rest) = p.strip_prefix('~') {
        if rest.is_empty() || rest.starts_with('/') {
            if let Some(base) = directories::BaseDirs::new() {
                return base.home_dir().join(rest.strip_prefix('/').unwrap_or(rest));
            }
        }
    }
    PathBuf::from(p)
}
