use std::path::PathBuf;

use windows::ApplicationModel::AppInfo;
use winreg::{RegKey, enums::HKEY_CURRENT_USER};

use crate::system_modules::games::domain::GamesSystem;

pub struct WindowsGames;

impl GamesSystem for WindowsGames {
    fn create() -> Self {
        Self
    }

    fn steam_root(&self) -> Option<PathBuf> {
        let from_registry = RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey(r"Software\Valve\Steam")
            .and_then(|key| key.get_value::<String, _>("SteamPath"))
            .ok()
            .map(|path| PathBuf::from(path.replace('/', "\\")));
        from_registry
            .into_iter()
            .chain([PathBuf::from(r"C:\Program Files (x86)\Steam")])
            .find(|path| path.join("steamapps").is_dir())
    }

    fn epic_manifests_dir(&self) -> Option<PathBuf> {
        let program_data =
            std::env::var_os("ProgramData").map_or(PathBuf::from(r"C:\ProgramData"), PathBuf::from);
        let dir = program_data.join(r"Epic\EpicGamesLauncher\Data\Manifests");
        dir.is_dir().then_some(dir)
    }

    fn is_store_game(&self, umid: &str) -> bool {
        let installed_path = || -> windows::core::Result<PathBuf> {
            let package = AppInfo::GetFromAppUserModelId(&umid.into())?.Package()?;
            Ok(PathBuf::from(package.InstalledPath()?.to_os_string()))
        };
        installed_path()
            .map(|path| path.join("MicrosoftGame.config").exists())
            .unwrap_or(false)
    }
}
