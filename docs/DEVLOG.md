# Journal de développement

Notes de travail (en français) : étapes, mesures et pièges de gpui / Windows rencontrés en route.
La présentation du projet est dans le [README](../README.md).

## Build

Le build release doit tourner sous **Windows natif** (gpui compile ses shaders avec `fxc.exe`) :

```powershell
$env:CARGO_TARGET_DIR="$env:LOCALAPPDATA\atray-target"
cargo run --release --manifest-path <chemin>\atray\Cargo.toml
```

**Exe à distribuer** (portable, un seul fichier, icône embarquée, sans console) :
`cargo build --profile dist` (même `--manifest-path`) → `%LOCALAPPDATA%\atray-target\dist\atray.exe` (~7,7 Mo).
Les données vont dans `%APPDATA%\Atray\` (`config.toml`, `history.jsonl`). Logo source : `app/assets/logo.png`
(vortex sur fond noir) ; `python3 make_icon.py` depuis `app/assets` (ffmpeg requis) en tire `atray.ico`
(exe + tray, 16→256 px) et `atray-256.png` (en-tête du panneau) : noir → transparence, recadré sur le vortex,
bords fondus. Windows 11 range toute nouvelle icône du tray dans le débordement (^) : au démarrage, Atray met
`IsPromoted = 1` dans `HKCU\Control Panel\NotifyIconSettings\<id>` tant que l'utilisateur n'a rien choisi. Après changement : `touch app/atray.rc app/build.rs` pour réembarquer l'icône.

Depuis WSL, uniquement pour vérifier les types : `cargo check --target x86_64-pc-windows-gnu`.
Ne jamais lancer `cargo update` sans argument : libc reste épinglée en 0.2.186 (gpui 0.2.2 → xattr).

## Étape 0 — prototype hotkey + tray + gpui (même thread) — validée le 2026-11-08

| Critère | Résultat |
|---|---|
| Démarre, Alt+Espace enregistré sans conflit | ✅ (test automatique) |
| Instance unique (2ᵉ lancement → « Atray tourne déjà ») | ✅ (test automatique) |
| RAM au repos, fenêtre cachée | 61 Mo (working set) |
| 50 bascules Alt+Espace sans raté | ✅ |
| La frappe arrive dans la fenêtre (focus pris) | ✅ |
| Échap et clic ailleurs masquent la fenêtre | ✅ |
| Menu du tray (clic droit) : Ouvrir, Quitter ; clic gauche = Ouvrir | ✅ |
| Quitter retire l'icône et termine le processus | ✅ |
| Latence hotkey → rendu < 100 ms (ligne `hotkey -> rendu` sur stderr) | ✅ 5–9 ms (10 mesures) |

## Étape 1 — recherche + menu Démarrer + lancement — validée le 2026-11-08

- Index : raccourcis `.lnk`/`.url` du menu Démarrer (utilisateur + machine), sans les désinstalleurs ni les readme ; rescan à l'ouverture si le dernier date de plus de 60 s, ou « Réindexer » dans le tray.
- Recherche floue `nucleo-matcher` (initiales, accents ignorés, préfixe favorisé) ; ↑/↓, Entrée, clic.
- Interface FR/EN selon la langue de Windows. Chaque lancement est ajouté à `%APPDATA%\Atray\history.jsonl`.

## Étape 2 — icônes

Icône du shell pour chaque élément (miniature pour les `.jpg` de Steam), extraite en arrière-plan, cache en mémoire. 141 icônes en ~1 s.

## Étape 3 — Steam / Epic / GOG (à valider)

- Steam : `libraryfolders.vdf` + `appmanifest_*.acf` (installés seulement), lancement `steam://rungameid/<id>`.
- Epic : manifestes `.item` (sans DLC ni installations incomplètes), lancement `com.epicgames.launcher://…`.
- GOG : registre ; lancement via GOG Galaxy s'il est installé, sinon l'exe du jeu.
- Doublons du menu Démarrer retirés (comparaison des noms réduits aux lettres et chiffres).
- `cargo run -p atray-core --example scan` liste ce que trouvent les sources.

## Étape 4 — catégories

Onglets (Tab / Shift+Tab, Ctrl+1…9, Ctrl+0 = Tout), catégorie Jeux remplie automatiquement, clic droit ou Alt+1…9 pour ranger.
`%APPDATA%\Atray\config.toml`, relu à chaque ouverture, jamais écrasé s'il contient une erreur.

## Étape 5 — tray par catégorie + suggestions (à valider)

- Menu du tray : un sous-menu par catégorie (clic = lancement), « Effacer l'historique ».
- Score d'usage (`core/src/history.rs`) : Σ e^(−âge/14 j) · (1 + 0,5 si même créneau ±1 h + 0,3 si même type de jour).
- Barre vide sur « Tout » = suggestions ; dans la recherche, score = flou + 8 · ln(1 + usage).

## Étape 6 — réglages, démarrage avec Windows, packaging (à valider)

- `[settings]` dans `config.toml` (relu à chaque ouverture) : `hotkey`, `theme` (system/dark/light),
  `language` (auto/fr/en), `start_with_windows` (clé `HKCU\…\Run`), `disabled_sources`.
- Relancer l'exe quand Atray tourne ouvre sa barre (événement nommé `Local\AtrayShow`).
- Exe sans console en release, icône dans l'exe et le tray, profil `dist` avec LTO.

## Panneau du tray + design system noir/violet (à valider)

- Clic gauche sur l'icône : panneau (`app/src/panel.rs`) avec onglets Apps (grille par catégorie) et Réglages
  (hotkey capturée au clavier, langue, démarrage avec Windows, sources, catégories, données). Clic droit : mini menu natif.
- Tokens dans `app/src/theme.rs`, partagés par la barre et le panneau ; coins arrondis + bordure via DWM (Windows 11).
- Le réglage `theme` a disparu : noir/violet uniquement.

## Champ de recherche + hotkeys de catégorie (à valider)

- Vrai champ (`app/src/field.rs`) : curseur, sélection, ←/→, Maj, Ctrl (par mot), Début/Fin, Ctrl+A/C/X/V,
  Ctrl+Retour arrière / Ctrl+Suppr. Pas d'IME ni de touches mortes.
- Barre : Ctrl+1…9 = onglet, Ctrl+0 = Tout, Alt+1…9 = ranger le résultat sélectionné.
- `hotkey = "Alt+Shift+1"` par catégorie (capsule dans Réglages → Catégories, Suppr = aucune) : ouvre la barre
  sur cette catégorie ; ré-appuyer la ferme.

## Mouvement (à valider)

Principe : la barre apparaît et réagit sans attendre ; le mouvement ne sert qu'à montrer un changement
d'état ou à accuser réception d'une action. Tokens dans `theme::motion`.
- La pastille de l'onglet actif glisse vers le nouvel onglet (160 ms) ; le contenu change tout de suite.
- Entrée / clic : la ligne s'allume (120 ms), puis la barre se ferme.
- Alt+1…9 : le nom de la catégorie s'allume en violet puis revient au gris.
- Panneau : les interrupteurs glissent.
- Curseur qui clignote au rythme de Windows (plein pendant la frappe). Molette et Page↑/↓ dans les résultats.
- Retirés après essai : fondu/glissement d'ouverture, cascade des lignes, surlignage qui glisse,
  glissement du contenu entre onglets (ressentis comme de la latence).
- « Effets d'animation » coupés dans Windows : plus de mouvement, les accusés restent visibles.

## Direction « Aurora » (à valider)

- Barre : fenêtre fixe à fond transparent (`ui::WINDOW_W/H`, marge `aurora::MARGIN`) ; surtout pas
  `WindowBackgroundAppearance::Transparent` (DWM assombrit tout le rectangle), et rendu non-client DWM coupé
  (sinon ombre de cadre autour du rectangle). La carte dessine ses coins,
  sa bordure, un voile violet en haut et deux ombres (profondeur + lueur violette). Clic hors carte = fermeture.
- La hauteur de la carte glisse (160 ms) au lieu de redimensionner la fenêtre native à chaque frappe.
- Changement de catégorie : la pastille glisse, les lignes arrivent du même côté en fondu, l'une après l'autre (180 ms + 14 ms par ligne), la lueur s'avive un peu.
- Lueur : énergie 0…1 (ouverture 0,6, frappe +0,2, lancement 1) qui retombe en ~0,6 s ; au repos, plus aucune image.
- Ligne sélectionnée éclairée par la gauche (dégradé) ; au lancement la lumière s'intensifie.
- Panneau : même voile violet en fond. Tokens dans `theme::aurora`.
- Panneau : la pastille Apps/Réglages et celle des puces de catégorie glissent (`app/src/slide.rs`, partagé avec la
  barre) ; la page ou les tuiles arrivent du même côté. Catégorie montée/descendue : les deux lignes échangent leur
  place en glissant (écart relevé au clic), la ligne déplacée s'allume puis s'éteint.
- Tuiles : nom sur deux lignes ; gpui ne met pas de « … » sur du texte qui passe à la ligne, donc une 2ᵉ ligne
  trop longue reste coupée des deux côtés (ex. « Tom Clancy's Rainbow Six »).
- Mesuré : barre ouverte au repos ≈ 2 images/s (curseur), 0,6 % d'un cœur.
