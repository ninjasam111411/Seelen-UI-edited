//! Launching of dock items.
//!
//! Many apps install every update into a new versioned folder (Roblox Studio, Discord, Zoom, ...)
//! and delete the old one, so the executable path saved when the item was pinned stops existing.
//! The start menu shortcut of those apps is kept up to date by their updaters, which is why the
//! Windows search keeps finding them. So, like the Windows search, a dock item is opened through
//! its current start menu entry whenever one can be found, and the saved path is only a fallback.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use seelen_core::{
    state::WegItemData,
    system_state::{RelaunchArguments, StartMenuItem},
};

/// What will be executed to open a dock item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchTarget {
    /// A start menu shortcut file (`.lnk`), opened as if it was clicked.
    Shortcut(PathBuf),
    /// An installed app opened by its app id, as the start menu does with packaged apps.
    AppId(String),
    /// A program with its own arguments and working dir (e.g. a browser profile).
    Command {
        program: String,
        args: Option<RelaunchArguments>,
        working_dir: Option<PathBuf>,
    },
    /// The file saved when the item was pinned.
    File(PathBuf),
}

impl LaunchTarget {
    /// Program, arguments and working dir to be passed to the shell.
    pub fn to_shell_args(&self) -> (String, Option<String>, Option<PathBuf>) {
        match self {
            LaunchTarget::Shortcut(path) | LaunchTarget::File(path) => {
                (path.to_string_lossy().to_string(), None, None)
            }
            LaunchTarget::AppId(app_id) => (format!("shell:AppsFolder\\{app_id}"), None, None),
            LaunchTarget::Command {
                program,
                args,
                working_dir,
            } => (
                program.clone(),
                args.as_ref().map(|args| args.to_string()),
                working_dir.clone(),
            ),
        }
    }
}

/// Decides how to open a dock item, in order:
///
/// 1. The start menu entry with the same app id: it is how Windows itself identifies the app, and
///    it also distinguishes browser profiles and installed web apps.
/// 2. The saved launch command, while the program it points to still exists.
/// 3. The start menu shortcut pointing to the same program, to one with the same file name in
///    another folder (versioned updates), or with the same display name.
/// 4. The app id alone, the saved command or the saved file, as before.
///
/// `by_umid` is the start menu lookup by app id, `start_menu` all the start menu entries with user
/// ones first, and `exists` tells if a file exists (injected to keep this testable).
pub fn resolve_launch_target(
    item: &WegItemData,
    by_umid: Option<Arc<StartMenuItem>>,
    start_menu: &[Arc<StartMenuItem>],
    exists: impl Fn(&Path) -> bool,
) -> LaunchTarget {
    if let Some(entry) = by_umid {
        if let Some(target) = entry_target(&entry, &exists) {
            return target;
        }
    }

    let saved_command = item
        .relaunch
        .as_ref()
        .map(|relaunch| LaunchTarget::Command {
            program: relaunch.command.clone(),
            args: relaunch.args.clone(),
            working_dir: relaunch.working_dir.clone(),
        });

    if let Some(relaunch) = &item.relaunch
        && program_is_available(&relaunch.command, &exists)
    {
        return saved_command.unwrap_or_else(|| LaunchTarget::File(item.path.clone()));
    }

    let saved_program = item
        .relaunch
        .as_ref()
        .map(|relaunch| PathBuf::from(&relaunch.command))
        .filter(|program| program.is_absolute());
    let programs: Vec<&Path> = [Some(item.path.as_path()), saved_program.as_deref()]
        .into_iter()
        .flatten()
        .filter(|path| !path.as_os_str().is_empty())
        .collect();

    if let Some(entry) = find_start_menu_entry(item, &programs, start_menu)
        && let Some(target) = entry_target(&entry, &exists)
    {
        return target;
    }

    if let Some(umid) = &item.umid {
        return LaunchTarget::AppId(umid.clone());
    }
    saved_command.unwrap_or_else(|| LaunchTarget::File(item.path.clone()))
}

/// How to open a start menu entry: the shortcut file if it still exists, packaged apps (no shortcut
/// target) by their app id.
fn entry_target(entry: &StartMenuItem, exists: &impl Fn(&Path) -> bool) -> Option<LaunchTarget> {
    if entry.target.is_some() {
        if exists(&entry.path) {
            return Some(LaunchTarget::Shortcut(entry.path.clone()));
        }
        return None;
    }
    entry.umid.clone().map(LaunchTarget::AppId)
}

/// Commands without a folder (e.g. `explorer.exe`) are resolved by the shell, so they are trusted.
fn program_is_available(program: &str, exists: &impl Fn(&Path) -> bool) -> bool {
    let path = Path::new(program);
    if !path.is_absolute() {
        return !program.trim().is_empty();
    }
    exists(path)
}

/// Shortcut targeting one of the programs, then one targeting a program with the same file name
/// (the app moved to another folder), then one with the same display name.
fn find_start_menu_entry(
    item: &WegItemData,
    programs: &[&Path],
    start_menu: &[Arc<StartMenuItem>],
) -> Option<Arc<StartMenuItem>> {
    let same_path = |a: &Path, b: &Path| {
        a.to_string_lossy()
            .eq_ignore_ascii_case(&b.to_string_lossy())
    };
    let same_name = |a: &Path, b: &Path| match (a.file_name(), b.file_name()) {
        (Some(a), Some(b)) => a
            .to_string_lossy()
            .eq_ignore_ascii_case(&b.to_string_lossy()),
        _ => false,
    };

    let by_target = |matches: &dyn Fn(&Path, &Path) -> bool| {
        start_menu
            .iter()
            .find(|entry| {
                entry
                    .target
                    .as_deref()
                    .is_some_and(|target| programs.iter().any(|program| matches(target, program)))
            })
            .cloned()
    };

    by_target(&same_path)
        .or_else(|| by_target(&same_name))
        .or_else(|| {
            let name = item.display_name.trim();
            if name.is_empty() {
                return None;
            }
            start_menu
                .iter()
                .find(|entry| entry.display_name.trim().eq_ignore_ascii_case(name))
                .cloned()
        })
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use seelen_core::system_state::Relaunch;

    use super::*;

    const OLD_STUDIO: &str =
        r"C:\Users\me\AppData\Local\Roblox\Versions\version-old\RobloxStudioBeta.exe";
    const NEW_STUDIO: &str =
        r"C:\Users\me\AppData\Local\Roblox\Versions\version-new\RobloxStudioBeta.exe";
    const STUDIO_LNK: &str =
        r"C:\Users\me\AppData\Roaming\Microsoft\Windows\Start Menu\Programs\Roblox Studio.lnk";

    fn shortcut(lnk: &str, target: &str, name: &str, umid: Option<&str>) -> Arc<StartMenuItem> {
        Arc::new(StartMenuItem {
            path: lnk.into(),
            umid: umid.map(str::to_owned),
            toast_activator: None,
            target: Some(target.into()),
            display_name: name.into(),
        })
    }

    fn packaged(umid: &str, name: &str) -> Arc<StartMenuItem> {
        Arc::new(StartMenuItem {
            path: PathBuf::new(),
            umid: Some(umid.into()),
            toast_activator: None,
            target: None,
            display_name: name.into(),
        })
    }

    fn pinned(path: &str, name: &str) -> WegItemData {
        WegItemData {
            path: path.into(),
            display_name: name.into(),
            pinned: true,
            ..Default::default()
        }
    }

    fn files(list: &[&str]) -> impl Fn(&Path) -> bool {
        let set: HashSet<String> = list.iter().map(|s| s.to_lowercase()).collect();
        move |path: &Path| set.contains(&path.to_string_lossy().to_lowercase())
    }

    #[test]
    fn app_moved_by_update_opens_through_its_start_menu_shortcut() {
        let menu = vec![shortcut(STUDIO_LNK, NEW_STUDIO, "Roblox Studio", None)];
        let item = pinned(OLD_STUDIO, "Roblox Studio");
        let target = resolve_launch_target(&item, None, &menu, files(&[NEW_STUDIO, STUDIO_LNK]));
        assert_eq!(target, LaunchTarget::Shortcut(STUDIO_LNK.into()));
    }

    #[test]
    fn moved_app_found_by_display_name_when_file_name_changed() {
        let launcher = r"C:\Roblox\Versions\version-new\RobloxStudioLauncherBeta.exe";
        let menu = vec![shortcut(STUDIO_LNK, launcher, "Roblox Studio", None)];
        let item = pinned(OLD_STUDIO, "roblox studio");
        let target = resolve_launch_target(&item, None, &menu, files(&[STUDIO_LNK]));
        assert_eq!(target, LaunchTarget::Shortcut(STUDIO_LNK.into()));
    }

    #[test]
    fn stale_relaunch_command_is_replaced_by_the_shortcut() {
        let menu = vec![shortcut(STUDIO_LNK, NEW_STUDIO, "Roblox Studio", None)];
        let mut item = pinned("", "Roblox Studio");
        item.relaunch = Some(Relaunch {
            command: OLD_STUDIO.into(),
            ..Default::default()
        });
        let target = resolve_launch_target(&item, None, &menu, files(&[STUDIO_LNK]));
        assert_eq!(target, LaunchTarget::Shortcut(STUDIO_LNK.into()));
    }

    #[test]
    fn umid_match_wins_and_keeps_browser_profiles_apart() {
        let profile_lnk = r"C:\Start Menu\Programs\Chrome - Work.lnk";
        let chrome = r"C:\Program Files\Google\Chrome\Application\chrome.exe";
        let by_umid = shortcut(
            profile_lnk,
            chrome,
            "Chrome - Work",
            Some("Chrome.Profile2"),
        );
        let menu = vec![
            shortcut(
                r"C:\Start Menu\Programs\Chrome.lnk",
                chrome,
                "Google Chrome",
                None,
            ),
            by_umid.clone(),
        ];
        let mut item = pinned(chrome, "Chrome - Work");
        item.umid = Some("Chrome.Profile2".into());
        let target =
            resolve_launch_target(&item, Some(by_umid), &menu, files(&[chrome, profile_lnk]));
        assert_eq!(target, LaunchTarget::Shortcut(profile_lnk.into()));
    }

    #[test]
    fn packaged_apps_open_by_app_id() {
        let app = packaged(
            "Microsoft.WindowsCalculator_8wekyb3d8bbwe!App",
            "Calculator",
        );
        let mut item = pinned("", "Calculator");
        item.umid = app.umid.clone();
        let target = resolve_launch_target(&item, Some(app.clone()), &[app], files(&[]));
        assert_eq!(
            target,
            LaunchTarget::AppId("Microsoft.WindowsCalculator_8wekyb3d8bbwe!App".into())
        );
    }

    #[test]
    fn working_relaunch_command_with_arguments_is_kept() {
        let program = r"C:\Apps\tool.exe";
        let mut item = pinned(program, "Tool");
        item.relaunch = Some(Relaunch {
            command: program.into(),
            args: Some(RelaunchArguments::String("--profile work".into())),
            ..Default::default()
        });
        let menu = vec![shortcut(r"C:\Start Menu\Tool.lnk", program, "Tool", None)];
        let target = resolve_launch_target(&item, None, &menu, files(&[program]));
        assert!(matches!(target, LaunchTarget::Command { .. }));
    }

    #[test]
    fn falls_back_to_the_saved_file_without_start_menu_entry() {
        let item = pinned(r"C:\Docs\notes.txt", "notes");
        let target = resolve_launch_target(&item, None, &[], files(&[]));
        assert_eq!(target, LaunchTarget::File(r"C:\Docs\notes.txt".into()));
    }

    #[test]
    fn deleted_shortcut_is_skipped() {
        let menu = vec![shortcut(STUDIO_LNK, NEW_STUDIO, "Roblox Studio", None)];
        let mut item = pinned(OLD_STUDIO, "Roblox Studio");
        item.umid = Some("Roblox.Studio".into());
        let target = resolve_launch_target(&item, None, &menu, files(&[]));
        assert_eq!(target, LaunchTarget::AppId("Roblox.Studio".into()));
    }
}
