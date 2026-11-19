use std::path::{Path, PathBuf};

use seelen_core::system_state::{GameSource, InstalledGame};

/// Steam apps that are installed like games but aren't (runtimes, redistributables, tools).
const STEAM_NON_GAMES: &[&str] = &[
    "228980",  // Steamworks Common Redistributables
    "1070560", // Steam Linux Runtime
    "1391110", // Steam Linux Runtime - Soldier
    "1628350", // Steam Linux Runtime - Sniper
    "250820",  // SteamVR
];

/// OS contract of the games module: where each launcher keeps its data.
pub trait GamesSystem: Sized + Send + Sync {
    /// Never fails: no OS calls here.
    fn create() -> Self;

    /// Steam installation folder, if Steam is installed.
    fn steam_root(&self) -> Option<PathBuf>;

    /// Folder with the Epic Games launcher manifests (`*.item`), if Epic is installed.
    fn epic_manifests_dir(&self) -> Option<PathBuf>;
}

/// Flattens the quoted `"key" "value"` pairs of a Valve KeyValues (`.vdf` / `.acf`) file,
/// ignoring the nesting, which is enough to read library folders and app manifests.
pub fn vdf_pairs(text: &str) -> Vec<(String, String)> {
    let mut tokens: Vec<Option<String>> = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                let mut value = String::new();
                while let Some(c) = chars.next() {
                    match c {
                        '\\' => {
                            if let Some(escaped) = chars.next() {
                                value.push(match escaped {
                                    'n' => '\n',
                                    't' => '\t',
                                    other => other,
                                });
                            }
                        }
                        '"' => break,
                        other => value.push(other),
                    }
                }
                tokens.push(Some(value));
            }
            // braces break a key from being paired with the next section's first string
            '{' | '}' => tokens.push(None),
            _ => {}
        }
    }

    let mut pairs = Vec::new();
    let mut i = 0;
    while i + 1 < tokens.len() {
        if let (Some(key), Some(value)) = (&tokens[i], &tokens[i + 1]) {
            pairs.push((key.clone(), value.clone()));
            i += 2;
        } else {
            i += 1;
        }
    }
    pairs
}

/// Library folders listed in `steamapps/libraryfolders.vdf`.
pub fn steam_library_paths(libraryfolders_vdf: &str) -> Vec<PathBuf> {
    vdf_pairs(libraryfolders_vdf)
        .into_iter()
        .filter(|(key, _)| key.eq_ignore_ascii_case("path"))
        .map(|(_, value)| PathBuf::from(value))
        .collect()
}

/// `(appid, name)` of a `steamapps/appmanifest_<appid>.acf`, skipping non games.
pub fn steam_app_manifest(acf: &str) -> Option<(String, String)> {
    let pairs = vdf_pairs(acf);
    let get = |name: &str| {
        pairs
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.clone())
    };
    let appid = get("appid")?;
    let name = get("name")?;
    if STEAM_NON_GAMES.contains(&appid.as_str()) || name.to_lowercase().contains("redistributable")
    {
        return None;
    }
    Some((appid, name))
}

/// Builds a Steam game. `cover` is looked up by the caller, as it depends on the file system.
pub fn steam_game(appid: &str, name: &str, cover: Option<PathBuf>) -> InstalledGame {
    InstalledGame {
        id: format!("steam:{appid}"),
        name: name.to_owned(),
        source: GameSource::Steam,
        launch: format!("steam://rungameid/{appid}"),
        cover,
        executable: None,
    }
}

/// Reads an Epic Games launcher manifest (`*.item`, JSON). Engine/tool installs are skipped.
pub fn epic_game(manifest_json: &str) -> Option<InstalledGame> {
    let json: serde_json::Value = serde_json::from_str(manifest_json).ok()?;
    let text = |key: &str| json.get(key).and_then(|v| v.as_str()).map(str::to_owned);

    let app_name = text("AppName")?;
    let name = text("DisplayName").unwrap_or_else(|| app_name.clone());
    let namespace = text("CatalogNamespace")?;
    let item_id = text("CatalogItemId")?;

    let categories: Vec<String> = json
        .get("AppCategories")
        .and_then(|v| v.as_array())
        .map(|list| {
            list.iter()
                .filter_map(|c| c.as_str().map(str::to_lowercase))
                .collect()
        })
        .unwrap_or_default();
    if !categories.is_empty() && !categories.iter().any(|c| c == "games") {
        return None;
    }

    let executable = match (text("InstallLocation"), text("LaunchExecutable")) {
        (Some(dir), Some(exe)) if !exe.is_empty() => Some(Path::new(&dir).join(exe)),
        _ => None,
    };

    Some(InstalledGame {
        id: format!("epic:{app_name}"),
        name,
        source: GameSource::Epic,
        launch: format!(
            "com.epicgames.launcher://apps/{namespace}%3A{item_id}%3A{app_name}?action=launch&silent=true"
        ),
        cover: None,
        executable,
    })
}

/// Portrait cover from a list of files in Steam's library cache for one app: the 600x900
/// capsule, or the wide header as a fallback.
pub fn pick_steam_cover(files: &[PathBuf]) -> Option<PathBuf> {
    let named = |prefix: &str| {
        files
            .iter()
            .find(|path| {
                path.file_name()
                    .map(|n| n.to_string_lossy().to_lowercase())
                    .is_some_and(|n| n.starts_with(prefix))
            })
            .cloned()
    };
    named("library_600x900").or_else(|| named("header"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_steam_library_folders() {
        let vdf = r#"
"libraryfolders"
{
	"0"
	{
		"path"		"C:\\Program Files (x86)\\Steam"
		"apps"
		{
			"730"		"123"
		}
	}
	"1"
	{
		"path"		"D:\\SteamLibrary"
	}
}"#;
        assert_eq!(
            steam_library_paths(vdf),
            vec![
                PathBuf::from(r"C:\Program Files (x86)\Steam"),
                PathBuf::from(r"D:\SteamLibrary")
            ]
        );
    }

    #[test]
    fn reads_steam_app_manifest_and_skips_redistributables() {
        let game = "\"AppState\"\n{\n\t\"appid\"\t\t\"730\"\n\t\"name\"\t\t\"Counter-Strike 2\"\n}";
        assert_eq!(
            steam_app_manifest(game),
            Some(("730".into(), "Counter-Strike 2".into()))
        );
        let redist = "\"AppState\"\n{\n\t\"appid\"\t\t\"228980\"\n\t\"name\"\t\t\"Steamworks Common Redistributables\"\n}";
        assert_eq!(steam_app_manifest(redist), None);
    }

    #[test]
    fn steam_game_launches_through_steam() {
        let game = steam_game("730", "Counter-Strike 2", None);
        assert_eq!(game.launch, "steam://rungameid/730");
        assert_eq!(game.id, "steam:730");
    }

    #[test]
    fn reads_epic_manifest() {
        let json = r#"{
            "DisplayName": "Fortnite",
            "AppName": "Fortnite",
            "CatalogNamespace": "fn",
            "CatalogItemId": "4fe75bbc5a674f4f9b356b5c90567da5",
            "InstallLocation": "C:\\Program Files\\Epic Games\\Fortnite",
            "LaunchExecutable": "FortniteGame\\Binaries\\Win64\\FortniteLauncher.exe",
            "AppCategories": ["public", "games", "applications"]
        }"#;
        let game = epic_game(json).unwrap();
        assert_eq!(game.name, "Fortnite");
        assert_eq!(
            game.launch,
            "com.epicgames.launcher://apps/fn%3A4fe75bbc5a674f4f9b356b5c90567da5%3AFortnite?action=launch&silent=true"
        );
        assert!(game.executable.is_some());
    }

    #[test]
    fn skips_epic_non_games() {
        let json = r#"{"DisplayName":"Unreal Engine","AppName":"UE_5.4","CatalogNamespace":"ue",
            "CatalogItemId":"x","AppCategories":["engines"]}"#;
        assert_eq!(epic_game(json), None);
    }

    #[test]
    fn prefers_portrait_cover() {
        let files = vec![
            PathBuf::from(r"C:\cache\730\header.jpg"),
            PathBuf::from(r"C:\cache\730\library_600x900.jpg"),
        ];
        assert_eq!(
            pick_steam_cover(&files),
            Some(PathBuf::from(r"C:\cache\730\library_600x900.jpg"))
        );
    }
}
