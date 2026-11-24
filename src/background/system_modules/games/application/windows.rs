use std::path::PathBuf;

use windows::ApplicationModel::AppInfo;
use winreg::{
    RegKey,
    enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE},
};

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

    fn gog_games(&self) -> Vec<(String, String, PathBuf)> {
        let Ok(root) =
            RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey(r"SOFTWARE\WOW6432Node\GOG.com\Games")
        else {
            return Vec::new();
        };
        root.enum_keys()
            .flatten()
            .filter_map(|id| {
                let key = root.open_subkey(&id).ok()?;
                let name: String = key.get_value("gameName").ok()?;
                let exe: String = key.get_value("exe").ok()?;
                let exe = PathBuf::from(exe);
                exe.exists().then_some((id, name, exe))
            })
            .collect()
    }

    fn ubisoft_games(&self) -> Vec<(String, PathBuf)> {
        let Ok(root) = RegKey::predef(HKEY_LOCAL_MACHINE)
            .open_subkey(r"SOFTWARE\WOW6432Node\Ubisoft\Launcher\Installs")
        else {
            return Vec::new();
        };
        root.enum_keys()
            .flatten()
            .filter_map(|id| {
                let dir: String = root.open_subkey(&id).ok()?.get_value("InstallDir").ok()?;
                let dir = PathBuf::from(dir.trim_end_matches(['/', '\\']).replace('/', "\\"));
                dir.is_dir().then_some((id, dir))
            })
            .collect()
    }

    fn itch_apps_dir(&self) -> Option<PathBuf> {
        let dir = PathBuf::from(std::env::var_os("APPDATA")?).join(r"itch\apps");
        dir.is_dir().then_some(dir)
    }

    fn game_folders(&self) -> Vec<PathBuf> {
        let mut folders = Vec::new();
        if let Some(home) = std::env::var_os("USERPROFILE").map(PathBuf::from) {
            for sub in [
                r"Games",
                r"Downloads\Games",
                r"Desktop\Games",
                r"Documents\Games",
            ] {
                folders.push(home.join(sub));
            }
        }
        for drive in ["C", "D", "E", "F"] {
            folders.push(PathBuf::from(format!(r"{drive}:\Games")));
        }
        folders.retain(|f| f.is_dir());
        folders
    }
}
