use serde::{Deserialize, Serialize};

use crate::history::{HistoryDay, HistoryEncounterItem};

use super::ViewMode;

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
    #[serde(default)]
    pub detail_mode: ViewMode,
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
            detail_mode: ViewMode::Dps,
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
        self.detail_mode = ViewMode::Dps;
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
