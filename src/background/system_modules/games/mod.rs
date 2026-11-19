mod application;
mod domain;

use seelen_core::system_state::InstalledGame;

use self::application::GamesManager;

impl crate::tauri_handlers::Handlers {
    pub fn get_installed_games() -> Vec<InstalledGame> {
        GamesManager::instance().installed_games()
    }
}
