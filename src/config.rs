use std::env;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default = "default_idle_seconds")]
    pub idle_seconds: u64,
    #[serde(default = "default_decoration")]
    pub default_decoration: String,
    #[serde(default = "default_mode")]
    pub default_mode: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            idle_seconds: default_idle_seconds(),
            default_decoration: default_decoration(),
            default_mode: default_mode(),
        }
    }
}

fn default_idle_seconds() -> u64 {
    5
}

fn default_decoration() -> String {
    "underline".to_string()
}

fn default_mode() -> String {
    "dps".to_string()
}

pub fn load() -> Result<AppConfig> {
    let path = config_path();
    match fs::read(&path) {
        Ok(bytes) => {
            let cfg: AppConfig = serde_json::from_slice(&bytes)
                .with_context(|| format!("Failed to parse config at {}", path.display()))?;
            Ok(cfg)
        }
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(AppConfig::default()),
        Err(err) => {
            Err(err).with_context(|| format!("Failed to read config at {}", path.display()))
        }
    }
}

pub fn save(cfg: &AppConfig) -> Result<()> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Unable to create config directory {}", parent.display()))?;
    }
    let data = serde_json::to_vec_pretty(cfg)?;
    fs::write(&path, data)
        .with_context(|| format!("Failed to write config to {}", path.display()))?;
    Ok(())
}

pub fn config_path() -> PathBuf {
    config_dir().join("iinact-tui.config")
}

fn config_dir() -> PathBuf {
    if let Some(path) = env::var_os("IINACT_TUI_CONFIG_DIR") {
        PathBuf::from(path)
    } else if let Some(path) = env::var_os("XDG_CONFIG_HOME") {
        PathBuf::from(path).join("iinact-tui")
    } else if let Some(home) = env::var_os("HOME") {
        Path::new(&home).join(".config").join("iinact-tui")
    } else if let Some(appdata) = env::var_os("APPDATA") {
        PathBuf::from(appdata).join("iinact-tui")
    } else {
        PathBuf::from(".")
    }
}
