//! Panneau du tray (clic gauche sur l'icône) : apps par catégorie, et tous les réglages.
//! Il n'a pas d'état métier : il affiche celui de la fenêtre de recherche et le modifie
//! par `Search::update_config`, qui enregistre et applique tout de suite.
use std::{
    cell::RefCell,
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};

use atray_core::{
    config::{Category, Config, Language},
    search::browse,
};
use gpui::{
    Bounds, Context, Div, FocusHandle, Image, ImageFormat, KeyDownEvent, Keystroke, Pixels, SharedString, Stateful, Subscription,
    Window, WindowHandle, div, img, prelude::*, px, rgb, rgba,
};

use crate::{
    slide::{Change, Slider},
    i18n::t,
    platform::{self, Anchor},
    theme::*,
    ui::Search,
};

pub const WIDTH: f32 = 380.;
pub const HEIGHT: f32 = 540.;
const TILE_W: f32 = 80.;
const TILE_ICON: f32 = 36.;
/// Au-delà, les tuiles arrivent ensemble : une grande catégorie ne traîne pas.
const TILE_STAGGER_MAX: usize = 12;
/// Cliquer sur l'icône du tray quand le panneau est ouvert le ferme d'abord (perte de focus),
/// puis l'événement de clic le rouvrirait : on ignore une réouverture aussi rapprochée.
const REOPEN_GUARD: Duration = Duration::from_millis(300);
/// Sources affichées dans les réglages : (nom dans la config, libellé).
const SOURCES: [(&str, &str); 4] = [("start_menu", ""), ("steam", "Steam"), ("epic", "Epic Games"), ("gog", "GOG")];

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Apps,
    Settings,
}

/// Action destructrice en attente d'un second clic.
#[derive(Clone, Copy, PartialEq)]
enum Confirm {
    DeleteCategory(usize),
    ClearHistory,
}

pub struct Panel {
    focus: FocusHandle,
    search: WindowHandle<Search>,
    logo: Arc<Image>,
    tab: Tab,
    category: usize,
    /// En attente de la combinaison de touches : 0 = hotkey principale, n = catégorie n-1.
    capturing: Option<usize>,
    renaming: Option<(usize, String)>,
    confirm: Option<Confirm>,
    visible: bool,
    hidden_at: Instant,
    /// Interrupteur tout juste basculé : son bouton glisse.
    flipped: Option<(&'static str, Instant)>,
    /// Pastilles qui glissent (Apps/Réglages, puces de catégorie) et contenu qui arrive du même côté.
    page_pill: Slider,
    page_change: Change,
    chip_pill: Slider,
    category_change: Change,
    /// Catégorie déplacée : nouvelle place, place de l'autre ligne, écart vertical entre les deux
    /// (relevé au clic, avant tout décalage), instant. Et position des lignes à l'écran.
    reordered: Option<(usize, usize, f32, Instant)>,
    category_slots: Rc<RefCell<Vec<Bounds<Pixels>>>>,
    _subscriptions: Vec<Subscription>,
}

impl Panel {
    pub fn new(search: WindowHandle<Search>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut subscriptions = vec![cx.observe_window_activation(window, |this, window, cx| {
            if !window.is_window_active() {
                this.hide(window, cx);
            }
        })];
        // Icônes chargées, config modifiée… : le panneau se redessine avec la recherche.
        if let Ok(entity) = search.entity(cx) {
            subscriptions.push(cx.observe(&entity, |_, _, cx| cx.notify()));
        }
        Panel {
            focus: cx.focus_handle(),
            search,
            logo: Arc::new(Image::from_bytes(ImageFormat::Png, include_bytes!("../assets/atray-256.png").to_vec())),
            tab: Tab::Apps,
            category: 0,
            capturing: None,
            renaming: None,
            confirm: None,
            visible: false,
            hidden_at: Instant::now() - REOPEN_GUARD,
            flipped: None,
            page_pill: Slider::default(),
            page_change: Change::default(),
            chip_pill: Slider::default(),
            category_change: Change::default(),
            reordered: None,
            category_slots: Rc::default(),
            _subscriptions: subscriptions,
        }
    }

    pub fn toggle(&mut self, anchor: Anchor, window: &mut Window, cx: &mut Context<Self>) {
        if self.visible {
            self.hide(window, cx);
        } else if self.hidden_at.elapsed() > REOPEN_GUARD {
            self.show(anchor, window, cx);
        }
    }

    fn show(&mut self, anchor: Anchor, window: &mut Window, cx: &mut Context<Self>) {
        // Le fichier a pu être modifié à la main depuis la dernière ouverture.
        _ = self.search.update(cx, |s, window, cx| s.reload_config(window, cx));
        self.visible = true;
        motion::set_enabled(platform::animations_enabled());
        self.flipped = None;
        // Même ordre que la recherche : gpui applique sa taille, puis on place et on affiche.
        window.activate_window();
        cx.spawn_in(window, async move |_, cx| {
            _ = cx.update(|window, _| platform::show_near(window, anchor, WIDTH, HEIGHT));
        })
        .detach();
        window.focus(&self.focus);
        cx.notify();
    }

    fn hide(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.visible {
            return;
        }
        (self.visible, self.hidden_at) = (false, Instant::now());
        (self.capturing, self.renaming, self.confirm) = (None, None, None);
        self.page_pill.reset();
        self.chip_pill.reset();
        platform::hide(window);
        cx.notify();
    }

    /// Modifie la config via la recherche (enregistrée et appliquée aussitôt).
    fn edit(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut Config) + 'static) {
        _ = self.search.update(cx, |s, window, cx| s.update_config(window, cx, f));
        cx.notify();
    }

    /// La ligne qui arrive en `moved` vient de `other`, et inversement : on retient l'écart actuel.
    fn reorder(&mut self, moved: usize, other: usize) {
        let y = |i: usize| self.category_slots.borrow().get(i).map_or(0., |b| f32::from(b.origin.y));
        self.reordered = Some((moved, other, y(other) - y(moved), Instant::now()));
    }

    fn launch(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        self.hide(window, cx);
        _ = self.search.update(cx, |s, window, cx| s.launch_id(&id, window, cx));
    }

    fn open_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.hide(window, cx);
        _ = self.search.update(cx, |s, window, cx| s.show(Instant::now(), 0, window, cx));
    }

    /// Premier clic : demande confirmation ; second clic : exécute.
    fn confirmed(&mut self, action: Confirm, cx: &mut Context<Self>) -> bool {
        if self.confirm == Some(action) {
            self.confirm = None;
            true
        } else {
            self.confirm = Some(action);
            cx.notify();
            false
        }
    }

    fn on_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = ev.keystroke.key.as_str();
        if let Some(which) = self.capturing {
            // Une catégorie peut n'avoir aucune hotkey ; la barre principale en garde toujours une.
            let spec = match key {
                "backspace" | "delete" if which > 0 => Some(String::new()),
                _ => hotkey_spec(&ev.keystroke),
            };
            if key == "escape" {
                self.capturing = None;
            } else if let Some(spec) = spec {
                self.capturing = None;
                self.edit(cx, move |c| match which {
                    0 => c.settings.hotkey = spec,
                    n => {
                        if let Some(cat) = c.categories.get_mut(n - 1) {
                            cat.hotkey = spec;
                        }
                    }
                });
            }
            return cx.notify();
        }
        if let Some((index, name)) = &mut self.renaming {
            match key {
                "escape" => self.renaming = None,
                "enter" => {
                    let (index, name) = (*index, name.trim().to_string());
                    self.renaming = None;
                    if !name.is_empty() {
                        self.edit(cx, move |c| {
                            if let Some(cat) = c.categories.get_mut(index) {
                                cat.name = name;
                            }
                        });
                    }
                }
                "backspace" => _ = name.pop(),
                _ if ev.keystroke.modifiers.control && !ev.keystroke.modifiers.alt => {}
                _ => name.extend(ev.keystroke.key_char.as_deref()),
            }
            return cx.notify();
        }
        if key == "escape" {
            self.hide(window, cx);
        }
    }

    fn render_header(&self, pill: Option<[f32; 4]>, cx: &Context<Self>) -> Div {
        let sliding = self.page_pill.is_sliding();
        let tab = |id: &'static str, label: &'static str, which: Tab| {
            let active = self.tab == which;
            div()
                .id(id)
                .px_3()
                .py_1()
                .rounded(px(RADIUS))
                .text_color(rgb(if active { ACCENT_TEXT } else { TEXT_DIM }))
                .when(active && !sliding, |d| d.bg(rgba(ACCENT_SOFT)))
                .hover(|d| d.text_color(rgb(TEXT)))
                .cursor_pointer()
                .on_click(cx.listener(move |this, _, _, cx| {
                    if this.tab != which {
                        this.page_pill.start(this.tab as usize);
                        this.page_change = Change::new(this.tab as usize, which as usize);
                    }
                    (this.tab, this.renaming, this.confirm, this.capturing) = (which, None, None, None);
                    cx.notify();
                }))
                .child(label)
        };
        div()
            .h(px(52.))
            .flex_none()
            .flex()
            .items_center()
            .justify_between()
            .px_4()
            .border_b_1()
            .border_color(rgb(BORDER))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(img(self.logo.clone()).size(px(28.)))
                    .child(div().text_size(px(15.)).font_weight(gpui::FontWeight::SEMIBOLD).child("Atray")),
            )
            .child(
                self.page_pill
                    .container(pill, |d| d.rounded(px(RADIUS)).bg(rgba(ACCENT_SOFT)))
                    .flex()
                    .gap_1()
                    .text_size(px(13.))
                    .child(tab("tab-apps", t().panel_apps, Tab::Apps))
                    .child(tab("tab-settings", t().settings, Tab::Settings)),
            )
    }

    fn render_apps(&self, s: &Search, pill: Option<[f32; 4]>, cx: &Context<Self>) -> Div {
        let cats = &s.config.categories;
        let current = self.category.min(cats.len().saturating_sub(1));
        let sliding = self.chip_pill.is_sliding();
        let chips = cats.iter().enumerate().map(|(i, cat)| {
            let (active, lit) = (i == current && !sliding, i == current);
            div()
                .id(("chip", i))
                .px_3()
                .py_1()
                .rounded_full()
                .border_1()
                .border_color(rgb(if active { ACCENT } else { BORDER }))
                .text_color(rgb(if lit { ACCENT_TEXT } else { TEXT_DIM }))
                .when(active, |d| d.bg(rgba(ACCENT_SOFT)))
                .hover(|d| d.text_color(rgb(TEXT)))
                .cursor_pointer()
                .on_click(cx.listener(move |this, _, _, cx| {
                    if i != current {
                        this.chip_pill.start(current);
                        this.category_change = Change::new(current, i);
                    }
                    this.category = i;
                    cx.notify();
                }))
                .child(cat.name.clone())
        });
        let shown = cats.get(current).map(|cat| browse(&s.items, Some(cat), &s.usage)).unwrap_or_default();
        // Tuiles : arrivent du côté où va la puce, l'une après l'autre (décalage plafonné).
        let tiles = shown.into_iter().enumerate().map(|(n, i)| {
            let item = &s.items[i];
            let id = item.id.clone();
            let tile = div().id(SharedString::from(format!("app-{}", item.id)));
            self.category_change
                .arriving(n.min(TILE_STAGGER_MAX), tile)
                .w(px(TILE_W))
                .h(px(100.))
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap_1p5()
                .rounded(px(RADIUS))
                .hover(|d| d.bg(rgb(SURFACE_HOVER)))
                .on_click(cx.listener(move |this, _, window, cx| this.launch(id.clone(), window, cx)))
                .child(match s.icons.get(&item.id) {
                    Some(icon) => img(icon.clone()).size(px(TILE_ICON)).into_any_element(),
                    // Pas d'icône : l'initiale sur une pastille violette, jamais un trou.
                    None => div()
                        .size(px(TILE_ICON))
                        .rounded(px(RADIUS))
                        .bg(rgba(ACCENT_SOFT))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(rgb(ACCENT_TEXT))
                        .child(item.name.chars().next().unwrap_or('?').to_string())
                        .into_any_element(),
                })
                // Nom sur deux lignes au plus (centré et coupé, il perdait son début).
                .child(div().w_full().px_1().text_size(px(12.)).line_height(px(15.)).text_center().line_clamp(2).text_ellipsis().child(item.name.clone()))
        });
        let tiles: Vec<_> = tiles.collect();
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                self.chip_pill
                    .container(pill, |d| d.rounded_full().border_1().border_color(rgb(ACCENT)).bg(rgba(ACCENT_SOFT)))
                    .flex()
                    .flex_wrap()
                    .gap_1p5()
                    .text_size(px(12.))
                    .children(chips),
            )
            .child(if tiles.is_empty() {
                let empty = div().id("empty").py_8().px_4().text_center().text_size(px(13.)).text_color(rgb(TEXT_DIM));
                div().child(self.category_change.arriving(0, empty.child(t().empty_category)))
            } else {
                div().flex().flex_wrap().gap_2().children(tiles)
            })
    }

    fn render_settings(&self, s: &Search, window: &Window, cx: &Context<Self>) -> Div {
        // Interrupteur : son bouton glisse s'il vient d'être basculé.
        let switch = |name: &str, on: bool| {
            let t = match self.flipped {
                Some((n, at)) if n == name => motion::progress(window, at, motion::SWITCH),
                _ => 1.,
            };
            switch(if on { t } else { 1. - t })
        };
        let settings = &s.config.settings;

        // Raccourci : on clique, puis on tape la combinaison.
        let hotkey = self.hotkey_capsule(0, &settings.hotkey, cx);

        let language = div().flex().gap_1().children(
            [(Language::Auto, "Auto"), (Language::Fr, "Français"), (Language::En, "English")].map(|(lang, label)| {
                let active = settings.language == lang;
                div()
                    .id(label)
                    .px_2p5()
                    .py_1()
                    .rounded(px(RADIUS))
                    .text_color(rgb(if active { ACCENT_TEXT } else { TEXT_DIM }))
                    .when(active, |d| d.bg(rgba(ACCENT_SOFT)))
                    .hover(|d| d.text_color(rgb(TEXT)))
                    .on_click(cx.listener(move |this, _, _, cx| this.edit(cx, move |c| c.settings.language = lang)))
                    .child(label)
            }),
        );

        let autostart = setting_row("autostart", t().start_with_windows)
            .child(switch("autostart", settings.start_with_windows))
            .on_click(cx.listener(|this, _, _, cx| {
                this.flipped = Some(("autostart", Instant::now()));
                this.edit(cx, |c| c.settings.start_with_windows = !c.settings.start_with_windows)
            }));

        let sources = SOURCES.map(|(name, label)| {
            let on = !settings.disabled_sources.iter().any(|d| d == name);
            let label = if name == "start_menu" { t().start_menu } else { label };
            setting_row(name, label).child(switch(name, on)).on_click(cx.listener(move |this, _, _, cx| {
                this.flipped = Some((name, Instant::now()));
                this.edit(cx, move |c| {
                    let disabled = &mut c.settings.disabled_sources;
                    match disabled.iter().position(|d| d == name) {
                        Some(i) => _ = disabled.remove(i),
                        None => disabled.push(name.to_string()),
                    }
                })
            }))
        });

        let count = s.config.categories.len();
        // Déplacement : chaque ligne part de son ancienne place (FLIP) ; celle qu'on a bougée s'allume.
        let categories = s.config.categories.iter().enumerate().map(|(i, cat)| {
            let row = self.category_row(i, count, cat, cx);
            match self.reordered {
                Some((moved, other, gap, at)) if i == moved || i == other => {
                    let t = motion::eased(at, motion::REORDER);
                    row.relative()
                        .top(px(if i == moved { gap } else { -gap } * (1. - t)))
                        .when(i == moved, |d| d.bg(aurora::lit(aurora::LIT * (1. - motion::eased(at, motion::FILED)))))
                }
                _ => row,
            }
        });
        let slots = self.category_slots.clone();
        let categories = div()
            .flex()
            .flex_col()
            .gap_1p5()
            .on_children_prepainted(move |b, _, _| *slots.borrow_mut() = b)
            .children(categories);
        let add = div()
            .id("add-category")
            .flex()
            .items_center()
            .gap_2()
            .px_2()
            .py_1p5()
            .rounded(px(RADIUS))
            .text_color(rgb(ACCENT_TEXT))
            .hover(|d| d.bg(rgb(SURFACE_HOVER)))
            .on_click(cx.listener(move |this, _, _, cx| {
                let name = t().new_category.to_string();
                this.edit(cx, move |c| c.categories.push(Category { name, games: false, items: vec![], hotkey: String::new() }));
                this.renaming = Some((count, t().new_category.to_string()));
                cx.notify();
            }))
            .child(icon(glyph::ADD))
            .child(t().new_category);

        let clear_label = if self.confirm == Some(Confirm::ClearHistory) { t().confirm } else { t().clear_history };
        let data = div()
            .flex()
            .flex_col()
            .gap_0p5()
            .child(action_row("reindex", glyph::REFRESH, t().reindex, TEXT).on_click(cx.listener(|this, _, _, cx| {
                _ = this.search.update(cx, |s, window, cx| s.reindex(window, cx));
            })))
            .child(
                action_row("clear", glyph::DELETE, clear_label, DANGER).on_click(cx.listener(|this, _, _, cx| {
                    if this.confirmed(Confirm::ClearHistory, cx) {
                        _ = this.search.update(cx, |s, window, cx| s.clear_history(window, cx));
                    }
                })),
            )
            .child(action_row("config", glyph::DOCUMENT, t().open_config, TEXT).on_click(cx.listener(|this, _, _, cx| {
                if let Ok(path) = this.search.read(cx).map(|s| s.config_path().clone()) {
                    cx.open_with_system(&path);
                }
            })));

        let problems = s.config_error.iter().chain(&s.notices).map(|p| {
            div().px_3().py_2().rounded(px(RADIUS)).bg(rgb(SURFACE)).text_size(px(12.)).text_color(rgb(DANGER)).child(p.clone())
        });

        div()
            .flex()
            .flex_col()
            .gap_5()
            .children(problems)
            .child(section(t().shortcut).child(setting_row("hotkey-row", t().open).child(hotkey)))
            .child(section(t().language).child(language))
            .child(section(t().startup).child(autostart))
            .child(section(t().sources).children(sources))
            .child(section(t().categories).child(categories).child(add))
            .child(section(t().data).child(data))
    }

    fn hotkey_capsule(&self, which: usize, spec: &str, cx: &Context<Self>) -> Stateful<Div> {
        let capturing = self.capturing == Some(which);
        let small = which > 0;
        let label = match (capturing, spec) {
            (true, _) if small => t().press_keys_or_clear.to_string(),
            (true, _) => t().press_keys.to_string(),
            (false, "") => t().category_hotkey.to_string(),
            (false, spec) => spec.to_string(),
        };
        div()
            .id(("hotkey", which))
            .flex_none()
            .px(px(if small { 8. } else { 12. }))
            .py(px(if small { 2. } else { 4. }))
            .rounded(px(RADIUS))
            .border_1()
            .border_color(rgb(if capturing { ACCENT } else { BORDER }))
            .bg(rgb(SURFACE))
            .when(small, |d| d.text_size(px(11.)))
            .text_color(rgb(match (capturing, spec) {
                (true, _) => ACCENT_TEXT,
                (false, "") => TEXT_DIM,
                _ => TEXT,
            }))
            .hover(|d| d.border_color(rgb(ACCENT)))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.capturing = if this.capturing == Some(which) { None } else { Some(which) };
                cx.notify();
            }))
            .child(label)
    }

    fn category_row(&self, i: usize, count: usize, cat: &Category, cx: &Context<Self>) -> Stateful<Div> {
        let deleting = self.confirm == Some(Confirm::DeleteCategory(i));
        let name = match &self.renaming {
            Some((r, text)) if *r == i => div()
                .flex()
                .items_center()
                .gap_0p5()
                .px_2()
                .py_0p5()
                .rounded(px(RADIUS))
                .border_1()
                .border_color(rgb(ACCENT))
                .bg(rgb(SURFACE))
                .child(text.clone())
                .child(div().w(px(2.)).h(px(16.)).bg(rgb(ACCENT)))
                .into_any_element(),
            // Clic sur le nom : on le renomme sur place (Entrée valide, Échap annule).
            _ => div()
                .id(("rename", i))
                .truncate()
                .hover(|d| d.text_color(rgb(ACCENT_TEXT)))
                .cursor_text()
                .on_click(cx.listener(move |this, _, _, cx| {
                    let current = this.search.read(cx).ok().and_then(|s| Some(s.config.categories.get(i)?.name.clone()));
                    this.renaming = current.map(|n| (i, n));
                    cx.notify();
                }))
                .child(cat.name.clone())
                .into_any_element(),
        };
        let button = |id: &'static str, glyph: &'static str, enabled: bool| {
            div()
                .id((id, i))
                .size(px(26.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(RADIUS))
                .text_color(rgb(if enabled { TEXT_DIM } else { BORDER }))
                .when(enabled, |d| d.hover(|d| d.bg(rgb(SURFACE_HOVER)).text_color(rgb(TEXT))))
                .child(icon(glyph))
        };
        div()
            .id(("category", i))
            .flex()
            .items_center()
            .gap_2()
            .px_2()
            .py_1()
            .rounded(px(RADIUS))
            .hover(|d| d.bg(rgb(SURFACE)))
            .child(div().flex_1().min_w_0().flex().items_center().gap_2().child(name).when(cat.games, |d| {
                d.child(div().text_size(px(11.)).text_color(rgb(TEXT_DIM)).child(t().auto_games))
            }))
            .child(self.hotkey_capsule(i + 1, &cat.hotkey, cx))
            .child(button("up", glyph::UP, i > 0).on_click(cx.listener(move |this, _, _, cx| {
                if i > 0 {
                    this.reorder(i - 1, i);
                    this.edit(cx, move |c| c.categories.swap(i, i - 1));
                }
            })))
            .child(button("down", glyph::DOWN, i + 1 < count).on_click(cx.listener(move |this, _, _, cx| {
                if i + 1 < count {
                    this.reorder(i + 1, i);
                    this.edit(cx, move |c| c.categories.swap(i, i + 1));
                }
            })))
            .child(
                button("delete", glyph::DELETE, true)
                    .when(deleting, |d| d.w_auto().px_2().text_color(rgb(DANGER)).child(t().confirm))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if this.confirmed(Confirm::DeleteCategory(i), cx) {
                            this.edit(cx, move |c| _ = c.categories.remove(i));
                        }
                    })),
            )
    }

    fn render_footer(&self, s: &Search, cx: &Context<Self>) -> Div {
        div()
            .h(px(52.))
            .flex_none()
            .flex()
            .items_center()
            .justify_between()
            .px_3()
            .border_t_1()
            .border_color(rgb(BORDER))
            .child(
                div()
                    .id("open-search")
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .py_1p5()
                    .rounded(px(RADIUS))
                    .hover(|d| d.bg(rgb(SURFACE_HOVER)))
                    .on_click(cx.listener(|this, _, window, cx| this.open_search(window, cx)))
                    .child(div().text_color(rgb(ACCENT_TEXT)).child(icon(glyph::SEARCH)))
                    .child(t().search)
                    .child(
                        div()
                            .px_1p5()
                            .rounded(px(4.))
                            .border_1()
                            .border_color(rgb(BORDER))
                            .text_size(px(11.))
                            .text_color(rgb(TEXT_DIM))
                            .child(s.config.settings.hotkey.clone()),
                    ),
            )
            .child(
                div()
                    .id("quit")
                    .size(px(32.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(RADIUS))
                    .text_color(rgb(TEXT_DIM))
                    .hover(|d| d.bg(rgb(SURFACE_HOVER)).text_color(rgb(DANGER)))
                    .on_click(cx.listener(|_, _, _, cx| {
                        cx.remove_global::<platform::Platform>(); // retire l'icône du tray avant de sortir
                        cx.quit();
                    }))
                    .child(icon(glyph::POWER)),
            )
    }
}

impl Render for Panel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tab = self.tab as usize;
        let (page_pill, chip_pill) = (self.page_pill.sliding(window, tab), self.chip_pill.sliding(window, self.category));
        self.page_change.animate(window, 0);
        self.category_change.animate(window, TILE_STAGGER_MAX);
        if let Some((_, _, _, at)) = self.reordered
            && motion::progress(window, at, motion::FILED) >= 1.
        {
            self.reordered = None;
        }
        let cx: &Context<Self> = cx;
        let root = div()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::on_key))
            .size_full()
            .flex()
            .flex_col()
            .bg(aurora::card()) // même voile violet que la barre
            .text_color(rgb(TEXT))
            .text_size(px(14.))
            .font_family(FONT);
        let Ok(s) = self.search.read(cx) else { return root };
        let body = match self.tab {
            Tab::Apps => self.render_apps(s, chip_pill, cx),
            Tab::Settings => self.render_settings(s, window, cx),
        };
        root.child(
            div()
                .size_full()
                .flex()
                .flex_col()
                .child(self.render_header(page_pill, cx))
                // Apps ↔ Réglages : la page arrive du côté où va la pastille.
                .child(div().id("body").flex_1().min_h_0().overflow_y_scroll().p_4().child(self.page_change.arriving(0, div().id("page").child(body))))
                .child(self.render_footer(s, cx)),
        )
    }
}

/// Glyphe de la police d'icônes de Windows.
fn icon(glyph: &'static str) -> Div {
    div().font_family(glyph::FONT).text_size(px(14.)).child(glyph)
}

fn section(title: &'static str) -> Div {
    div().flex().flex_col().gap_1p5().child(
        div().px_2().text_size(px(11.)).font_weight(gpui::FontWeight::SEMIBOLD).text_color(rgb(TEXT_DIM)).child(title.to_uppercase()),
    )
}

fn setting_row(id: &'static str, label: &'static str) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_between()
        .gap_3()
        .px_2()
        .py_1p5()
        .rounded(px(RADIUS))
        .hover(|d| d.bg(rgb(SURFACE)))
        .child(label)
}

fn action_row(id: &'static str, glyph: &'static str, label: &'static str, color: u32) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .gap_2()
        .px_2()
        .py_1p5()
        .rounded(px(RADIUS))
        .text_color(rgb(color))
        .hover(|d| d.bg(rgb(SURFACE_HOVER)))
        .child(div().text_color(rgb(TEXT_DIM)).child(icon(glyph)))
        .child(label)
}

/// Interrupteur : 0 = éteint, 1 = allumé, entre les deux pendant qu'il glisse.
fn switch(on: f32) -> Div {
    div()
        .w(px(36.))
        .h(px(20.))
        .flex_none()
        .flex()
        .items_center()
        .p(px(2.))
        .rounded_full()
        .bg(motion::mix(BORDER, ACCENT, on))
        .child(div().ml(px(16. * on)).size(px(16.)).rounded_full().bg(motion::mix(TEXT_DIM, 0xffffff, on)))
}

/// Combinaison tapée → format de `global-hotkey` (« Ctrl+Shift+K »). None tant qu'elle n'est pas
/// utilisable : il faut Ctrl, Alt ou Super, plus une lettre, un chiffre, Espace ou F1…F24.
fn hotkey_spec(k: &Keystroke) -> Option<String> {
    let m = &k.modifiers;
    if !(m.control || m.alt || m.platform) {
        return None;
    }
    let key = match k.key.as_str() {
        "space" => "Space".to_string(),
        key if key.len() == 1 && key.chars().all(|c| c.is_ascii_alphanumeric()) => key.to_uppercase(),
        key if key.starts_with('f') && key[1..].parse::<u8>().is_ok_and(|n| (1..=24).contains(&n)) => key.to_uppercase(),
        key => crate::ui::digit(key)?.to_string(),
    };
    let mods = [(m.control, "Ctrl"), (m.alt, "Alt"), (m.shift, "Shift"), (m.platform, "Super")];
    let mut parts: Vec<&str> = mods.iter().filter(|(on, _)| *on).map(|(_, name)| *name).collect();
    parts.push(&key);
    Some(parts.join("+"))
}

#[test]
fn hotkey_specs() {
    let spec = |s: &str| hotkey_spec(&Keystroke::parse(s).unwrap());
    assert_eq!(spec("alt-space").as_deref(), Some("Alt+Space"));
    assert_eq!(spec("ctrl-shift-k").as_deref(), Some("Ctrl+Shift+K"));
    assert_eq!(spec("alt-f1").as_deref(), Some("Alt+F1"));
    assert_eq!(spec("ctrl-&").as_deref(), Some("Ctrl+1")); // AZERTY
    assert_eq!(spec("k"), None); // pas de modificateur
    assert_eq!(spec("shift-k"), None); // Maj seule ne suffit pas
    assert_eq!(spec("ctrl-enter"), None);
}
