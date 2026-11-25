use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

use windows::ApplicationModel::AppInfo;
use winreg::{
    RegKey,
    enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE},
};

use seelen_core::system_state::GamepadState;
use windows::Win32::UI::Input::XboxController::{XINPUT_STATE, XInputGetState};

use crate::{system_modules::games::domain::GamesSystem, utils::lock_free::TracedMutex};

/// XInput supports up to 4 controllers.
const XUSER_MAX_COUNT: u32 = 4;
const ERROR_SUCCESS: u32 = 0;
/// Asking XInput about empty slots is slow, so with no controller it is asked again only
/// after this long.
const NO_PAD_RECHECK: Duration = Duration::from_secs(2);

static NEXT_PAD_CHECK: TracedMutex<Option<Instant>> = TracedMutex::new(None);

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
        special_folders("Games")
    }

    fn media_folders(&self) -> Vec<PathBuf> {
        special_folders("Media")
    }

    fn gamepad_state(&self) -> Option<GamepadState> {
        if NEXT_PAD_CHECK.lock().is_some_and(|next| Instant::now() < next) {
            return None;
        }
        let found = (0..XUSER_MAX_COUNT).find_map(|user| {
            let mut state = XINPUT_STATE::default();
            // SAFETY: `state` is a valid, writable XINPUT_STATE for the call
            let result = unsafe { XInputGetState(user, &mut state) };
            (result == ERROR_SUCCESS).then(|| GamepadState {
                buttons: state.Gamepad.wButtons.0,
                left_x: state.Gamepad.sThumbLX,
                left_y: state.Gamepad.sThumbLY,
            })
        });
        *NEXT_PAD_CHECK.lock() = found.is_none().then(|| Instant::now() + NO_PAD_RECHECK);
        found
    }
}

/// The existing folders with this name in the user's home (and its Downloads, Desktop and
/// Documents) and at the root of the usual drives, user ones first.
fn special_folders(name: &str) -> Vec<PathBuf> {
    let mut folders = Vec::new();
    if let Some(home) = std::env::var_os("USERPROFILE").map(PathBuf::from) {
        folders.push(home.join(name));
        for parent in ["Downloads", "Desktop", "Documents"] {
            folders.push(home.join(parent).join(name));
        }
    }
    for drive in ["C", "D", "E", "F"] {
        folders.push(PathBuf::from(format!(r"{drive}:\{name}")));
    }
    folders.retain(|f| f.is_dir());
    folders
}
