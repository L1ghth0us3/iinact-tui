use std::sync::Arc;

use serde_json::Value;
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio::task;

use crate::model::{CombatantRow, EncounterSummary};

use super::store::HistoryStore;
use super::types::{EncounterFrame, EncounterRecord, EncounterSnapshot};

pub struct RecorderHandle {
    inner: Arc<RecorderInner>,
}

struct RecorderInner {
    tx: mpsc::UnboundedSender<RecorderMessage>,
    shutdown: Mutex<Option<oneshot::Receiver<()>>>,
}

impl RecorderHandle {
    pub fn record(&self, snapshot: EncounterSnapshot) {
        let _ = self
            .inner
            .tx
            .send(RecorderMessage::Snapshot(Box::new(snapshot)));
    }

    pub fn record_components(
        &self,
        encounter: EncounterSummary,
        rows: Vec<CombatantRow>,
        raw: Value,
    ) {
        self.record(EncounterSnapshot::new(encounter, rows, raw));
    }

    pub fn flush(&self) {
        let _ = self.inner.tx.send(RecorderMessage::Flush);
    }

    pub async fn shutdown(&self) {
        let _ = self.inner.tx.send(RecorderMessage::Shutdown);
        if let Some(rx) = self.take_shutdown_receiver().await {
            let _ = rx.await;
        }
    }

    async fn take_shutdown_receiver(&self) -> Option<oneshot::Receiver<()>> {
        let mut guard = self.inner.shutdown.lock().await;
        guard.take()
    }
}

impl Clone for RecorderHandle {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

enum RecorderMessage {
    Snapshot(Box<EncounterSnapshot>),
    Flush,
    Shutdown,
}

pub fn spawn_recorder(store: Arc<HistoryStore>) -> RecorderHandle {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    tokio::spawn(async move {
        let mut worker = RecorderWorker::new(store);
        loop {
            match rx.recv().await {
                Some(RecorderMessage::Snapshot(snapshot)) => worker.on_snapshot(*snapshot).await,
                Some(RecorderMessage::Flush) => worker.on_flush().await,
                Some(RecorderMessage::Shutdown) => {
                    worker.on_flush().await;
                    break;
                }
                None => {
                    worker.on_flush().await;
                    break;
                }
            }
        }
        let _ = shutdown_tx.send(());
    });
    RecorderHandle {
        inner: Arc::new(RecorderInner {
            tx,
            shutdown: Mutex::new(Some(shutdown_rx)),
        }),
    }
}

struct RecorderWorker {
    store: Arc<HistoryStore>,
    current: Option<ActiveEncounter>,
}

impl RecorderWorker {
    fn new(store: Arc<HistoryStore>) -> Self {
        Self {
            store,
            current: None,
        }
    }

    async fn on_snapshot(&mut self, snapshot: EncounterSnapshot) {
        if self.current.is_none() {
            if !snapshot.encounter.is_active {
                return;
            }
            if !snapshot_has_activity(&snapshot) {
                return;
            }
        }

        if let Some(active) = self.current.as_ref() {
            if should_rollover(active, &snapshot) {
                self.flush_active().await;
            }
        }

        if let Some(active) = self.current.as_mut() {
            active.update(snapshot);
        } else {
            self.current = Some(ActiveEncounter::from_snapshot(snapshot));
        }

        if let Some(active) = self.current.as_ref() {
            if !active.latest_summary.is_active {
                self.flush_active().await;
            }
        }
    }

    async fn on_flush(&mut self) {
        self.flush_active().await;
    }

    async fn flush_active(&mut self) {
        if let Some(active) = self.current.take() {
            let store = Arc::clone(&self.store);
            let record = EncounterRecord::from_active(active);
            if !record.saw_active && record.rows.is_empty() {
                return;
            }
            match task::spawn_blocking(move || store.append(&record)).await {
                Ok(Ok(_)) => {}
                Ok(Err(err)) => {
                    eprintln!("Failed to persist encounter history: {err:#}");
                }
                Err(err) => {
                    eprintln!("History recorder join error: {err}");
                }
            }
        }
    }
}

#[derive(Debug)]
struct ActiveEncounter {
    first_seen_ms: u64,
    last_seen_ms: u64,
    latest_summary: EncounterSummary,
    latest_rows: Vec<CombatantRow>,
    last_raw: Value,
    saw_active: bool,
    frames: Vec<EncounterFrame>,
}

impl ActiveEncounter {
    fn from_snapshot(snapshot: EncounterSnapshot) -> Self {
        let EncounterSnapshot {
            encounter,
            rows,
            raw,
            received_ms,
        } = snapshot;
        let is_active = encounter.is_active;
        let frame = EncounterFrame::new(received_ms, encounter.clone(), rows.clone(), raw.clone());
        Self {
            first_seen_ms: received_ms,
            last_seen_ms: received_ms,
            latest_summary: encounter,
            latest_rows: rows,
            last_raw: raw,
            saw_active: is_active,
            frames: vec![frame],
        }
    }

    fn update(&mut self, snapshot: EncounterSnapshot) {
        self.last_seen_ms = snapshot.received_ms;
        let EncounterSnapshot {
            encounter,
            rows,
            raw,
            received_ms,
        } = snapshot;
        let frame = EncounterFrame::new(received_ms, encounter.clone(), rows.clone(), raw.clone());
        self.latest_summary = encounter;
        self.latest_rows = rows;
        self.last_raw = raw;
        self.frames.push(frame);
        self.saw_active |= self.latest_summary.is_active;
    }
}

impl EncounterRecord {
    fn from_active(active: ActiveEncounter) -> Self {
        let ActiveEncounter {
            first_seen_ms,
            last_seen_ms,
            latest_summary,
            latest_rows,
            last_raw,
            saw_active,
            frames,
        } = active;
        let snapshots = frames.len() as u32;
        let raw_last = if let Some(frame) = frames.last() {
            Some(frame.raw.clone())
        } else {
            Some(last_raw)
        };

        Self {
            version: super::types::SCHEMA_VERSION,
            stored_ms: super::types::now_ms(),
            first_seen_ms,
            last_seen_ms,
            encounter: latest_summary,
            rows: latest_rows,
            raw_last,
            snapshots,
            saw_active,
            frames,
        }
    }
}

impl EncounterFrame {
    fn new(
        received_ms: u64,
        encounter: EncounterSummary,
        rows: Vec<CombatantRow>,
        raw: Value,
    ) -> Self {
        Self {
            received_ms,
            encounter,
            rows,
            raw,
        }
    }
}

fn should_rollover(active: &ActiveEncounter, incoming: &EncounterSnapshot) -> bool {
    let previous = &active.latest_summary;
    let next = &incoming.encounter;

    if next.is_active {
        if !active.saw_active {
            return true;
        }

        if let (Some(prev_secs), Some(next_secs)) = (
            parse_duration_secs(&previous.duration),
            parse_duration_secs(&next.duration),
        ) {
            if next_secs + 2 < prev_secs {
                return true;
            }
            if prev_secs > 10 && next_secs == 0 {
                return true;
            }
        }

        let prev_damage = parse_number(&previous.damage);
        let next_damage = parse_number(&next.damage);
        if next_damage + 1.0 < prev_damage {
            return true;
        }
    }

    false
}

fn snapshot_has_activity(snapshot: &EncounterSnapshot) -> bool {
    if snapshot.encounter.is_active {
        return true;
    }
    if parse_number(&snapshot.encounter.damage) > 0.0
        || parse_number(&snapshot.encounter.healed) > 0.0
        || parse_number(&snapshot.encounter.encdps) > 0.0
        || parse_number(&snapshot.encounter.enchps) > 0.0
    {
        return true;
    }
    snapshot
        .rows
        .iter()
        .any(|row| row.damage > 0.0 || row.healed > 0.0 || row.encdps > 0.0 || row.enchps > 0.0)
}

fn parse_duration_secs(s: &str) -> Option<u64> {
    if s.trim().is_empty() {
        return None;
    }
    let mut parts: Vec<&str> = s.trim().split(':').collect();
    if parts.is_empty() {
        return None;
    }
    if parts.len() > 3 {
        return None;
    }
    let mut value = 0u64;
    let mut multiplier = 1u64;
    while let Some(part) = parts.pop() {
        let part = part.trim();
        if part.is_empty() || part.contains('-') {
            return None;
        }
        let parsed = part.parse::<u64>().ok()?;
        value += parsed.saturating_mul(multiplier);
        multiplier = multiplier.saturating_mul(60);
    }
    Some(value)
}

fn parse_number(s: &str) -> f64 {
    let mut buf = String::with_capacity(s.len());
    for ch in s.chars() {
        if ch.is_ascii_digit() || matches!(ch, '.' | '+' | '-') {
            buf.push(ch);
        }
    }
    if buf.is_empty() {
        return 0.0;
    }
    buf.parse::<f64>().unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn build_snapshot(active: bool, duration: &str, damage: &str) -> EncounterSnapshot {
        let encounter = EncounterSummary {
            title: "Test Encounter".into(),
            zone: "Test Zone".into(),
            duration: duration.into(),
            encdps: "1000".into(),
            damage: damage.into(),
            enchps: "0".into(),
            healed: "0".into(),
            is_active: active,
        };
        let row = CombatantRow {
            name: "Alice".into(),
            job: "NIN".into(),
            encdps: 1000.0,
            encdps_str: "1000".into(),
            damage: 1000.0,
            damage_str: damage.into(),
            share: 1.0,
            share_str: "100%".into(),
            enchps: 0.0,
            enchps_str: "0".into(),
            healed: 0.0,
            healed_str: "0".into(),
            heal_share: 0.0,
            heal_share_str: "0%".into(),
            overheal_pct: "0".into(),
            crit: "0".into(),
            dh: "0".into(),
            deaths: "0".into(),
        };
        EncounterSnapshot::new(encounter, vec![row], json!({ "type": "CombatData" }))
    }

    #[test]
    fn duration_parsing_supports_mm_ss() {
        assert_eq!(parse_duration_secs("01:30"), Some(90));
        assert_eq!(parse_duration_secs("1:02:03"), Some(3723));
        assert_eq!(parse_duration_secs("--:--"), None);
    }

    #[test]
    fn rollover_detects_duration_reset() {
        let active = ActiveEncounter::from_snapshot(build_snapshot(true, "01:20", "5000"));
        let incoming = build_snapshot(true, "00:05", "100");
        assert!(should_rollover(&active, &incoming));
    }

    #[test]
    fn rollover_ignores_inactive_duration_reset() {
        let active = ActiveEncounter::from_snapshot(build_snapshot(true, "01:20", "5000"));
        let incoming = build_snapshot(false, "00:00", "5000");
        assert!(!should_rollover(&active, &incoming));
    }

    #[test]
    fn rollover_ignores_title_change_mid_fight() {
        let active = ActiveEncounter::from_snapshot(build_snapshot(true, "01:20", "5000"));
        let mut incoming = build_snapshot(true, "01:21", "5200");
        incoming.encounter.title = "Renamed Encounter".into();
        incoming.encounter.zone = "Updated Zone".into();
        assert!(!should_rollover(&active, &incoming));
    }

    #[test]
    fn encounter_record_preserves_all_frames() {
        let mut active = ActiveEncounter::from_snapshot(build_snapshot(true, "00:01", "100"));
        active.update(build_snapshot(true, "00:02", "200"));
        active.update(build_snapshot(false, "00:02", "200"));
        let record = EncounterRecord::from_active(active);
        assert_eq!(record.snapshots, 3);
        assert_eq!(record.frames.len(), 3);
        assert!(record.frames.first().unwrap().encounter.is_active);
        assert!(!record.frames.last().unwrap().encounter.is_active);
    }

    #[test]
    fn snapshot_activity_detects_idle_state() {
        let idle = EncounterSnapshot::new(
            EncounterSummary {
                title: String::new(),
                zone: String::new(),
                duration: "00:00".into(),
                encdps: "0".into(),
                damage: "0".into(),
                enchps: "0".into(),
                healed: "0".into(),
                is_active: false,
            },
            Vec::new(),
            json!({ "type": "CombatData" }),
        );
        assert!(!snapshot_has_activity(&idle));

        let mut tick = idle.clone();
        tick.encounter.encdps = "15".into();
        assert!(snapshot_has_activity(&tick));
    }

    #[test]
    fn parse_number_handles_commas_and_percent() {
        assert_eq!(parse_number("12,345.6"), 12345.6);
        assert_eq!(parse_number("98%"), 98.0);
    }
}
