use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::config::AppConfig;

use super::{Decoration, ViewMode};

#[derive(Copy, Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum SettingsField {
    #[default]
    IdleTimeout,
    DefaultDecoration,
    DefaultMode,
}

impl SettingsField {
    pub fn next(self) -> Self {
        match self {
            SettingsField::IdleTimeout => SettingsField::DefaultDecoration,
            SettingsField::DefaultDecoration => SettingsField::DefaultMode,
            SettingsField::DefaultMode => SettingsField::IdleTimeout,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            SettingsField::IdleTimeout => SettingsField::DefaultMode,
            SettingsField::DefaultDecoration => SettingsField::IdleTimeout,
            SettingsField::DefaultMode => SettingsField::DefaultDecoration,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppSettings {
    pub idle_seconds: u64,
    pub default_decoration: Decoration,
    pub default_mode: ViewMode,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            idle_seconds: 5,
            default_decoration: Decoration::Underline,
            default_mode: ViewMode::Dps,
        }
    }
}

impl AppSettings {
    pub fn idle_duration(&self) -> Option<Duration> {
        if self.idle_seconds == 0 {
            None
        } else {
            Some(Duration::from_secs(self.idle_seconds))
        }
    }
}

impl From<AppConfig> for AppSettings {
    fn from(value: AppConfig) -> Self {
        Self {
            idle_seconds: value.idle_seconds,
            default_decoration: Decoration::from_config_key(&value.default_decoration),
            default_mode: ViewMode::from_config_key(&value.default_mode),
        }
    }
}

impl From<AppSettings> for AppConfig {
    fn from(value: AppSettings) -> Self {
        AppConfig {
            idle_seconds: value.idle_seconds,
            default_decoration: value.default_decoration.config_key().to_string(),
            default_mode: value.default_mode.config_key().to_string(),
        }
    }
}
