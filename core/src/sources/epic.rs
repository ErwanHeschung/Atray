//! Epic Games Launcher : un manifeste JSON `.item` par jeu installé.
use std::{fs, path::PathBuf};

use serde::Deserialize;

use crate::{Item, Launch};

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Manifest {
    display_name: String,
    app_name: String,
    #[serde(default)]
    main_game_app_name: String,
    #[serde(default)]
    catalog_namespace: String,
    #[serde(default)]
    catalog_item_id: String,
    #[serde(default)]
    install_location: String,
    #[serde(default)]
    launch_executable: String,
    #[serde(rename = "bIsIncompleteInstall", default)]
    incomplete: bool,
}

pub fn scan() -> Vec<Item> {
    let Some(data) = std::env::var_os("PROGRAMDATA") else { return vec![] };
    let dir = PathBuf::from(data).join(r"Epic\EpicGamesLauncher\Data\Manifests");
    let Ok(files) = fs::read_dir(dir) else { return vec![] };
    files
        .flatten()
        .filter(|f| f.path().extension().is_some_and(|e| e == "item"))
        .filter_map(|f| parse(&fs::read_to_string(f.path()).ok()?))
        .collect()
}

fn parse(json: &str) -> Option<Item> {
    let m: Manifest = serde_json::from_str(json.trim_start_matches('\u{feff}')).ok()?;
    let dlc = !m.main_game_app_name.is_empty() && m.main_game_app_name != m.app_name;
    if m.incomplete || dlc {
        return None;
    }
    // Le launcher identifie le jeu par namespace:item:appname (« : » encodé en %3A).
    let uri = format!(
        "com.epicgames.launcher://apps/{}%3A{}%3A{}?action=launch&silent=true",
        m.catalog_namespace, m.catalog_item_id, m.app_name
    );
    let exe = (!m.launch_executable.is_empty()).then(|| PathBuf::from(&m.install_location).join(&m.launch_executable));
    Some(Item { id: format!("epic:{}", m.app_name), name: m.display_name, launch: Launch::Uri(uri), icon: exe })
}

#[test]
fn reads_manifest() {
    let json = r#"{"DisplayName":"Fortnite","AppName":"Fortnite","MainGameAppName":"","CatalogNamespace":"fn",
        "CatalogItemId":"4fe7","bIsIncompleteInstall":false,"InstallLocation":"C:\\Games\\Fortnite",
        "LaunchExecutable":"FortniteGame/Binaries/Win64/FortniteBootstrapper.exe","Unknown":1}"#;
    let item = parse(json).unwrap();
    assert_eq!(item.id, "epic:Fortnite");
    assert!(matches!(item.launch, Launch::Uri(ref u)
        if u == "com.epicgames.launcher://apps/fn%3A4fe7%3AFortnite?action=launch&silent=true"));
    assert!(item.icon.unwrap().ends_with("FortniteBootstrapper.exe"));

    let dlc = r#"{"DisplayName":"DLC","AppName":"Dlc","MainGameAppName":"Fortnite"}"#;
    assert!(parse(dlc).is_none());
    let partial = r#"{"DisplayName":"X","AppName":"X","bIsIncompleteInstall":true}"#;
    assert!(parse(partial).is_none());
}
