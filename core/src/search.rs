use std::collections::HashMap;

use nucleo_matcher::{
    Config, Matcher, Utf32Str,
    pattern::{CaseMatching, Normalization, Pattern},
};

use crate::{Item, config::Category};

/// Poids de l'historique dans le classement : score final = flou + K · ln(1 + score d'usage).
/// Le logarithme amortit l'effet : avec un usage normal (score ~20, une vingtaine de lancements
/// récents), une correspondance médiocre reste derrière une bonne ; il faut un usage très intensif
/// (score ~100) pour que « fire » donne File Explorer avant Firefox. Réglage : baisser K pour l'éviter.
const USAGE_WEIGHT: f64 = 8.;

pub struct Searcher {
    matcher: Matcher,
    buf: Vec<char>,
}

impl Default for Searcher {
    fn default() -> Self {
        let mut config = Config::DEFAULT;
        config.prefer_prefix = true; // « fire » → Firefox avant « Wildfire »
        Searcher { matcher: Matcher::new(config), buf: vec![] }
    }
}

impl Searcher {
    /// Indices des éléments qui correspondent, meilleur d'abord. `usage` = score d'usage par id.
    /// Requête vide : dans une catégorie, tout son contenu (les plus utilisés d'abord, puis par nom) ;
    /// sans catégorie, les suggestions (éléments déjà utilisés, les plus utilisés d'abord).
    /// Normalization::Smart : « parametres » trouve « Paramètres ».
    pub fn search(&mut self, items: &[Item], query: &str, within: Option<&Category>, usage: &HashMap<String, f64>) -> Vec<usize> {
        if query.trim().is_empty() {
            return browse(items, within, usage);
        }
        let used = |i: usize| usage.get(&items[i].id).copied().unwrap_or(0.);
        let allowed = items.iter().enumerate().filter(|(_, it)| within.is_none_or(|c| c.contains(it)));
        let pattern = Pattern::parse(query, CaseMatching::Ignore, Normalization::Smart);
        let mut scored: Vec<(usize, f64)> = allowed
            .filter_map(|(i, it)| {
                let fuzzy = pattern.score(Utf32Str::new(&it.name, &mut self.buf), &mut self.matcher)?;
                Some((i, fuzzy as f64 + USAGE_WEIGHT * used(i).ln_1p()))
            })
            .collect();
        // À score égal, le nom le plus court (« Code » avant « Code - Insiders »).
        scored.sort_by(|a, b| b.1.total_cmp(&a.1).then(items[a.0].name.len().cmp(&items[b.0].name.len())));
        scored.into_iter().map(|(i, _)| i).collect()
    }
}

/// Sans requête : le contenu de la catégorie (les plus utilisés d'abord, puis par nom),
/// ou sans catégorie les suggestions (éléments déjà utilisés seulement).
pub fn browse(items: &[Item], within: Option<&Category>, usage: &HashMap<String, f64>) -> Vec<usize> {
    let used = |i: usize| usage.get(&items[i].id).copied().unwrap_or(0.);
    let mut all: Vec<usize> = (0..items.len())
        .filter(|&i| match within {
            Some(c) => c.contains(&items[i]),
            None => used(i) > 0.,
        })
        .collect();
    all.sort_by_cached_key(|&i| (std::cmp::Reverse((used(i) * 1e6) as u64), items[i].name.to_lowercase()));
    all
}

#[test]
fn ranks_names() {
    let items: Vec<Item> = ["Microsoft Edge", "Wildfire", "Firefox", "File Explorer", "Visual Studio Code", "Paramètres", "Microsoft Solitaire Collection", "League of Legends", "Lollipop Chainsaw"]
        .map(|n| Item { id: n.into(), name: n.into(), launch: crate::Launch::Uri(n.into()), icon: None })
        .into();
    let mut s = Searcher::default();
    let none = HashMap::new();
    let top = |s: &mut Searcher, q| items[s.search(&items, q, None, &none)[0]].name.clone();
    assert_eq!(top(&mut s, "fire"), "Firefox");
    assert_eq!(top(&mut s, "vsc"), "Visual Studio Code");
    assert_eq!(top(&mut s, "parametres"), "Paramètres");
    assert_eq!(top(&mut s, "EDGE"), "Microsoft Edge");
    // Initiales : chaque lettre en début de mot.
    let lol: Vec<_> = s.search(&items, "lol", None, &none).into_iter().map(|i| items[i].name.as_str()).collect();
    assert!(lol.contains(&"League of Legends"));
    assert!(s.search(&items, "", None, &none).is_empty());
    assert!(s.search(&items, "zzz", None, &none).is_empty());

    // Dans une catégorie : requête vide = tout le contenu, trié ; sinon filtré.
    let work = Category { name: "Travail".into(), games: false, items: vec!["Visual Studio Code".into(), "Firefox".into()], hotkey: String::new() };
    let names = |v: Vec<usize>| v.into_iter().map(|i| items[i].name.as_str()).collect::<Vec<_>>();
    assert_eq!(names(s.search(&items, "", Some(&work), &none)), ["Firefox", "Visual Studio Code"]);
    assert_eq!(names(s.search(&items, "fi", Some(&work), &none)), ["Firefox"]);

    // L'historique : LoL lancé souvent passe devant Lollipop pour « lol » ; barre vide = suggestions.
    let usage = HashMap::from([("League of Legends".to_string(), 6.), ("Firefox".to_string(), 1.)]);
    assert_eq!(names(s.search(&items, "lol", None, &usage))[0], "League of Legends");
    assert_eq!(names(s.search(&items, "", None, &usage)), ["League of Legends", "Firefox"]);
    assert_eq!(names(s.search(&items, "", Some(&work), &usage)), ["Firefox", "Visual Studio Code"]);
    // Avec un usage normal, une correspondance médiocre ne passe pas devant une bonne.
    assert_eq!(names(s.search(&items, "fire", None, &usage))[0], "Firefox");
    let heavy = HashMap::from([("File Explorer".to_string(), 20.)]);
    assert_eq!(names(s.search(&items, "fire", None, &heavy))[0], "Firefox");
}
