// Glass player: shared types and the `player_*` Tauri commands over Spotify, YT Music and mpv.

use std::future::Future;

use serde::{Deserialize, Serialize};

use history::HistoryEntry;
use spotify_api::{ApiError, PlayTarget};

pub mod history;
#[cfg(target_os = "linux")]
pub mod mpv;
pub mod spotify_api;
pub mod spotify_auth;
pub mod spotify_device;
#[cfg(target_os = "linux")]
pub mod ytmusic;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Source {
    Spotify,
    Ytmusic,
}

/// `Context` is a DJ or a list.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    Track,
    Artist,
    Album,
    Context,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub source: Source,
    pub kind: Kind,
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub art_url: String,
}

// ── Tauri commands (src/core/bridge.ts, shapes in src/core/player.ts) ─────────

/// Spotify's DJ. Confirmed in Task 11; if it changes, only this constant changes.
pub const DJ_URI: &str = "spotify:playlist:37i9dQZF1EYkqdzj48dyYq";
const RECENTS_MAX: usize = 20;

/// One source failed; the other's results still go out.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SourceError {
    pub source: Source,
    pub message: String,
    pub login_needed: bool,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub seq: u32,
    pub items: Vec<Item>,
    pub errors: Vec<SourceError>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ArtistPage {
    pub tracks: Vec<Item>,
    pub albums: Vec<Item>,
}

/// Why a play, the DJ or an artist/album load failed, as the view shows it (`loginNeeded`: offer to sign in).
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PlayerError {
    pub message: String,
    pub login_needed: bool,
}

impl From<String> for PlayerError {
    fn from(message: String) -> Self {
        PlayerError { message, login_needed: false }
    }
}

impl From<ApiError> for PlayerError {
    fn from(e: ApiError) -> Self {
        let SourceError { message, login_needed, .. } = api_error(&e);
        PlayerError { message, login_needed }
    }
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PlayResult {
    pub fallback_used: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Wanted {
    Spotify,
    Ytmusic,
    Both,
}

fn wanted(source: &str) -> Result<Wanted, String> {
    match source {
        "spotify" => Ok(Wanted::Spotify),
        "ytmusic" => Ok(Wanted::Ytmusic),
        "both" => Ok(Wanted::Both),
        other => Err(format!("unknown player source: {other}")),
    }
}

fn api_error(e: &ApiError) -> SourceError {
    let (message, login_needed) = match e {
        ApiError::LoginNeeded => (crate::i18n::t("Spotify login needed"), true),
        ApiError::RateLimited { retry_after_s } => (
            crate::i18n::tf("Spotify is busy, try again in {seconds} s", &[("seconds", &retry_after_s.to_string())]),
            false,
        ),
        ApiError::NoDevice => (crate::i18n::t("No Spotify device"), false),
        ApiError::Other(m) => (m.clone(), false),
    };
    SourceError { source: Source::Spotify, message, login_needed }
}

fn yt_error(message: String) -> SourceError {
    SourceError { source: Source::Ytmusic, message, login_needed: false }
}

/// Newest first by `at_ms`, one entry per (source, id), at most `max`.
pub fn merge_recents(local: Vec<HistoryEntry>, remote: Vec<HistoryEntry>, max: usize) -> Vec<Item> {
    let mut all: Vec<HistoryEntry> = local.into_iter().chain(remote).collect();
    all.sort_by(|a, b| b.at_ms.cmp(&a.at_ms));
    let mut out: Vec<Item> = Vec::new();
    for e in all {
        if out.len() >= max {
            break;
        }
        if !out.iter().any(|i| i.source == e.item.source && i.id == e.item.id) {
            out.push(e.item);
        }
    }
    out
}

async fn maybe<F: Future>(f: Option<F>) -> Option<F::Output> {
    match f {
        Some(f) => Some(f.await),
        None => None,
    }
}

/// Runs the asked sources at once; Spotify's items come first. A failing source only adds an error.
async fn combine<S, Y>(spotify: Option<S>, ytmusic: Option<Y>) -> (Vec<Item>, Vec<SourceError>)
where
    S: Future<Output = Result<Vec<Item>, SourceError>>,
    Y: Future<Output = Result<Vec<Item>, SourceError>>,
{
    let (a, b) = tokio::join!(maybe(spotify), maybe(ytmusic));
    let mut items = Vec::new();
    let mut errors = Vec::new();
    for r in [a, b].into_iter().flatten() {
        match r {
            Ok(v) => items.extend(v),
            Err(e) => errors.push(e),
        }
    }
    (items, errors)
}

// YT Music and mpv exist only on Linux; elsewhere the same calls answer "only on Linux".

#[cfg(target_os = "linux")]
async fn yt_search(q: &str) -> Result<Vec<Item>, SourceError> {
    ytmusic::search(q).await.map_err(yt_error)
}

#[cfg(target_os = "linux")]
async fn yt_artist(id: &str) -> Result<Vec<Item>, String> {
    ytmusic::artist(id).await
}

#[cfg(target_os = "linux")]
async fn yt_album(id: &str) -> Result<Vec<Item>, String> {
    ytmusic::album(id).await
}

#[cfg(target_os = "linux")]
async fn yt_play(video_id: &str) -> Result<(), String> {
    mpv::play_ytmusic(video_id).await
}

#[cfg(target_os = "linux")]
async fn pause_ytmusic() {
    match tokio::time::timeout(PAUSE_WAIT, mpv::pause()).await {
        Ok(Ok(())) => {}
        Ok(Err(e)) => eprintln!("player: could not pause YT Music: {e}"),
        Err(_) => eprintln!("player: pausing YT Music took too long, playing anyway"),
    }
}

#[cfg(not(target_os = "linux"))]
fn linux_only() -> String {
    crate::i18n::t("YT Music only works on Linux")
}

#[cfg(not(target_os = "linux"))]
async fn yt_search(_q: &str) -> Result<Vec<Item>, SourceError> {
    Err(yt_error(linux_only()))
}

#[cfg(not(target_os = "linux"))]
async fn yt_artist(_id: &str) -> Result<Vec<Item>, String> {
    Err(linux_only())
}

#[cfg(not(target_os = "linux"))]
async fn yt_album(_id: &str) -> Result<Vec<Item>, String> {
    Err(linux_only())
}

#[cfg(not(target_os = "linux"))]
async fn yt_play(_video_id: &str) -> Result<(), String> {
    Err(linux_only())
}

#[cfg(not(target_os = "linux"))]
async fn pause_ytmusic() {}

/// Pausing the other source never holds up the play for long: past this it is given up.
const PAUSE_WAIT: std::time::Duration = std::time::Duration::from_millis(1500);

async fn pause_spotify() {
    match tokio::time::timeout(PAUSE_WAIT, spotify_api::pause()).await {
        Ok(Ok(())) => {}
        Ok(Err(e)) => eprintln!("player: could not pause Spotify: {e}"),
        Err(_) => eprintln!("player: pausing Spotify took too long, playing anyway"),
    }
}

/// Plays on the chosen Spotify device; true when that is the official app (the fallback).
async fn play_spotify(target: &PlayTarget) -> Result<bool, ApiError> {
    let (device, fallback_used) = spotify_device::ensure_device().await?;
    spotify_api::play(&device, target).await?;
    Ok(fallback_used)
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// One play (or DJ) at a time: a second press waits for the first to finish instead of racing it.
static PLAY_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

async fn one_at_a_time<F: Future>(f: F) -> F::Output {
    let _guard = PLAY_LOCK.lock().await;
    f.await
}

/// Serialises load → push → save so two plays never lose each other's entry.
static HISTORY_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Adds a played item to the local history. A disk problem is logged, never a failed play.
fn remember(item: Item) {
    let _guard = HISTORY_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let mut list = history::load();
    history::push(&mut list, item, now_ms());
    if let Err(e) = history::save(&list) {
        eprintln!("player: could not save the history: {e}");
    }
}

fn dj_item() -> Item {
    Item {
        source: Source::Spotify,
        kind: Kind::Context,
        id: DJ_URI.into(),
        title: "DJ".into(),
        subtitle: "Spotify".into(),
        art_url: String::new(),
    }
}

#[tauri::command]
pub async fn player_search(source: String, query: String, offset: u32, seq: u32) -> Result<SearchResult, String> {
    let which = wanted(&source)?;
    let q = query.trim();
    if q.is_empty() {
        return Ok(SearchResult { seq, items: Vec::new(), errors: Vec::new() });
    }
    let spotify = matches!(which, Wanted::Spotify | Wanted::Both)
        .then(|| async move { spotify_api::search(q, offset).await.map_err(|e| api_error(&e)) });
    // "both" on Windows is Spotify alone. YT Music has no paging: its first page is all there is.
    let yt_asked = which == Wanted::Ytmusic || (which == Wanted::Both && cfg!(target_os = "linux"));
    let ytmusic = (yt_asked && offset == 0).then(|| yt_search(q));
    let (items, errors) = combine(spotify, ytmusic).await;
    Ok(SearchResult { seq, items, errors })
}

#[tauri::command]
pub async fn player_artist(source: String, id: String, name: String) -> Result<ArtistPage, PlayerError> {
    match wanted(&source)? {
        Wanted::Spotify => {
            let (tracks, albums) = tokio::join!(spotify_api::artist_tracks(&name), spotify_api::artist_albums(&id));
            Ok(ArtistPage { tracks: tracks?, albums: albums? })
        }
        Wanted::Ytmusic => {
            let (tracks, albums) = yt_artist(&id).await?.into_iter().partition(|i| i.kind == Kind::Track);
            Ok(ArtistPage { tracks, albums })
        }
        Wanted::Both => Err(String::from("an artist belongs to one source").into()),
    }
}

#[tauri::command]
pub async fn player_album(source: String, id: String) -> Result<Vec<Item>, PlayerError> {
    match wanted(&source)? {
        Wanted::Spotify => Ok(spotify_api::album_tracks(&id).await?),
        Wanted::Ytmusic => Ok(yt_album(&id).await?),
        Wanted::Both => Err(String::from("an album belongs to one source").into()),
    }
}

/// Plays where the user listens, after pausing the other source; then records it in the history.
#[tauri::command]
pub async fn player_play(item: Item) -> Result<PlayResult, PlayerError> {
    one_at_a_time(play_item(item)).await
}

async fn play_item(item: Item) -> Result<PlayResult, PlayerError> {
    if item.source == Source::Spotify && item.id == DJ_URI {
        return start_dj().await; // replayed from the recents: keep the DJ's app fallback
    }
    let fallback_used = match item.source {
        Source::Spotify => {
            pause_ytmusic().await;
            let target = match item.kind {
                Kind::Track => PlayTarget::Track(spotify_api::uri(Kind::Track, &item.id)),
                kind => PlayTarget::Context(spotify_api::uri(kind, &item.id)),
            };
            play_spotify(&target).await?
        }
        Source::Ytmusic => {
            // mpv plays one video id: an album starts at its first song. It is found before
            // Spotify is paused, so a failed lookup leaves what plays alone.
            let video_id = match item.kind {
                Kind::Track => item.id.clone(),
                Kind::Album => yt_album(&item.id)
                    .await?
                    .into_iter()
                    .find(|i| i.kind == Kind::Track)
                    .map(|i| i.id)
                    .ok_or_else(|| crate::i18n::t("Couldn't play this"))?,
                Kind::Artist | Kind::Context => return Err(crate::i18n::t("Couldn't play this").into()),
            };
            pause_spotify().await;
            yt_play(&video_id).await?;
            false
        }
    };
    remember(item);
    Ok(PlayResult { fallback_used })
}

/// Local history plus Spotify's recently played (only with a session: never asks to sign in for this).
#[tauri::command]
pub async fn player_recents() -> Vec<Item> {
    let remote = match spotify_auth::access_token().await {
        Ok(_) => spotify_api::recently_played().await.unwrap_or_else(|e| {
            eprintln!("player: recently played failed: {e}");
            Vec::new()
        }),
        Err(_) => Vec::new(),
    };
    merge_recents(history::load(), remote, RECENTS_MAX)
}

/// Starts Spotify's DJ; when the Web API refuses it, opens it in the Spotify app instead.
#[tauri::command]
pub async fn player_dj() -> Result<PlayResult, PlayerError> {
    one_at_a_time(start_dj()).await
}

async fn start_dj() -> Result<PlayResult, PlayerError> {
    pause_ytmusic().await;
    let open_app = || {
        crate::platform::open_url(DJ_URI);
        spotify_device::minimize_app();
    };
    dj_with(play_spotify(&PlayTarget::Context(DJ_URI.into())), open_app, || remember(dj_item())).await
}

/// Only the API refusing the DJ itself sends it to the app; signing in, the rate limit and a
/// missing device would fail there too, so they are told to the user instead.
fn dj_opens_app(e: &ApiError) -> bool {
    matches!(e, ApiError::Other(_))
}

/// The DJ's outcome: played through the API (recorded in the history), opened in the app, or failed.
async fn dj_with<P>(play: P, open_app: impl FnOnce(), record: impl FnOnce()) -> Result<PlayResult, PlayerError>
where
    P: Future<Output = Result<bool, ApiError>>,
{
    match play.await {
        Ok(fallback_used) => {
            record();
            Ok(PlayResult { fallback_used })
        }
        Err(e) if dj_opens_app(&e) => {
            eprintln!("player: the DJ through the Web API failed ({e}); opening it in the Spotify app");
            open_app();
            Ok(PlayResult { fallback_used: true })
        }
        Err(e) => Err(e.into()),
    }
}

/// Opens Spotify's sign-in in the browser and waits for it.
#[tauri::command]
pub async fn player_spotify_login(app: tauri::AppHandle) -> Result<(), String> {
    spotify_auth::login(&app).await
}

#[tauri::command]
pub async fn player_spotify_connected() -> bool {
    spotify_auth::access_token().await.is_ok()
}

#[cfg(test)]
mod tests {
    use super::history::HistoryEntry;
    use super::*;

    fn item(source: Source, id: &str) -> Item {
        Item { source, kind: Kind::Track, id: id.into(), title: id.into(), subtitle: String::new(), art_url: String::new() }
    }

    fn at(source: Source, id: &str, at_ms: i64) -> HistoryEntry {
        HistoryEntry { item: item(source, id), at_ms }
    }

    fn ids(v: &[Item]) -> Vec<(Source, &str)> {
        v.iter().map(|i| (i.source, i.id.as_str())).collect()
    }

    #[test]
    fn merge_recents_orders_by_time_and_dedupes() {
        let local = vec![at(Source::Ytmusic, "y1", 50), at(Source::Spotify, "a", 30), at(Source::Spotify, "b", 10)];
        // "a" was also played on Spotify elsewhere, more recently; "y1" on Spotify is another song.
        let remote = vec![at(Source::Spotify, "a", 40), at(Source::Spotify, "c", 20), at(Source::Spotify, "y1", 5)];
        let got = merge_recents(local, remote, 20);
        assert_eq!(
            ids(&got),
            vec![
                (Source::Ytmusic, "y1"),
                (Source::Spotify, "a"),
                (Source::Spotify, "c"),
                (Source::Spotify, "b"),
                (Source::Spotify, "y1"),
            ]
        );
    }

    #[test]
    fn merge_recents_caps() {
        let local: Vec<_> = (0..15).map(|i| at(Source::Spotify, &format!("l{i}"), i)).collect();
        let remote: Vec<_> = (0..15).map(|i| at(Source::Spotify, &format!("r{i}"), 100 + i)).collect();
        let got = merge_recents(local, remote, 20);
        assert_eq!(got.len(), 20);
        assert_eq!(got[0].id, "r14");
        assert_eq!(got[19].id, "l10");
        assert!(merge_recents(vec![at(Source::Spotify, "a", 1)], vec![], 0).is_empty());
    }

    #[tokio::test]
    async fn dj_records_only_when_the_api_played() {
        let (mut opened, mut recorded) = (false, false);
        let r = dj_with(async { Ok(false) }, || opened = true, || recorded = true).await;
        assert_eq!(r, Ok(PlayResult { fallback_used: false }));
        assert!(recorded && !opened);

        let (mut opened, mut recorded) = (false, false);
        let refused = async { Err(ApiError::Other("Spotify: HTTP 403".into())) };
        let r = dj_with(refused, || opened = true, || recorded = true).await;
        assert_eq!(r, Ok(PlayResult { fallback_used: true }));
        assert!(opened && !recorded, "the app opened; nothing is known to have played");
    }

    #[tokio::test]
    async fn dj_surfaces_login_rate_limit_and_no_device() {
        for e in [ApiError::LoginNeeded, ApiError::RateLimited { retry_after_s: 3 }, ApiError::NoDevice] {
            let (mut opened, mut recorded) = (false, false);
            let failing = async { Err(e.clone()) };
            let err = dj_with(failing, || opened = true, || recorded = true).await.unwrap_err();
            assert_eq!(err, PlayerError::from(e.clone()));
            assert_eq!(err.login_needed, e == ApiError::LoginNeeded);
            assert!(!opened && !recorded, "{e:?} must not open the app");
        }
    }

    #[test]
    fn player_error_serializes_message_and_login_flag() {
        let v = serde_json::to_value(PlayerError::from(ApiError::LoginNeeded)).unwrap();
        assert_eq!(v["loginNeeded"], true);
        assert_eq!(v["message"], crate::i18n::t("Spotify login needed"));
        let v = serde_json::to_value(PlayerError::from(String::from("boom"))).unwrap();
        assert_eq!(v, serde_json::json!({ "message": "boom", "loginNeeded": false }));
    }

    #[tokio::test]
    async fn plays_never_overlap() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;
        let running = Arc::new(AtomicUsize::new(0));
        let most = Arc::new(AtomicUsize::new(0));
        let one = |running: Arc<AtomicUsize>, most: Arc<AtomicUsize>| async move {
            let now = running.fetch_add(1, Ordering::SeqCst) + 1;
            most.fetch_max(now, Ordering::SeqCst);
            tokio::time::sleep(std::time::Duration::from_millis(30)).await;
            running.fetch_sub(1, Ordering::SeqCst);
        };
        tokio::join!(
            one_at_a_time(one(running.clone(), most.clone())),
            one_at_a_time(one(running.clone(), most.clone())),
            one_at_a_time(one(running.clone(), most.clone())),
        );
        assert_eq!(most.load(Ordering::SeqCst), 1);
        // Without the lock the same three do overlap: the test sees it.
        most.store(0, Ordering::SeqCst);
        tokio::join!(one(running.clone(), most.clone()), one(running.clone(), most.clone()));
        assert_eq!(most.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn both_keeps_spotify_when_ytmusic_fails() {
        let spotify = async { Ok(vec![item(Source::Spotify, "s1"), item(Source::Spotify, "s2")]) };
        let ytmusic = async { Err(SourceError { source: Source::Ytmusic, message: "boom".into(), login_needed: false }) };
        let (items, errors) = combine(Some(spotify), Some(ytmusic)).await;
        assert_eq!(ids(&items), vec![(Source::Spotify, "s1"), (Source::Spotify, "s2")]);
        assert_eq!(errors.len(), 1);
        assert_eq!((errors[0].source, errors[0].message.as_str()), (Source::Ytmusic, "boom"));
    }

    #[tokio::test]
    async fn both_concatenates_spotify_then_ytmusic_and_skips_a_missing_source() {
        let (items, errors) = combine(
            Some(async { Ok(vec![item(Source::Spotify, "s")]) }),
            Some(async { Ok(vec![item(Source::Ytmusic, "y")]) }),
        )
        .await;
        assert_eq!(ids(&items), vec![(Source::Spotify, "s"), (Source::Ytmusic, "y")]);
        assert!(errors.is_empty());
        let none: Option<std::future::Ready<Result<Vec<Item>, SourceError>>> = None;
        let (items, errors) = combine(Some(async { Ok(vec![item(Source::Spotify, "s")]) }), none).await;
        assert_eq!(items.len(), 1);
        assert!(errors.is_empty());
    }
}
