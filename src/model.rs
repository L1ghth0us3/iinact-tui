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
    pub settings: AppSettings,
    pub show_settings: bool,
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
    pub settings: AppSettings,
    pub show_settings: bool,
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
            settings: self.settings.clone(),
            show_settings: self.show_settings,
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

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppSettings {
    pub idle_seconds: u64,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self { idle_seconds: 5 }
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
        }
    }
}

impl From<AppSettings> for AppConfig {
    fn from(value: AppSettings) -> Self {
        AppConfig {
            idle_seconds: value.idle_seconds,
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

    pub fn wide_label(self) -> &'static str {
        match self {
            Decoration::Underline => "Decor: underline",
            Decoration::Background => "Decor: background",
            Decoration::None => "Decor: none",
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
    pub fn short_label(self) -> &'static str {
        match self {
            ViewMode::Dps => "mode:DPS",
            ViewMode::Heal => "mode:HEAL",
        }
    }
}
