//! Fenêtre de recherche : champ, onglets de catégories, résultats, panneau « ranger dans ».
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use atray_core::{
    Item, Launch,
    config::{Config, Language},
    history::{self, Event},
    search::Searcher,
    sources,
};
use gpui::{
    ClipboardItem, Context, FocusHandle, KeyDownEvent, MouseButton, RenderImage, ScrollWheelEvent, SharedString,
    Subscription, Task, Window, div, img, prelude::*, px, rgb, rgba, size,
};

use crate::{
    field::Field,
    slide::{Change, Slider},
    i18n::{self, t},
    platform,
    theme::*,
};

pub const WIDTH: f32 = 640.;
const INPUT_H: f32 = 56.;
const TABS_H: f32 = 36.;
/// Hauteur sans résultats, aussi celle de la création : gpui l'applique au premier affichage,
/// après nos redimensionnements, donc elle doit déjà être juste.
pub const COMPACT_H: f32 = INPUT_H + TABS_H;
/// Marge sous la dernière ligne, pour qu'elle ne touche pas les coins arrondis.
const CARD_PAD: f32 = 6.;
/// Fenêtre transparente, fixe : la carte y change de hauteur sans redimensionner la fenêtre.
pub const WINDOW_W: f32 = WIDTH + 2. * aurora::MARGIN;
pub const WINDOW_H: f32 = COMPACT_H + MAX_ROWS as f32 * ROW_H + CARD_PAD + 2. * aurora::MARGIN;
const ROW_H: f32 = 40.;
const MAX_ROWS: usize = 8;
const ICON: f32 = 24.;
/// Taille demandée au shell : nette jusqu'à 200 % de mise à l'échelle.
const ICON_SOURCE_PX: i32 = 48;
/// À l'ouverture, on rescanne en arrière-plan si le dernier scan est plus vieux que ça.
const RESCAN_AFTER: Duration = Duration::from_secs(60);
/// Chiffres 0…9 d'un clavier AZERTY avec Ctrl ou Alt : gpui donne le caractère sans Maj de la touche.
pub const AZERTY_DIGITS: [&str; 10] = ["à", "&", "é", "\"", "'", "(", "-", "è", "_", "ç"];

/// Touche chiffre, en QWERTY comme en AZERTY.
pub fn digit(key: &str) -> Option<usize> {
    key.parse::<usize>().ok().filter(|d| *d <= 9).or_else(|| AZERTY_DIGITS.iter().position(|k| *k == key))
}

pub struct Search {
    focus: FocusHandle,
    pub(crate) items: Vec<Item>,
    /// Par id d'élément ; rempli en arrière-plan, gardé d'un scan à l'autre.
    pub(crate) icons: HashMap<String, Arc<RenderImage>>,
    searcher: Searcher,
    query: Field,
    /// 0 = « Tout », n = catégorie n-1.
    tab: usize,
    /// Indices dans `items`, meilleur d'abord.
    results: Vec<usize>,
    selected: usize,
    /// Panneau « ranger dans » ouvert pour cet élément (indice dans `items`), et sa ligne sélectionnée.
    editing: Option<usize>,
    panel_selected: usize,
    height: f32,
    pub(crate) config: Config,
    config_path: PathBuf,
    /// Fichier illisible : on l'affiche et on ne l'écrase jamais tant qu'il n'est pas corrigé.
    pub(crate) config_error: Option<String>,
    /// Réglages refusés (raccourci pris, registre…) : affichés, sans bloquer le reste.
    pub(crate) notices: Vec<String>,
    history: PathBuf,
    events: Vec<Event>,
    /// Score d'usage par id, recalculé à chaque ouverture (il dépend de l'heure).
    pub(crate) usage: HashMap<String, f64>,
    scanned_at: Instant,
    pressed_at: Option<Instant>,
    visible: bool,
    /// Dernière frappe : le curseur reste plein pendant qu'on tape.
    typed_at: Instant,
    /// Pastille de l'onglet actif, qui glisse d'un onglet à l'autre.
    pill: Slider,
    /// Dernier changement de catégorie : les lignes arrivent du côté où va la pastille.
    tab_change: Change,
    /// Lancement en cours : la ligne s'allume, puis la barre se ferme.
    launched: Option<(usize, Instant)>,
    /// Élément tout juste rangé (élément, catégorie) : le nom de la catégorie s'allume.
    filed: Option<(usize, usize, Instant)>,
    /// Énergie de la lueur à cet instant ; elle retombe ensuite exponentiellement.
    energy: (f32, Instant),
    /// Hauteur de la carte : glisse de `from` vers `to` depuis `at`.
    card: (f32, f32, Instant),
    /// Molette : reste de défilement pas encore converti en lignes.
    wheel: f32,
    _blink: Task<()>,
    _activation: Subscription,
}

impl Search {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let _activation = cx.observe_window_activation(window, |this, window, cx| {
            if !window.is_window_active() {
                this.hide(window, cx);
            }
        });
        let data = PathBuf::from(std::env::var_os("APPDATA").unwrap_or_default()).join("Atray");
        let mut this = Search {
            focus: cx.focus_handle(),
            items: vec![],
            icons: HashMap::new(),
            searcher: Searcher::default(),
            query: Field::default(),
            tab: 0,
            results: vec![],
            selected: 0,
            editing: None,
            panel_selected: 0,
            height: WINDOW_H,
            config: Config::new(t().default_categories),
            config_path: data.join("config.toml"),
            config_error: None,
            notices: vec![],
            history: data.join("history.jsonl"),
            events: history::load(&data.join("history.jsonl")),
            usage: HashMap::new(),
            scanned_at: Instant::now(),
            pressed_at: None,
            visible: false,
            typed_at: Instant::now(),
            pill: Slider::default(),
            tab_change: Change::default(),
            launched: None,
            filed: None,
            energy: (0., Instant::now()),
            card: (COMPACT_H, COMPACT_H, Instant::now()),
            wheel: 0.,
            _blink: Task::ready(()),
            _activation,
        };
        this.reload_config(window, cx);
        this.rescore();
        this.reindex(window, cx);
        this
    }

    fn rescore(&mut self) {
        self.usage = history::scores(&self.events, now(), platform::utc_offset());
    }

    pub fn clear_history(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Err(e) = history::clear(&self.history) {
            eprintln!("history: {e}");
        }
        self.events.clear();
        self.rescore();
        self.refresh(window, cx);
    }

    pub fn config_path(&self) -> &PathBuf {
        &self.config_path
    }

    /// Relu à chaque ouverture : les modifications faites à la main s'appliquent sans redémarrer.
    pub(crate) fn reload_config(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let disabled_before = self.config.settings.disabled_sources.clone();
        match Config::load(&self.config_path) {
            Ok(Some(config)) => {
                (self.config, self.config_error) = (config, None);
                // Ajoute la section [settings] aux fichiers créés avant qu'elle existe.
                let has_settings = |t: String| t.lines().any(|l| l.trim() == "[settings]"); // pas le commentaire d'en-tête
                if !std::fs::read_to_string(&self.config_path).is_ok_and(has_settings) {
                    self.save_config();
                }
            }
            Ok(None) => {
                // Premier lancement : noms des catégories dans la langue de Windows.
                i18n::set_language(Language::Auto);
                (self.config, self.config_error) = (Config::new(t().default_categories), None);
                self.save_config();
            }
            Err(e) => self.config_error = Some(e),
        }
        self.apply_settings(disabled_before, window, cx);
    }

    /// Modification depuis le panneau du tray : appliquée, enregistrée, puis effective tout de suite.
    pub(crate) fn update_config(&mut self, window: &mut Window, cx: &mut Context<Self>, f: impl FnOnce(&mut Config)) {
        if self.config_error.is_some() {
            return; // le fichier a une erreur : ne pas l'écraser
        }
        let disabled_before = self.config.settings.disabled_sources.clone();
        f(&mut self.config);
        self.save_config();
        self.apply_settings(disabled_before, window, cx);
        self.refresh(window, cx);
    }

    /// Chaque réglage est idempotent : on les réapplique tous, seuls les changements agissent.
    fn apply_settings(&mut self, disabled_before: Vec<String>, window: &mut Window, cx: &mut Context<Self>) {
        self.tab = self.tab.min(self.config.categories.len());
        let s = &self.config.settings;
        self.notices.clear();
        if i18n::set_language(s.language) {
            platform::rebuild_tray_menu(cx);
        }
        let main = std::iter::once((0, s.hotkey.as_str()));
        let wanted: Vec<_> = main.chain(self.config.categories.iter().enumerate().map(|(i, c)| (i + 1, c.hotkey.as_str()))).collect();
        for e in platform::set_hotkeys(cx, &wanted) {
            self.notices.push(format!("{} {e}", t().hotkey_error));
        }
        if let Err(e) = platform::set_autostart(s.start_with_windows) {
            self.notices.push(format!("{} {e}", t().autostart_error));
        }
        for n in &self.notices {
            eprintln!("{n}");
        }
        if s.disabled_sources != disabled_before {
            self.reindex(window, cx);
        }
    }

    fn save_config(&self) {
        if self.config_error.is_none()
            && let Err(e) = self.config.save(&self.config_path)
        {
            eprintln!("config: {e}");
        }
    }

    pub fn reindex(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.scanned_at = Instant::now();
        let disabled = self.config.settings.disabled_sources.clone();
        let scan = cx.background_executor().spawn(async move { sources::scan_all(&disabled) });
        cx.spawn_in(window, async move |this, cx| {
            let items = scan.await;
            _ = this.update_in(cx, |this, window, cx| {
                this.items = items;
                this.editing = None;
                this.refresh(window, cx);
                this.load_icons(cx);
            });
        })
        .detach();
    }

    /// Extrait les icônes manquantes hors du thread de l'UI, puis les affiche d'un coup.
    fn load_icons(&mut self, cx: &mut Context<Self>) {
        let missing: Vec<(String, PathBuf)> = self
            .items
            .iter()
            .filter(|i| !self.icons.contains_key(&i.id))
            .filter_map(|i| Some((i.id.clone(), i.icon.clone()?)))
            .collect();
        if missing.is_empty() {
            return;
        }
        let extract = cx.background_executor().spawn(async move {
            let start = Instant::now();
            let (mut icons, mut failed) = (vec![], vec![]);
            for (id, path) in missing {
                match platform::file_icon(&path, ICON_SOURCE_PX) {
                    Some(icon) => icons.push((id, icon)),
                    None => failed.push(path),
                }
            }
            eprintln!("icons: {} in {:?}, failed: {failed:?}", icons.len(), start.elapsed());
            icons
        });
        cx.spawn(async move |this, cx| {
            let icons = extract.await;
            _ = this.update(cx, |this, cx| {
                this.icons.extend(icons);
                cx.notify();
            });
        })
        .detach();
    }

    /// Lignes sous les onglets : message d'erreur, panneau, résultats ou « aucun résultat ».
    fn rows(&self) -> usize {
        let body = match self.editing {
            Some(_) => self.config.categories.len(),
            // Barre vide sur « Tout » sans historique : rien à suggérer, barre compacte.
            None if self.results.is_empty() && self.query.text.is_empty() && self.tab == 0 => 0,
            None if self.results.is_empty() => 1,
            None => self.results.len().min(MAX_ROWS),
        };
        body + self.config_error.is_some() as usize + self.notices.len()
    }

    /// Relance la recherche (la sélection reste en place) et ajuste la hauteur de la fenêtre.
    fn refresh(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let within = self.tab.checked_sub(1).and_then(|c| self.config.categories.get(c));
        self.results = self.searcher.search(&self.items, &self.query.text, within, &self.usage);
        self.selected = self.selected.min(self.results.len().saturating_sub(1));
        let rows = self.rows();
        let target = COMPACT_H + rows as f32 * ROW_H + if rows > 0 { CARD_PAD } else { 0. };
        if target != self.card.1 {
            self.card = (self.card_h(), target, Instant::now());
        }
        // Rare (messages d'erreur en plus des 8 lignes) : la fenêtre ne fait que grandir.
        let needed = target + 2. * aurora::MARGIN;
        if needed > self.height {
            self.height = needed;
            window.resize(size(px(WINDOW_W), px(needed)));
        }
        cx.notify();
    }

    fn card_h(&self) -> f32 {
        let (from, to, at) = self.card;
        motion::lerp(from, to, motion::eased(at, motion::PILL))
    }

    fn energy(&self) -> f32 {
        let (level, at) = self.energy;
        if !motion::enabled() {
            return 0.;
        }
        level * (-at.elapsed().as_secs_f32() / aurora::DECAY).exp()
    }

    fn kindle(&mut self, amount: f32) {
        self.energy = ((self.energy() + amount).min(1.), Instant::now());
    }

    /// `tab` : 0 = « Tout », n = catégorie n-1 (hotkey de catégorie).
    pub fn show(&mut self, at: Instant, tab: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.pressed_at = Some(at);
        self.reload_config(window, cx);
        self.tab = tab.min(self.config.categories.len());
        motion::set_enabled(platform::animations_enabled());
        (self.visible, self.typed_at, self.energy) = (true, Instant::now(), (aurora::ON_OPEN, Instant::now()));
        self.blink(cx);
        self.rescore();
        if self.scanned_at.elapsed() > RESCAN_AFTER {
            self.reindex(window, cx);
        }
        self.refresh(window, cx);
        self.card = (self.card.1, self.card.1, Instant::now() - motion::PILL); // à la bonne taille d'emblée
        // Avec `show: false`, gpui garde la taille demandée en attente et ne l'applique que dans
        // sa tâche `activate` : sans ça la fenêtre garde la taille par défaut de Windows (immense).
        // Notre ShowWindow passe dans une tâche suivante, donc après cette tâche de gpui.
        window.activate_window();
        cx.spawn_in(window, async |_, cx| _ = cx.update(|window, _| platform::show(window))).detach();
        window.focus(&self.focus);
    }

    fn hide(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Remise à zéro avant de masquer : la prochaine apparition ne montre jamais l'ancien état.
        self.query.clear();
        (self.tab, self.selected, self.editing) = (0, 0, None);
        (self.visible, self._blink) = (false, Task::ready(()));
        (self.launched, self.filed) = (None, None);
        self.pill.reset();
        self.refresh(window, cx);
        platform::hide(window);
    }

    /// Hotkey principale (tab 0) : ouvre ou ferme. Hotkey de catégorie : ferme si on y est déjà,
    /// sinon y va.
    pub fn toggle(&mut self, at: Instant, tab: usize, window: &mut Window, cx: &mut Context<Self>) {
        match platform::is_foreground(window) {
            true if tab == 0 || tab == self.tab => self.hide(window, cx),
            true => self.set_tab(tab, window, cx),
            false => self.show(at, tab, window, cx),
        }
    }

    /// Redessine au rythme de clignotement de Windows, calé sur la dernière frappe, tant que la barre est visible.
    fn blink(&mut self, cx: &mut Context<Self>) {
        let Some(period) = platform::caret_blink() else { return };
        self._blink = cx.spawn(async move |this, cx| {
            loop {
                let Ok(wait) = this.update(cx, |this, cx| {
                    cx.notify();
                    let into = this.typed_at.elapsed().as_nanos() % period.as_nanos();
                    period - Duration::from_nanos(into as u64) + Duration::from_millis(1)
                }) else {
                    break;
                };
                cx.background_executor().timer(wait).await;
            }
        });
    }

    fn caret_on(&self) -> bool {
        platform::caret_blink().is_none_or(|p| (self.typed_at.elapsed().as_nanos() / p.as_nanos()) % 2 == 0)
    }

    fn move_selection(&mut self, by: isize, cx: &mut Context<Self>) {
        let last = self.results.len().saturating_sub(1);
        self.selected = self.selected.saturating_add_signed(by).min(last);
        cx.notify();
    }

    /// Un cran de molette = une ligne. gpui donne un cran en « lignes Windows » (3 par défaut) :
    /// on le ramène à un cran ; pavé tactile (pixels) : une ligne tous les ROW_H pixels.
    fn on_wheel(&mut self, ev: &ScrollWheelEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.wheel -= match ev.delta {
            gpui::ScrollDelta::Lines(l) => l.y / platform::wheel_lines(),
            gpui::ScrollDelta::Pixels(p) => f32::from(p.y) / ROW_H,
        };
        let rows = self.wheel.trunc();
        if rows != 0. && self.editing.is_none() {
            self.wheel -= rows;
            self.move_selection(rows as isize, cx);
        }
    }

    fn set_tab(&mut self, tab: usize, window: &mut Window, cx: &mut Context<Self>) {
        if tab != self.tab && self.visible {
            self.pill.start(self.tab);
            self.tab_change = Change::new(self.tab, tab);
            self.kindle(aurora::ON_KEY);
        }
        (self.tab, self.selected, self.editing) = (tab, 0, None);
        self.refresh(window, cx);
    }

    fn open_panel(&mut self, rank: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(&item) = self.results.get(rank) else { return };
        (self.selected, self.editing, self.panel_selected) = (rank, Some(item), 0);
        self.refresh(window, cx);
    }

    /// Range l'élément dans la catégorie ou l'en retire, puis enregistre.
    fn toggle_category(&mut self, item: usize, category: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.config_error.is_some() {
            return; // le fichier a une erreur : ne pas l'écraser
        }
        self.config.toggle(category, &self.items[item]);
        self.filed = Some((item, category, Instant::now()));
        self.save_config();
        self.refresh(window, cx);
    }

    fn launch(&mut self, rank: usize, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(&i) = self.results.get(rank) {
            self.launch_item(i, window, cx);
        }
    }

    /// Depuis le menu du tray.
    pub fn launch_id(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(i) = self.items.iter().position(|it| it.id == id) {
            self.launch_item(i, window, cx);
        }
    }

    fn launch_item(&mut self, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        let item = &self.items[i];
        match &item.launch {
            Launch::Open(path) => cx.open_with_system(path),
            Launch::Uri(uri) => cx.open_url(uri),
            Launch::Cmd { program, args, dir } => {
                let mut cmd = std::process::Command::new(program);
                cmd.args(args);
                if let Some(dir) = dir {
                    cmd.current_dir(dir);
                }
                if let Err(e) = cmd.spawn() {
                    eprintln!("{}: {e}", program.display());
                }
            }
        }
        let event = Event { id: item.id.clone(), at: now() };
        if let Err(e) = history::append(&self.history, &event) {
            eprintln!("history: {e}");
        }
        self.events.push(event);
        self.rescore();
        // Accusé de réception : la ligne s'allume un instant, puis la barre se ferme.
        match self.results.iter().position(|&r| r == i).filter(|_| self.visible) {
            Some(rank) => {
                self.launched = Some((rank, Instant::now()));
                self.kindle(aurora::ON_LAUNCH);
                cx.notify();
                cx.spawn_in(window, async |this, cx| {
                    cx.background_executor().timer(motion::PULSE).await;
                    _ = this.update_in(cx, |this, window, cx| this.hide(window, cx));
                })
                .detach();
            }
            None => self.hide(window, cx),
        }
    }

    fn on_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = ev.keystroke.key.as_str();
        let m = &ev.keystroke.modifiers;
        if self.launched.is_some() {
            return; // la barre se ferme
        }

        if let Some(item) = self.editing {
            let last = self.config.categories.len().saturating_sub(1);
            match key {
                "escape" => self.editing = None,
                "up" => self.panel_selected = self.panel_selected.saturating_sub(1),
                "down" => self.panel_selected = (self.panel_selected + 1).min(last),
                "enter" | "space" => return self.toggle_category(item, self.panel_selected, window, cx),
                _ => return,
            }
            return self.refresh(window, cx);
        }

        let tabs = self.config.categories.len() + 1;
        // Ctrl+Alt = AltGr (@, #, €…) : ni l'un ni l'autre.
        let (ctrl, alt) = (m.control && !m.alt, m.alt && !m.control);
        let before = self.query.text.clone();
        match key {
            "escape" => return self.hide(window, cx),
            "enter" => return self.launch(self.selected, window, cx),
            "up" => return self.move_selection(-1, cx),
            "down" => return self.move_selection(1, cx),
            "pageup" => return self.move_selection(-(MAX_ROWS as isize), cx),
            "pagedown" => return self.move_selection(MAX_ROWS as isize, cx),
            "tab" if m.shift => return self.set_tab((self.tab + tabs - 1) % tabs, window, cx),
            "tab" => return self.set_tab((self.tab + 1) % tabs, window, cx),
            // Ctrl+1…9 : onglet de la catégorie, Ctrl+0 : « Tout ».
            _ if ctrl && digit(key).is_some() => {
                if let Some(d) = digit(key).filter(|d| *d < tabs) {
                    self.set_tab(d, window, cx);
                }
                return;
            }
            // Alt+1…9 : range le résultat sélectionné dans la catégorie (ou l'en retire).
            _ if alt && digit(key).is_some_and(|d| d > 0) => {
                if let (Some(d), Some(&item)) = (digit(key), self.results.get(self.selected)) {
                    self.toggle_category(item, d - 1, window, cx);
                }
                return;
            }
            "c" | "x" if ctrl => {
                if !self.query.selected().is_empty() {
                    cx.write_to_clipboard(ClipboardItem::new_string(self.query.selected().to_string()));
                    if key == "x" {
                        self.query.insert("");
                    }
                }
            }
            "v" if ctrl => {
                let text = cx.read_from_clipboard().and_then(|c| c.text()).unwrap_or_default();
                self.query.insert(&text.replace(['\r', '\n'], " "));
            }
            _ if self.query.key(key, ctrl, m.shift) => {}
            _ if ctrl || alt => return,
            // ponytail: pas d'IME ni de touches mortes ; passer à EntityInputHandler
            // (exemple input.rs de gpui) si ça manque.
            _ => match ev.keystroke.key_char.as_deref() {
                Some(c) => self.query.insert(c),
                None => return,
            },
        }
        self.typed_at = Instant::now(); // curseur plein pendant la frappe
        if self.query.text != before {
            self.kindle(aurora::ON_KEY);
            self.selected = 0;
            return self.refresh(window, cx);
        }
        cx.notify();
    }

    fn render_tabs(&self, pill: Option<[f32; 4]>, cx: &mut Context<Self>) -> impl IntoElement {
        let names = std::iter::once(SharedString::from(t().all))
            .chain(self.config.categories.iter().map(|c| SharedString::from(c.name.clone())));
        // Pendant le glissement, la pastille est dessinée à part (sous les onglets) ; sinon sur l'onglet.
        let sliding = self.pill.is_sliding();
        div()
            .h(px(TABS_H))
            .flex_none()
            .border_b_1()
            .border_color(rgb(BORDER))
            .flex()
            .items_center()
            .px_4()
            .text_size(px(13.))
            .child(self.pill.container(pill, |d| d.rounded_md().bg(rgba(ACCENT_SOFT))).flex().items_center().gap_1().children(
                names.enumerate().map(|(i, name)| {
                    div()
                        .id(("tab", i))
                        .px_3()
                        .py_1()
                        .rounded_md()
                        .text_color(rgb(if i == self.tab { ACCENT_TEXT } else { TEXT_DIM }))
                        .when(i == self.tab && !sliding, |d| d.bg(rgba(ACCENT_SOFT)))
                        .when(i != self.tab, |d| d.hover(|d| d.bg(rgb(SURFACE_HOVER))))
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _, window, cx| this.set_tab(i, window, cx)))
                        .child(name)
                }),
            ))
    }

    /// Ligne visible (0…7) où se trouve la sélection : la liste défile pour la garder en vue.
    fn first_row(&self) -> usize {
        self.selected.saturating_sub(MAX_ROWS - 1)
    }

    fn render_results(&self, cx: &mut Context<Self>) -> Vec<gpui::AnyElement> {
        if self.results.is_empty() {
            let empty = if self.query.text.is_empty() { t().empty_category } else { t().no_results };
            return match (self.query.text.is_empty(), self.tab) {
                (true, 0) => vec![],
                _ => vec![self.tab_change.arriving(0, row().id("empty").text_color(rgb(TEXT_DIM)).child(empty)).into_any_element()],
            };
        }
        // La sélection reste visible quand on descend sous la 8ᵉ ligne.
        let first = self.first_row();
        self.results
            .iter()
            .enumerate()
            .skip(first)
            .take(MAX_ROWS)
            .map(|(rank, &i)| {
                let item = &self.items[i];
                // Catégorie tout juste rangée : son nom s'allume en violet puis s'éteint.
                let cats = self.config.categories.iter().enumerate().filter(|(_, c)| c.contains(item)).map(|(ci, c)| {
                    let glow = match self.filed {
                        Some((it, cat, at)) if it == i && cat == ci => 1. - motion::eased(at, motion::FILED),
                        _ => 0.,
                    };
                    div().text_color(motion::mix(TEXT_DIM, ACCENT_TEXT, glow)).child(c.name.clone())
                });
                // Lancement : la ligne s'enfonce vers un violet plus soutenu (constant si animations coupées).
                // Ligne sélectionnée éclairée par la gauche ; au lancement la lumière s'intensifie.
                let light = match self.launched {
                    Some((r, at)) if r == rank => {
                        let t = if motion::enabled() { motion::eased(at, motion::PULSE) } else { 1. };
                        Some(motion::lerp(aurora::LIT, aurora::LIT_LAUNCH, t))
                    }
                    _ if rank == self.selected => Some(aurora::LIT),
                    _ => None,
                };
                self.tab_change.arriving(rank - first, row().id(("row", rank)))
                    .when_some(light, |d, a| d.bg(aurora::lit(a)))
                    .cursor_pointer()
                    .when(rank != self.selected, |d| d.hover(|d| d.bg(rgb(SURFACE_HOVER))))
                    .on_click(cx.listener(move |this, _, window, cx| this.launch(rank, window, cx)))
                    .on_mouse_down(MouseButton::Right, cx.listener(move |this, _, window, cx| this.open_panel(rank, window, cx)))
                    .child(match self.icons.get(&item.id) {
                        Some(icon) => img(icon.clone()).size(px(ICON)).into_any_element(),
                        None => div().size(px(ICON)).into_any_element(), // garde l'alignement en attendant
                    })
                    .child(div().flex_1().truncate().when(rank == self.selected, |d| d.text_color(rgb(TEXT))).child(item.name.clone()))
                    .child(div().flex().gap_2().text_size(px(12.)).children(cats))
                    .into_any_element()
            })
            .collect()
    }

    fn render_panel(&self, item: usize, cx: &mut Context<Self>) -> Vec<gpui::AnyElement> {
        self.config
            .categories
            .iter()
            .enumerate()
            .map(|(ci, cat)| {
                let checked = cat.contains(&self.items[item]);
                row()
                    .id(("cat", ci))
                    .when(ci == self.panel_selected, |d| d.bg(aurora::lit(aurora::LIT)))
                    .cursor_pointer()
                    .when(ci != self.panel_selected, |d| d.hover(|d| d.bg(rgb(SURFACE_HOVER))))
                    .on_click(cx.listener(move |this, _, window, cx| this.toggle_category(item, ci, window, cx)))
                    .child(div().w(px(ICON)).font_family(glyph::FONT).text_color(rgb(ACCENT_TEXT)).child(if checked { glyph::CHECK } else { "" }))
                    .child(div().flex_1().child(cat.name.clone()))
                    .child(div().text_size(px(12.)).text_color(rgb(TEXT_DIM)).child(format!("Alt+{}", ci + 1)))
                    .into_any_element()
            })
            .collect()
    }
}

impl Search {
    /// Texte avant | sélection | après, curseur au bon bout.
    fn render_input(&self) -> gpui::Div {
        let field = div().flex().items_center().min_w_0().overflow_hidden().whitespace_nowrap();
        let dim = |s: &str| div().truncate().text_color(rgb(TEXT_DIM)).child(s.to_string());
        if let Some(i) = self.editing {
            return field.child(dim(&format!("{} — {}", self.items[i].name, t().file_under)));
        }
        let q = &self.query;
        let caret = || caret().when(!self.caret_on(), |d| d.opacity(0.));
        if q.text.is_empty() {
            return field.gap_0p5().child(caret()).child(dim(t().placeholder));
        }
        // ponytail: le texte ne défile pas ; un curseur au-delà de la largeur sort de la vue.
        let sel = q.selection();
        let span = |r: std::ops::Range<usize>| div().flex_none().child(q.text[r].to_string());
        let caret_first = q.cursor() == sel.start;
        field
            .child(span(0..sel.start))
            .when(caret_first, |d| d.child(caret()))
            .when(!sel.is_empty(), |d| d.child(span(sel.clone()).bg(rgba(ACCENT_SOFT)).text_color(rgb(ACCENT_TEXT))))
            .when(!caret_first, |d| d.child(caret()))
            .child(span(sel.end..q.text.len()))
    }
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

fn row() -> gpui::Div {
    div().h(px(ROW_H)).flex_none().flex().items_center().gap_3().px_5().text_size(px(16.))
}

fn caret() -> gpui::Div {
    div().w(px(2.)).h(px(26.)).bg(rgb(ACCENT))
}

impl Render for Search {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(at) = self.pressed_at.take() {
            eprintln!("hotkey -> render: {:?}", at.elapsed());
        }
        let body = match self.editing {
            Some(item) => self.render_panel(item, cx),
            None => self.render_results(cx),
        };
        // Animations en cours : on redemande des images jusqu'à leur fin, puis plus rien.
        let pill = self.pill.sliding(window, self.tab);
        for at in [self.launched.map(|l| l.1), self.filed.map(|f| f.2)].into_iter().flatten() {
            motion::progress(window, at, motion::FILED);
        }
        let energy = self.energy();
        if energy > 0.005 {
            window.request_animation_frame(); // la lueur retombe ; plus aucune image une fois éteinte
        }
        self.tab_change.animate(window, MAX_ROWS);
        let card_h = motion::lerp(self.card.0, self.card.1, motion::progress(window, self.card.2, motion::PILL));
        let close = |this: &mut Self, _: &gpui::MouseDownEvent, window: &mut Window, cx: &mut Context<Self>| this.hide(window, cx);
        let inside = |_: &gpui::MouseDownEvent, _: &mut Window, cx: &mut gpui::App| cx.stop_propagation();
        // Fenêtre transparente : un clic hors de la carte ferme la barre, comme un clic ailleurs.
        div()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::on_key))
            .on_scroll_wheel(cx.listener(Self::on_wheel))
            .on_mouse_down(MouseButton::Left, cx.listener(close))
            .on_mouse_down(MouseButton::Right, cx.listener(close))
            .size_full()
            .text_color(rgb(TEXT))
            .font_family(FONT)
            .child(
                div()
                    .absolute()
                    .left(px(aurora::MARGIN))
                    .top(px(aurora::MARGIN))
                    .w(px(WIDTH))
                    .h(px(card_h))
                    .rounded(px(aurora::RADIUS))
                    .border_1()
                    .border_color(rgb(BORDER))
                    .bg(aurora::card())
                    .shadow(aurora::glow(energy))
                    .overflow_hidden()
                    .on_mouse_down(MouseButton::Left, inside)
                    .on_mouse_down(MouseButton::Right, inside)
                    .child(self.render_content(body, pill, cx)),
            )
    }
}

impl Search {
    fn render_content(&self, body: Vec<gpui::AnyElement>, pill: Option<[f32; 4]>, cx: &mut Context<Self>) -> gpui::Div {
        div()
            .w_full()
            .flex_none()
            .flex()
            .flex_col()
            .child(
                div()
                    .h(px(INPUT_H))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_3()
                    .px_5()
                    .border_b_1()
                    .border_color(rgb(BORDER))
                    .child(div().font_family(glyph::FONT).text_size(px(16.)).text_color(rgb(ACCENT_TEXT)).child(glyph::SEARCH))
                    .text_size(px(22.))
                    .child(self.render_input()),
            )
            .child(self.render_tabs(pill, cx))
            .when_some(self.config_error.clone(), |d, e| {
                d.child(row().text_size(px(12.)).text_color(rgb(DANGER)).child(format!("{} {e}", t().config_error)))
            })
            .children(self.notices.iter().map(|n| row().text_size(px(12.)).text_color(rgb(DANGER)).child(n.clone())))
            .children(body)
    }
}
