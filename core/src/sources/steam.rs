//! Steam : `libraryfolders.vdf` donne les bibliothèques, chaque `appmanifest_<id>.acf` un jeu installé.
use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::{Item, Launch};

/// Outils que Steam installe comme des « jeux ».
const NOT_GAMES: [&str; 4] = ["Steamworks Common Redistributables", "Proton", "Steam Linux Runtime", "SteamVR"];
/// Bit « entièrement installé » de StateFlags.
const FULLY_INSTALLED: u32 = 4;

pub fn scan() -> Vec<Item> {
    let Some(root) = root() else { return vec![] };
    let Ok(folders) = fs::read_to_string(root.join("steamapps").join("libraryfolders.vdf")) else { return vec![] };
    let cache = root.join("appcache").join("librarycache");
    library_paths(&folders)
        .into_iter()
        .filter_map(|lib| fs::read_dir(PathBuf::from(lib).join("steamapps")).ok())
        .flatten()
        .flatten()
        .filter(|f| f.file_name().to_str().is_some_and(|n| n.starts_with("appmanifest_") && n.ends_with(".acf")))
        .filter_map(|f| parse_manifest(&fs::read_to_string(f.path()).ok()?, &cache))
        .collect()
}

#[cfg(windows)]
fn root() -> Option<PathBuf> {
    let from_registry = windows_registry::CURRENT_USER
        .open(r"Software\Valve\Steam")
        .and_then(|k| k.get_string("SteamPath"))
        .ok()
        .map(PathBuf::from);
    let default = std::env::var_os("PROGRAMFILES(X86)").map(|p| PathBuf::from(p).join("Steam"));
    from_registry.into_iter().chain(default).find(|p| p.is_dir())
}

// ponytail: Linux (~/.steam/steam) au moment du portage.
#[cfg(not(windows))]
fn root() -> Option<PathBuf> {
    None
}

/// Chaînes entre guillemets d'un fichier VDF texte, dans l'ordre (les accolades sont ignorées).
fn strings(vdf: &str) -> Vec<String> {
    let mut out = vec![];
    let mut chars = vdf.chars();
    while let Some(c) = chars.next() {
        if c != '"' {
            continue;
        }
        let mut s = String::new();
        while let Some(c) = chars.next() {
            match c {
                '"' => break,
                '\\' => s.extend(chars.next()), // \\ → \ et \" → "
                c => s.push(c),
            }
        }
        out.push(s);
    }
    out
}

/// Première valeur qui suit `key`. Lecture à plat : suffisant pour les clés de premier niveau.
fn value<'a>(strings: &'a [String], key: &str) -> Option<&'a str> {
    strings.windows(2).find(|w| w[0].eq_ignore_ascii_case(key)).map(|w| w[1].as_str())
}

fn library_paths(vdf: &str) -> Vec<String> {
    let s = strings(vdf);
    s.windows(2).filter(|w| w[0] == "path").map(|w| w[1].clone()).collect()
}

fn parse_manifest(acf: &str, cache: &Path) -> Option<Item> {
    let s = strings(acf);
    let id = value(&s, "appid")?;
    let name = value(&s, "name")?;
    let flags: u32 = value(&s, "StateFlags")?.parse().ok()?;
    if flags & FULLY_INSTALLED == 0 || NOT_GAMES.iter().any(|n| name.starts_with(n)) {
        return None;
    }
    Some(Item {
        id: format!("steam:{id}"),
        name: name.into(),
        launch: Launch::Uri(format!("steam://rungameid/{id}")),
        icon: icon(cache, id),
    })
}

/// L'icône du jeu dans le cache de Steam : `librarycache/<id>/<sha1>.jpg` (format actuel),
/// sinon `librarycache/<id>_icon.jpg` (ancien format).
fn icon(cache: &Path, id: &str) -> Option<PathBuf> {
    let is_sha1 = |p: &Path| {
        p.extension().is_some_and(|e| e == "jpg")
            && p.file_stem().and_then(|s| s.to_str()).is_some_and(|s| s.len() == 40 && s.bytes().all(|b| b.is_ascii_hexdigit()))
    };
    fs::read_dir(cache.join(id))
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .find(|p| is_sha1(p))
        .or_else(|| Some(cache.join(format!("{id}_icon.jpg"))).filter(|p| p.is_file()))
}

#[test]
fn reads_libraries_and_manifests() {
    let folders = r#""libraryfolders"
{
	"0"
	{
		"path"		"C:\\Program Files (x86)\\Steam"
		"label"		""
		"apps" { "1030300" "8207380004" }
	}
	"1" { "path" "D:\\SteamLibrary" }
}"#;
    assert_eq!(library_paths(folders), [r"C:\Program Files (x86)\Steam", r"D:\SteamLibrary"]);

    let acf = |flags: &str, name: &str| {
        format!("\"AppState\"\n{{\n\t\"appid\"\t\t\"1030300\"\n\t\"name\"\t\t\"{name}\"\n\t\"StateFlags\"\t\t\"{flags}\"\n\t\"UserConfig\" {{ \"language\" \"french\" }}\n}}")
    };
    let item = parse_manifest(&acf("4", "Hollow Knight: Silksong"), Path::new("nowhere")).unwrap();
    assert_eq!(item.id, "steam:1030300");
    assert_eq!(item.name, "Hollow Knight: Silksong");
    assert!(matches!(item.launch, Launch::Uri(ref u) if u == "steam://rungameid/1030300"));
    assert!(parse_manifest(&acf("6", "En mise à jour"), Path::new("nowhere")).is_some());
    assert!(parse_manifest(&acf("2", "Pas fini"), Path::new("nowhere")).is_none());
    assert!(parse_manifest(&acf("4", "Steamworks Common Redistributables"), Path::new("nowhere")).is_none());
}
