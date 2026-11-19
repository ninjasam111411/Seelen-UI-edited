#[cfg(windows)]
mod windows;

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::LazyLock,
};

use seelen_core::system_state::InstalledGame;

use super::domain::{
    GamesSystem, epic_game, pick_steam_cover, steam_app_manifest, steam_game, steam_library_paths,
};

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

        let mut seen = HashSet::new();
        games.retain(|game| seen.insert(game.id.clone()));
        games.sort_by_key(|game| game.name.to_lowercase());
        games
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
                games.push(steam_game(&appid, &name, steam_cover(&cache, &appid)));
            }
        }
    }
    games
}

/// Steam keeps the covers either flat (`<appid>_library_600x900.jpg`, older clients) or in a
/// folder per app, sometimes nested one level more.
fn steam_cover(cache: &Path, appid: &str) -> Option<PathBuf> {
    let flat = cache.join(format!("{appid}_library_600x900.jpg"));
    if flat.exists() {
        return Some(flat);
    }

    let dir = cache.join(appid);
    let mut files = Vec::new();
    for entry in std::fs::read_dir(&dir).ok()?.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Ok(inner) = std::fs::read_dir(&path) {
                files.extend(inner.flatten().map(|e| e.path()));
            }
        } else {
            files.push(path);
        }
    }
    pick_steam_cover(&files).or_else(|| {
        let header = cache.join(format!("{appid}_header.jpg"));
        header.exists().then_some(header)
    })
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
