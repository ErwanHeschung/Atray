// Pas de console en release ; les messages (eprintln) restent visibles en redirigeant stderr.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(not(windows))]
compile_error!("Atray ne cible que Windows pour l'instant");

mod field;
mod i18n;
mod panel;
mod slide;
#[cfg_attr(windows, path = "platform/windows.rs")]
mod platform;
mod theme;
mod ui;

use std::time::Instant;

use futures::StreamExt;
use gpui::{
    App, Application, Bounds, WindowBounds, WindowKind, WindowOptions, point, prelude::*, px,
    size,
};
use panel::Panel;
use theme::aurora;
use ui::{Search, WINDOW_H, WINDOW_W, WIDTH};

pub enum PlatformEvent {
    /// Hotkey pressée (id global-hotkey) ; l'instant sert à mesurer la latence jusqu'au rendu.
    Hotkey(u32, Instant),
    Open,
    /// Clic gauche sur l'icône du tray : ouvre ou ferme le panneau, placé contre l'icône.
    Panel(platform::Anchor),
    Quit,
}

fn main() {
    // Déjà lancé : l'instance en place vient d'ouvrir sa barre.
    let Some(instance) = platform::single_instance() else { return };

    Application::new().run(|cx: &mut App| {
        let (tx, mut rx) = futures::channel::mpsc::unbounded();
        // Créés sur le thread de gpui : sa boucle GetMessageW leur distribue WM_HOTKEY et les messages du tray.
        cx.set_global(platform::start(tx, instance));

        // Carte centrée horizontalement, au quart de la hauteur ; la fenêtre (transparente) déborde
        // de MARGIN tout autour pour la lueur.
        let display = cx.primary_display().expect("un écran").bounds();
        let origin = point(
            display.center().x - px(WIDTH / 2. + aurora::MARGIN),
            display.origin.y + display.size.height * 0.25 - px(aurora::MARGIN),
        );
        let handle = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::new(origin, size(px(WINDOW_W), px(WINDOW_H))))),
                    // Pas `Transparent` : gpui y ajoute un fond « dégradé transparent » de DWM qui assombrit
                    // tout le rectangle. Son rendu (DirectComposition, alpha prémultiplié) suffit : la carte
                    // et sa lueur sont dessinées, le reste reste vraiment transparent.
                    titlebar: None,
                    kind: WindowKind::PopUp,
                    show: false,
                    focus: false,
                    is_movable: false,
                    is_resizable: false,
                    is_minimizable: false,
                    ..Default::default()
                },
                |window, cx| {
                    platform::style_frameless(window);
                    cx.new(|cx| Search::new(window, cx))
                },
            )
            .expect("fenêtre de recherche");

        // Panneau du tray : créé caché, placé contre l'icône à chaque ouverture.
        let corner = point(display.origin.x + display.size.width - px(panel::WIDTH + 12.), display.origin.y + display.size.height - px(panel::HEIGHT + 60.));
        let panel = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::new(corner, size(px(panel::WIDTH), px(panel::HEIGHT))))),
                    titlebar: None,
                    kind: WindowKind::PopUp,
                    show: false,
                    focus: false,
                    is_movable: false,
                    is_resizable: false,
                    is_minimizable: false,
                    ..Default::default()
                },
                |window, cx| {
                    platform::style_popup(window);
                    cx.new(|cx| Panel::new(handle, window, cx))
                },
            )
            .expect("panneau du tray");

        cx.spawn(async move |cx| {
            while let Some(ev) = rx.next().await {
                match ev {
                    PlatformEvent::Hotkey(id, at) => {
                        _ = handle.update(cx, |s, w, cx| {
                            if let Some(tab) = platform::hotkey_tab(cx, id) {
                                s.toggle(at, tab, w, cx);
                            }
                        })
                    }
                    PlatformEvent::Open => _ = handle.update(cx, |s, w, cx| s.show(Instant::now(), 0, w, cx)),
                    PlatformEvent::Panel(anchor) => _ = panel.update(cx, |p, w, cx| p.toggle(anchor, w, cx)),
                    PlatformEvent::Quit => {
                        _ = cx.update(|cx| {
                            cx.remove_global::<platform::Platform>(); // retire l'icône du tray avant de sortir
                            cx.quit();
                        });
                    }
                }
            }
        })
        .detach();
    });
}
