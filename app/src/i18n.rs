//! Textes de l'interface, en français ou en anglais (réglage `language`, par défaut celle de Windows).
use std::sync::atomic::{AtomicBool, Ordering};

use atray_core::config::Language;

pub struct Strings {
    pub placeholder: &'static str,
    pub no_results: &'static str,
    pub open: &'static str,
    pub reindex: &'static str,
    pub quit: &'static str,
    pub settings: &'static str,
    pub clear_history: &'static str,
    pub all: &'static str,
    pub default_categories: [&'static str; 4],
    pub empty_category: &'static str,
    pub file_under: &'static str,
    pub config_error: &'static str,
    pub hotkey_error: &'static str,
    pub hotkey_duplicate: &'static str,
    pub category_hotkey: &'static str,
    pub press_keys_or_clear: &'static str,
    pub panel_apps: &'static str,
    pub search: &'static str,
    pub shortcut: &'static str,
    pub press_keys: &'static str,
    pub language: &'static str,
    pub startup: &'static str,
    pub start_with_windows: &'static str,
    pub sources: &'static str,
    pub start_menu: &'static str,
    pub categories: &'static str,
    pub new_category: &'static str,
    pub auto_games: &'static str,
    pub data: &'static str,
    pub open_config: &'static str,
    pub confirm: &'static str,
    pub autostart_error: &'static str,
}

const FR: Strings = Strings {
    placeholder: "Rechercher une application…",
    no_results: "Aucun résultat",
    open: "Ouvrir la recherche",
    reindex: "Réindexer",
    quit: "Quitter",
    settings: "Réglages",
    clear_history: "Effacer l'historique",
    all: "Tout",
    default_categories: ["Jeux", "Travail", "Outils", "Divers"],
    empty_category: "Catégorie vide : clic droit sur une application (ou Alt+1…9) pour l'y ranger",
    file_under: "ranger dans (Entrée pour cocher, Échap pour revenir)",
    config_error: "config.toml illisible, modifications non enregistrées :",
    hotkey_error: "Raccourci refusé, l'ancien reste actif :",
    hotkey_duplicate: "déjà utilisé par une autre catégorie",
    category_hotkey: "Raccourci",
    press_keys_or_clear: "Touches… (Suppr : aucun)",
    panel_apps: "Apps",
    search: "Rechercher",
    shortcut: "Raccourci",
    press_keys: "Tape la combinaison…",
    language: "Langue",
    startup: "Démarrage",
    start_with_windows: "Lancer avec Windows",
    sources: "Sources",
    start_menu: "Menu Démarrer",
    categories: "Catégories",
    new_category: "Nouvelle catégorie",
    auto_games: "jeux auto",
    data: "Données",
    open_config: "Ouvrir config.toml",
    confirm: "Confirmer ?",
    autostart_error: "Démarrage avec Windows impossible :",
};

const EN: Strings = Strings {
    placeholder: "Search apps…",
    no_results: "No results",
    open: "Open search",
    reindex: "Reindex",
    quit: "Quit",
    settings: "Settings",
    clear_history: "Clear history",
    all: "All",
    default_categories: ["Games", "Work", "Tools", "Misc"],
    empty_category: "Empty category: right-click an app (or Alt+1…9) to add it",
    file_under: "file under (Enter to toggle, Esc to go back)",
    config_error: "config.toml is invalid, changes not saved:",
    hotkey_error: "Hotkey rejected, keeping the previous one:",
    hotkey_duplicate: "already used by another category",
    category_hotkey: "Hotkey",
    press_keys_or_clear: "Keys… (Del: none)",
    panel_apps: "Apps",
    search: "Search",
    shortcut: "Shortcut",
    press_keys: "Press the keys…",
    language: "Language",
    startup: "Startup",
    start_with_windows: "Start with Windows",
    sources: "Sources",
    start_menu: "Start menu",
    categories: "Categories",
    new_category: "New category",
    auto_games: "auto games",
    data: "Data",
    open_config: "Open config.toml",
    confirm: "Confirm?",
    autostart_error: "Couldn't set start with Windows:",
};

static FRENCH: AtomicBool = AtomicBool::new(false);

/// Choisit la langue (réglage `language`) ; renvoie vrai si elle a changé.
pub fn set_language(language: Language) -> bool {
    let french = match language {
        Language::Fr => true,
        Language::En => false,
        Language::Auto => crate::platform::ui_is_french(),
    };
    FRENCH.swap(french, Ordering::Relaxed) != french
}

pub fn t() -> &'static Strings {
    if FRENCH.load(Ordering::Relaxed) { &FR } else { &EN }
}
