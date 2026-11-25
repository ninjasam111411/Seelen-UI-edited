//! Updates for this personal edited build, from the GitHub release of its fork.
//!
//! Every push to the fork builds a new installer and publishes it with `edited-version.txt`,
//! which holds the commit it was built from. Builds made by that workflow embed the same commit
//! (`SLU_EDITED_SHA`). On launch, if the published commit differs, the new installer is
//! downloaded, Seelen UI closes normally (so the taskbar is restored), and the installer runs
//! in its update mode and opens the app again. Local builds have no embedded commit and never
//! update themselves.

use std::{path::PathBuf, time::Duration};

use seelen_core::state::{HideMode, SeelenWegMode};

use crate::{
    error::ResultLogExt, get_tokio_handle, state::FULL_STATE,
    utils::constants::SEELEN_COMMON,
};

const RELEASE_URL: &str =
    "https://github.com/ninjasam111411/Seelen-UI-edited/releases/download/edited-latest";
const BUILD_SHA: Option<&str> = option_env!("SLU_EDITED_SHA");
/// waits for the desktop to settle after login before checking
const START_DELAY: Duration = Duration::from_secs(20);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(10 * 60);

/// One-time changes to settings saved by earlier edited builds, each recorded by a marker file so
/// the user can change the setting back afterwards.
pub fn migrate_settings() {
    migrate_toolbar();
    migrate_dock();
    migrate_dock_full_width();
    migrate_dock_tab();
    migrate_dock_no_gaps();
}

fn migrate_toolbar() {
    let marker = SEELEN_COMMON.app_data_dir().join("edited-migrated-toolbar-never-hide");
    if marker.exists() {
        return;
    }
    // the toolbar used to hide when a window covered it, which made it slide in and out every
    // time the mouse reached the top of the screen (e.g. for browser tabs)
    if FULL_STATE.load().settings.by_widget.fancy_toolbar.hide_mode != HideMode::Never {
        FULL_STATE.rcu(|state| {
            let mut state = state.cloned();
            state.settings.by_widget.fancy_toolbar.hide_mode = HideMode::Never;
            state
        });
        FULL_STATE.load().write_settings().log_error();
    }
    std::fs::write(&marker, b"done").log_error();
}

/// Same as the toolbar for the dock, which slid in and out over maximized and full screen apps.
fn migrate_dock() {
    let marker = SEELEN_COMMON.app_data_dir().join("edited-migrated-dock-never-hide");
    if marker.exists() {
        return;
    }
    if FULL_STATE.load().settings.by_widget.weg.hide_mode != HideMode::Never {
        FULL_STATE.rcu(|state| {
            let mut state = state.cloned();
            state.settings.by_widget.weg.hide_mode = HideMode::Never;
            state
        });
        FULL_STATE.load().write_settings().log_error();
    }
    std::fs::write(&marker, b"done").log_error();
}

/// The dock's strip showed the wallpaper on both sides of a centered dock (and around its margin)
/// above maximized windows: the dock now fills the strip, like a taskbar.
fn migrate_dock_full_width() {
    let marker = SEELEN_COMMON.app_data_dir().join("edited-migrated-dock-full-width");
    if marker.exists() {
        return;
    }
    let needed = {
        let state = FULL_STATE.load();
        let weg = &state.settings.by_widget.weg;
        weg.mode != SeelenWegMode::FullWidth || weg.margin != 0
    };
    if needed {
        FULL_STATE.rcu(|state| {
            let mut state = state.cloned();
            state.settings.by_widget.weg.mode = SeelenWegMode::FullWidth;
            state.settings.by_widget.weg.margin = 0;
            state
        });
        FULL_STATE.load().write_settings().log_error();
    }
    std::fs::write(&marker, b"done").log_error();
}

/// The dock folds into a small tab at the top (its "always hide" mode): pressing the tab opens it
/// as a compact, floating dock with bigger icons, and windows get the whole screen below the
/// toolbar.
fn migrate_dock_tab() {
    let marker = SEELEN_COMMON.app_data_dir().join("edited-migrated-dock-tab");
    if marker.exists() {
        return;
    }
    FULL_STATE.rcu(|state| {
        let mut state = state.cloned();
        let weg = &mut state.settings.by_widget.weg;
        weg.hide_mode = HideMode::Always;
        weg.mode = SeelenWegMode::MinContent;
        weg.margin = 8;
        weg.size = weg.size.max(48);
        state
    });
    FULL_STATE.load().write_settings().log_error();
    std::fs::write(&marker, b"done").log_error();
}

/// No space around the dock: it hangs right under the toolbar and its tab right under it.
fn migrate_dock_no_gaps() {
    let marker = SEELEN_COMMON.app_data_dir().join("edited-migrated-dock-no-gaps");
    if marker.exists() {
        return;
    }
    FULL_STATE.rcu(|state| {
        let mut state = state.cloned();
        state.settings.by_widget.weg.margin = 0;
        state
    });
    FULL_STATE.load().write_settings().log_error();
    std::fs::write(&marker, b"done").log_error();
}

/// Checks once for a newer build in the background, and installs it if there is one.
pub fn start() {
    let Some(current) = BUILD_SHA.map(str::trim).filter(|sha| !sha.is_empty()) else {
        log::info!("Edited updater: local build, updates are off");
        return;
    };
    get_tokio_handle().spawn(async move {
        tokio::time::sleep(START_DELAY).await;
        match check_and_download(current).await {
            Ok(Some(installer)) => install(installer),
            Ok(None) => log::info!("Edited updater: up to date ({current})"),
            Err(err) => log::warn!("Edited updater: check failed: {err}"),
        }
    });
}

/// Downloads the published installer when its commit differs from this build's.
async fn check_and_download(current: &str) -> Result<Option<PathBuf>, String> {
    let client = reqwest::Client::builder()
        .timeout(DOWNLOAD_TIMEOUT)
        .build()
        .map_err(|e| e.to_string())?;

    let published = client
        .get(format!("{RELEASE_URL}/edited-version.txt"))
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| e.to_string())?
        .text()
        .await
        .map_err(|e| e.to_string())?;
    let published = published.trim();
    if published.is_empty() || published.eq_ignore_ascii_case(current) {
        return Ok(None);
    }
    log::info!("Edited updater: new build {published} (running {current}), downloading");

    let bytes = client
        .get(format!("{RELEASE_URL}/Seelen-UI-Edited-Setup.exe"))
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| e.to_string())?
        .bytes()
        .await
        .map_err(|e| e.to_string())?;
    // a real installer is tens of MB; anything tiny is an error page
    if bytes.len() < 1_000_000 {
        return Err(format!(
            "installer download too small ({} bytes)",
            bytes.len()
        ));
    }

    let path = SEELEN_COMMON.app_temp_dir().join(format!(
        "Seelen-UI-Edited-Setup-{}.exe",
        &published[..published.len().min(12)]
    ));
    std::fs::create_dir_all(SEELEN_COMMON.app_temp_dir()).map_err(|e| e.to_string())?;
    std::fs::write(&path, &bytes).map_err(|e| e.to_string())?;
    Ok(Some(path))
}

/// Starts the installer a few seconds later, then closes Seelen UI normally.
/// `/P` shows only a progress bar, `/UPDATE` keeps the settings, `/R` opens the app again.
fn install(installer: PathBuf) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        use windows::Win32::System::Threading::CREATE_NO_WINDOW;

        let command = format!(
            "timeout /t 4 /nobreak >nul & start \"\" \"{}\" /P /UPDATE /R",
            installer.display()
        );
        // raw_arg: cmd.exe doesn't understand the quoting Rust uses for normal arguments
        let spawned = std::process::Command::new("cmd")
            .arg("/C")
            .raw_arg(&command)
            .creation_flags(CREATE_NO_WINDOW.0)
            .spawn();
        match spawned {
            Ok(_) => {
                log::info!("Edited updater: installing {}", installer.display());
                crate::app::get_app_handle().exit(0);
            }
            Err(err) => log::error!("Edited updater: could not start the installer: {err}"),
        }
    }
    #[cfg(not(windows))]
    log::warn!("Edited updater: installing is only supported on Windows ({installer:?})");
}
