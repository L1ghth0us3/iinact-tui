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
    let root = value.as_object()?;
    if root.get("type")?.as_str()? != "CombatData" {
        return None;
    }

    let encounter = parse_encounter(root);

    let combatants = root
        .get("Combatant")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();

    let mut rows = combatant_rows(&combatants);

    compute_damage_shares(&mut rows, &combatants, encounter.damage.as_str());
    compute_heal_shares(&mut rows, &combatants, encounter.healed.as_str());

    rows.sort_by(|a, b| {
        b.encdps
            .partial_cmp(&a.encdps)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.name.cmp(&b.name))
    });

    Some((encounter, rows))
}

fn parse_encounter(root: &Map<String, Value>) -> EncounterSummary {
    let enc_obj = root
        .get("Encounter")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();

    let title = enc_obj
        .get("title")
        .or_else(|| get_ci(&enc_obj, "Encounter"))
        .map(val_to_string)
        .unwrap_or_default();
    let zone = get_ci(&enc_obj, "CurrentZoneName")
        .map(val_to_string)
        .unwrap_or_default();
    let duration = get_ci(&enc_obj, "duration")
        .map(val_to_string)
        .unwrap_or_default();
    let encdps = get_ci(&enc_obj, "encdps")
        .or_else(|| get_ci(&enc_obj, "ENCDPS"))
        .or_else(|| get_ci(&enc_obj, "DPS"))
        .map(val_to_string)
        .unwrap_or_default();
    let damage = get_ci(&enc_obj, "damage")
        .or_else(|| get_ci(&enc_obj, "damageTotal"))
        .map(val_to_string)
        .unwrap_or_default();
    let enchps = get_ci(&enc_obj, "enchps")
        .or_else(|| get_ci(&enc_obj, "ENCHPS"))
        .map(val_to_string)
        .unwrap_or_default();
    let healed = get_ci(&enc_obj, "healed")
        .map(val_to_string)
        .unwrap_or_default();

    let is_active = root
        .get("isActive")
        .and_then(|v| v.as_str())
        .map(|s| s.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    EncounterSummary {
        title,
        zone,
        duration,
        encdps,
        damage,
        enchps,
        healed,
        is_active,
    }
}

fn combatant_rows(combatants: &Map<String, Value>) -> Vec<CombatantRow> {
    let mut rows = Vec::new();
    for (name, stats_v) in combatants {
        if let Some(stats) = stats_v.as_object() {
            if let Some(row) = parse_combatant(name, stats) {
                rows.push(row);
            }
        }
    }
    rows
}

fn parse_combatant(name: &str, stats: &Map<String, Value>) -> Option<CombatantRow> {
    let job = get_ci(stats, "Job").map(val_to_string).unwrap_or_default();
    let job_up = upper(&job);
    if !known_jobs().contains(job_up.as_str()) {
        return None;
    }

    let encdps_str = get_ci(stats, "encdps")
        .or_else(|| get_ci(stats, "ENCDPS"))
        .or_else(|| get_ci(stats, "dps"))
        .map(val_to_string)
        .unwrap_or_else(|| "0".into());
    let encdps = to_f64_any(&encdps_str);

    let damage_str = get_ci(stats, "damage")
        .or_else(|| get_ci(stats, "Damage"))
        .map(val_to_string)
        .unwrap_or_else(|| "0".into());
    let damage = to_f64_any(&damage_str);

    let crit = get_ci(stats, "crithit%")
        .or_else(|| get_ci(stats, "Crit%"))
        .or_else(|| get_ci(stats, "crithit"))
        .map(val_to_string)
        .unwrap_or_default();

    let dh = get_ci(stats, "DirectHitPct")
        .or_else(|| get_ci(stats, "DirectHit%"))
        .or_else(|| get_ci(stats, "DirectHit"))
        .or_else(|| get_ci(stats, "Direct%"))
        .or_else(|| get_ci(stats, "DH%"))
        .map(val_to_string)
        .unwrap_or_default();

    let deaths = get_ci(stats, "deaths")
        .or_else(|| get_ci(stats, "Deaths"))
        .map(val_to_string)
        .unwrap_or_else(|| "0".into());

    let enchps_str = get_ci(stats, "enchps")
        .or_else(|| get_ci(stats, "ENCHPS"))
        .map(val_to_string)
        .unwrap_or_else(|| "0".into());
    let enchps = to_f64_any(&enchps_str);

    let healed_str = get_ci(stats, "healed")
        .map(val_to_string)
        .unwrap_or_else(|| "0".into());
    let healed = to_f64_any(&healed_str);

    let overheal_pct = get_ci(stats, "OverHealPct")
        .map(val_to_string)
        .unwrap_or_default();

    Some(CombatantRow {
        name: name.to_string(),
        job: job_up,
        encdps,
        encdps_str,
        damage,
        damage_str,
        share: 0.0,
        share_str: String::new(),
        enchps,
        enchps_str,
        healed,
        healed_str,
        heal_share: 0.0,
        heal_share_str: String::new(),
        overheal_pct,
        crit,
        dh,
        deaths,
    })
}

fn compute_damage_shares(
    rows: &mut [CombatantRow],
    combatants: &Map<String, Value>,
    encounter_damage: &str,
) {
    let mut total_damage = to_f64_any(encounter_damage);
    if total_damage <= 0.0 {
        total_damage = rows.iter().map(|r| r.damage).sum::<f64>();
    }

    if total_damage <= 0.0 {
        for row in rows {
            row.share = 0.0;
            row.share_str = "0.0%".into();
        }
        return;
    }

    for row in rows {
        if let Some(stats) = combatants
            .get(&row.name)
            .and_then(|v| v.as_object())
            .and_then(|m| get_ci(m, "damage%"))
        {
            let pct = to_f64_any(val_to_string(stats));
            row.share = (pct / 100.0).clamp(0.0, 1.0);
        } else {
            row.share = (row.damage / total_damage).clamp(0.0, 1.0);
        }
        row.share_str = format!("{:.1}%", row.share * 100.0);
    }
}

fn compute_heal_shares(
    rows: &mut [CombatantRow],
    combatants: &Map<String, Value>,
    encounter_healed: &str,
) {
    let mut total_healed = to_f64_any(encounter_healed);
    if total_healed <= 0.0 {
        total_healed = rows.iter().map(|r| r.healed).sum::<f64>();
    }

    if total_healed <= 0.0 {
        for row in rows {
            row.heal_share = 0.0;
            row.heal_share_str = "0.0%".into();
        }
        return;
    }

    for row in rows {
        if let Some(stats) = combatants
            .get(&row.name)
            .and_then(|v| v.as_object())
            .and_then(|m| get_ci(m, "healed%"))
        {
            let pct = to_f64_any(val_to_string(stats));
            row.heal_share = (pct / 100.0).clamp(0.0, 1.0);
        } else {
            row.heal_share = (row.healed / total_healed).clamp(0.0, 1.0);
        }
        row.heal_share_str = format!("{:.1}%", row.heal_share * 100.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_basic_combat_data() {
        let payload = json!({
            "type": "CombatData",
            "Encounter": {
                "title": "Dummy",
                "duration": "90",
                "encdps": "2,000",
                "damage": "10,000",
                "enchps": "1,000",
                "healed": "2,000",
                "CurrentZoneName": "Somewhere"
            },
            "Combatant": {
                "Alice": {
                    "Job": "NIN",
                    "encdps": "6,000",
                    "damage": "6,000",
                    "crithit%": "10%",
                    "DirectHit%": "20%",
                    "deaths": "0",
                    "enchps": "100",
                    "healed": "500",
                    "OverHealPct": "5%"
                },
                "Bob": {
                    "Job": "WHM",
                    "ENCDPS": "4,000",
                    "damage": "4,000",
                    "Crit%": "5%",
                    "DH%": "15%",
                    "Deaths": "1",
                    "ENCHPS": "900",
                    "healed": "1,500",
                    "OverHealPct": "15%"
                }
            },
            "isActive": "true"
        });

        let (encounter, rows) = parse_combat_data(&payload).expect("parsed");

        assert_eq!(encounter.title, "Dummy");
        assert_eq!(encounter.zone, "Somewhere");
        assert!(encounter.is_active);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].name, "Alice");
        assert_eq!(rows[0].share_str, "60.0%");
        assert_eq!(rows[1].name, "Bob");
        assert_eq!(rows[1].heal_share_str, "75.0%");
    }

    #[test]
    fn respects_server_provided_percentages() {
        let payload = json!({
            "type": "CombatData",
            "Encounter": {
                "title": "Boss",
                "duration": "30",
                "damage": "1,000",
                "encdps": "120"
            },
            "Combatant": {
                "Alice": {
                    "Job": "NIN",
                    "encdps": "80",
                    "damage": "600",
                    "damage%": "70%"
                },
                "Bob": {
                    "Job": "WHM",
                    "encdps": "40",
                    "damage": "400",
                    "damage%": "30%"
                }
            }
        });

        let (_encounter, rows) = parse_combat_data(&payload).expect("parsed");

        assert!((rows[0].share - 0.7).abs() < 1e-6);
        assert_eq!(rows[0].share_str, "70.0%");
        assert!((rows[1].share - 0.3).abs() < 1e-6);
    }
}
