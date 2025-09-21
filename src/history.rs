use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use chrono::{DateTime, Local, TimeZone};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::mpsc;
use tokio::task;

use crate::config;
use crate::model::{CombatantRow, EncounterSummary};

const ENCOUNTER_NAMESPACE: &str = "enc";
const KEY_SEPARATOR: u8 = 0x1F;
const SCHEMA_VERSION: u32 = 1;
const META_SCHEMA_VERSION_KEY: &[u8] = b"schema/version";

/// Snapshot prepared for persistence; keeps the raw payload around for future use.
#[derive(Debug, Clone)]
pub struct EncounterSnapshot {
    pub encounter: EncounterSummary,
    pub rows: Vec<CombatantRow>,
    pub raw: Value,
    pub received_ms: u64,
}

impl EncounterSnapshot {
    pub fn new(encounter: EncounterSummary, rows: Vec<CombatantRow>, raw: Value) -> Self {
        Self {
            encounter,
            rows,
            raw,
            received_ms: now_ms(),
        }
    }
}

/// Opaque key wrapper whose encoded representation sorts by timestamp, then discriminator.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct HistoryKey {
    namespace: String,
    timestamp_ms: u64,
    discriminator: u64,
}

impl HistoryKey {
    pub fn new(namespace: impl Into<String>, timestamp_ms: u64, discriminator: u64) -> Self {
        Self {
            namespace: namespace.into(),
            timestamp_ms,
            discriminator,
        }
    }

    pub fn as_bytes(&self) -> Vec<u8> {
        encode_key(&self.namespace, self.timestamp_ms, self.discriminator)
    }

    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        decode_key(bytes)
    }

    #[allow(dead_code)]
    pub fn prefix(namespace: &str) -> Vec<u8> {
        let mut buf = Vec::with_capacity(namespace.len() + 1);
        buf.extend_from_slice(namespace.as_bytes());
        buf.push(KEY_SEPARATOR);
        buf
    }
}

/// Data captured for each concluded encounter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncounterRecord {
    pub version: u32,
    pub stored_ms: u64,
    pub first_seen_ms: u64,
    pub last_seen_ms: u64,
    pub encounter: EncounterSummary,
    pub rows: Vec<CombatantRow>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_last: Option<Value>,
    #[serde(default)]
    pub snapshots: u32,
    #[serde(default)]
    pub saw_active: bool,
}

impl EncounterRecord {
    fn new(active: ActiveEncounter) -> Self {
        Self {
            version: SCHEMA_VERSION,
            stored_ms: now_ms(),
            first_seen_ms: active.first_seen_ms,
            last_seen_ms: active.last_seen_ms,
            encounter: active.latest_summary,
            rows: active.latest_rows,
            raw_last: Some(active.last_raw),
            snapshots: active.snapshot_count,
            saw_active: active.saw_active,
        }
    }
}

#[derive(Debug, Clone)]
struct StoredEncounter {
    key: HistoryKey,
    record: EncounterRecord,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HistoryEncounterItem {
    pub key: Vec<u8>,
    pub display_title: String,
    pub base_title: String,
    pub occurrence: u32,
    pub time_label: String,
    pub last_seen_ms: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HistoryDay {
    pub iso_date: String,
    pub label: String,
    pub encounter_count: usize,
    #[serde(default)]
    pub encounters: Vec<HistoryEncounterItem>,
}

/// Thin wrapper around the sled database.
pub struct HistoryStore {
    encounters: sled::Tree,
    meta: sled::Tree,
    db: sled::Db,
    root: PathBuf,
}

impl HistoryStore {
    pub const ENCOUNTERS_TREE: &'static str = "encounters";
    pub const META_TREE: &'static str = "meta";

    pub fn open(path: &Path) -> Result<Self> {
        let db = sled::open(path)
            .with_context(|| format!("Failed to open history database at {}", path.display()))?;
        let encounters = db
            .open_tree(Self::ENCOUNTERS_TREE)
            .context("Unable to open encounters history tree")?;
        let meta = db
            .open_tree(Self::META_TREE)
            .context("Unable to open history metadata tree")?;
        let store = Self {
            encounters,
            meta,
            db,
            root: path.to_path_buf(),
        };
        store.init_schema()?;
        Ok(store)
    }

    pub fn open_default() -> Result<Self> {
        let path = config::history_db_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("Unable to create history directory {}", parent.display())
            })?;
        }
        Self::open(&path)
    }

    pub fn append(&self, record: &EncounterRecord) -> Result<HistoryKey> {
        let timestamp = record.last_seen_ms;
        let discriminator = self
            .db
            .generate_id()
            .context("Failed to generate sled identifier for encounter key")?;
        let key = HistoryKey::new(ENCOUNTER_NAMESPACE, timestamp, discriminator);
        let bytes = serde_cbor::to_vec(record).context("Failed to serialize encounter record")?;
        self.encounters
            .insert(key.as_bytes(), bytes)
            .context("Failed to persist encounter record")?;
        Ok(key)
    }

    #[allow(dead_code)]
    pub fn remove(&self, key: &HistoryKey) -> Result<()> {
        self.encounters
            .remove(key.as_bytes())
            .context("Failed to delete encounter record")?;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn tree(&self, name: &str) -> Result<sled::Tree> {
        self.db
            .open_tree(name)
            .with_context(|| format!("Unable to open history tree {name}"))
    }

    pub fn load_days(&self) -> Result<Vec<HistoryDay>> {
        let stored = self.read_all()?;
        Ok(group_by_day(stored))
    }

    fn read_all(&self) -> Result<Vec<StoredEncounter>> {
        let mut records = Vec::new();
        for entry in self.encounters.iter() {
            let (key_bytes, value_bytes) = entry.context("Failed to iterate encounters history")?;
            let key = match HistoryKey::from_bytes(key_bytes.as_ref()) {
                Some(k) => k,
                None => continue,
            };
            let record: EncounterRecord = serde_cbor::from_slice(value_bytes.as_ref())
                .context("Failed to deserialize encounter record")?;
            records.push(StoredEncounter { key, record });
        }
        Ok(records)
    }

    fn init_schema(&self) -> Result<()> {
        match self
            .meta
            .get(META_SCHEMA_VERSION_KEY)
            .context("Failed to read schema version from history metadata")?
        {
            Some(bytes) if bytes.len() == 4 => {
                let mut arr = [0u8; 4];
                arr.copy_from_slice(&bytes);
                let version = u32::from_be_bytes(arr);
                if version != SCHEMA_VERSION {
                    eprintln!(
                        "Warning: history schema version mismatch (stored: {}, expected: {})",
                        version, SCHEMA_VERSION
                    );
                }
            }
            Some(bytes) => {
                eprintln!(
                    "Warning: history schema version entry had unexpected size: {} bytes",
                    bytes.len()
                );
            }
            None => {
                let version_bytes = SCHEMA_VERSION.to_be_bytes();
                self.meta
                    .insert(META_SCHEMA_VERSION_KEY, &version_bytes)
                    .context("Failed to initialize history schema version")?;
            }
        }
        Ok(())
    }

    #[allow(dead_code)]
    pub fn root(&self) -> &Path {
        &self.root
    }
}

/// Handle used by producers to send snapshots to the recorder task.
#[derive(Clone)]
pub struct RecorderHandle {
    tx: mpsc::UnboundedSender<RecorderMessage>,
}

impl RecorderHandle {
    pub fn record(&self, snapshot: EncounterSnapshot) {
        let _ = self.tx.send(RecorderMessage::Snapshot(Box::new(snapshot)));
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
        let _ = self.tx.send(RecorderMessage::Flush);
    }

    pub fn shutdown(&self) {
        let _ = self.tx.send(RecorderMessage::Shutdown);
    }
}

enum RecorderMessage {
    Snapshot(Box<EncounterSnapshot>),
    Flush,
    Shutdown,
}

pub fn spawn_recorder(store: Arc<HistoryStore>) -> RecorderHandle {
    let (tx, mut rx) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        let mut worker = RecorderWorker::new(store);
        while let Some(msg) = rx.recv().await {
            match msg {
                RecorderMessage::Snapshot(snapshot) => worker.on_snapshot(*snapshot).await,
                RecorderMessage::Flush => worker.on_flush().await,
                RecorderMessage::Shutdown => {
                    worker.on_flush().await;
                    break;
                }
            }
        }
    });
    RecorderHandle { tx }
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
            let record = EncounterRecord::new(active);
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
    snapshot_count: u32,
    saw_active: bool,
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
        Self {
            first_seen_ms: received_ms,
            last_seen_ms: received_ms,
            latest_summary: encounter,
            latest_rows: rows,
            last_raw: raw,
            snapshot_count: 1,
            saw_active: is_active,
        }
    }

    fn update(&mut self, snapshot: EncounterSnapshot) {
        self.last_seen_ms = snapshot.received_ms;
        self.latest_summary = snapshot.encounter;
        self.latest_rows = snapshot.rows;
        self.last_raw = snapshot.raw;
        self.snapshot_count = self.snapshot_count.saturating_add(1);
        self.saw_active |= self.latest_summary.is_active;
    }
}

fn should_rollover(active: &ActiveEncounter, incoming: &EncounterSnapshot) -> bool {
    let previous = &active.latest_summary;
    let next = &incoming.encounter;

    if !active.saw_active && next.is_active {
        return true;
    }

    if next.is_active && previous.title != next.title && !next.title.is_empty() {
        return true;
    }

    if next.is_active && previous.zone != next.zone && !next.zone.is_empty() {
        return true;
    }

    if let (Some(prev_secs), Some(next_secs)) = (
        parse_duration_secs(&previous.duration),
        parse_duration_secs(&next.duration),
    ) {
        if next_secs + 2 < prev_secs {
            return true;
        }
        if prev_secs > 10 && next_secs == 0 && next.is_active {
            return true;
        }
    }

    if next.is_active {
        let prev_damage = parse_number(&previous.damage);
        let next_damage = parse_number(&next.damage);
        if next_damage + 1.0 < prev_damage {
            return true;
        }
    }

    false
}

fn encode_key(namespace: &str, timestamp_ms: u64, discriminator: u64) -> Vec<u8> {
    let mut buf = Vec::with_capacity(namespace.len() + 1 + 8 + 1 + 8);
    buf.extend_from_slice(namespace.as_bytes());
    buf.push(KEY_SEPARATOR);
    buf.extend_from_slice(&timestamp_ms.to_be_bytes());
    buf.push(KEY_SEPARATOR);
    buf.extend_from_slice(&discriminator.to_be_bytes());
    buf
}

fn decode_key(bytes: &[u8]) -> Option<HistoryKey> {
    let mut parts = bytes.split(|b| *b == KEY_SEPARATOR);
    let namespace = parts.next()?;
    let timestamp_bytes = parts.next()?;
    let discriminator_bytes = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    if timestamp_bytes.len() != 8 || discriminator_bytes.len() != 8 {
        return None;
    }
    let mut ts_arr = [0u8; 8];
    ts_arr.copy_from_slice(timestamp_bytes);
    let mut disc_arr = [0u8; 8];
    disc_arr.copy_from_slice(discriminator_bytes);
    Some(HistoryKey {
        namespace: String::from_utf8(namespace.to_vec()).ok()?,
        timestamp_ms: u64::from_be_bytes(ts_arr),
        discriminator: u64::from_be_bytes(disc_arr),
    })
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

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_millis() as u64
}

fn group_by_day(records: Vec<StoredEncounter>) -> Vec<HistoryDay> {
    let mut days: BTreeMap<String, DayBucket> = BTreeMap::new();

    for stored in records {
        let dt = match millis_to_local(stored.record.last_seen_ms) {
            Some(dt) => dt,
            None => continue,
        };
        let date = dt.date_naive();
        let iso_date = date.to_string();
        let day_label = dt.format("%Y-%m-%d (%a)").to_string();

        let bucket = days
            .entry(iso_date.clone())
            .or_insert_with(|| DayBucket::new(iso_date.clone(), day_label));

        let base_title = resolve_title(&stored.record);
        let time_label = dt.format("%H:%M").to_string();

        bucket.entries.push(DayEncounter {
            key: stored.key,
            base_title,
            time_label,
            last_seen_ms: stored.record.last_seen_ms,
        });
    }

    let mut days_vec: Vec<HistoryDay> = days
        .into_values()
        .map(DayBucket::into_history_day)
        .collect();

    days_vec.sort_by(|a, b| b.iso_date.cmp(&a.iso_date));
    days_vec
}

fn resolve_title(record: &EncounterRecord) -> String {
    let primary = record.encounter.title.trim();
    if !primary.is_empty() {
        return primary.to_string();
    }
    let zone = record.encounter.zone.trim();
    if !zone.is_empty() {
        return zone.to_string();
    }
    "Unknown Encounter".to_string()
}

fn millis_to_local(ms: u64) -> Option<DateTime<Local>> {
    let millis = i64::try_from(ms).ok()?;
    Local.timestamp_millis_opt(millis).single()
}

struct DayBucket {
    iso_date: String,
    label: String,
    entries: Vec<DayEncounter>,
}

impl DayBucket {
    fn new(iso_date: String, label: String) -> Self {
        Self {
            iso_date,
            label,
            entries: Vec::new(),
        }
    }

    fn into_history_day(mut self) -> HistoryDay {
        self.entries.sort_by_key(|entry| entry.last_seen_ms);

        let mut totals: HashMap<String, u32> = HashMap::new();
        for entry in &self.entries {
            *totals.entry(entry.base_title.clone()).or_insert(0) += 1;
        }

        let mut occurrences: HashMap<String, u32> = HashMap::new();
        let encounters = self
            .entries
            .into_iter()
            .map(|entry| {
                let total = totals.get(&entry.base_title).copied().unwrap_or(1);
                let counter = occurrences.entry(entry.base_title.clone()).or_insert(0);
                *counter += 1;
                let occurrence = *counter;
                let display_title = if total > 1 {
                    format!("{} ({})", entry.base_title, occurrence)
                } else {
                    entry.base_title.clone()
                };
                HistoryEncounterItem {
                    key: entry.key.as_bytes(),
                    display_title,
                    base_title: entry.base_title,
                    occurrence,
                    time_label: entry.time_label,
                    last_seen_ms: entry.last_seen_ms,
                }
            })
            .collect::<Vec<_>>();

        HistoryDay {
            iso_date: self.iso_date,
            label: format!("{} · {} encounters", self.label, encounters.len()),
            encounter_count: encounters.len(),
            encounters,
        }
    }
}

struct DayEncounter {
    key: HistoryKey,
    base_title: String,
    time_label: String,
    last_seen_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
    fn key_roundtrip() {
        let key = HistoryKey::new("enc", 12345, 42);
        let encoded = key.as_bytes();
        let decoded = HistoryKey::from_bytes(&encoded).expect("decode key");
        assert_eq!(decoded.namespace, "enc");
        assert_eq!(decoded.timestamp_ms, 12345);
        assert_eq!(decoded.discriminator, 42);
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
    fn parse_number_handles_commas_and_percent() {
        assert_eq!(parse_number("12,345.6"), 12345.6);
        assert_eq!(parse_number("98%"), 98.0);
    }

    fn stored_encounter(ts: u64, title: &str) -> StoredEncounter {
        let encounter = EncounterSummary {
            title: title.into(),
            zone: String::new(),
            duration: "00:30".into(),
            encdps: "0".into(),
            damage: "0".into(),
            enchps: "0".into(),
            healed: "0".into(),
            is_active: false,
        };
        let record = EncounterRecord {
            version: SCHEMA_VERSION,
            stored_ms: ts,
            first_seen_ms: ts,
            last_seen_ms: ts,
            encounter,
            rows: Vec::new(),
            raw_last: None,
            snapshots: 1,
            saw_active: false,
        };
        let key = HistoryKey::new(ENCOUNTER_NAMESPACE, ts, ts);
        StoredEncounter { key, record }
    }

    #[test]
    fn group_by_day_numbers_duplicate_titles() {
        let records = vec![
            stored_encounter(1_000, "Doma Castle"),
            stored_encounter(2_000, "Doma Castle"),
            stored_encounter(3_000, "Striking Dummy"),
        ];
        let days = group_by_day(records);
        assert_eq!(days.len(), 1);
        let day = &days[0];
        assert_eq!(day.encounters.len(), 3);
        assert_eq!(day.encounters[0].display_title, "Doma Castle (1)");
        assert_eq!(day.encounters[1].display_title, "Doma Castle (2)");
        assert_eq!(day.encounters[2].display_title, "Striking Dummy");
    }
}
