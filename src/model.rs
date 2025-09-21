use std::collections::HashSet;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::config::AppConfig;

pub const WS_URL_DEFAULT: &str = "ws://127.0.0.1:10501/ws";

// App-side snapshot used by the UI
#[derive(Clone, Default, Debug, Serialize, Deserialize)]
pub struct AppSnapshot {
    pub connected: bool,
    pub last_update_ms: u128,
    pub encounter: Option<EncounterSummary>,
    pub rows: Vec<CombatantRow>,
    pub decoration: Decoration,
    pub mode: ViewMode,
    pub is_idle: bool,
    pub idle_scene: IdleScene,
    pub settings: AppSettings,
    pub show_settings: bool,
    pub settings_cursor: SettingsField,
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum IdleScene {
    #[default]
    Status,
    TopCritChain,
    AsciiArt,
    TipOfTheDay,
    AchievementTicker,
}

impl IdleScene {
    pub fn label(self) -> &'static str {
        match self {
            IdleScene::Status => "status",
            IdleScene::TopCritChain => "top-crit-chain",
            IdleScene::AsciiArt => "ascii-art",
            IdleScene::TipOfTheDay => "tip",
            IdleScene::AchievementTicker => "achievements",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            IdleScene::Status => "Connection & encounter healthcheck",
            IdleScene::TopCritChain => "Highlights the longest critical damage streak",
            IdleScene::AsciiArt => "Rotating ASCII art showcase",
            IdleScene::TipOfTheDay => "Rotation and encounter tips",
            IdleScene::AchievementTicker => "Recently unlocked achievements",
        }
    }
}

#[derive(Clone, Default, Debug)]
pub struct AppState {
    pub connected: bool,
    pub last_update: Option<Instant>,
    pub last_active: Option<Instant>,
    pub connected_since: Option<Instant>,
    pub encounter: Option<EncounterSummary>,
    pub rows: Vec<CombatantRow>,
    pub decoration: Decoration,
    pub mode: ViewMode,
    pub idle_scene: IdleScene,
    pub settings: AppSettings,
    pub show_settings: bool,
    pub settings_cursor: SettingsField,
}

impl AppState {
    pub fn apply(&mut self, evt: AppEvent) {
        match evt {
            AppEvent::Connected => {
                self.connected = true;
                let now = Instant::now();
                self.last_update = Some(now);
                self.last_active = None;
                self.connected_since = Some(now);
            }
            AppEvent::Disconnected => {
                self.connected = false;
                self.last_update = None;
                self.last_active = None;
                self.connected_since = None;
            }
            AppEvent::CombatData { encounter, rows } => {
                let now = Instant::now();
                self.encounter = Some(encounter);
                self.rows = rows;
                self.last_update = Some(now);
                self.idle_scene = IdleScene::Status;
                if self
                    .encounter
                    .as_ref()
                    .map(|enc| enc.is_active)
                    .unwrap_or(false)
                {
                    self.last_active = Some(now);
                }
            }
        }
    }

    pub fn clone_snapshot(&self) -> AppSnapshot {
        let now = Instant::now();
        let elapsed_ms = self
            .last_update
            .map(|t| now.saturating_duration_since(t).as_millis())
            .unwrap_or(0);
        AppSnapshot {
            connected: self.connected,
            last_update_ms: elapsed_ms,
            encounter: self.encounter.clone(),
            rows: self.rows.clone(),
            decoration: self.decoration,
            mode: self.mode,
            is_idle: self.is_idle(now),
            idle_scene: self.idle_scene,
            settings: self.settings.clone(),
            show_settings: self.show_settings,
            settings_cursor: self.settings_cursor,
        }
    }
}

impl AppState {
    fn is_idle(&self, now: Instant) -> bool {
        if !self.connected {
            return false;
        }
        let Some(threshold) = self.settings.idle_duration() else {
            return false;
        };
        if self
            .encounter
            .as_ref()
            .map(|enc| enc.is_active)
            .unwrap_or(false)
        {
            return false;
        }
        if let Some(active) = self.last_active {
            if now.saturating_duration_since(active) >= threshold {
                return true;
            }
            return false;
        }
        if let Some(since) = self.connected_since {
            return now.saturating_duration_since(since) >= threshold;
        }
        false
    }

    pub fn apply_settings(&mut self, settings: AppSettings) {
        self.settings = settings;
        self.sync_current_with_defaults();
    }

    pub fn adjust_idle_seconds(&mut self, delta: i64) -> bool {
        let current = self.settings.idle_seconds;
        let raw = current as i64 + delta;
        let adjusted = if raw < 0 { 0 } else { raw as u64 };
        if adjusted != current {
            self.settings.idle_seconds = adjusted;
            true
        } else {
            false
        }
    }

    pub fn adjust_selected_setting(&mut self, forward: bool) -> bool {
        match self.settings_cursor {
            SettingsField::IdleTimeout => self.adjust_idle_seconds(if forward { 1 } else { -1 }),
            SettingsField::DefaultDecoration => {
                let changed = self.cycle_default_decoration(forward);
                if changed {
                    self.sync_current_with_defaults();
                }
                changed
            }
            SettingsField::DefaultMode => {
                let changed = self.cycle_default_mode(forward);
                if changed {
                    self.sync_current_with_defaults();
                }
                changed
            }
        }
    }

    pub fn next_setting(&mut self) {
        self.settings_cursor = self.settings_cursor.next();
    }

    pub fn prev_setting(&mut self) {
        self.settings_cursor = self.settings_cursor.prev();
    }

    fn cycle_default_decoration(&mut self, forward: bool) -> bool {
        let current = self.settings.default_decoration;
        let next = if forward {
            current.next()
        } else {
            current.prev()
        };
        if next != current {
            self.settings.default_decoration = next;
            true
        } else {
            false
        }
    }

    fn cycle_default_mode(&mut self, forward: bool) -> bool {
        let current = self.settings.default_mode;
        let next = if forward {
            current.next()
        } else {
            current.prev()
        };
        if next != current {
            self.settings.default_mode = next;
            true
        } else {
            false
        }
    }

    fn sync_current_with_defaults(&mut self) {
        self.decoration = self.settings.default_decoration;
        self.mode = self.settings.default_mode;
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct EncounterSummary {
    pub title: String,
    pub zone: String,
    pub duration: String,
    pub encdps: String,
    pub damage: String,
    pub enchps: String,
    pub healed: String,
    pub is_active: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CombatantRow {
    pub name: String,
    pub job: String,
    pub encdps: f64,
    pub encdps_str: String,
    pub damage: f64,
    pub damage_str: String,
    pub share: f64,        // 0.0..=1.0
    pub share_str: String, // e.g., "23.4%"
    pub enchps: f64,
    pub enchps_str: String,
    pub healed: f64,
    pub healed_str: String,
    pub heal_share: f64,
    pub heal_share_str: String,
    pub overheal_pct: String,
    pub crit: String,
    pub dh: String,
    pub deaths: String,
}

#[derive(Debug)]
pub enum AppEvent {
    Connected,
    Disconnected,
    CombatData {
        encounter: EncounterSummary,
        rows: Vec<CombatantRow>,
    },
}

// Known job codes for party filtering and color mapping
pub fn known_jobs() -> &'static HashSet<&'static str> {
    use once_cell::sync::Lazy;
    static JOBS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
        [
            // Tanks
            "PLD", "WAR", "DRK", "GNB", // Healers
            "WHM", "SCH", "AST", "SGE", // Melee
            "MNK", "DRG", "NIN", "SAM", "RPR", "VPR", // Ranged phys
            "BRD", "MCH", "DNC", // Casters
            "BLM", "SMN", "RDM", "PCT", // Limited
            "BLU",
        ]
        .into_iter()
        .collect()
    });
    &JOBS
}

// (reserved for future outbound WS messages via in-TUI controls)

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

// Visual decoration modes for rows; designed to be easily extensible.
#[derive(Copy, Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum Decoration {
    // No additional decoration; compact one-line rows
    None,
    // Thin role-colored underline on the line below each row (two-line rows)
    #[default]
    Underline,
    // Role-colored background meter behind each row (one-line rows)
    Background,
}

impl Decoration {
    pub fn next(self) -> Self {
        match self {
            Decoration::Underline => Decoration::Background,
            Decoration::Background => Decoration::None,
            Decoration::None => Decoration::Underline,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Decoration::Underline => Decoration::None,
            Decoration::Background => Decoration::Underline,
            Decoration::None => Decoration::Background,
        }
    }

    pub fn row_height(self) -> u16 {
        match self {
            Decoration::Underline => 2,
            Decoration::Background | Decoration::None => 1,
        }
    }

    pub fn short_label(self) -> &'static str {
        match self {
            Decoration::Underline => "decor:line",
            Decoration::Background => "decor:bg",
            Decoration::None => "decor:none",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Decoration::Underline => "Underline",
            Decoration::Background => "Background",
            Decoration::None => "None",
        }
    }

    pub fn config_key(self) -> &'static str {
        match self {
            Decoration::Underline => "underline",
            Decoration::Background => "background",
            Decoration::None => "none",
        }
    }

    pub fn from_config_key<S: AsRef<str>>(key: S) -> Self {
        match key.as_ref().to_ascii_lowercase().as_str() {
            "background" => Decoration::Background,
            "none" => Decoration::None,
            _ => Decoration::Underline,
        }
    }
}

// High-level view mode of the table
#[derive(Copy, Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum ViewMode {
    #[default]
    Dps,
    Heal,
}

impl ViewMode {
    pub fn next(self) -> Self {
        match self {
            ViewMode::Dps => ViewMode::Heal,
            ViewMode::Heal => ViewMode::Dps,
        }
    }

    pub fn prev(self) -> Self {
        self.next()
    }
    pub fn short_label(self) -> &'static str {
        match self {
            ViewMode::Dps => "mode:DPS",
            ViewMode::Heal => "mode:HEAL",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ViewMode::Dps => "DPS",
            ViewMode::Heal => "HEAL",
        }
    }

    pub fn config_key(self) -> &'static str {
        match self {
            ViewMode::Dps => "dps",
            ViewMode::Heal => "heal",
        }
    }

    pub fn from_config_key<S: AsRef<str>>(key: S) -> Self {
        match key.as_ref().to_ascii_lowercase().as_str() {
            "heal" => ViewMode::Heal,
            _ => ViewMode::Dps,
        }
    }
}
