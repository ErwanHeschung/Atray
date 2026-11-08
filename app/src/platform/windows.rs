use std::{path::Path, sync::Arc, time::Instant};

use futures::channel::mpsc::UnboundedSender;
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState, hotkey::HotKey};
use gpui::{App, Global, RenderImage, Window};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use tray_icon::{
    Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
    menu::{Menu, MenuEvent, MenuItem},
};
use windows::{
    Win32::{
        Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE, HWND, POINT, SIZE},
        Graphics::Dwm::{DWMNCRP_DISABLED, DWMWA_BORDER_COLOR, DWMWA_COLOR_NONE, DWMWA_NCRENDERING_POLICY, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DONOTROUND, DWMWCP_ROUND, DwmSetWindowAttribute},
        Graphics::Gdi::{
            BITMAP, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, DeleteObject, GetDC, GetDIBits, GetMonitorInfoW,
            GetObjectW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint, ReleaseDC,
        },
        Globalization::GetUserDefaultUILanguage,
        System::{
            Com::{COINIT_APARTMENTTHREADED, CoInitializeEx},
            Threading::{CreateEventW, INFINITE, SetEvent, WaitForSingleObject},
            Time::{GetTimeZoneInformation, TIME_ZONE_INFORMATION},
        },
        UI::Shell::{IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_ICONONLY, SIIGBF_RESIZETOFIT},
        UI::WindowsAndMessaging::{
            ASFW_ANY, AllowSetForegroundWindow, GetCaretBlinkTime, GetForegroundWindow, SPI_GETCLIENTAREAANIMATION, SPI_GETWHEELSCROLLLINES,
            SW_HIDE, SW_SHOW, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
            SetForegroundWindow, SetWindowPos, ShowWindow, SystemParametersInfoW,
        },
    },
    core::{HSTRING, w},
};

use crate::{PlatformEvent, i18n::t, theme};

/// Global gpui : la lâcher (remove_global) désenregistre la hotkey et retire l'icône.
pub struct Platform {
    hotkeys: GlobalHotKeyManager,
    /// Hotkeys enregistrées → onglet (0 = barre principale, n = catégorie n-1).
    bound: Vec<(HotKey, usize)>,
    tray: TrayIcon,
}

impl Global for Platform {}

/// Rectangle de l'icône dans la zone de notification, en pixels physiques.
#[derive(Clone, Copy)]
pub struct Anchor {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// Événement nommé : sa présence signale qu'Atray tourne, et un second lancement
/// s'en sert pour demander à l'instance en place d'ouvrir sa barre.
pub struct Instance(HANDLE);

/// None si Atray tourne déjà : on vient alors de lui demander d'ouvrir sa barre.
pub fn single_instance() -> Option<Instance> {
    unsafe {
        let h = CreateEventW(None, false, false, w!("Local\\AtrayShow")).ok()?;
        if GetLastError() == ERROR_ALREADY_EXISTS {
            // Nous avons le droit au premier plan (l'utilisateur vient de nous lancer) : on le cède.
            _ = AllowSetForegroundWindow(ASFW_ANY);
            _ = SetEvent(h);
            _ = CloseHandle(h);
            return None;
        }
        Some(Instance(h))
    }
}

/// La hotkey est enregistrée ensuite, par `set_hotkey`, depuis les réglages.
pub fn start(tx: UnboundedSender<PlatformEvent>, instance: Instance) -> Platform {
    let raw = instance.0.0 as usize; // HANDLE n'est pas Send ; l'événement reste ouvert jusqu'à la fin
    let sender = tx.clone();
    std::thread::spawn(move || {
        while unsafe { WaitForSingleObject(HANDLE(raw as *mut _), INFINITE) }.0 == 0 {
            if sender.unbounded_send(PlatformEvent::Open).is_err() {
                break;
            }
        }
    });

    let hotkeys = GlobalHotKeyManager::new().expect("gestionnaire de hotkeys");
    let sender = tx.clone();
    GlobalHotKeyEvent::set_event_handler(Some(move |e: GlobalHotKeyEvent| {
        if e.state == HotKeyState::Pressed {
            _ = sender.unbounded_send(PlatformEvent::Hotkey(e.id, Instant::now()));
        }
    }));

    // Clic droit : mini menu natif, en secours. Ids fixes : il est reconstruit au changement de langue.
    let sender = tx.clone();
    MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
        let ev = match e.id.as_ref() {
            "open" => PlatformEvent::Open,
            "quit" => PlatformEvent::Quit,
            _ => return,
        };
        _ = sender.unbounded_send(ev);
    }));
    // Clic gauche : le panneau.
    TrayIconEvent::set_event_handler(Some(move |e: TrayIconEvent| {
        if let TrayIconEvent::Click { rect, button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
            _ = tx.unbounded_send(PlatformEvent::Panel(Anchor {
                x: rect.position.x as i32,
                y: rect.position.y as i32,
                width: rect.size.width as i32,
                height: rect.size.height as i32,
            }));
        }
    }));

    let tray = TrayIconBuilder::new()
        .with_tooltip("Atray")
        .with_icon(Icon::from_resource(1, None).expect("icône embarquée par build.rs"))
        .with_menu(Box::new(menu()))
        .with_menu_on_left_click(false)
        .build()
        .expect("icône du tray");
    std::thread::spawn(promote_tray_icon);
    Platform { hotkeys, bound: vec![], tray }
}

/// Windows 11 range toute nouvelle icône dans le débordement (^). Tant que l'utilisateur n'a rien
/// choisi (pas de valeur IsPromoted), on la sort sur la barre ; s'il la masque ensuite, on n'y touche plus.
/// Explorer crée la clé peu après l'ajout de l'icône, d'où les quelques essais.
fn promote_tray_icon() {
    let Ok(exe) = std::env::current_exe() else { return };
    let exe = exe.to_string_lossy().to_lowercase();
    for _ in 0..10 {
        let found = (|| -> windows_registry::Result<bool> {
            let root = windows_registry::CURRENT_USER.open(r"Control Panel\NotifyIconSettings")?;
            for name in root.keys()? {
                let key = windows_registry::CURRENT_USER.create(format!(r"Control Panel\NotifyIconSettings\{name}"))?;
                if key.get_string("ExecutablePath").is_ok_and(|p| p.to_lowercase() == exe) {
                    if key.get_u32("IsPromoted").is_err() {
                        key.set_u32("IsPromoted", 1)?;
                    }
                    return Ok(true);
                }
            }
            Ok(false)
        })();
        if found.unwrap_or(false) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
}

/// Enregistre exactement ces hotkeys (onglet, « Alt+Space »…) ; spec vide = aucune.
/// N'enregistre que les nouvelles ; si l'une est refusée, l'ancienne du même onglet reste active.
pub fn set_hotkeys(cx: &mut App, wanted: &[(usize, &str)]) -> Vec<String> {
    let p = cx.global_mut::<Platform>();
    let (mut next, mut errors) = (Vec::<(HotKey, usize)>::new(), vec![]);
    for &(tab, spec) in wanted.iter().filter(|(_, s)| !s.is_empty()) {
        let result = spec.parse::<HotKey>().map_err(|e| e.to_string()).and_then(|hk| {
            if next.iter().any(|(h, _)| *h == hk) {
                return Err(t().hotkey_duplicate.to_string());
            }
            // Typiquement refusé parce qu'une autre appli (PowerToys Run…) tient déjà ce raccourci.
            if !p.bound.iter().any(|(h, _)| *h == hk) {
                p.hotkeys.register(hk).map_err(|e| e.to_string())?;
            }
            Ok(hk)
        });
        match result {
            Ok(hk) => next.push((hk, tab)),
            Err(e) => {
                errors.push(format!("« {spec} » : {e}"));
                let old = p.bound.iter().find(|(h, t)| *t == tab && !next.iter().any(|(n, _)| n == h));
                next.extend(old.copied());
            }
        }
    }
    for (old, _) in &p.bound {
        if !next.iter().any(|(h, _)| h == old) {
            _ = p.hotkeys.unregister(*old);
        }
    }
    p.bound = next;
    errors
}

pub fn hotkey_tab(cx: &App, id: u32) -> Option<usize> {
    cx.global::<Platform>().bound.iter().find(|(h, _)| h.id() == id).map(|(_, tab)| *tab)
}

/// Ajoute ou retire Atray de la clé « Run » de l'utilisateur ; n'écrit que si ça change.
pub fn set_autostart(enabled: bool) -> Result<(), String> {
    let key = windows_registry::CURRENT_USER
        .create(r"Software\Microsoft\Windows\CurrentVersion\Run")
        .map_err(|e| e.to_string())?;
    let current = key.get_string("Atray").ok();
    if enabled {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let value = format!("\"{}\"", exe.display());
        if current.as_deref() != Some(value.as_str()) {
            key.set_string("Atray", &value).map_err(|e| e.to_string())?;
        }
    } else if current.is_some() {
        key.remove_value("Atray").map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn rebuild_tray_menu(cx: &App) {
    if let Some(p) = cx.try_global::<Platform>() {
        p.tray.set_menu(Some(Box::new(menu())));
    }
}

fn menu() -> Menu {
    let menu = Menu::new();
    _ = menu.append(&MenuItem::with_id("open", t().open, true, None));
    _ = menu.append(&MenuItem::with_id("quit", t().quit, true, None));
    menu
}

fn hwnd(window: &Window) -> Option<HWND> {
    // `Window` a aussi une méthode inhérente `window_handle()` : appel qualifié obligatoire.
    let RawWindowHandle::Win32(h) = HasWindowHandle::window_handle(window).ok()?.as_raw() else { return None };
    Some(HWND(h.hwnd.get() as *mut _))
}

/// gpui 0.2.2 ne sait ni masquer ni passer au premier plan sous Windows : on le fait sur le HWND.
pub fn show(window: &Window) {
    if let Some(h) = hwnd(window) {
        // Autorisé : le processus qui reçoit WM_HOTKEY (ou le clic du tray) a le droit au premier plan.
        unsafe {
            _ = ShowWindow(h, SW_SHOW);
            _ = SetForegroundWindow(h);
        }
    }
}

pub fn hide(window: &Window) {
    if let Some(h) = hwnd(window) {
        unsafe { _ = ShowWindow(h, SW_HIDE) };
    }
}

pub fn is_foreground(window: &Window) -> bool {
    hwnd(window).is_some_and(|h| unsafe { GetForegroundWindow() } == h)
}

/// Réglage Windows « Effets d'animation » (Accessibilité → Effets visuels).
pub fn animations_enabled() -> bool {
    let mut on = windows::core::BOOL(1);
    let ptr = Some(&mut on as *mut _ as *mut core::ffi::c_void);
    unsafe { SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION, 0, ptr, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0)) }.is_err() || on.as_bool()
}

/// Lignes par cran de molette réglées dans Windows (gpui multiplie les crans par ce nombre).
pub fn wheel_lines() -> f32 {
    let mut lines = 3u32;
    let ptr = Some(&mut lines as *mut _ as *mut core::ffi::c_void);
    _ = unsafe { SystemParametersInfoW(SPI_GETWHEELSCROLLLINES, 0, ptr, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0)) };
    // 0 = molette coupée, u32::MAX = « une page » : on garde un cran = une ligne.
    if lines == 0 || lines == u32::MAX { 1. } else { lines as f32 }
}

/// Cadence de clignotement du curseur réglée dans Windows ; None = ne clignote pas.
pub fn caret_blink() -> Option<std::time::Duration> {
    let ms = unsafe { GetCaretBlinkTime() };
    (ms != 0 && ms != u32::MAX).then(|| std::time::Duration::from_millis(ms as u64))
}

pub fn ui_is_french() -> bool {
    const LANG_FRENCH: u16 = 0x0c;
    let lang = unsafe { GetUserDefaultUILanguage() };
    lang & 0x3ff == LANG_FRENCH
}

/// Icône que l'Explorateur afficherait pour ce fichier (cible du raccourci comprise).
/// À appeler hors du thread de l'UI : le shell peut mettre quelques ms par fichier.
pub fn file_icon(path: &Path, size: i32) -> Option<Arc<RenderImage>> {
    unsafe {
        // Sans effet si ce thread est déjà initialisé ; le shell exige COM.
        _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        // Le shell refuse les « / » (Steam et Epic en écrivent dans leurs chemins).
        let path = path.to_string_lossy().replace('/', "\\");
        let factory: IShellItemImageFactory = SHCreateItemFromParsingName(&HSTRING::from(&path), None).ok()?;
        // Une image (icône .jpg de Steam) : sa miniature, pas l'icône du type de fichier.
        let ext = Path::new(&path).extension().map(|e| e.to_ascii_lowercase());
        let is_image = ext.is_some_and(|e| e == "jpg" || e == "png");
        let flags = if is_image { SIIGBF_RESIZETOFIT } else { SIIGBF_ICONONLY };
        let bitmap = factory.GetImage(SIZE { cx: size, cy: size }, flags).ok()?;
        // Taille réelle : une miniature peut être plus petite que demandé.
        let mut bm = BITMAP::default();
        GetObjectW(bitmap.into(), size_of::<BITMAP>() as i32, Some((&raw mut bm).cast()));
        let (w, h) = (bm.bmWidth, bm.bmHeight);
        if w <= 0 || h <= 0 {
            _ = DeleteObject(bitmap.into());
            return None;
        }

        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h, // négatif = lignes de haut en bas
                biPlanes: 1,
                biBitCount: 32,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bgra = vec![0u8; (w * h * 4) as usize];
        let dc = GetDC(None);
        let rows = GetDIBits(dc, bitmap, 0, h as u32, Some(bgra.as_mut_ptr().cast()), &mut info, DIB_RGB_COLORS);
        ReleaseDC(None, dc);
        _ = DeleteObject(bitmap.into());
        if rows != h {
            return None;
        }
        // Le shell rend l'alpha prémultiplié, gpui l'attend « droit ».
        // Réglage : si les bords des icônes paraissent trop clairs, retirer cette boucle.
        for px in bgra.chunks_exact_mut(4) {
            let a = px[3] as u32;
            if a > 0 && a < 255 {
                for c in &mut px[..3] {
                    *c = (*c as u32 * 255 / a).min(255) as u8;
                }
            }
        }
        let frame = image::Frame::new(image::RgbaImage::from_raw(w as u32, h as u32, bgra)?);
        Some(Arc::new(RenderImage::new([frame])))
    }
}

/// Décalage de l'heure locale sur UTC, en secondes (heure d'été comprise).
pub fn utc_offset() -> i64 {
    let mut tz = TIME_ZONE_INFORMATION::default();
    const TIME_ZONE_ID_DAYLIGHT: u32 = 2;
    let extra = match unsafe { GetTimeZoneInformation(&mut tz) } {
        TIME_ZONE_ID_DAYLIGHT => tz.DaylightBias,
        _ => tz.StandardBias,
    };
    -((tz.Bias + extra) as i64) * 60 // Bias = UTC − local, en minutes
}

/// Coins arrondis et bordure violet sombre, tracés par Windows 11 (sans effet sur Windows 10).
pub fn style_popup(window: &Window) {
    let Some(h) = hwnd(window) else { return };
    unsafe {
        let round = DWMWCP_ROUND;
        _ = DwmSetWindowAttribute(h, DWMWA_WINDOW_CORNER_PREFERENCE, (&raw const round).cast(), size_of_val(&round) as u32);
        let border = theme::WINDOW_BORDER_COLORREF;
        _ = DwmSetWindowAttribute(h, DWMWA_BORDER_COLOR, (&raw const border).cast(), size_of_val(&border) as u32);
    }
}

/// Fenêtre transparente dont la carte dessine elle-même coins, bordure et ombre : rien de DWM.
pub fn style_frameless(window: &Window) {
    let Some(h) = hwnd(window) else { return };
    unsafe {
        let square = DWMWCP_DONOTROUND;
        _ = DwmSetWindowAttribute(h, DWMWA_WINDOW_CORNER_PREFERENCE, (&raw const square).cast(), size_of_val(&square) as u32);
        let none = DWMWA_COLOR_NONE;
        _ = DwmSetWindowAttribute(h, DWMWA_BORDER_COLOR, (&raw const none).cast(), size_of_val(&none) as u32);
        // Sans ça, DWM ombre tout le rectangle (transparent) de la fenêtre, loin autour de la carte.
        let off = DWMNCRP_DISABLED;
        _ = DwmSetWindowAttribute(h, DWMWA_NCRENDERING_POLICY, (&raw const off).cast(), size_of_val(&off) as u32);
    }
}

/// Place la fenêtre contre l'icône du tray, côté bureau (au-dessus d'une barre des tâches en bas,
/// en dessous d'une barre en haut…), sans dépasser la zone de travail, puis l'affiche.
/// `width`/`height` en pixels logiques.
pub fn show_near(window: &Window, anchor: Anchor, width: f32, height: f32) {
    let Some(h) = hwnd(window) else { return };
    let scale = window.scale_factor();
    let (w, hgt) = ((width * scale) as i32, (height * scale) as i32);
    let gap = (12. * scale) as i32;
    let mut info = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
    unsafe {
        let monitor = MonitorFromPoint(POINT { x: anchor.x, y: anchor.y }, MONITOR_DEFAULTTONEAREST);
        _ = GetMonitorInfoW(monitor, &mut info);
    }
    let work = info.rcWork;
    let center_x = anchor.x + anchor.width / 2;
    let x = (center_x - w / 2).clamp(work.left + gap, (work.right - w - gap).max(work.left + gap));
    // L'icône est dans la barre des tâches, donc hors de la zone de travail : on se colle au bord le plus proche.
    let y = if anchor.y >= work.bottom - gap {
        work.bottom - hgt - gap
    } else if anchor.y + anchor.height <= work.top + gap {
        work.top + gap
    } else {
        (anchor.y - hgt - gap).clamp(work.top + gap, (work.bottom - hgt - gap).max(work.top + gap))
    };
    unsafe {
        _ = SetWindowPos(h, None, x, y, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
        _ = ShowWindow(h, SW_SHOW);
        _ = SetForegroundWindow(h);
    }
}
