//! Design system Atray : noir et violet. Toutes les couleurs, tailles et glyphes passent par ici.
//! Contrastes sur BG : TEXT ≈ 17:1, TEXT_DIM ≈ 5.6:1, ACCENT_TEXT ≈ 10:1.

// Surfaces, du plus profond au plus haut.
pub const BG: u32 = 0x0d0b12;
pub const SURFACE: u32 = 0x17141f;
pub const SURFACE_HOVER: u32 = 0x221d2e;
pub const BORDER: u32 = 0x2b2440;

// Texte.
pub const TEXT: u32 = 0xede9f6;
pub const TEXT_DIM: u32 = 0x8e87a3;

// Violet.
pub const ACCENT: u32 = 0x8b5cf6;
/// Violet clair : texte et icônes d'accent sur fond sombre.
pub const ACCENT_TEXT: u32 = 0xc4b5fd;
/// RGBA : fond de la sélection, puce active.
pub const ACCENT_SOFT: u32 = 0x8b5cf62e;

pub const DANGER: u32 = 0xf87171;
/// Bordure de fenêtre tracée par Windows 11 (format COLORREF 0x00BBGGRR de BORDER).
pub const WINDOW_BORDER_COLORREF: u32 = 0x00402b24;

pub const RADIUS: f32 = 8.;
pub const FONT: &str = "Segoe UI";

/// Glyphes de la police d'icônes de Windows (présente sur Windows 10 et 11).
pub mod glyph {
    pub const FONT: &str = "Segoe MDL2 Assets";
    pub const SEARCH: &str = "\u{E721}";
    pub const POWER: &str = "\u{E7E8}";
    pub const UP: &str = "\u{E70E}";
    pub const DOWN: &str = "\u{E70D}";
    pub const DELETE: &str = "\u{E74D}";
    pub const ADD: &str = "\u{E710}";
    pub const REFRESH: &str = "\u{E72C}";
    pub const DOCUMENT: &str = "\u{E8A5}";
    pub const CHECK: &str = "\u{E73E}";
}

/// Mouvements : la barre apparaît et réagit sans attendre ; le mouvement ne sert qu'à montrer
/// qu'un état change (pastille d'onglet, interrupteur) ou qu'une action est prise en compte.
/// Coupés si Windows a « Effets d'animation » désactivé (les accusés restent visibles, sans mouvement).
pub mod motion {
    use std::{
        sync::atomic::{AtomicBool, Ordering},
        time::{Duration, Instant},
    };

    /// La pastille de l'onglet actif glisse vers le nouvel onglet.
    pub const PILL: Duration = Duration::from_millis(160);
    /// Lancement : la ligne s'allume, puis la barre se ferme.
    pub const PULSE: Duration = Duration::from_millis(120);
    /// Rangement : le nom de la catégorie s'allume puis revient au gris.
    pub const FILED: Duration = Duration::from_millis(700);
    pub const SWITCH: Duration = Duration::from_millis(140);
    /// Changement de catégorie : les lignes arrivent du côté où part la pastille, l'une après l'autre.
    pub const TAB: Duration = Duration::from_millis(180);
    pub const TAB_STAGGER: Duration = Duration::from_millis(14);
    pub const TAB_SHIFT: f32 = 18.;
    /// Catégorie montée ou descendue : les deux lignes échangent leur place en glissant.
    pub const REORDER: Duration = Duration::from_millis(200);

    static ENABLED: AtomicBool = AtomicBool::new(true);

    pub fn set_enabled(on: bool) {
        ENABLED.store(on, Ordering::Relaxed);
    }

    pub fn enabled() -> bool {
        ENABLED.load(Ordering::Relaxed)
    }

    /// Avancement 0→1 (ease-out cubique) depuis `since` (0 tant que `since` est dans le futur).
    pub fn eased(since: Instant, duration: Duration) -> f32 {
        let t = since.elapsed().as_secs_f32() / duration.as_secs_f32();
        if !enabled() || t >= 1. { 1. } else { 1. - (1. - t).powi(3) }
    }

    /// Comme `eased`, et redemande une image tant que ce n'est pas fini.
    pub fn progress(window: &gpui::Window, since: Instant, duration: Duration) -> f32 {
        let t = eased(since, duration);
        if t < 1. {
            window.request_animation_frame();
        }
        t
    }

    pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
        a + (b - a) * t
    }

    /// Mélange de deux couleurs 0xRRGGBB.
    pub fn mix(a: u32, b: u32, t: f32) -> gpui::Rgba {
        let ch = |c: u32, s: u32| ((c >> s) & 0xff) as f32;
        let m = |s| lerp(ch(a, s), ch(b, s), t).round() as u32;
        gpui::rgb((m(16) << 16) | (m(8) << 8) | m(0))
    }
}

/// Aurora : la carte flotte sur une lueur violette qui s'avive quand on tape et retombe au repos.
/// La lumière sert aussi d'accusé : ligne sélectionnée éclairée par la gauche, éclat au lancement.
pub mod aurora {
    use gpui::{Background, BoxShadow, Hsla, linear_color_stop, linear_gradient, point, px, rgb};

    /// Place transparente autour de la carte, pour que la lueur ne soit pas coupée.
    pub const MARGIN: f32 = 60.;
    pub const RADIUS: f32 = 14.;
    /// Haut de la carte, teinté violet, qui se fond dans BG.
    const TOP: u32 = 0x1b1530;
    /// Opacité de la lueur : au repos, et au maximum.
    const IDLE: f32 = 0.32;
    const PEAK: f32 = 0.85;
    /// Énergie (0…1) donnée par une ouverture, une frappe, un lancement ; elle retombe en DECAY s.
    pub const ON_OPEN: f32 = 0.6;
    pub const ON_KEY: f32 = 0.2;
    pub const ON_LAUNCH: f32 = 1.;
    pub const DECAY: f32 = 0.6;
    /// Lumière de la ligne sélectionnée, et au lancement.
    pub const LIT: f32 = 0.32;
    pub const LIT_LAUNCH: f32 = 0.75;

    fn accent(alpha: f32) -> Hsla {
        Hsla::from(rgb(super::ACCENT)).opacity(alpha)
    }

    /// Fond de la carte : un voile violet en haut.
    pub fn card() -> Background {
        linear_gradient(180., linear_color_stop(rgb(TOP), 0.), linear_color_stop(rgb(super::BG), 0.55))
    }

    /// Ombre sombre (profondeur) + lueur violette dont l'intensité suit l'énergie.
    pub fn glow(energy: f32) -> Vec<BoxShadow> {
        let a = IDLE + (PEAK - IDLE) * energy;
        vec![
            BoxShadow { color: gpui::black().opacity(0.55), offset: point(px(0.), px(10.)), blur_radius: px(28.), spread_radius: px(-6.) },
            // Le flou reste dans MARGIN : la lueur n'est jamais coupée au bord de la fenêtre.
            BoxShadow { color: accent(a), offset: point(px(0.), px(8.)), blur_radius: px(44.), spread_radius: px(2.) },
        ]
    }

    /// Ligne éclairée depuis la gauche.
    pub fn lit(alpha: f32) -> Background {
        linear_gradient(90., linear_color_stop(accent(alpha), 0.), linear_color_stop(accent(0.), 0.85))
    }
}
