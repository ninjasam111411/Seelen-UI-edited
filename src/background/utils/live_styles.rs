//! Live styles for this edited version: the default theme's styles are downloaded from the
//! GitHub repository into a user theme ("@edited/live-styles") applied on top of the bundled
//! default theme. Style changes pushed to the repository then show up a few minutes later (the
//! user themes folder is watched and reloaded), without building and installing a new version.
//! Nothing else is downloaded this way: new features still come with the installer.

use std::time::Duration;

use seelen_core::state::ThemeId;

use crate::{
    error::ResultLogExt, get_tokio_handle, state::FULL_STATE, utils::constants::SEELEN_COMMON,
};

const THEME_URL: &str = "https://raw.githubusercontent.com/ninjasam111411/Seelen-UI-edited/better-version/src/static/themes/default";
const LIVE_THEME_ID: &str = "@edited/live-styles";
const LIVE_THEME_FOLDER: &str = "edited-live-styles";
/// first check soon after start, then every few minutes while running
const FIRST_DELAY: Duration = Duration::from_secs(8);
const INTERVAL: Duration = Duration::from_secs(3 * 60);
const TIMEOUT: Duration = Duration::from_secs(30);

/// Starts checking for style changes in the background.
pub fn start() {
    get_tokio_handle().spawn(async {
        tokio::time::sleep(FIRST_DELAY).await;
        loop {
            match sync().await {
                Ok(true) => log::info!("Live styles: updated"),
                Ok(false) => {}
                Err(err) => log::warn!("Live styles: sync failed: {err}"),
            }
            tokio::time::sleep(INTERVAL).await;
        }
    });
}

/// Downloads the theme's styles and writes the ones that changed. Returns whether any did.
/// Every file is downloaded before anything is written, so a failed sync changes nothing.
async fn sync() -> Result<bool, String> {
    let client = reqwest::Client::builder()
        .timeout(TIMEOUT)
        .build()
        .map_err(|e| e.to_string())?;
    let get = |path: String| {
        let client = client.clone();
        async move {
            client
                .get(format!("{THEME_URL}/{path}"))
                .header(reqwest::header::CACHE_CONTROL, "no-cache")
                .send()
                .await
                .and_then(|r| r.error_for_status())
                .map_err(|e| format!("{path}: {e}"))?
                .text()
                .await
                .map_err(|e| format!("{path}: {e}"))
        }
    };

    let metadata = get("metadata.yml".to_owned()).await?;
    let entries = style_entries(&metadata);
    if entries.is_empty() {
        return Err("no styles listed in the theme".into());
    }
    let mut files = Vec::with_capacity(entries.len());
    for (_, path) in &entries {
        files.push((path.clone(), get(path.clone()).await?));
    }

    let folder = SEELEN_COMMON.user_themes_path().join(LIVE_THEME_FOLDER);
    let mut changed = false;
    let mut write = |relative: &str, content: &str| -> Result<(), String> {
        let path = folder.join(relative);
        if std::fs::read_to_string(&path).is_ok_and(|current| current == content) {
            return Ok(());
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(&path, content).map_err(|e| e.to_string())?;
        changed = true;
        Ok(())
    };
    // styles first, so the theme never points at a file that is not there yet
    for (path, content) in &files {
        write(path, content)?;
    }
    write("metadata.yml", &live_metadata(&entries))?;

    enable_live_theme();
    Ok(changed)
}

/// Puts the live theme right after the bundled default theme, so its styles win over it.
fn enable_live_theme() {
    let id: ThemeId = LIVE_THEME_ID.into();
    if FULL_STATE.load().settings.active_themes.contains(&id) {
        return;
    }
    FULL_STATE.rcu(|state| {
        let mut state = state.cloned();
        let themes = &mut state.settings.active_themes;
        let at = themes
            .iter()
            .position(|t| t.to_string() == "@default/theme")
            .map_or(themes.len(), |i| i + 1);
        themes.insert(at, id.clone());
        state
    });
    FULL_STATE.load().write_settings().log_error();
}

/// The `"<widget id>": !include styles/<file>` entries of a theme's metadata.
fn style_entries(metadata: &str) -> Vec<(String, String)> {
    metadata
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.starts_with('#') {
                return None;
            }
            let (key, value) = line.split_once(": !include ")?;
            let widget = key.trim().trim_matches('"');
            let path = value.trim().trim_matches('"').trim_start_matches("./");
            let safe = widget.starts_with('@')
                && path.starts_with("styles/")
                && !path.contains("..")
                && [".scss", ".css", ".sass"].iter().any(|ext| path.ends_with(ext));
            safe.then(|| (widget.to_owned(), path.to_owned()))
        })
        .collect()
}

/// Metadata of the live theme: the same styles, under its own id.
fn live_metadata(entries: &[(String, String)]) -> String {
    let mut yaml = format!(
        "id: \"{LIVE_THEME_ID}\"\nmetadata:\n  displayName:\n    en: Live styles (Seelen UI Edited)\n  description:\n    en: The newest look from the repository, applied without reinstalling.\nstyles:\n"
    );
    for (widget, path) in entries {
        yaml.push_str(&format!("  \"{widget}\": !include {path}\n"));
    }
    yaml
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_theme_styles() {
        let metadata = r#"id: "@default/theme"
metadata:
  displayName: !extend i18n/display_name.yml
styles:
  # comment: "@x/y": !include styles/no.scss
  "@seelen/fancy-toolbar": !include styles/fancy-toolbar.scss
  "@seelen/window-manager": !include styles/window-manager.css
  "@seelen/bad": !include ../../secrets.scss
sharedStyles: !include ./shared/index.scss
"#;
        let entries = style_entries(metadata);
        assert_eq!(
            entries,
            vec![
                ("@seelen/fancy-toolbar".to_owned(), "styles/fancy-toolbar.scss".to_owned()),
                ("@seelen/window-manager".to_owned(), "styles/window-manager.css".to_owned()),
            ]
        );
        let yaml = live_metadata(&entries);
        assert!(yaml.starts_with("id: \"@edited/live-styles\""));
        assert!(yaml.contains("  \"@seelen/fancy-toolbar\": !include styles/fancy-toolbar.scss\n"));
    }
}
