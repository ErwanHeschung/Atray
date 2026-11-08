//! GOG : les jeux installés sont dans le registre.
use crate::Item;

#[cfg(windows)]
pub fn scan() -> Vec<Item> {
    use std::path::PathBuf;

    use windows_registry::LOCAL_MACHINE;

    use crate::Launch;

    let Ok(games) = LOCAL_MACHINE.open(r"SOFTWARE\WOW6432Node\GOG.com\Games") else { return vec![] };
    let Ok(ids) = games.keys() else { return vec![] };
    // Passer par Galaxy garde la synchro des sauvegardes, comme les raccourcis créés par GOG.
    let galaxy = LOCAL_MACHINE
        .open(r"SOFTWARE\WOW6432Node\GOG.com\GalaxyClient\paths")
        .and_then(|k| k.get_string("client"))
        .ok()
        .map(|dir| PathBuf::from(dir).join("GalaxyClient.exe"))
        .filter(|p| p.is_file());

    ids.filter_map(|id| {
        let key = games.open(&id).ok()?;
        let name = key.get_string("gameName").ok()?;
        let exe = PathBuf::from(key.get_string("exe").ok().filter(|e| !e.is_empty())?); // vide = DLC
        let dir = key.get_string("path").ok().map(PathBuf::from);
        let launch = match &galaxy {
            Some(client) => Launch::Cmd {
                program: client.clone(),
                args: vec!["/command=runGame".into(), format!("/gameId={id}")],
                dir: None,
            },
            None => Launch::Cmd { program: exe.clone(), args: vec![], dir },
        };
        Some(Item { id: format!("gog:{id}"), name, launch, icon: Some(exe) })
    })
    .collect()
}

#[cfg(not(windows))]
pub fn scan() -> Vec<Item> {
    vec![]
}
