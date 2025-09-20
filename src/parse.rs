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
    let enc_enchps = get_ci(&enc_obj, "enchps")
        .or_else(|| get_ci(&enc_obj, "ENCHPS"))
        .map(val_to_string)
        .unwrap_or_default();
    let enc_healed = get_ci(&enc_obj, "healed")
        .map(val_to_string)
        .unwrap_or_default();

    let is_active = obj
        .get("isActive")
        .and_then(|v| v.as_str())
        .map(|s| s.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    let encounter = EncounterSummary {
        title: enc_title,
        zone: enc_zone,
        duration: enc_duration,
        encdps: enc_encdps,
        damage: enc_damage.clone(),
        enchps: enc_enchps,
        healed: enc_healed,
        is_active,
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
        // total damage per combatant
        let damage_s = get_ci(s, "damage")
            .or_else(|| get_ci(s, "Damage"))
            .map(val_to_string)
            .unwrap_or_else(|| "0".into());
        let damage = to_f64_any(&damage_s);
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

        // Healing stats
        let enchps_s = get_ci(s, "enchps")
            .or_else(|| get_ci(s, "ENCHPS"))
            .map(val_to_string)
            .unwrap_or_else(|| "0".into());
        let enchps = to_f64_any(&enchps_s);
        let healed_s = get_ci(s, "healed")
            .map(val_to_string)
            .unwrap_or_else(|| "0".into());
        let healed = to_f64_any(&healed_s);
        let overheal_pct = get_ci(s, "OverHealPct")
            .map(val_to_string)
            .unwrap_or_default();

        rows.push(CombatantRow {
            name,
            job: job_up,
            encdps,
            encdps_str: encdps_s,
            damage,
            damage_str: damage_s,
            share: 0.0,
            share_str: String::new(),
            enchps,
            enchps_str: enchps_s,
            healed,
            healed_str: healed_s,
            heal_share: 0.0,
            heal_share_str: String::new(),
            overheal_pct,
            crit,
            dh,
            deaths,
        });
    }

    // Determine encounter total damage and compute shares
    let mut total_damage = to_f64_any(&enc_damage);
    if total_damage <= 0.0 {
        total_damage = rows.iter().map(|r| r.damage).sum::<f64>();
    }
    if total_damage > 0.0 {
        for r in &mut rows {
            // Prefer server-provided damage% when available
            if let Some(pct_val) = obj
                .get("Combatant")
                .and_then(|c| c.get(&r.name))
                .and_then(|v| v.as_object())
                .and_then(|m| get_ci(m, "damage%"))
            {
                let pct = to_f64_any(val_to_string(pct_val));
                r.share = (pct / 100.0).clamp(0.0, 1.0);
            } else {
                r.share = (r.damage / total_damage).clamp(0.0, 1.0);
            }
            r.share_str = format!("{:.1}%", r.share * 100.0);
        }
    } else {
        for r in &mut rows {
            r.share = 0.0;
            r.share_str = "0.0%".into();
        }
    }

    // Healing totals and shares
    let mut total_healed = to_f64_any(&encounter.healed);
    if total_healed <= 0.0 {
        total_healed = rows.iter().map(|r| r.healed).sum::<f64>();
    }
    if total_healed > 0.0 {
        for r in &mut rows {
            // Prefer server-provided healed% when available
            if let Some(pct_val) = obj
                .get("Combatant")
                .and_then(|c| c.get(&r.name))
                .and_then(|v| v.as_object())
                .and_then(|m| get_ci(m, "healed%"))
            {
                let pct = to_f64_any(val_to_string(pct_val));
                r.heal_share = (pct / 100.0).clamp(0.0, 1.0);
            } else {
                r.heal_share = (r.healed / total_healed).clamp(0.0, 1.0);
            }
            r.heal_share_str = format!("{:.1}%", r.heal_share * 100.0);
        }
    } else {
        for r in &mut rows {
            r.heal_share = 0.0;
            r.heal_share_str = "0.0%".into();
        }
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
