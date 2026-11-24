use std::path::PathBuf;

/// A game installed through one of the known game launchers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(feature = "gen-binds", not(feature = "salvo")), derive(ts_rs::TS))]
#[cfg_attr(all(feature = "gen-binds", not(feature = "salvo")), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct InstalledGame {
    /// unique id, prefixed by the launcher (e.g. `steam:730`)
    pub id: String,
    pub name: String,
    pub source: GameSource,
    /// uri or program to open the game through its launcher
    pub launch: String,
    /// cover art image (portrait), when the launcher keeps one on disk
    pub cover: Option<PathBuf>,
    /// wide background art, when the launcher keeps one on disk
    pub hero: Option<PathBuf>,
    /// app user model id, used to show the icon of store games
    pub umid: Option<String>,
    /// executable of the game, used to show its icon
    pub executable: Option<PathBuf>,
    /// arguments for `launch` when it is a program (e.g. the game file for an emulator)
    #[serde(default)]
    pub launch_args: Option<String>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(feature = "gen-binds", not(feature = "salvo")), derive(ts_rs::TS))]
#[cfg_attr(all(feature = "gen-binds", not(feature = "salvo")), ts(export, repr(enum = name)))]
pub enum GameSource {
    Steam,
    Epic,
    /// Microsoft Store / Xbox (GDK) games
    Store,
    /// known game launchers found in the start menu (e.g. Roblox Player)
    Shortcut,
    Gog,
    Ubisoft,
    Itch,
    /// games downloaded from a website, found by the files next to their executable
    Local,
}
