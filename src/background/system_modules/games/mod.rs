mod application;
mod domain;

use seelen_core::system_state::{GamepadState, InstalledGame};

use self::application::GamesManager;

impl crate::tauri_handlers::Handlers {
    pub fn get_installed_games() -> Vec<InstalledGame> {
        GamesManager::instance().installed_games()
    }

    pub fn get_media_folder_items() -> Vec<std::path::PathBuf> {
        GamesManager::instance().media_folder_items()
    }

    /// What is in a folder, for looking through it in the room's file sorter: (path, is folder),
    /// folders first, then by name. Hidden and system files are left out.
    pub fn get_folder_content(path: std::path::PathBuf) -> Vec<(std::path::PathBuf, bool)> {
        folder_content(&path)
    }

    /// The first connected Xbox controller, only while the desktop (or a Seelen UI widget) is in
    /// front, so playing a game with it never moves the console home behind the game.
    pub fn get_gamepad_state() -> Option<GamepadState> {
        GamesManager::instance().gamepad_state()
    }
}

/// most files a folder shows in the file sorter
const MAX_FOLDER_ENTRIES: usize = 2000;

fn folder_content(path: &std::path::Path) -> Vec<(std::path::PathBuf, bool)> {
    let Ok(read) = std::fs::read_dir(path) else {
        return Vec::new();
    };
    let mut entries: Vec<(std::path::PathBuf, bool)> = read
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_lowercase();
            if name.starts_with(['.', '$']) || name == "desktop.ini" || name == "thumbs.db" {
                return None;
            }
            let metadata = entry.metadata().ok()?;
            if application::is_hidden_file(&metadata) {
                return None;
            }
            Some((entry.path(), metadata.is_dir()))
        })
        .take(MAX_FOLDER_ENTRIES)
        .collect();
    entries.sort_by(|(a, a_dir), (b, b_dir)| {
        let name =
            |p: &std::path::PathBuf| p.file_name().map(|n| n.to_string_lossy().to_lowercase());
        b_dir.cmp(a_dir).then_with(|| name(a).cmp(&name(b)))
    });
    entries
}
