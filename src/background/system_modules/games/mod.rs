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

    /// The first connected Xbox controller, only while the desktop (or a Seelen UI widget) is in
    /// front, so playing a game with it never moves the console home behind the game.
    pub fn get_gamepad_state() -> Option<GamepadState> {
        GamesManager::instance().gamepad_state()
    }
}
