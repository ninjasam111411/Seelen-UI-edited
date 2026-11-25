//! Cover art for games that don't have any on disk.
//!
//! Steam games use their public store images; other games are looked up on the Steam store by
//! name, then on Wikipedia (games not sold on Steam, e.g. console games played in an emulator).
//! Only an exact name match is used. Title logos and, for games with no program of their own to
//! take an icon from, icons come from Steam too. Images are downloaded once into the cache folder
//! and remembered in an index, so each game is looked up only once (failures are retried after a
//! week). Everything runs in the background: the next games scan picks up what was found.

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::LazyLock,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use seelen_core::system_state::{GameSource, InstalledGame};
use serde::{Deserialize, Serialize};

use crate::{
    get_tokio_handle,
    system_modules::games::domain::{
        EpicArt, base_game_name, itch_search_match, steam_art_urls, steam_icon_url,
        steam_search_match, steam_search_similar, wikipedia_cover_match,
    },
    utils::{constants::SEELEN_COMMON, lock_free::TracedMutex},
};

const RETRY_AFTER_SECS: u64 = 24 * 60 * 60;
/// Bumped when new kinds of art are looked up, so games checked before get them too.
const ART_VERSION: u32 = 4;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

static INDEX: LazyLock<TracedMutex<HashMap<String, ArtEntry>>> =
    LazyLock::new(|| TracedMutex::new(load_index()));
/// Art urls a launcher knows for its games (e.g. the Epic catalog), by game id.
static HINTS: LazyLock<TracedMutex<HashMap<String, EpicArt>>> =
    LazyLock::new(|| TracedMutex::new(HashMap::new()));
static PENDING: LazyLock<TracedMutex<HashSet<String>>> =
    LazyLock::new(|| TracedMutex::new(HashSet::new()));
static HTTP: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        // Wikipedia refuses requests without one
        .user_agent("SeelenUI-Edited/1.0 (https://github.com/ninjasam111411/Seelen-UI-edited)")
        .build()
        .unwrap_or_default()
});

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ArtEntry {
    cover: Option<PathBuf>,
    hero: Option<PathBuf>,
    #[serde(default)]
    icon: Option<PathBuf>,
    #[serde(default)]
    logo: Option<PathBuf>,
    checked_at: u64,
    #[serde(default)]
    version: u32,
}

/// Remembers art urls a launcher gave for a game, tried before searching for it.
pub fn hint(game_id: &str, art: EpicArt) {
    HINTS.lock().insert(game_id.to_owned(), art);
}

/// Fills in the cover/hero of games without one, from art found earlier.
pub fn apply(games: &mut [InstalledGame]) {
    let index = INDEX.lock();
    for game in games.iter_mut() {
        let Some(entry) = index.get(&game.id) else {
            continue;
        };
        if game.cover.is_none() {
            game.cover = entry.cover.clone().filter(|p| p.exists());
        }
        if game.hero.is_none() {
            game.hero = entry.hero.clone().filter(|p| p.exists());
        }
        if game.icon.is_none() {
            game.icon = entry.icon.clone().filter(|p| p.exists());
        }
        if game.logo.is_none() {
            game.logo = entry.logo.clone().filter(|p| p.exists());
        }
    }
}

/// Starts looking up art for the games never checked (or checked by an older version), and
/// again after a while for the ones that still have no cover.
pub fn request(games: &[InstalledGame]) {
    let now = now_secs();
    let wanted: Vec<InstalledGame> = {
        let index = INDEX.lock();
        let mut pending = PENDING.lock();
        games
            .iter()
            .filter(|game| {
                index.get(&game.id).is_none_or(|e| {
                    e.version < ART_VERSION
                        || (game.cover.is_none() && now - e.checked_at > RETRY_AFTER_SECS)
                })
            })
            .filter(|game| pending.insert(game.id.clone()))
            .cloned()
            .collect()
    };
    if wanted.is_empty() {
        return;
    }

    get_tokio_handle().spawn(async move {
        for game in wanted {
            let previous = INDEX.lock().get(&game.id).cloned().unwrap_or_default();
            let entry = find_art(&game, previous).await;
            INDEX.lock().insert(game.id.clone(), entry);
            PENDING.lock().remove(&game.id);
            save_index();
        }
    });
}

/// Fills in what the game and the previous lookup are both missing.
async fn find_art(game: &InstalledGame, previous: ArtEntry) -> ArtEntry {
    let mut entry = ArtEntry {
        checked_at: now_secs(),
        version: ART_VERSION,
        ..previous
    };
    let dir = art_dir();
    if std::fs::create_dir_all(&dir).is_err() {
        return entry;
    }
    let file_id = sanitize(&game.id);
    let path = |kind: &str, ext: &str| dir.join(format!("{file_id}_{kind}.{ext}"));
    let need_cover = game.cover.is_none() && entry.cover.is_none();
    let need_hero = game.hero.is_none() && entry.hero.is_none();
    let need_logo = game.logo.is_none() && entry.logo.is_none();
    // a game with a program or an app id gets its icon from there, which looks sharper
    let need_icon = game.icon.is_none()
        && entry.icon.is_none()
        && game.executable.is_none()
        && game.umid.is_none();
    if !(need_cover || need_hero || need_logo || need_icon) {
        return entry;
    }

    // art the game's own launcher knows comes first
    let hints = HINTS.lock().get(&game.id).cloned().unwrap_or_default();
    if need_cover && !hints.cover.is_empty() {
        entry.cover = download_first(&hints.cover, &path("cover", "jpg")).await;
    }
    if need_hero && !hints.hero.is_empty() {
        entry.hero = download_first(&hints.hero, &path("hero", "jpg")).await;
    }
    if need_logo && !hints.logo.is_empty() {
        entry.logo = download_first(&hints.logo, &path("logo", "png")).await;
    }
    let need_cover = need_cover && entry.cover.is_none();
    let need_hero = need_hero && entry.hero.is_none();
    let need_logo = need_logo && entry.logo.is_none();

    let appid = match game.source {
        GameSource::Steam => game.id.strip_prefix("steam:").map(str::to_owned),
        _ => search_steam(&game.name).await,
    };
    if let Some(appid) = &appid {
        let (covers, heroes, logos) = steam_art_urls(appid);
        if need_cover {
            entry.cover = download_first(&covers, &path("cover", "jpg")).await;
        }
        if need_hero {
            entry.hero = download_first(&heroes, &path("hero", "jpg")).await;
        }
        if need_logo {
            entry.logo = download_first(&logos, &path("logo", "png")).await;
        }
        if need_icon {
            if let Some(url) = steam_icon(appid).await {
                entry.icon = download_first(&[url], &path("icon", "jpg")).await;
            }
        }
    }
    if need_cover && entry.cover.is_none() && game.source != GameSource::Steam {
        if let Some(url) = search_wikipedia(&game.name).await {
            entry.cover = download_first(&[url], &path("cover", "jpg")).await;
        }
    }
    // small games downloaded from websites are often only on itch.io
    if need_cover && entry.cover.is_none() && game.source != GameSource::Steam {
        if let Some(url) = search_itch(&game.name).await {
            entry.cover = download_first(&[url], &path("cover", "png")).await;
        }
    }
    entry
}

/// Searches Steam by the game's name, then by its name without edition words: a result with the
/// same name wins, else one with a similar name.
async fn search_steam(name: &str) -> Option<String> {
    let mut similar = None;
    for term in search_terms(name) {
        let url = format!(
            "https://store.steampowered.com/api/storesearch/?term={}&l=english&cc=US",
            urlencoding::encode(&term)
        );
        let Some(body) = get_text(&url).await else {
            continue;
        };
        if let Some(found) = steam_search_match(&body, name) {
            return Some(found);
        }
        similar = similar.or_else(|| steam_search_similar(&body, name));
    }
    similar
}

async fn search_itch(name: &str) -> Option<String> {
    for term in search_terms(name) {
        let url = format!("https://itch.io/search?q={}", urlencoding::encode(&term));
        if let Some(found) = get_text(&url).await.and_then(|b| itch_search_match(&b, name)) {
            return Some(found);
        }
    }
    None
}

/// The name, and the name without edition words when it differs (in its original letter case,
/// as Wikipedia titles are case sensitive).
fn search_terms(name: &str) -> Vec<String> {
    let base = base_game_name(name);
    let mut terms = vec![name.to_owned()];
    let cleaned: String = name
        .chars()
        .filter(|c| !matches!(c, '™' | '®' | '©' | '\''))
        .collect();
    let base_cased: String = cleaned.chars().take(base.chars().count()).collect();
    if !base.is_empty() && base_cased.to_lowercase() != name.to_lowercase() {
        terms.push(base_cased.trim().to_owned());
    }
    terms
}

/// The app's icon as the Steam client shows it, from the public store browse api.
async fn steam_icon(appid: &str) -> Option<String> {
    let input = format!(
        r#"{{"ids":[{{"appid":{appid}}}],"context":{{"language":"english","country_code":"US"}},"data_request":{{"include_assets":true}}}}"#
    );
    let url = format!(
        "https://api.steampowered.com/IStoreBrowseService/GetItems/v1/?input_json={}",
        urlencoding::encode(&input)
    );
    let body = get_text(&url).await?;
    steam_icon_url(&body, appid)
}

async fn get_text(url: &str) -> Option<String> {
    let response = HTTP.get(url).send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    response.text().await.ok()
}

/// Cover art from the game's Wikipedia page: the page with the game's exact name (or with
/// "(video game)" after it), else a search result with that name.
async fn search_wikipedia(name: &str) -> Option<String> {
    for term in search_terms(name) {
        if let Some(found) = search_wikipedia_for(&term).await {
            return Some(found);
        }
    }
    None
}

async fn search_wikipedia_for(name: &str) -> Option<String> {
    let api = "https://en.wikipedia.org/w/api.php?action=query&format=json&redirects=1\
               &prop=pageimages|description&piprop=thumbnail&pithumbsize=600&pilicense=any";
    let by_title = format!(
        "{api}&titles={}",
        urlencoding::encode(&format!("{name}|{name} (video game)"))
    );
    let by_search = format!(
        "{api}&generator=search&gsrlimit=5&gsrsearch={}",
        urlencoding::encode(&format!("{name} video game"))
    );
    for url in [by_title, by_search] {
        if let Some(found) = get_text(&url).await.and_then(|b| wikipedia_cover_match(&b, name)) {
            return Some(found);
        }
    }
    None
}

/// Downloads the first url that answers with an image.
async fn download_first(urls: &[String], target: &Path) -> Option<PathBuf> {
    for url in urls {
        let Ok(response) = HTTP.get(url).send().await else {
            continue;
        };
        let is_image = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.starts_with("image/"));
        if !response.status().is_success() || !is_image {
            continue;
        }
        let Ok(bytes) = response.bytes().await else {
            continue;
        };
        if std::fs::write(target, &bytes).is_ok() {
            return Some(target.to_path_buf());
        }
    }
    None
}

fn art_dir() -> PathBuf {
    SEELEN_COMMON.app_cache_dir().join("games-art")
}

fn index_path() -> PathBuf {
    art_dir().join("index.json")
}

fn load_index() -> HashMap<String, ArtEntry> {
    std::fs::read_to_string(index_path())
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn save_index() {
    let snapshot = INDEX.lock().clone();
    if std::fs::create_dir_all(art_dir()).is_err() {
        return;
    }
    if let Ok(text) = serde_json::to_string(&snapshot) {
        let _ = std::fs::write(index_path(), text);
    }
}

fn sanitize(id: &str) -> String {
    let safe: String = id
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    // long paths (e.g. ids made of executable paths) are shortened with a hash
    if safe.len() <= 80 {
        return safe;
    }
    let hash = id.bytes().fold(0xcbf29ce484222325_u64, |h, b| {
        (h ^ b as u64).wrapping_mul(0x100000001b3)
    });
    format!("{}_{hash:016x}", &safe[..60])
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default()
}
