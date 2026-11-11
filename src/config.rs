// Configuration loading and platform data-directory resolution.

use std::path::{Path, PathBuf};

use serde::Deserialize;
use thiserror::Error;

/// Runtime settings loaded from `config.toml`.
#[derive(Debug, Default, Deserialize)]
pub struct Config {
    data_dir: Option<PathBuf>,
}

/// Errors that can occur while loading configuration or resolving paths.
#[derive(Debug, Error)]
pub enum ConfigError {
    /// The configuration file could not be read.
    #[error("could not read configuration file: {0}")]
    Read(#[from] std::io::Error),
    /// The configuration file contains invalid TOML.
    #[error("could not parse configuration file: {0}")]
    Parse(#[from] toml::de::Error),
    /// No platform data directory is available.
    #[error("platform data directory is unavailable")]
    DataDirectoryUnavailable,
}

impl Config {
    /// Loads settings from a TOML file, using defaults when the file is absent.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(error) => return Err(ConfigError::Read(error)),
        };

        let config = toml::from_str::<Self>(&text)?;
        Ok(config)
    }

    /// Returns the configured data directory or the platform default.
    pub fn data_dir(&self) -> Result<PathBuf, ConfigError> {
        if let Some(path) = self
            .data_dir
            .as_ref()
            .filter(|path| !path.as_os_str().is_empty())
        {
            return Ok(path.clone());
        }

        dirs::data_local_dir()
            .map(|path| path.join("tg-desktop-rs"))
            .ok_or(ConfigError::DataDirectoryUnavailable)
    }
}
