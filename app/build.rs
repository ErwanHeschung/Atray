fn main() {
    // Icône (ressource n° 1) : celle de l'exe, et celle du tray via Icon::from_resource(1).
    // Seulement avec MSVC : le cross-check depuis WSL (cible gnu) n'a pas de compilateur de ressources.
    if std::env::var("TARGET").is_ok_and(|t| t.contains("msvc")) {
        embed_resource::compile("atray.rc", embed_resource::NONE).manifest_required().unwrap();
    }
}
