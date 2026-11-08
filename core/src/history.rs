//! Journal des lancements (une ligne JSON par lancement, on ne fait qu'ajouter) et score de suggestion.
use std::{collections::HashMap, fs, io::Write, path::Path};

use serde::{Deserialize, Serialize};

// Réglages du score : à ajuster à l'usage.
/// Constante de temps de l'oubli : un lancement vieux de 14 jours pèse e⁻¹ ≈ 37 %.
const TAU_SECS: f64 = 14. * 86_400.;
/// Bonus si le lancement passé tombait à ±1 h de l'heure actuelle.
const SAME_HOURS_BONUS: f64 = 0.5;
/// Bonus si c'était le même type de jour (semaine / week-end).
const SAME_DAYTYPE_BONUS: f64 = 0.3;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Event {
    pub id: String,
    /// Secondes Unix (UTC).
    pub at: u64,
}

/// Lignes illisibles ignorées : un fichier abîmé ne fait perdre que ces lignes.
pub fn load(path: &Path) -> Vec<Event> {
    let Ok(text) = fs::read_to_string(path) else { return vec![] };
    text.lines().filter_map(|l| serde_json::from_str(l).ok()).collect()
}

pub fn append(path: &Path, event: &Event) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let mut f = fs::OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(f, "{}", serde_json::to_string(event)?)
}

pub fn clear(path: &Path) -> std::io::Result<()> {
    match fs::remove_file(path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// Score de chaque élément maintenant : Σ e^(−âge/τ) · (1 + bonus même créneau + bonus même type de jour).
/// `utc_offset` = décalage local en secondes (la règle d'été du jour s'applique à tout l'historique ;
/// l'erreur d'une heure sur les vieux lancements reste dans la fenêtre de ±1 h).
pub fn scores(events: &[Event], now: u64, utc_offset: i64) -> HashMap<String, f64> {
    let (hour, weekend) = local(now, utc_offset);
    let mut out = HashMap::new();
    for e in events.iter().filter(|e| e.at <= now) {
        let (h, we) = local(e.at, utc_offset);
        let hours_apart = (h as i32 - hour as i32).rem_euclid(24).min((hour as i32 - h as i32).rem_euclid(24));
        let context = 1.
            + if hours_apart <= 1 { SAME_HOURS_BONUS } else { 0. }
            + if we == weekend { SAME_DAYTYPE_BONUS } else { 0. };
        *out.entry(e.id.clone()).or_default() += (-((now - e.at) as f64) / TAU_SECS).exp() * context;
    }
    out
}

/// Heure locale (0-23) et « est-ce le week-end ».
fn local(t: u64, utc_offset: i64) -> (u32, bool) {
    let local = t as i64 + utc_offset;
    let hour = (local.rem_euclid(86_400) / 3600) as u32;
    let weekday = (local.div_euclid(86_400) + 4).rem_euclid(7); // 1970-01-01 était un jeudi ; 0 = dimanche
    (hour, weekday == 0 || weekday == 6)
}

#[test]
fn scores_favor_recent_and_same_context() {
    const SUNDAY_20H: u64 = 1_794_168_000; // 2026-11-08 20:00 UTC
    const DAY: u64 = 86_400;
    let ev = |id: &str, at| Event { id: id.into(), at };
    assert_eq!(local(SUNDAY_20H, 0), (20, true));
    assert_eq!(local(SUNDAY_20H - 4 * DAY, 3600), (21, false)); // mercredi 21 h à Paris (UTC+1)

    let s = scores(
        &[
            ev("soir", SUNDAY_20H - 7 * DAY),             // dimanche dernier, même heure
            ev("matin", SUNDAY_20H - 7 * DAY - 12 * 3600), // dimanche dernier, 8 h
            ev("vieux", SUNDAY_20H - 120 * DAY),            // même créneau, mais il y a 4 mois
            ev("futur", SUNDAY_20H + DAY),                   // horloge déréglée : ignoré
        ],
        SUNDAY_20H,
        0,
    );
    assert!(s["soir"] > s["matin"]);
    assert!(s["matin"] > s["vieux"]);
    assert!(s["vieux"] < 0.001);
    assert!(!s.contains_key("futur"));

    // Fréquence : 3 lancements valent plus qu'un seul à la même date.
    let s = scores(&[ev("a", SUNDAY_20H - DAY), ev("b", SUNDAY_20H - DAY), ev("b", SUNDAY_20H - DAY), ev("b", SUNDAY_20H - DAY)], SUNDAY_20H, 0);
    assert!(s["b"] > 2.9 * s["a"]);
}

#[test]
fn load_skips_bad_lines() {
    let path = std::env::temp_dir().join(format!("atray-history-{}.jsonl", std::process::id()));
    append(&path, &Event { id: "a".into(), at: 1 }).unwrap();
    fs::OpenOptions::new().append(true).open(&path).unwrap().write_all(b"{abime\n").unwrap();
    append(&path, &Event { id: "b".into(), at: 2 }).unwrap();
    assert_eq!(load(&path).len(), 2);
    clear(&path).unwrap();
    assert!(load(&path).is_empty());
    clear(&path).unwrap(); // déjà absent : pas d'erreur
}
