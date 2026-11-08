//! Raccourcis du menu Démarrer (utilisateur puis machine).
use std::{collections::HashSet, fs, path::{Path, PathBuf}};

use crate::{Item, Launch};

/// Raccourcis qui ne sont pas des applications (en / fr).
const NOISE: [&str; 6] = ["uninstall", "désinstaller", "desinstaller", "readme", "lisez-moi", "release notes"];

pub fn scan() -> Vec<Item> {
    let roots = ["APPDATA", "PROGRAMDATA"]
        .into_iter()
        .filter_map(std::env::var_os)
        .map(|base| PathBuf::from(base).join(r"Microsoft\Windows\Start Menu\Programs"));
    let mut seen = HashSet::new();
    let mut out = vec![];
    for root in roots {
        walk(&root, &mut |path| {
            // Le même raccourci existe souvent dans les deux dossiers : on garde le premier.
            if let Some(item) = item_for(path).filter(|i| seen.insert(i.id.clone())) {
                out.push(item);
            }
        });
    }
    out
}

fn walk(dir: &Path, f: &mut impl FnMut(PathBuf)) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let path = e.path();
        if e.file_type().is_ok_and(|t| t.is_dir()) {
            walk(&path, f);
        } else {
            f(path);
        }
    }
}

fn item_for(path: PathBuf) -> Option<Item> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    if ext != "lnk" && ext != "url" {
        return None;
    }
    let name = path.file_stem()?.to_str()?.to_string();
    let lower = name.to_lowercase();
    if NOISE.iter().any(|n| lower.contains(n)) {
        return None;
    }
    Some(Item { id: format!("start:{lower}"), name, launch: Launch::Open(path.clone()), icon: Some(path) })
}

#[test]
fn keeps_apps_skips_noise() {
    let item = |p: &str| item_for(PathBuf::from(p)).map(|i| i.name);
    assert_eq!(item("C:/SM/Firefox.lnk").as_deref(), Some("Firefox"));
    assert_eq!(item("C:/SM/Steam/Steam Support Center.URL").as_deref(), Some("Steam Support Center"));
    assert_eq!(item("C:/SM/7-Zip/Uninstall 7-Zip.lnk"), None);
    assert_eq!(item("C:/SM/Jeu/Désinstaller Jeu.lnk"), None);
    assert_eq!(item("C:/SM/desktop.ini"), None);
}
