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

/// Start menu entries of games or game launchers that aren't found any other way.
/// Matched against the entry name, case insensitive.
const KNOWN_GAME_NAMES: &[&str] = &[
    "Roblox Player",
    "Minecraft",
    "Minecraft Launcher",
    "Minecraft for Windows",
    "Minecraft Java Edition",
    "Minecraft: Java Edition",
    "Minecraft Education",
    "Riot Client",
    "League of Legends",
    "VALORANT",
    "Battle.net",
    "Fortnite",
    "Rocket League",
    "Among Us",
    "Bloons TD 6",
];

/// Package family prefixes of store games that don't ship a `MicrosoftGame.config`.
const KNOWN_STORE_FAMILIES: &[&str] = &[
    "Microsoft.4297127D64EC6_",             // Minecraft Launcher
    "Microsoft.MinecraftUWP_",              // Minecraft for Windows (Bedrock)
    "Microsoft.MinecraftEducationEdition_", // Minecraft Education
    "ROBLOXCORPORATION.ROBLOX_",            // Roblox (store app)
];

/// Launchers and tools whose folders can look like a game's but aren't games themselves.
const NON_GAME_NAMES: &[&str] = &[
    "unity",
    "unity hub",
    "unreal engine",
    "epic games launcher",
    "steam",
    "gog galaxy",
    "ubisoft connect",
    "ea",
    "ea app",
    "itch",
];

/// Parts of executable names that are never the game itself.
const NON_GAME_EXE_PARTS: &[&str] = &[
    "unins",
    "uninstall",
    "crash",
    "setup",
    "redist",
    "vcredist",
    "dxsetup",
    "report",
    "updater",
    "helper",
    "language",
    "dlc",
    "toggler",
    "changer",
    "quicksfv",
    "dxwebsetup",
    "ndp4",
    "_be.exe",
    "-cmd.exe",
    "-room.exe",
    "benchmark",
];

/// Short names people use for games in folder names, written out so the art lookup finds them.
const NAME_ABBREVIATIONS: &[(&str, &str)] = &[
    ("totk", "Tears of the Kingdom"),
    ("botw", "Breath of the Wild"),
];

/// Files that a game engine or a game store SDK leaves next to the game executable.
const GAME_MARKER_FILES: &[&str] = &[
    "unityplayer.dll",
    "gameassembly.dll",
    "steam_api.dll",
    "steam_api64.dll",
    "eossdk-win64-shipping.dll",
    "galaxy.dll",
    "galaxy64.dll",
    "renpy",
];

/// OS contract of the games module: where each launcher keeps its data.
pub trait GamesSystem: Sized + Send + Sync {
    /// Never fails: no OS calls here.
    fn create() -> Self;

    /// Steam installation folder, if Steam is installed.
    fn steam_root(&self) -> Option<PathBuf>;

    /// Folder with the Epic Games launcher manifests (`*.item`), if Epic is installed.
    fn epic_manifests_dir(&self) -> Option<PathBuf>;

    /// Whether a packaged (store) app is a game: Xbox/GDK games ship a `MicrosoftGame.config`.
    fn is_store_game(&self, umid: &str) -> bool;

    /// `(id, name, executable)` of the games installed with GOG Galaxy.
    fn gog_games(&self) -> Vec<(String, String, PathBuf)>;

    /// `(id, install folder)` of the games installed with Ubisoft Connect.
    fn ubisoft_games(&self) -> Vec<(String, PathBuf)>;

    /// Folder where the itch.io app installs games, if any.
    fn itch_apps_dir(&self) -> Option<PathBuf>;

    /// Folders where people usually keep games downloaded from websites.
    fn game_folders(&self) -> Vec<PathBuf>;
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
pub fn steam_game(
    appid: &str,
    name: &str,
    cover: Option<PathBuf>,
    hero: Option<PathBuf>,
) -> InstalledGame {
    InstalledGame {
        id: format!("steam:{appid}"),
        name: name.to_owned(),
        source: GameSource::Steam,
        launch: format!("steam://rungameid/{appid}"),
        cover,
        hero,
        umid: None,
        executable: None,
        launch_args: None,
        icon: None,
        logo: None,
    }
}

/// A store (packaged) game, opened by its app id like the start menu does.
pub fn store_game(umid: &str, name: &str) -> InstalledGame {
    InstalledGame {
        id: format!("store:{umid}"),
        name: name.to_owned(),
        source: GameSource::Store,
        launch: format!("shell:AppsFolder\\{umid}"),
        cover: None,
        hero: None,
        umid: Some(umid.to_owned()),
        executable: None,
        launch_args: None,
        icon: None,
        logo: None,
    }
}

/// A game launcher shortcut from the start menu, if its name is a known one.
pub fn shortcut_game(
    shortcut: &Path,
    name: &str,
    target: Option<&Path>,
    umid: Option<&str>,
) -> Option<InstalledGame> {
    if !is_known_game_name(name) {
        return None;
    }
    Some(InstalledGame {
        id: format!("shortcut:{}", name.to_lowercase()),
        name: name.to_owned(),
        source: GameSource::Shortcut,
        launch: shortcut.to_string_lossy().to_string(),
        cover: None,
        hero: None,
        umid: umid.map(str::to_owned),
        executable: target.map(Path::to_path_buf),
        launch_args: None,
        icon: None,
        logo: None,
    })
}

/// Whether a start menu entry name is one of the known games or game launchers.
pub fn is_known_game_name(name: &str) -> bool {
    KNOWN_GAME_NAMES
        .iter()
        .any(|known| name.trim().eq_ignore_ascii_case(known))
}

/// Whether a packaged app is a known store game, by its package family.
pub fn is_known_store_game(umid: &str) -> bool {
    let umid = umid.to_lowercase();
    KNOWN_STORE_FAMILIES
        .iter()
        .any(|family| umid.starts_with(&family.to_lowercase()))
}

pub fn is_non_game_name(name: &str) -> bool {
    NON_GAME_NAMES
        .iter()
        .any(|known| name.trim().eq_ignore_ascii_case(known))
}

/// A game executable outside of any launcher, opened directly.
pub fn local_game(executable: &Path, name: &str, source: GameSource) -> InstalledGame {
    let prefix = match source {
        GameSource::Gog => "gog",
        GameSource::Itch => "itch",
        _ => "local",
    };
    InstalledGame {
        id: format!("{prefix}:{}", executable.to_string_lossy().to_lowercase()),
        name: name.to_owned(),
        source,
        launch: executable.to_string_lossy().to_string(),
        cover: None,
        hero: None,
        umid: None,
        executable: Some(executable.to_path_buf()),
        launch_args: None,
        icon: None,
        logo: None,
    }
}

/// A Ubisoft Connect game, opened through the launcher.
pub fn ubisoft_game(id: &str, install_dir: &Path) -> Option<InstalledGame> {
    let name = install_dir.file_name()?.to_string_lossy().to_string();
    Some(InstalledGame {
        id: format!("ubisoft:{id}"),
        name,
        source: GameSource::Ubisoft,
        launch: format!("uplay://launch/{id}/0"),
        cover: None,
        hero: None,
        umid: None,
        executable: None,
        launch_args: None,
        icon: None,
        logo: None,
    })
}

/// Whether an executable can't be the game itself (uninstallers, crash reporters...).
pub fn is_non_game_exe(file_name: &str) -> bool {
    let name = file_name.to_lowercase();
    !name.ends_with(".exe") || NON_GAME_EXE_PARTS.iter().any(|part| name.contains(part))
}

/// Whether a folder holds a game, from the names of its entries (lower case) and the executable
/// to check: engine or store SDK files next to it, `<exe>_Data` (Unity), `<exe>.pck` (Godot) or
/// an Unreal shipping build.
pub fn is_game_folder(exe_name: &str, entries: &[String]) -> bool {
    let exe = exe_name.to_lowercase();
    if is_non_game_exe(&exe) {
        return false;
    }
    let stem = exe.trim_end_matches(".exe");
    if exe.ends_with("-win64-shipping.exe") {
        return true;
    }
    let data_dir = format!("{stem}_data");
    let godot_pack = format!("{stem}.pck");
    entries.iter().any(|entry| {
        GAME_MARKER_FILES.contains(&entry.as_str()) || *entry == data_dir || *entry == godot_pack
    })
}

/// The game executable of a folder: the one named like the folder, the one with a `_Data` folder
/// next to it, or the only one, if the folder looks like a game's.
pub fn pick_game_exe(folder_name: &str, entries: &[String]) -> Option<String> {
    let exes: Vec<&String> = entries.iter().filter(|e| !is_non_game_exe(e)).collect();
    let normalized_folder = normalize_name(folder_name);
    let candidate = exes
        .iter()
        .find(|exe| normalize_name(exe.trim_end_matches(".exe")) == normalized_folder)
        .or_else(|| {
            exes.iter().find(|exe| {
                let data_dir = format!("{}_data", exe.trim_end_matches(".exe"));
                entries.contains(&data_dir)
            })
        })
        .or_else(|| if exes.len() == 1 { exes.first() } else { None })?;
    is_game_folder(candidate, entries).then(|| (*candidate).clone())
}

/// A readable name from an executable or folder name: `_`/`-` become spaces, ` - ` becomes `: `
/// and known abbreviations are written out.
pub fn display_name_from(raw: &str) -> String {
    let base = raw
        .trim_end_matches(".exe")
        .trim_end_matches(".EXE")
        .trim_end_matches("-Win64-Shipping")
        .replace(" - ", ": ");
    let spaced: String = base
        .chars()
        .map(|c| if c == '_' || c == '-' { ' ' } else { c })
        .collect();
    spaced
        .split_whitespace()
        .map(|word| {
            NAME_ABBREVIATIONS
                .iter()
                .find(|(short, _)| word.eq_ignore_ascii_case(short))
                .map_or(word, |(_, long)| long)
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The program of a folder kept in a games folder (where every folder is a game, so no engine
/// files are needed): the engine-detected exe, the exe named like the folder, a `Play...` exe,
/// a launcher, or else the biggest exe. `files` are lower case file names with their size.
pub fn pick_trusted_exe(folder_name: &str, files: &[(String, u64)]) -> Option<String> {
    let names: Vec<String> = files.iter().map(|(name, _)| name.clone()).collect();
    if let Some(exe) = pick_game_exe(folder_name, &names) {
        return Some(exe);
    }
    let exes: Vec<&(String, u64)> = files.iter().filter(|(n, _)| !is_non_game_exe(n)).collect();
    let stem = |name: &str| name.trim_end_matches(".exe").to_owned();
    let folder = normalize_name(folder_name);
    exes.iter()
        .find(|(n, _)| normalize_name(&stem(n)) == folder)
        .or_else(|| exes.iter().find(|(n, _)| n.starts_with("play")))
        .or_else(|| exes.iter().find(|(n, _)| n.contains("launcher")))
        .or_else(|| exes.iter().max_by_key(|(_, size)| *size))
        .map(|(n, _)| n.clone())
}

/// Arguments that make Cemu (Wii U emulator) start a game right away.
pub fn cemu_args(game_file: &Path) -> String {
    format!("-g \"{}\"", game_file.to_string_lossy())
}

/// Cover image url from a Wikipedia API response (`prop=pageimages|description`), only from a page
/// named like the game whose description says it is a game (so other art is never used).
pub fn wikipedia_cover_match(api_json: &str, name: &str) -> Option<String> {
    let json: serde_json::Value = serde_json::from_str(api_json).ok()?;
    let wanted = normalize_name(name);
    if wanted.is_empty() {
        return None;
    }
    let pages = json.get("query")?.get("pages")?;
    let pages: Vec<&serde_json::Value> = match pages {
        serde_json::Value::Object(map) => map.values().collect(),
        serde_json::Value::Array(list) => list.iter().collect(),
        _ => return None,
    };
    let mut matching: Vec<&serde_json::Value> = pages
        .into_iter()
        .filter(|page| {
            page.get("title")
                .and_then(|t| t.as_str())
                .map(|t| t.trim_end_matches(" (video game)"))
                .is_some_and(|t| normalize_name(t) == wanted)
                && page
                    .get("description")
                    .and_then(|d| d.as_str())
                    .is_some_and(|d| d.to_lowercase().contains("game"))
        })
        .collect();
    matching.sort_by_key(|page| page.get("index").and_then(|i| i.as_u64()).unwrap_or(0));
    matching.into_iter().find_map(|page| {
        Some(page.get("thumbnail")?.get("source")?.as_str()?.to_owned())
    })
}

/// Lower case letters and digits only, to compare names written differently.
pub fn normalize_name(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Steam app id of a game from a Steam store search response, only when a result's name is
/// the same as the game's (so a different game's art is never used).
pub fn steam_search_match(search_json: &str, name: &str) -> Option<String> {
    let json: serde_json::Value = serde_json::from_str(search_json).ok()?;
    let wanted = normalize_name(name);
    if wanted.is_empty() {
        return None;
    }
    json.get("items")?.as_array()?.iter().find_map(|item| {
        let item_name = item.get("name")?.as_str()?;
        if normalize_name(item_name) != wanted {
            return None;
        }
        match item.get("id")? {
            serde_json::Value::Number(n) => Some(n.to_string()),
            serde_json::Value::String(s) => Some(s.clone()),
            _ => None,
        }
    })
}

/// Public Steam store image urls of an app: `(cover candidates, hero candidates, logo candidates)`.
pub fn steam_art_urls(appid: &str) -> (Vec<String>, Vec<String>, Vec<String>) {
    let bases = [
        format!("https://shared.cloudflare.steamstatic.com/store_item_assets/steam/apps/{appid}"),
        format!("https://cdn.cloudflare.steamstatic.com/steam/apps/{appid}"),
    ];
    (
        bases
            .iter()
            .map(|b| format!("{b}/library_600x900.jpg"))
            .collect(),
        bases
            .iter()
            .map(|b| format!("{b}/library_hero.jpg"))
            .collect(),
        bases.iter().map(|b| format!("{b}/logo.png")).collect(),
    )
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
        hero: None,
        umid: None,
        executable,
        launch_args: None,
        icon: None,
        logo: None,
    })
}

/// Portrait cover from a list of files in Steam's library cache for one app: the 600x900
/// capsule, or the wide header as a fallback.
pub fn pick_steam_cover(files: &[PathBuf]) -> Option<PathBuf> {
    pick_steam_art(files, &["library_600x900", "header"])
}

/// Wide background art from Steam's library cache for one app.
pub fn pick_steam_hero(files: &[PathBuf]) -> Option<PathBuf> {
    pick_steam_art(files, &["library_hero", "page_bg"])
}

/// Transparent title logo from Steam's library cache for one app.
pub fn pick_steam_logo(files: &[PathBuf]) -> Option<PathBuf> {
    pick_steam_art(files, &["logo"])
}

/// The app icon in Steam's library cache: the image named by its 40 characters hash.
pub fn pick_steam_icon(files: &[PathBuf]) -> Option<PathBuf> {
    files
        .iter()
        .find(|path| {
            let is_hash = path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .is_some_and(|s| s.len() == 40 && s.chars().all(|c| c.is_ascii_hexdigit()));
            let is_image = path.extension().is_some_and(|e| {
                ["jpg", "png", "ico"].iter().any(|ext| e.eq_ignore_ascii_case(ext))
            });
            is_hash && is_image
        })
        .cloned()
}

/// An icon file a game ships next to its program (e.g. `game.ico`), from lower case file names.
pub fn pick_folder_icon(files: &[String]) -> Option<String> {
    let icons: Vec<&String> = files
        .iter()
        .filter(|f| f.ends_with(".ico") && !NON_GAME_EXE_PARTS.iter().any(|p| f.contains(p)))
        .collect();
    icons
        .iter()
        .find(|f| f.contains("game") || f.contains("icon"))
        .or_else(|| icons.first())
        .map(|f| (*f).clone())
}

/// Steam community icon url from an `IStoreBrowseService/GetItems` response.
pub fn steam_icon_url(get_items_json: &str, appid: &str) -> Option<String> {
    let json: serde_json::Value = serde_json::from_str(get_items_json).ok()?;
    let item = json.get("response")?.get("store_items")?.as_array()?.first()?;
    let hash = item.get("assets")?.get("community_icon")?.as_str()?;
    if hash.is_empty() || !hash.chars().all(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    Some(format!(
        "https://cdn.cloudflare.steamstatic.com/steamcommunity/public/images/apps/{appid}/{hash}.jpg"
    ))
}

fn pick_steam_art(files: &[PathBuf], prefixes: &[&str]) -> Option<PathBuf> {
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
    prefixes.iter().find_map(|prefix| named(prefix))
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
        let game = steam_game("730", "Counter-Strike 2", None, None);
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

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_lowercase()).collect()
    }

    #[test]
    fn detects_unity_unreal_and_godot_games() {
        assert!(is_game_folder(
            "AmongUs.exe",
            &names(&["AmongUs.exe", "UnityPlayer.dll", "AmongUs_Data"])
        ));
        assert!(is_game_folder(
            "MyGame-Win64-Shipping.exe",
            &names(&["MyGame-Win64-Shipping.exe"])
        ));
        assert!(is_game_folder(
            "cool.exe",
            &names(&["cool.exe", "cool.pck"])
        ));
        assert!(is_game_folder(
            "Story.exe",
            &names(&["Story.exe", "renpy", "game"])
        ));
    }

    #[test]
    fn ignores_normal_apps_and_uninstallers() {
        assert!(!is_game_folder(
            "Discord.exe",
            &names(&["Discord.exe", "ffmpeg.dll", "resources"])
        ));
        assert!(!is_game_folder(
            "unins000.exe",
            &names(&["unins000.exe", "UnityPlayer.dll"])
        ));
        assert!(!is_game_folder(
            "UnityCrashHandler64.exe",
            &names(&["UnityCrashHandler64.exe", "UnityPlayer.dll"])
        ));
    }

    #[test]
    fn picks_the_game_exe_of_a_folder() {
        let entries = names(&[
            "UnityCrashHandler64.exe",
            "Hollow Knight.exe",
            "Hollow Knight_Data",
            "UnityPlayer.dll",
        ]);
        assert_eq!(
            pick_game_exe("Hollow Knight", &entries),
            Some("hollow knight.exe".into())
        );
        let entries = names(&["game.exe", "game_Data", "UnityPlayer.dll", "unins000.exe"]);
        assert_eq!(
            pick_game_exe("Something Else", &entries),
            Some("game.exe".into())
        );
        let entries = names(&["app.exe", "tool.exe", "readme.txt"]);
        assert_eq!(pick_game_exe("Tools", &entries), None);
    }

    #[test]
    fn readable_names() {
        assert_eq!(display_name_from("Hollow_Knight.exe"), "Hollow Knight");
        assert_eq!(display_name_from("MyGame-Win64-Shipping.exe"), "MyGame");
        assert_eq!(
            display_name_from("The Legend of Zelda - TotK"),
            "The Legend of Zelda: Tears of the Kingdom"
        );
        assert_eq!(
            display_name_from("The Legend of Zelda - Breath of the Wild"),
            "The Legend of Zelda: Breath of the Wild"
        );
    }

    fn sized(list: &[(&str, u64)]) -> Vec<(String, u64)> {
        list.iter().map(|(n, s)| (n.to_lowercase(), *s)).collect()
    }

    #[test]
    fn picks_the_program_of_folders_in_a_games_folder() {
        // GTA V: the Play exe, not the language tools, BattlEye or uninstaller
        let gta = sized(&[
            ("GTA V Language Changer.exe", 777216),
            ("GTA5_BE.exe", 1473144),
            ("GTA5.exe", 49217144),
            ("GTAVLanguageSelect.exe", 543272),
            ("GTAVLauncher.exe", 543272),
            ("PlayGTAV.exe", 98816),
            ("unins000.exe", 1515889),
            ("bink2w64.dll", 1),
        ]);
        assert_eq!(
            pick_trusted_exe("Grand Theft Auto V Legacy", &gta).as_deref(),
            Some("playgtav.exe")
        );
        // Sims 4: the launcher, not the dlc and language tools
        let sims = sized(&[
            ("dlc-toggler.exe", 951296),
            ("dlc-uninstaller.exe", 957440),
            ("EASetup.exe", 33792),
            ("language-changer.exe", 946688),
            ("Launcher.exe", 1880576),
            ("sims-4-updater-v2.4.12.exe", 24580232),
            ("unins000.exe", 1515889),
        ]);
        assert_eq!(pick_trusted_exe("The Sims 4", &sims).as_deref(), Some("launcher.exe"));
        // emulator bundle: the launcher, not the emulator tools
        let totk = sized(&[
            ("Launcher.exe", 335872),
            ("QtWebEngineProcess.exe", 591344),
            ("yuzu_ea-cmd.exe", 16134144),
            ("yuzu_ea-room.exe", 634880),
            ("yuzu.exe", 22603264),
            ("unins000.exe", 1515889),
        ]);
        assert_eq!(
            pick_trusted_exe("The Legend of Zelda - TotK", &totk).as_deref(),
            Some("launcher.exe")
        );
        // Unity game: found by its engine files
        let house = sized(&[
            ("GameAssembly.dll", 1),
            ("HouseParty.exe", 666624),
            ("UnityCrashHandler64.exe", 1185200),
            ("UnityPlayer.dll", 1),
            ("unins000.exe", 1515889),
        ]);
        assert_eq!(pick_trusted_exe("House Party", &house).as_deref(), Some("houseparty.exe"));
        // nothing but an uninstaller
        assert_eq!(pick_trusted_exe("Poppy Playtime", &sized(&[("unins000.exe", 1)])), None);
    }

    #[test]
    fn wikipedia_cover_needs_the_same_name() {
        let json = r#"{"query":{"pages":{
            "1":{"title":"The Legend of Zelda: Tears of the Kingdom","index":2,
                 "description":"2023 video game",
                 "thumbnail":{"source":"https://upload.wikimedia.org/totk.jpg"}},
            "2":{"title":"The Legend of Zelda","index":1,"description":"Video game series",
                 "thumbnail":{"source":"https://upload.wikimedia.org/zelda.jpg"}}}}}"#;
        assert_eq!(
            wikipedia_cover_match(json, "The Legend of Zelda: Tears of the Kingdom").as_deref(),
            Some("https://upload.wikimedia.org/totk.jpg")
        );
        assert_eq!(wikipedia_cover_match(json, "Breath of the Wild"), None);
        let json = r#"{"query":{"pages":{
            "8":{"title":"House party","description":"Party held at a home",
                 "thumbnail":{"source":"https://upload.wikimedia.org/party.jpg"}},
            "9":{"title":"House Party (video game)","description":"2022 video game",
                 "thumbnail":{"source":"https://upload.wikimedia.org/hp.jpg"}}}}}"#;
        assert_eq!(
            wikipedia_cover_match(json, "House Party").as_deref(),
            Some("https://upload.wikimedia.org/hp.jpg")
        );
    }

    #[test]
    fn finds_icons_and_logos() {
        let files = vec![
            PathBuf::from("/cache/730/library_600x900.jpg"),
            PathBuf::from("/cache/730/logo.png"),
            PathBuf::from("/cache/730/8dbc71957312bbd3baea65848b545be9eae2a355.jpg"),
        ];
        assert_eq!(pick_steam_logo(&files), Some(files[1].clone()));
        assert_eq!(pick_steam_icon(&files), Some(files[2].clone()));
        let folder = names(&["unins000.exe", "launcher.exe", "game.ico", "other.ico"]);
        assert_eq!(pick_folder_icon(&folder).as_deref(), Some("game.ico"));
        assert_eq!(pick_folder_icon(&names(&["a.exe"])), None);
        let json = r#"{"response":{"store_items":[{"appid":730,
            "assets":{"community_icon":"8dbc71957312bbd3baea65848b545be9eae2a355"}}]}}"#;
        assert_eq!(
            steam_icon_url(json, "730").as_deref(),
            Some("https://cdn.cloudflare.steamstatic.com/steamcommunity/public/images/apps/730/8dbc71957312bbd3baea65848b545be9eae2a355.jpg")
        );
    }

    #[test]
    fn cemu_starts_the_game() {
        assert_eq!(
            cemu_args(Path::new("C:/Games/BOTW/cemu/games/BOTW_EUR/code/U-King.rpx")),
            "-g \"C:/Games/BOTW/cemu/games/BOTW_EUR/code/U-King.rpx\""
        );
    }

    #[test]
    fn minecraft_is_found() {
        assert!(is_known_game_name("Minecraft Launcher"));
        assert!(is_known_game_name("minecraft"));
        assert!(is_known_store_game(
            "Microsoft.4297127D64EC6_8wekyb3d8bbwe!Minecraft"
        ));
        assert!(is_known_store_game(
            "Microsoft.MinecraftUWP_8wekyb3d8bbwe!App"
        ));
        assert!(!is_known_store_game(
            "Microsoft.WindowsCalculator_8wekyb3d8bbwe!App"
        ));
    }

    #[test]
    fn steam_search_needs_the_same_name() {
        let json = r#"{"total":2,"items":[{"id":945360,"name":"Among Us VR"},{"id":945361,"name":"Among Us"}]}"#;
        assert_eq!(steam_search_match(json, "Among Us"), Some("945361".into()));
        assert_eq!(steam_search_match(json, "among-us"), Some("945361".into()));
        assert_eq!(steam_search_match(json, "Minecraft"), None);
    }

    #[test]
    fn known_launchers_become_games() {
        let lnk = Path::new(r"C:\Start Menu\Programs\Roblox\Roblox Player.lnk");
        let game = shortcut_game(lnk, "Roblox Player", None, None).unwrap();
        assert_eq!(game.launch, lnk.to_string_lossy());
        assert!(shortcut_game(lnk, "Roblox Studio", None, None).is_none());
    }

    #[test]
    fn store_games_open_by_app_id() {
        let game = store_game("ROBLOXCorporation.RobloxGDK_55nm5eh3cm0pr!Game", "Roblox");
        assert_eq!(
            game.launch,
            "shell:AppsFolder\\ROBLOXCorporation.RobloxGDK_55nm5eh3cm0pr!Game"
        );
    }

    #[test]
    fn picks_hero_art() {
        let files = vec![
            PathBuf::from(r"C:\cache\730\library_600x900.jpg"),
            PathBuf::from(r"C:\cache\730\library_hero.jpg"),
        ];
        assert_eq!(
            pick_steam_hero(&files),
            Some(PathBuf::from(r"C:\cache\730\library_hero.jpg"))
        );
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
