use std::collections::HashSet;
use std::time::Instant;

use serde::{Deserialize, Serialize};

pub const WS_URL_DEFAULT: &str = "ws://127.0.0.1:10501/ws";

// App-side snapshot used by the UI
#[derive(Clone, Default, Debug, Serialize, Deserialize)]
pub struct AppSnapshot {
    pub connected: bool,
    pub last_update_ms: u128,
    pub encounter: Option<EncounterSummary>,
    pub rows: Vec<CombatantRow>,
    pub inline_underline: bool,
}

#[derive(Clone, Default, Debug)]
pub struct AppState {
    pub connected: bool,
    pub last_update: Option<Instant>,
    pub encounter: Option<EncounterSummary>,
    pub rows: Vec<CombatantRow>,
    pub inline_underline: bool,
}

impl AppState {
    pub fn apply(&mut self, evt: AppEvent) {
        match evt {
            AppEvent::Connected => self.connected = true,
            AppEvent::Disconnected => self.connected = false,
            AppEvent::CombatData { encounter, rows } => {
                self.encounter = Some(encounter);
                self.rows = rows;
                self.last_update = Some(Instant::now());
            }
        }
    }

    pub fn clone_snapshot(&self) -> AppSnapshot {
        let now = Instant::now();
        AppSnapshot {
            connected: self.connected,
            last_update_ms: self
                .last_update
                .map(|t| now.saturating_duration_since(t).as_millis())
                .unwrap_or(0),
            encounter: self.encounter.clone(),
            rows: self.rows.clone(),
            inline_underline: self.inline_underline,
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
    pub is_active: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CombatantRow {
    pub name: String,
    pub job: String,
    pub encdps: f64,
    pub encdps_str: String,
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
