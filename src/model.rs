use std::collections::HashSet;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::config::AppConfig;
use crate::history::{EncounterRecord, HistoryDay, HistoryEncounterItem};

pub const WS_URL_DEFAULT: &str = "ws://127.0.0.1:10501/ws";

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum HistoryPanelLevel {
    #[default]
    Dates,
    Encounters,
    EncounterDetail,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HistoryPanel {
    pub visible: bool,
    pub loading: bool,
    pub level: HistoryPanelLevel,
    pub days: Vec<HistoryDay>,
    pub selected_day: usize,
    pub selected_encounter: usize,
    pub error: Option<String>,
}

impl Default for HistoryPanel {
    fn default() -> Self {
        Self {
            visible: false,
            loading: false,
            level: HistoryPanelLevel::Dates,
            days: Vec::new(),
            selected_day: 0,
            selected_encounter: 0,
            error: None,
        }
    }
}

impl HistoryPanel {
    pub fn reset(&mut self) {
        self.loading = false;
        self.level = HistoryPanelLevel::Dates;
        self.selected_day = 0;
        self.selected_encounter = 0;
        self.error = None;
        for day in &mut self.days {
            day.encounters.clear();
            day.encounters_loaded = false;
        }
    }

    pub fn current_day(&self) -> Option<&HistoryDay> {
        self.days.get(self.selected_day)
    }

    pub fn current_encounter(&self) -> Option<&HistoryEncounterItem> {
        self.current_day()
            .and_then(|day| day.encounters.get(self.selected_encounter))
    }

    pub fn find_day_mut(&mut self, date_id: &str) -> Option<&mut HistoryDay> {
        self.days.iter_mut().find(|day| day.iso_date == date_id)
    }

    pub fn find_encounter_mut(&mut self, key: &[u8]) -> Option<&mut HistoryEncounterItem> {
        for day in &mut self.days {
            if let Some(item) = day.encounters.iter_mut().find(|item| item.key == key) {
                return Some(item);
            }
        }
        None
    }
}

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
    pub history: HistoryPanel,
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
    pub history: HistoryPanel,
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
            AppEvent::HistoryDatesLoaded { days } => {
                self.history.loading = false;
                self.history.error = None;
                self.history.days = days;
                if self.history.selected_day >= self.history.days.len() {
                    self.history.selected_day = 0;
                }
                if let Some(day) = self.history.current_day() {
                    if day.encounters.is_empty() {
                        self.history.selected_encounter = 0;
                    } else if self.history.selected_encounter >= day.encounters.len() {
                        self.history.selected_encounter = day.encounters.len() - 1;
                    }
                } else {
                    self.history.selected_encounter = 0;
                }
                if self.history.level == HistoryPanelLevel::Encounters
                    && self
                        .history
                        .current_day()
                        .map(|day| day.encounters.is_empty())
                        .unwrap_or(true)
                {
                    self.history.level = HistoryPanelLevel::Dates;
                }
                if self.history.level == HistoryPanelLevel::EncounterDetail
                    && self.history.current_encounter().is_none()
                {
                    self.history.level = if self
                        .history
                        .current_day()
                        .map(|day| day.encounters.is_empty())
                        .unwrap_or(true)
                    {
                        HistoryPanelLevel::Dates
                    } else {
                        HistoryPanelLevel::Encounters
                    };
                }
            }
            AppEvent::HistoryEncountersLoaded {
                date_id,
                encounters,
            } => {
                let selected_matches = self
                    .history
                    .days
                    .get(self.history.selected_day)
                    .map(|d| d.iso_date == date_id)
                    .unwrap_or(false);

                let new_len = encounters.len();
                if let Some(day) = self.history.find_day_mut(&date_id) {
                    day.encounters = encounters;
                    day.encounters_loaded = true;
                    day.encounter_count = new_len;
                }
                self.history.loading = false;

                if selected_matches
                    && self.history.level == HistoryPanelLevel::Encounters
                    && self.history.selected_encounter >= new_len
                {
                    self.history.selected_encounter = new_len.saturating_sub(1);
                }
            }
            AppEvent::HistoryEncounterLoaded { key, record } => {
                if let Some(item) = self.history.find_encounter_mut(&key) {
                    item.record = Some(record);
                }
                self.history.loading = false;
            }
            AppEvent::HistoryError { message } => {
                self.history.loading = false;
                self.history.error = Some(message);
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
            history: self.history.clone(),
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

    pub fn toggle_history(&mut self) -> bool {
        if self.history.visible {
            self.history.visible = false;
            self.history.reset();
            false
        } else {
            self.history.visible = true;
            self.history.loading = true;
            self.history.error = None;
            self.history.level = HistoryPanelLevel::Dates;
            self.history.selected_day = 0;
            self.history.selected_encounter = 0;
            true
        }
    }

    pub fn history_set_loading(&mut self) {
        self.history.loading = true;
        self.history.error = None;
    }

    pub fn history_move_selection(&mut self, delta: i32) {
        if !self.history.visible || self.history.loading {
            return;
        }
        match self.history.level {
            HistoryPanelLevel::Dates => {
                if self.history.days.is_empty() {
                    return;
                }
                let len = self.history.days.len() as i32;
                let current = self.history.selected_day as i32;
                let mut next = current + delta;
                if next < 0 {
                    next = 0;
                } else if next >= len {
                    next = len - 1;
                }
                self.history.selected_day = next as usize;
                if let Some(day) = self.history.current_day() {
                    if day.encounters.is_empty() {
                        self.history.selected_encounter = 0;
                    } else if self.history.selected_encounter >= day.encounters.len() {
                        self.history.selected_encounter = day.encounters.len() - 1;
                    }
                }
            }
            HistoryPanelLevel::Encounters | HistoryPanelLevel::EncounterDetail => {
                if let Some(day) = self.history.current_day() {
                    if day.encounters.is_empty() {
                        return;
                    }
                    let len = day.encounters.len() as i32;
                    let current = self.history.selected_encounter as i32;
                    let mut next = current + delta;
                    if next < 0 {
                        next = 0;
                    } else if next >= len {
                        next = len - 1;
                    }
                    self.history.selected_encounter = next as usize;
                }
            }
        }
    }

    pub fn history_enter(&mut self) {
        if !self.history.visible || self.history.loading {
            return;
        }
        match self.history.level {
            HistoryPanelLevel::Dates => {
                if let Some(day) = self.history.current_day() {
                    if day.encounters_loaded {
                        if !day.encounters.is_empty() {
                            self.history.level = HistoryPanelLevel::Encounters;
                            self.history.selected_encounter = 0;
                        }
                    } else if !day.encounter_ids.is_empty() {
                        self.history.level = HistoryPanelLevel::Encounters;
                        self.history.selected_encounter = 0;
                    }
                }
            }
            HistoryPanelLevel::Encounters => {
                if self.history.current_encounter().is_some() {
                    self.history.level = HistoryPanelLevel::EncounterDetail;
                }
            }
            HistoryPanelLevel::EncounterDetail => {}
        }
    }

    pub fn history_back(&mut self) {
        if !self.history.visible {
            return;
        }
        match self.history.level {
            HistoryPanelLevel::EncounterDetail => {
                self.history.level = HistoryPanelLevel::Encounters;
            }
            HistoryPanelLevel::Encounters => {
                self.history.level = HistoryPanelLevel::Dates;
                self.history.selected_encounter = 0;
            }
            HistoryPanelLevel::Dates => {}
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
    HistoryDatesLoaded {
        days: Vec<HistoryDay>,
    },
    HistoryEncountersLoaded {
        date_id: String,
        encounters: Vec<HistoryEncounterItem>,
    },
    HistoryEncounterLoaded {
        key: Vec<u8>,
        record: EncounterRecord,
    },
    HistoryError {
        message: String,
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
