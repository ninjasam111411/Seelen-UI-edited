#[cfg(windows)]
mod windows;

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::LazyLock,
};

use seelen_core::system_state::InstalledGame;

use super::domain::{
    GamesSystem, epic_game, pick_steam_cover, pick_steam_hero, shortcut_game, steam_app_manifest,
    steam_game, steam_library_paths, store_game,
};
use crate::system_modules::start::StartMenuManager;

#[cfg(windows)]
type Platform = windows::WindowsGames;

pub struct GamesManager {
    platform: Platform,
}

impl GamesManager {
    pub fn instance() -> &'static Self {
        static MANAGER: LazyLock<GamesManager> = LazyLock::new(GamesManager::create);
        &MANAGER
    }

    fn create() -> Self {
        Self {
            platform: Platform::create(),
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

        // the same game can come from several sources (e.g. a launcher shortcut and the store)
        let mut seen_ids = HashSet::new();
        let mut seen_names = HashSet::new();
        games.retain(|game| {
            seen_ids.insert(game.id.clone()) && seen_names.insert(game.name.to_lowercase())
        });
        games.sort_by_key(|game| game.name.to_lowercase());
        games
    }
}

impl GamesManager {
    /// Store (Xbox/GDK) games and known launcher shortcuts, from the start menu entries.
    fn scan_start_menu(&self) -> Vec<InstalledGame> {
        StartMenuManager::instance()
            .get_all()
            .iter()
            .filter_map(|item| match (&item.target, &item.umid) {
                (None, Some(umid)) if self.platform.is_store_game(umid) => {
                    Some(store_game(umid, &item.display_name))
                }
                (None, _) => None,
                (Some(target), umid) => shortcut_game(
                    &item.path,
                    &item.display_name,
                    Some(target),
                    umid.as_deref(),
                ),
            })
            .collect()
    }
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
                games.push(steam_game(&appid, &name, cover, hero));
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
    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("item"))
        })
        .filter_map(|path| std::fs::read_to_string(path).ok())
        .filter_map(|json| epic_game(&json))
        .collect()
}
