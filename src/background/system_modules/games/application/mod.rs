mod art;
#[cfg(windows)]
mod windows;

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::LazyLock,
};

use seelen_core::system_state::{GamepadState, GameSource, InstalledGame};

use super::domain::{
    GamesSystem, cemu_args, display_name_from, epic_catalog_art, epic_game, is_game_folder, is_known_game_name,
    is_known_store_game, is_non_game_name, bat_args_for, local_game, normalize_name, pick_game_exe, pick_steam_cover, pick_trusted_exe,
    pick_folder_icon, pick_steam_hero, pick_steam_icon, pick_steam_logo, shortcut_game, steam_app_manifest, steam_game, steam_library_paths,
    store_game, ubisoft_game,
};
use crate::{system_modules::start::StartMenuManager, utils::lock_free::TracedMutex};

/// Folder entries read when looking for a game, so a huge folder can't stall the scan.
const MAX_FOLDER_ENTRIES: usize = 400;

#[cfg(windows)]
type Platform = windows::WindowsGames;

pub struct GamesManager {
    platform: Platform,
    /// whether the folder of a start menu shortcut target is a game's, by folder
    game_folder_cache: TracedMutex<HashMap<PathBuf, bool>>,
}

impl GamesManager {
    pub fn instance() -> &'static Self {
        static MANAGER: LazyLock<GamesManager> = LazyLock::new(GamesManager::create);
        &MANAGER
    }

    fn create() -> Self {
        Self {
            platform: Platform::create(),
            game_folder_cache: TracedMutex::new(HashMap::new()),
        }
    }

    /// Scans the launchers on every call: it only reads a few small files, and this way newly
    /// installed or uninstalled games show up without watching the launcher folders.
    pub fn installed_games(&self) -> Vec<InstalledGame> {
        let mut games = Vec::new();
        if let Some(root) = self.platform.steam_root() {
            games.extend(scan_steam(&root));
        }
        if let Some(dir) = self.platform.epic_manifests_dir() {
            games.extend(scan_epic(&dir));
        }
        games.extend(self.scan_start_menu());
        games.extend(self.scan_gog_and_ubisoft());
        if let Some(dir) = self.platform.itch_apps_dir() {
            games.extend(scan_game_folders(&dir, GameSource::Itch));
        }
        for folder in self.platform.game_folders() {
            games.extend(scan_game_folders(&folder, GameSource::Local));
        }

        // cover art found online for games without it on disk (filled in the background)
        art::apply(&mut games);

        // the same game can come from several sources (e.g. a launcher shortcut and the store):
        // the first one is kept, with the art the others have and it lacks
        let mut unique: Vec<InstalledGame> = Vec::with_capacity(games.len());
        let mut seen_ids = HashSet::new();
        for game in games {
            if is_non_game_name(&game.name) || !seen_ids.insert(game.id.clone()) {
                continue;
            }
            let key = normalize_name(&game.name);
            match unique.iter_mut().find(|kept| normalize_name(&kept.name) == key) {
                Some(kept) => {
                    kept.cover = kept.cover.take().or(game.cover);
                    kept.hero = kept.hero.take().or(game.hero);
                    kept.logo = kept.logo.take().or(game.logo);
                    kept.icon = kept.icon.take().or(game.icon);
                }
                None => unique.push(game),
            }
        }
        unique.sort_by_key(|game| game.name.to_lowercase());

        art::request(&unique);
        unique
    }
}

impl GamesManager {
    /// What to open for each item in a Media folder: shortcuts and programs as they are, and for
    /// a folder, the program in it.
    pub fn media_folder_items(&self) -> Vec<PathBuf> {
        let mut items = Vec::new();
        for root in self.platform.media_folders() {
            let Ok(children) = std::fs::read_dir(&root) else {
                continue;
            };
            for child in children.flatten().take(MAX_FOLDER_ENTRIES) {
                let path = child.path();
                if path.is_dir() {
                    let name = child.file_name().to_string_lossy().to_string();
                    if let Some(exe) = find_trusted_exe(&path, &name) {
                        items.push(exe);
                    }
                } else if path.extension().is_some_and(|ext| {
                    ["lnk", "url", "exe", "appref-ms"]
                        .iter()
                        .any(|e| ext.eq_ignore_ascii_case(e))
                }) {
                    items.push(path);
                }
            }
        }
        items
    }

    /// `None` while another app is in front; buttons are empty when no Xbox controller is
    /// connected (other controllers are read by the webview).
    pub fn gamepad_state(&self) -> Option<GamepadState> {
        let front = crate::platform::window::Window::get_foregrounded();
        if !(front.is_desktop() || front.process().is_seelen()) {
            return None;
        }
        Some(self.platform.gamepad_state().unwrap_or_default())
    }

    /// Store games, known launchers and games downloaded from websites, from the start menu.
    fn scan_start_menu(&self) -> Vec<InstalledGame> {
        // games kept in a games folder are started with their own program (found by the folder
        // scan), never through a shortcut to them, which may start the wrong file
        let game_roots: Vec<String> = self
            .platform
            .game_folders()
            .iter()
            .map(|root| root.to_string_lossy().to_lowercase())
            .collect();
        let in_games_folder = |target: &Path| {
            let target = target.to_string_lossy().to_lowercase();
            game_roots
                .iter()
                .any(|root| target.starts_with(&format!("{}\\", root.trim_end_matches('\\'))))
        };
        StartMenuManager::instance()
            .get_all()
            .iter()
            .filter(|item| !item.target.as_deref().is_some_and(in_games_folder))
            .filter_map(|item| match (&item.target, &item.umid) {
                (None, Some(umid)) => {
                    let is_game = is_known_store_game(umid)
                        || is_known_game_name(&item.display_name)
                        || self.platform.is_store_game(umid);
                    is_game.then(|| store_game(umid, &item.display_name))
                }
                (None, None) => None,
                (Some(target), umid) => {
                    shortcut_game(
                        &item.path,
                        &item.display_name,
                        Some(target),
                        umid.as_deref(),
                    )
                    .or_else(|| {
                        self.is_local_game_target(target).then(|| {
                            let mut game =
                                local_game(target, &item.display_name, GameSource::Local);
                            // the shortcut keeps the arguments and working folder it needs
                            game.launch = item.path.to_string_lossy().to_string();
                            game
                        })
                    })
                }
            })
            .collect()
    }

    /// Whether a shortcut target is a game outside of the launchers already scanned.
    fn is_local_game_target(&self, target: &Path) -> bool {
        let lower = target.to_string_lossy().to_lowercase();
        let in_launcher = lower.contains("\\steamapps\\")
            || lower.contains("\\epic games\\")
            || lower.contains("\\windowsapps\\");
        let (Some(dir), Some(exe)) = (target.parent(), target.file_name()) else {
            return false;
        };
        if in_launcher {
            return false;
        }
        let exe = exe.to_string_lossy().to_string();
        let key = target.to_path_buf();
        if let Some(cached) = self.game_folder_cache.lock().get(&key) {
            return *cached;
        }
        let is_game = is_game_folder(&exe, &folder_entries(dir));
        self.game_folder_cache.lock().insert(key, is_game);
        is_game
    }

    fn scan_gog_and_ubisoft(&self) -> Vec<InstalledGame> {
        let gog = self
            .platform
            .gog_games()
            .into_iter()
            .map(|(_, name, exe)| local_game(&exe, &name, GameSource::Gog));
        let ubisoft = self
            .platform
            .ubisoft_games()
            .into_iter()
            .filter_map(|(id, dir)| ubisoft_game(&id, &dir));
        gog.chain(ubisoft).collect()
    }
}

/// Lower case names of a folder's entries.
fn folder_entries(dir: &Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .take(MAX_FOLDER_ENTRIES)
                .map(|e| e.file_name().to_string_lossy().to_lowercase())
                .collect()
        })
        .unwrap_or_default()
}

/// Each subfolder of `root` (a games folder) is a game: its program is found directly in it, in an
/// emulator bundle, or one folder deeper (launcher subfolders, zip/itch.io downloads).
fn scan_game_folders(root: &Path, source: GameSource) -> Vec<InstalledGame> {
    let Ok(children) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut games = Vec::new();
    for child in children.flatten().take(MAX_FOLDER_ENTRIES) {
        let dir = child.path();
        if !dir.is_dir() {
            continue;
        }
        let name = display_name_from(&child.file_name().to_string_lossy());
        if let Some(mut game) = folder_game(&dir, &name, source) {
            game.icon = find_folder_icon(&dir, game.executable.as_deref());
            if game.launch_args.is_none() {
                game.launch_args = game.executable.as_deref().and_then(find_bat_args);
            }
            games.push(game);
        }
    }
    games
}

fn folder_game(dir: &Path, name: &str, source: GameSource) -> Option<InstalledGame> {
    let folder_name = dir.file_name()?.to_string_lossy().to_string();
    if let Some(exe) = find_game_exe(dir, &folder_name) {
        return Some(local_game(&exe, name, source));
    }
    if let Some((cemu, game_file)) = find_cemu_game(dir) {
        let mut game = local_game(&cemu, name, source);
        game.launch_args = Some(cemu_args(&game_file));
        return Some(game);
    }
    if let Some(exe) = find_trusted_exe(dir, &folder_name) {
        return Some(local_game(&exe, name, source));
    }

    let subdirs: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .take(40)
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    let sub_name = |p: &PathBuf| p.file_name().map(|n| n.to_string_lossy().to_string());
    // a game with several parts and its own launcher (e.g. Poppy Playtime chapters)
    let launcher = subdirs
        .iter()
        .filter(|p| sub_name(p).is_some_and(|n| n.to_lowercase().contains("launcher")))
        .find_map(|p| find_trusted_exe(p, &sub_name(p)?));
    // zip and itch.io downloads often add one more folder level
    let exe = launcher.or_else(|| {
        subdirs
            .iter()
            .find_map(|p| find_game_exe(p, &sub_name(p)?))
    })?;
    Some(local_game(&exe, name, source))
}

fn find_game_exe(dir: &Path, folder_name: &str) -> Option<PathBuf> {
    let entries = folder_entries(dir);
    let exe_lower = pick_game_exe(folder_name, &entries)?;
    real_file(dir, &exe_lower)
}

fn find_trusted_exe(dir: &Path, folder_name: &str) -> Option<PathBuf> {
    let files: Vec<(String, u64)> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .take(MAX_FOLDER_ENTRIES)
        .map(|e| {
            let size = e.metadata().map(|m| if m.is_file() { m.len() } else { 0 });
            (e.file_name().to_string_lossy().to_lowercase(), size.unwrap_or(0))
        })
        .collect();
    let exe_lower = pick_trusted_exe(folder_name, &files)?;
    real_file(dir, &exe_lower)
}

/// Arguments the game's own .bat starter gives its program (e.g. `PlayGTAV.bat` runs
/// `PlayGTAV.exe -nobattleye`): starting the program without them can fail.
fn find_bat_args(exe: &Path) -> Option<String> {
    let dir = exe.parent()?;
    let exe_name = exe.file_name()?.to_string_lossy().to_string();
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .take(MAX_FOLDER_ENTRIES)
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("bat") || ext.eq_ignore_ascii_case("cmd"))
        })
        // a starter is a short script; bigger ones are installers or tools
        .filter(|p| std::fs::metadata(p).is_ok_and(|m| m.len() < 4096))
        .find_map(|bat| bat_args_for(&std::fs::read_to_string(bat).ok()?, &exe_name))
}

/// An icon the game ships (e.g. `game.ico`): in its folder, or next to its program.
fn find_folder_icon(dir: &Path, exe: Option<&Path>) -> Option<PathBuf> {
    let mut places = vec![dir.to_path_buf()];
    if let Some(exe_dir) = exe.and_then(Path::parent) {
        if exe_dir != dir {
            places.push(exe_dir.to_path_buf());
        }
    }
    places.into_iter().find_map(|place| {
        let icon = pick_folder_icon(&folder_entries(&place))?;
        real_file(&place, &icon)
    })
}

/// The file of a folder with this lower case name (entries are compared in lower case).
fn real_file(dir: &Path, name_lower: &str) -> Option<PathBuf> {
    std::fs::read_dir(dir).ok()?.flatten().find_map(|e| {
        (e.file_name().to_string_lossy().to_lowercase() == name_lower).then(|| e.path())
    })
}

/// Cemu (Wii U emulator) bundled with a game: `Cemu.exe` in the folder or a subfolder, and the
/// game in its `games/<game>/code/*.rpx`.
fn find_cemu_game(dir: &Path) -> Option<(PathBuf, PathBuf)> {
    let mut places = vec![dir.to_path_buf()];
    places.extend(
        std::fs::read_dir(dir)
            .ok()?
            .flatten()
            .take(40)
            .map(|e| e.path())
            .filter(|p| p.is_dir()),
    );
    places.into_iter().find_map(|place| {
        let cemu = real_file(&place, "cemu.exe")?;
        let games = std::fs::read_dir(place.join("games")).ok()?;
        let game_file = games.flatten().take(20).find_map(|game| {
            std::fs::read_dir(game.path().join("code"))
                .ok()?
                .flatten()
                .map(|e| e.path())
                .find(|p| p.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("rpx")))
        })?;
        Some((cemu, game_file))
    })
}

fn scan_steam(root: &Path) -> Vec<InstalledGame> {
    let mut libraries = vec![root.to_path_buf()];
    if let Ok(vdf) = std::fs::read_to_string(root.join("steamapps").join("libraryfolders.vdf")) {
        libraries.extend(steam_library_paths(&vdf));
    }
    let mut seen_libraries = HashSet::new();
    libraries.retain(|lib| seen_libraries.insert(lib.to_string_lossy().to_lowercase()));

    let cache = root.join("appcache").join("librarycache");
    let mut games = Vec::new();
    for library in libraries {
        let Ok(entries) = std::fs::read_dir(library.join("steamapps")) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let is_manifest = path
                .file_name()
                .map(|n| n.to_string_lossy().to_lowercase())
                .is_some_and(|n| n.starts_with("appmanifest_") && n.ends_with(".acf"));
            if !is_manifest {
                continue;
            }
            let Ok(acf) = std::fs::read_to_string(&path) else {
                continue;
            };
            if let Some((appid, name)) = steam_app_manifest(&acf) {
                let files = steam_cache_files(&cache, &appid);
                let cover = pick_steam_cover(&files).or_else(|| {
                    let flat = cache.join(format!("{appid}_library_600x900.jpg"));
                    flat.exists().then_some(flat)
                });
                let hero = pick_steam_hero(&files).or_else(|| {
                    let flat = cache.join(format!("{appid}_library_hero.jpg"));
                    flat.exists().then_some(flat)
                });
                let mut game = steam_game(&appid, &name, cover, hero);
                game.logo = pick_steam_logo(&files).or_else(|| {
                    let flat = cache.join(format!("{appid}_logo.png"));
                    flat.exists().then_some(flat)
                });
                game.icon = pick_steam_icon(&files).or_else(|| {
                    let flat = cache.join(format!("{appid}_icon.jpg"));
                    flat.exists().then_some(flat)
                });
                games.push(game);
            }
        }
    }
    games
}

/// Files Steam keeps for one app in its library cache: a folder per app (newer clients), sometimes
/// nested one level more. Older clients keep them flat (`<appid>_library_600x900.jpg`), which the
/// caller checks separately.
fn steam_cache_files(cache: &Path, appid: &str) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let Ok(entries) = std::fs::read_dir(cache.join(appid)) else {
        return files;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Ok(inner) = std::fs::read_dir(&path) {
                files.extend(inner.flatten().map(|e| e.path()));
            }
        } else {
            files.push(path);
        }
    }
    files
}

fn scan_epic(dir: &Path) -> Vec<InstalledGame> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    // the launcher keeps the store art of its games in a catalog cache next to the manifests
    let catalog = dir
        .parent()
        .map(|data| data.join("Catalog").join("catcache.bin"))
        .and_then(|path| std::fs::read(path).ok())
        .and_then(|bytes| {
            use base64::Engine as _;
            let text: Vec<u8> = bytes.into_iter().filter(|b| !b.is_ascii_whitespace()).collect();
            base64::engine::general_purpose::STANDARD.decode(text).ok()
        })
        .map(|json| epic_catalog_art(&String::from_utf8_lossy(&json)))
        .unwrap_or_default();

    let mut games = Vec::new();
    for path in entries.flatten().map(|entry| entry.path()) {
        if !path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("item")) {
            continue;
        }
        let Ok(json) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Some(game) = epic_game(&json) else {
            continue;
        };
        let item_id = serde_json::from_str::<serde_json::Value>(&json)
            .ok()
            .and_then(|v| v.get("CatalogItemId")?.as_str().map(str::to_owned));
        if let Some(art) = item_id.and_then(|id| catalog.get(&id)) {
            art::hint(&game.id, art.clone());
        }
        games.push(game);
    }
    games
}
