//! `config.toml` : catégories (ordre, contenu). Éditable à la main ; relu à chaque ouverture.
use std::{fs, path::Path};

use serde::{Deserialize, Serialize};

use crate::Item;

const HEADER: &str = "\
# Atray — relu à chaque ouverture de la barre / reloaded every time the bar opens.
# [settings]
#   hotkey = \"Alt+Space\"  (Ctrl, Alt, Shift, Super = touche Windows / Windows key ; \"Ctrl+Shift+K\", \"Alt+F1\"…)
#   language = \"auto\" | \"fr\" | \"en\"
#   start_with_windows = true | false
#   disabled_sources = [\"start_menu\", \"steam\", \"epic\", \"gog\"]  (sources à ignorer / to skip)
# Catégories dans l'ordre des onglets / categories in tab order.
#   items = ids des éléments / item ids (`cargo run -p atray-core --example scan`)
#   games = true : tous les jeux Steam, Epic, GOG / all Steam, Epic, GOG games
#   hotkey = \"Alt+Shift+1\" : ouvre la barre sur cette catégorie / opens the bar on this category
# Barre / bar : Ctrl+1…9 = onglet / tab, Ctrl+0 = Tout / All ;
#   clic droit ou Alt+1…9 sur un résultat pour le ranger / right-click or Alt+1…9 on a result to file it.

";

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    /// Celle de Windows (français, sinon anglais).
    #[default]
    Auto,
    Fr,
    En,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Settings {
    /// Ex. « Alt+Space », « Ctrl+Shift+K », « Alt+F1 ».
    pub hotkey: String,
    pub language: Language,
    pub start_with_windows: bool,
    /// Sources à ne pas indexer, parmi `sources::NAMES`.
    pub disabled_sources: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            hotkey: "Alt+Space".into(),
            language: Language::Auto,
            start_with_windows: false,
            disabled_sources: vec![],
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Category {
    pub name: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub games: bool,
    #[serde(default)]
    pub items: Vec<String>,
    /// Hotkey globale qui ouvre la barre sur cette catégorie (« Alt+Shift+1 »…), vide = aucune.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub hotkey: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Config {
    #[serde(default)]
    pub settings: Settings,
    #[serde(rename = "category", default)]
    pub categories: Vec<Category>,
}

impl Category {
    pub fn contains(&self, item: &Item) -> bool {
        (self.games && item.is_game()) || self.items.contains(&item.id)
    }
}

impl Config {
    /// Catégories par défaut ; la première se remplit avec les jeux. Noms donnés dans la langue de l'UI.
    pub fn new(names: [&str; 4]) -> Self {
        let categories = names
            .iter()
            .enumerate()
            .map(|(i, n)| Category { name: n.to_string(), games: i == 0, items: vec![], hotkey: String::new() })
            .collect();
        Config { settings: Settings::default(), categories }
    }

    /// Ok(None) si le fichier n'existe pas encore. Err si illisible : l'appelant ne doit
    /// alors surtout pas l'écraser, il contient peut-être une modification à la main.
    pub fn load(path: &Path) -> Result<Option<Self>, String> {
        match fs::read_to_string(path) {
            Ok(text) => toml::from_str(&text).map(Some).map_err(|e| format!("{}: {e}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(format!("{}: {e}", path.display())),
        }
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let body = toml::to_string(self).map_err(std::io::Error::other)?;
        // Écrit à côté puis renomme : jamais de fichier à moitié écrit si on coupe au mauvais moment.
        let tmp = path.with_extension("toml.tmp");
        fs::write(&tmp, format!("{HEADER}{body}"))?;
        fs::rename(tmp, path)
    }

    /// Ajoute ou retire l'élément de la catégorie. Un jeu ne sort pas d'une catégorie `games`.
    pub fn toggle(&mut self, category: usize, item: &Item) {
        let Some(cat) = self.categories.get_mut(category) else { return };
        if let Some(pos) = cat.items.iter().position(|id| *id == item.id) {
            cat.items.remove(pos);
        } else if !(cat.games && item.is_game()) {
            cat.items.push(item.id.clone());
        }
    }
}

#[test]
fn round_trip_and_toggle() {
    let game = Item { id: "steam:1".into(), name: "G".into(), launch: crate::Launch::Uri("".into()), icon: None };
    let app = Item { id: "start:code".into(), ..game.clone() };
    let mut c = Config::new(["Jeux", "Travail", "Outils", "Divers"]);
    assert!(c.categories[0].contains(&game) && !c.categories[0].contains(&app));

    c.categories[1].hotkey = "Alt+Shift+2".into();
    c.toggle(1, &app);
    c.toggle(2, &app);
    c.toggle(2, &app); // retiré
    c.toggle(0, &game); // déjà là via `games` : sans effet
    assert!(c.categories[1].contains(&app) && !c.categories[2].contains(&app));
    assert!(c.categories[0].items.is_empty());

    let path = std::env::temp_dir().join(format!("atray-test-{}.toml", std::process::id()));
    c.save(&path).unwrap();
    assert_eq!(Config::load(&path).unwrap(), Some(c));
    fs::write(&path, "[[category]\nname = ").unwrap();
    assert!(Config::load(&path).is_err());
    fs::remove_file(&path).unwrap();
    assert_eq!(Config::load(&path).unwrap(), None);

    // Un ancien fichier sans [settings] reste valide, une clé disparue (theme) est ignorée,
    // une valeur inconnue est une erreur visible.
    fs::write(&path, "[settings]\ntheme = \"light\"\nlanguage = \"fr\"\n[[category]]\nname = \"X\"\n").unwrap();
    let c = Config::load(&path).unwrap().unwrap();
    assert_eq!((c.settings.language, c.settings.hotkey.as_str()), (Language::Fr, "Alt+Space"));
    fs::write(&path, "[[category]]\nname = \"X\"\n").unwrap();
    assert_eq!(Config::load(&path).unwrap().unwrap().settings, Settings::default());
    fs::write(&path, "[settings]\nlanguage = \"de\"\n").unwrap();
    assert!(Config::load(&path).is_err());
    fs::remove_file(&path).unwrap();
}
