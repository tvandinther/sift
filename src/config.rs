use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Configuration for sift.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Path to the SQLite database file.
    pub db_path: PathBuf,
    /// Path for cached model weights.
    pub model_cache: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            db_path: expand_home("~/.local/share/sift/index.db"),
            model_cache: expand_home("~/.local/share/sift/models"),
        }
    }
}

impl Config {
    /// Load configuration with resolution order: ENV → CLI overrides → config file → defaults.
    pub fn load(db_path_override: Option<PathBuf>) -> Result<Self> {
        let mut config = Self::default();

        // Try to load from config file
        let config_path = expand_home("~/.config/sift/config.toml");
        if config_path.exists() {
            let contents =
                std::fs::read_to_string(&config_path).context("Failed to read config file")?;
            let file_config: Config =
                toml::from_str(&contents).context("Failed to parse config file")?;
            config = file_config;
        }

        // CLI overrides take precedence
        if let Some(db_path) = db_path_override {
            config.db_path = expand_home_path(&db_path);
        }

        // Environment variable overrides (for Nix and testing)
        if let Ok(model_cache) = std::env::var("SIFT_MODEL_CACHE") {
            config.model_cache = PathBuf::from(model_cache);
        }

        // Ensure db_path and model_cache are expanded
        config.db_path = expand_home_path(&config.db_path);
        config.model_cache = expand_home_path(&config.model_cache);

        Ok(config)
    }

    /// Format config as TOML for display.
    pub fn to_toml(&self) -> Result<String> {
        toml::to_string_pretty(self).context("Failed to serialize config")
    }
}

/// Expand ~ in a string path to the home directory.
fn expand_home(path: &str) -> PathBuf {
    if let Some(stripped) = path.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join(stripped);
        }
    }
    PathBuf::from(path)
}

/// Expand ~ in a PathBuf to the home directory.
fn expand_home_path(path: &Path) -> PathBuf {
    if let Some(path_str) = path.to_str() {
        expand_home(path_str)
    } else {
        path.to_path_buf()
    }
}
