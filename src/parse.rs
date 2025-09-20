use regex::Regex;
use serde_json::{Map, Value};

use crate::model::{known_jobs, CombatantRow, EncounterSummary};

fn get_ci<'a>(obj: &'a Map<String, Value>, key: &str) -> Option<&'a Value> {
    if let Some(v) = obj.get(key) {
        return Some(v);
    }
    let lkey = key.to_lowercase();
    obj.iter()
        .find(|(k, _)| k.to_lowercase() == lkey)
        .map(|(_, v)| v)
}

fn val_to_string(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        _ => v.to_string(),
    }
}

fn clean_number_str(s: &str) -> String {
    // Keep digits, dot, plus, minus
    static RE: once_cell::sync::Lazy<Regex> =
        once_cell::sync::Lazy::new(|| Regex::new(r"[^0-9.+-]").unwrap());
    RE.replace_all(s, "").into_owned()
}

fn to_f64_any<S: AsRef<str>>(s: S) -> f64 {
    let cleaned = clean_number_str(s.as_ref());
    if cleaned.is_empty() {
        return 0.0;
    }
    cleaned.parse::<f64>().unwrap_or(0.0)
}

fn upper<S: AsRef<str>>(s: S) -> String {
    s.as_ref().to_uppercase()
}

pub fn parse_combat_data(value: &Value) -> Option<(EncounterSummary, Vec<CombatantRow>)> {
    let obj = value.as_object()?;
    if obj.get("type")?.as_str()? != "CombatData" {
        return None;
    }

    // Encounter summary
    let enc_obj = obj
        .get("Encounter")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();
    let enc_title = enc_obj
        .get("title")
        .or_else(|| get_ci(&enc_obj, "Encounter"))
        .map(val_to_string)
        .unwrap_or_default();
    let enc_zone = get_ci(&enc_obj, "CurrentZoneName")
        .map(val_to_string)
        .unwrap_or_default();
    let enc_duration = get_ci(&enc_obj, "duration")
        .map(val_to_string)
        .unwrap_or_default();
    let enc_encdps = get_ci(&enc_obj, "encdps")
        .or_else(|| get_ci(&enc_obj, "ENCDPS"))
        .or_else(|| get_ci(&enc_obj, "DPS"))
        .map(val_to_string)
        .unwrap_or_default();
    let enc_damage = get_ci(&enc_obj, "damage")
        .or_else(|| get_ci(&enc_obj, "damageTotal"))
        .map(val_to_string)
        .unwrap_or_default();

    let encounter = EncounterSummary {
        title: enc_title,
        zone: enc_zone,
        duration: enc_duration,
        encdps: enc_encdps,
        damage: enc_damage,
    };

    // Combatants → rows
    let mut rows: Vec<CombatantRow> = Vec::new();
    let comb = obj
        .get("Combatant")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();
    for (name, stats_v) in comb.into_iter() {
        let s = match stats_v.as_object() {
            Some(s) => s,
            None => continue,
        };
        // Job filter (party-only via known job codes)
        let job = get_ci(s, "Job").map(val_to_string).unwrap_or_default();
        let job_up = upper(&job);
        if !known_jobs().contains(job_up.as_str()) {
            continue;
        }

        let encdps_s = get_ci(s, "encdps")
            .or_else(|| get_ci(s, "ENCDPS"))
            .or_else(|| get_ci(s, "dps"))
            .map(val_to_string)
            .unwrap_or_else(|| "0".into());
        let encdps = to_f64_any(&encdps_s);
        let crit = get_ci(s, "crithit%")
            .or_else(|| get_ci(s, "Crit%"))
            .or_else(|| get_ci(s, "crithit"))
            .map(val_to_string)
            .unwrap_or_default();
        let dh = get_ci(s, "DirectHitPct")
            .or_else(|| get_ci(s, "DirectHit%"))
            .or_else(|| get_ci(s, "DirectHit"))
            .or_else(|| get_ci(s, "Direct%"))
            .or_else(|| get_ci(s, "DH%"))
            .map(val_to_string)
            .unwrap_or_default();
        let deaths = get_ci(s, "deaths")
            .or_else(|| get_ci(s, "Deaths"))
            .map(val_to_string)
            .unwrap_or_else(|| "0".into());

        rows.push(CombatantRow {
            name,
            job: job_up,
            encdps,
            encdps_str: encdps_s,
            crit,
            dh,
            deaths,
        });
    }

    // Sort by numeric DPS desc, stable by name
    rows.sort_by(|a, b| {
        b.encdps
            .partial_cmp(&a.encdps)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.name.cmp(&b.name))
    });

    Some((encounter, rows))
}
