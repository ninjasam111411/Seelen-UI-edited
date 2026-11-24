//! Cover art for games that don't have any on disk.
//!
//! Steam games use their public store images; other games are looked up on the Steam store by
//! name, and only an exact name match is used. Images are downloaded once into the cache folder
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
    system_modules::games::domain::{steam_art_urls, steam_search_match},
    utils::{constants::SEELEN_COMMON, lock_free::TracedMutex},
};

const RETRY_AFTER_SECS: u64 = 7 * 24 * 60 * 60;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

static INDEX: LazyLock<TracedMutex<HashMap<String, ArtEntry>>> =
    LazyLock::new(|| TracedMutex::new(load_index()));
static PENDING: LazyLock<TracedMutex<HashSet<String>>> =
    LazyLock::new(|| TracedMutex::new(HashSet::new()));
static HTTP: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .build()
        .unwrap_or_default()
});

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ArtEntry {
    cover: Option<PathBuf>,
    hero: Option<PathBuf>,
    checked_at: u64,
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
    }
}

/// Starts looking up art for the games that still have no cover.
pub fn request(games: &[InstalledGame]) {
    let now = now_secs();
    let wanted: Vec<InstalledGame> = {
        let index = INDEX.lock();
        let mut pending = PENDING.lock();
        games
            .iter()
            .filter(|game| game.cover.is_none())
            .filter(|game| {
                index
                    .get(&game.id)
                    .is_none_or(|e| e.cover.is_none() && now - e.checked_at > RETRY_AFTER_SECS)
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
            let entry = find_art(&game).await;
            INDEX.lock().insert(game.id.clone(), entry);
            PENDING.lock().remove(&game.id);
            save_index();
        }
    });
}

async fn find_art(game: &InstalledGame) -> ArtEntry {
    let appid = match game.source {
        GameSource::Steam => game.id.strip_prefix("steam:").map(str::to_owned),
        _ => search_steam(&game.name).await,
    };
    let mut entry = ArtEntry {
        checked_at: now_secs(),
        ..Default::default()
    };
    let Some(appid) = appid else {
        return entry;
    };

    let dir = art_dir();
    if std::fs::create_dir_all(&dir).is_err() {
        return entry;
    }
    let file_id = sanitize(&game.id);
    let (covers, heroes) = steam_art_urls(&appid);
    entry.cover = download_first(&covers, &dir.join(format!("{file_id}_cover.jpg"))).await;
    entry.hero = download_first(&heroes, &dir.join(format!("{file_id}_hero.jpg"))).await;
    entry
}

async fn search_steam(name: &str) -> Option<String> {
    let url = format!(
        "https://store.steampowered.com/api/storesearch/?term={}&l=english&cc=US",
        urlencoding::encode(name)
    );
    let response = HTTP.get(url).send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    let body = response.text().await.ok()?;
    steam_search_match(&body, name)
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
