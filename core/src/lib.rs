//! Logique d'Atray, sans gpui ni Win32 : le portage Linux ne réécrira que `app/`.
pub mod config;
pub mod history;
pub mod search;
pub mod sources {
    pub mod epic;
    pub mod gog;
    pub mod start_menu;
    pub mod steam;

    use std::collections::HashSet;

    use crate::{Item, name_key};

    /// Noms utilisables dans `disabled_sources`.
    pub const NAMES: [&str; 4] = ["start_menu", "steam", "epic", "gog"];

    /// Toutes les sources sauf `disabled` ; une source absente ou illisible donne une liste vide.
    pub fn scan_all(disabled: &[String]) -> Vec<Item> {
        let on = |name: &str| !disabled.iter().any(|d| d == name);
        let mut items = vec![];
        if on("steam") {
            items.extend(steam::scan());
        }
        if on("epic") {
            items.extend(epic::scan());
        }
        if on("gog") {
            items.extend(gog::scan());
        }
        if on("start_menu") {
            // Les launchers créent aussi des raccourcis dans le menu Démarrer : on garde leur version.
            let games: HashSet<String> = items.iter().map(|i| name_key(&i.name)).collect();
            items.extend(start_menu::scan().into_iter().filter(|i| !games.contains(&name_key(&i.name))));
        }
        items
    }
}

use std::path::PathBuf;

/// Nom réduit aux lettres et chiffres, en minuscules :
/// « Hollow Knight: Silksong » et le raccourci « Hollow Knight Silksong » donnent la même clé.
pub fn name_key(name: &str) -> String {
    name.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

#[derive(Clone, Debug)]
pub enum Launch {
    /// Ouvert par le shell (ShellExecute) : raccourci, exécutable, fichier, dossier.
    Open(PathBuf),
    /// Confié au launcher qui gère ce protocole (steam://, com.epicgames.launcher://).
    Uri(String),
    Cmd { program: PathBuf, args: Vec<String>, dir: Option<PathBuf> },
}

#[derive(Clone, Debug)]
pub struct Item {
    /// Stable d'un scan à l'autre : sert de clé dans l'historique.
    pub id: String,
    pub name: String,
    pub launch: Launch,
    /// Fichier dont on affiche l'icône (raccourci, exe, image).
    pub icon: Option<PathBuf>,
}

impl Item {
    /// Trouvé par un launcher de jeux (alimente la catégorie `games`).
    pub fn is_game(&self) -> bool {
        ["steam:", "epic:", "gog:"].iter().any(|p| self.id.starts_with(p))
    }
}

#[test]
fn name_keys_match_shortcuts() {
    assert_eq!(name_key("Hollow Knight: Silksong"), name_key("Hollow Knight Silksong"));
    assert_eq!(name_key("HELLDIVERS™ 2"), "helldivers2");
    assert_eq!(name_key("Tom Clancy's Rainbow Six® Siege"), name_key("Tom Clancy's Rainbow Six Siege"));
}
