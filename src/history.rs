use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use chrono::{DateTime, Local, NaiveDate, TimeZone};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio::task;

use crate::config;
use crate::model::{CombatantRow, EncounterSummary};

const ENCOUNTER_NAMESPACE: &str = "enc";
const KEY_SEPARATOR: u8 = 0x1F;
const SCHEMA_VERSION: u32 = 2;
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

    #[allow(dead_code)]
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
    #[serde(default)]
    pub frames: Vec<EncounterFrame>,
}

impl EncounterRecord {
    fn new(active: ActiveEncounter) -> Self {
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
            version: SCHEMA_VERSION,
            stored_ms: now_ms(),
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncounterFrame {
    pub received_ms: u64,
    pub encounter: EncounterSummary,
    pub rows: Vec<CombatantRow>,
    pub raw: Value,
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HistoryEncounterItem {
    pub key: Vec<u8>,
    pub display_title: String,
    pub base_title: String,
    pub occurrence: u32,
    pub time_label: String,
    pub last_seen_ms: u64,
    pub timestamp_label: String,
    #[serde(default)]
    pub record: Option<EncounterRecord>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HistoryDay {
    pub iso_date: String,
    pub label: String,
    pub encounter_count: usize,
    #[serde(default)]
    pub encounters: Vec<HistoryEncounterItem>,
    #[serde(default)]
    pub encounter_ids: Vec<Vec<u8>>,
    #[serde(default)]
    pub encounters_loaded: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncounterSummaryRecord {
    pub key: Vec<u8>,
    pub date_id: String,
    pub base_title: String,
    pub encounter_title: String,
    pub time_label: String,
    pub timestamp_label: String,
    pub last_seen_ms: u64,
    pub duration: String,
    pub encdps: String,
    pub damage: String,
    pub zone: String,
    pub snapshots: u32,
    pub frames: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DateSummaryRecord {
    pub date_id: String,
    pub last_seen_ms: u64,
    pub encounter_ids: Vec<Vec<u8>>,
}

/// Thin wrapper around the sled database.
pub struct HistoryStore {
    encounters: sled::Tree,
    encounter_summaries: sled::Tree,
    date_index: sled::Tree,
    meta: sled::Tree,
    db: sled::Db,
    root: PathBuf,
}

impl HistoryStore {
    pub const ENCOUNTERS_TREE: &'static str = "encounters";
    pub const ENCOUNTER_SUMMARIES_TREE: &'static str = "enc_summaries";
    pub const DATES_TREE: &'static str = "dates";
    pub const META_TREE: &'static str = "meta";

    pub fn open(path: &Path) -> Result<Self> {
        let db = sled::open(path)
            .with_context(|| format!("Failed to open history database at {}", path.display()))?;
        let encounters = db
            .open_tree(Self::ENCOUNTERS_TREE)
            .context("Unable to open encounters history tree")?;
        let encounter_summaries = db
            .open_tree(Self::ENCOUNTER_SUMMARIES_TREE)
            .context("Unable to open encounter summaries history tree")?;
        let date_index = db
            .open_tree(Self::DATES_TREE)
            .context("Unable to open history date index tree")?;
        let meta = db
            .open_tree(Self::META_TREE)
            .context("Unable to open history metadata tree")?;
        let store = Self {
            encounters,
            encounter_summaries,
            date_index,
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
        let key_bytes = key.as_bytes();
        let bytes = serde_cbor::to_vec(record).context("Failed to serialize encounter record")?;
        self.encounters
            .insert(key_bytes.as_slice(), bytes)
            .context("Failed to persist encounter record")?;

        let summary = self.build_encounter_summary(&key_bytes, record);
        let summary_bytes =
            serde_cbor::to_vec(&summary).context("Failed to serialize encounter summary")?;
        self.encounter_summaries
            .insert(key_bytes.as_slice(), summary_bytes)
            .context("Failed to persist encounter summary")?;

        self.update_date_summary(&summary)
            .context("Failed to update date summary")?;
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

    fn build_encounter_summary(
        &self,
        key: &[u8],
        record: &EncounterRecord,
    ) -> EncounterSummaryRecord {
        let date_time = millis_to_local(record.last_seen_ms);
        let (date_id, time_label, timestamp_label) = match date_time {
            Some(dt) => (
                dt.date_naive().to_string(),
                dt.format("%H:%M").to_string(),
                dt.format("%Y-%m-%d %H:%M:%S").to_string(),
            ),
            None => (
                "unknown".to_string(),
                "--:--".to_string(),
                "unknown".to_string(),
            ),
        };

        let base_title = resolve_title(record);

        EncounterSummaryRecord {
            key: key.to_vec(),
            date_id,
            base_title,
            encounter_title: record.encounter.title.clone(),
            time_label,
            timestamp_label,
            last_seen_ms: record.last_seen_ms,
            duration: record.encounter.duration.clone(),
            encdps: record.encounter.encdps.clone(),
            damage: record.encounter.damage.clone(),
            zone: record.encounter.zone.clone(),
            snapshots: record.snapshots,
            frames: record.frames.len() as u32,
        }
    }

    fn update_date_summary(&self, summary: &EncounterSummaryRecord) -> Result<()> {
        let key = summary.date_id.as_bytes();
        let existing = self
            .date_index
            .get(key)
            .context("Failed to read date summary")?;

        let record = if let Some(bytes) = existing {
            let mut record: DateSummaryRecord =
                serde_cbor::from_slice(&bytes).context("Failed to deserialize date summary")?;
            if !record
                .encounter_ids
                .iter()
                .any(|existing_key| existing_key == &summary.key)
            {
                record.encounter_ids.insert(0, summary.key.clone());
            }
            if summary.last_seen_ms > record.last_seen_ms {
                record.last_seen_ms = summary.last_seen_ms;
            }
            record
        } else {
            DateSummaryRecord {
                date_id: summary.date_id.clone(),
                last_seen_ms: summary.last_seen_ms,
                encounter_ids: vec![summary.key.clone()],
            }
        };

        let bytes =
            serde_cbor::to_vec(&record).context("Failed to serialize updated date summary")?;
        self.date_index
            .insert(key, bytes)
            .context("Failed to persist date summary")?;
        Ok(())
    }

    pub fn load_dates(&self) -> Result<Vec<HistoryDay>> {
        let mut days = Vec::new();
        for entry in self.date_index.iter() {
            let (key_bytes, value_bytes) = entry.context("Failed to iterate history date index")?;
            let record: DateSummaryRecord = serde_cbor::from_slice(value_bytes.as_ref())
                .context("Failed to deserialize date summary")?;
            let iso_date = String::from_utf8(key_bytes.to_vec()).unwrap_or(record.date_id.clone());
            let label = format_date_label(&iso_date, record.encounter_ids.len());
            days.push(HistoryDay {
                iso_date,
                label,
                encounter_count: record.encounter_ids.len(),
                encounters: Vec::new(),
                encounter_ids: record.encounter_ids,
                encounters_loaded: false,
            });
        }
        days.sort_by(|a, b| b.iso_date.cmp(&a.iso_date));
        Ok(days)
    }

    pub fn load_encounter_summaries(&self, date_id: &str) -> Result<Vec<HistoryEncounterItem>> {
        let key = date_id.as_bytes();
        let Some(bytes) = self
            .date_index
            .get(key)
            .context("Failed to read date summary for encounters")?
        else {
            return Ok(Vec::new());
        };

        let date_summary: DateSummaryRecord =
            serde_cbor::from_slice(bytes.as_ref()).context("Failed to deserialize date summary")?;

        let mut summaries = Vec::new();
        for encounter_id in &date_summary.encounter_ids {
            if let Some(bytes) = self
                .encounter_summaries
                .get(encounter_id)
                .context("Failed to read encounter summary")?
            {
                let summary: EncounterSummaryRecord = serde_cbor::from_slice(bytes.as_ref())
                    .context("Failed to deserialize encounter summary")?;
                summaries.push(summary);
            }
        }

        summaries.sort_by(|a, b| b.last_seen_ms.cmp(&a.last_seen_ms));

        Ok(build_history_items_from_summaries(summaries))
    }

    pub fn load_encounter_record(&self, key: &[u8]) -> Result<EncounterRecord> {
        let Some(bytes) = self
            .encounters
            .get(key)
            .context("Failed to read encounter record")?
        else {
            anyhow::bail!("Encounter record not found");
        };
        serde_cbor::from_slice(bytes.as_ref()).context("Failed to deserialize encounter record")
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

fn encode_key(namespace: &str, timestamp_ms: u64, discriminator: u64) -> Vec<u8> {
    let mut buf = Vec::with_capacity(namespace.len() + 1 + 8 + 1 + 8);
    buf.extend_from_slice(namespace.as_bytes());
    buf.push(KEY_SEPARATOR);
    buf.extend_from_slice(&timestamp_ms.to_be_bytes());
    buf.push(KEY_SEPARATOR);
    buf.extend_from_slice(&discriminator.to_be_bytes());
    buf
}

#[allow(dead_code)]
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

fn format_date_label(iso_date: &str, encounter_count: usize) -> String {
    match NaiveDate::parse_from_str(iso_date, "%Y-%m-%d") {
        Ok(date) => {
            let weekday = date.format("%a");
            format!(
                "{} ({}) · {} encounters",
                iso_date, weekday, encounter_count
            )
        }
        Err(_) => format!("{} · {} encounters", iso_date, encounter_count),
    }
}

fn build_history_items_from_summaries(
    summaries: Vec<EncounterSummaryRecord>,
) -> Vec<HistoryEncounterItem> {
    let mut totals: HashMap<String, u32> = HashMap::new();
    for summary in &summaries {
        *totals.entry(summary.base_title.clone()).or_insert(0) += 1;
    }

    let mut occurrences: HashMap<String, u32> = HashMap::new();
    summaries
        .into_iter()
        .map(|summary| {
            let total = totals.get(&summary.base_title).copied().unwrap_or(1);
            let counter = occurrences.entry(summary.base_title.clone()).or_insert(0);
            *counter += 1;
            let occurrence = *counter;
            let display_title = if total > 1 {
                format!("{} ({})", summary.base_title.as_str(), occurrence)
            } else {
                summary.base_title.clone()
            };
            HistoryEncounterItem {
                key: summary.key,
                display_title,
                base_title: summary.base_title,
                occurrence,
                time_label: summary.time_label,
                last_seen_ms: summary.last_seen_ms,
                timestamp_label: summary.timestamp_label,
                record: None,
            }
        })
        .collect()
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
        let record = EncounterRecord::new(active);
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

    fn make_summary(key: &[u8], base_title: &str, last_seen: u64) -> EncounterSummaryRecord {
        EncounterSummaryRecord {
            key: key.to_vec(),
            date_id: "2025-01-01".into(),
            base_title: base_title.into(),
            encounter_title: base_title.into(),
            time_label: "12:00".into(),
            timestamp_label: "2025-01-01 12:00:00".into(),
            last_seen_ms: last_seen,
            duration: "00:30".into(),
            encdps: "1000".into(),
            damage: "100000".into(),
            zone: "Zone".into(),
            snapshots: 3,
            frames: 3,
        }
    }

    #[test]
    fn build_history_items_numbers_duplicate_titles() {
        let summaries = vec![
            make_summary(&[1], "Doma Castle", 2_000),
            make_summary(&[2], "Doma Castle", 3_000),
            make_summary(&[3], "Striking Dummy", 1_000),
        ];
        let items = build_history_items_from_summaries(summaries);
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].display_title, "Doma Castle (1)");
        assert_eq!(items[1].display_title, "Doma Castle (2)");
        assert_eq!(items[2].display_title, "Striking Dummy");
        assert!(items.iter().all(|item| item.record.is_none()));
    }
}
