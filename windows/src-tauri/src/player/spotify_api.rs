//! Spotify Web API client for the glass player: search, artist, album, devices, play/pause, recently played.
//!
//! Parsers are pure (they read the documented JSON shapes after the Feb-2026 dev-mode changes: no
//! `popularity`, `followers` or `available_markets`). The async functions send the request with the bearer
//! token from `spotify_auth`; on a 401 the cached token is dropped and the request is retried once.
//! `Item.id` is always the bare Spotify id; `uri()` builds the `spotify:<kind>:<id>` form that play needs.

use std::future::Future;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::history::HistoryEntry;
use super::spotify_auth::{self, pct_encode, AuthError};
use super::{Item, Kind, Source};

const API_BASE: &str = "https://api.spotify.com/v1";
const USER_AGENT: &str = concat!("Coucou/", env!("CARGO_PKG_VERSION"));
const MAX_ANSWER: usize = 256 * 1024;
const SEARCH_LIMIT: u32 = 10;
const CACHE_ENTRIES: usize = 20;
const CACHE_TTL: Duration = Duration::from_secs(60);
const MIN_ART_WIDTH: u64 = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiError {
    LoginNeeded,
    RateLimited { retry_after_s: u64 },
    NoDevice,
    Other(String),
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiError::LoginNeeded => write!(f, "Spotify login needed"),
            ApiError::RateLimited { retry_after_s } => write!(f, "Spotify rate limit, retry in {retry_after_s}s"),
            ApiError::NoDevice => write!(f, "No active Spotify device"),
            ApiError::Other(e) => write!(f, "{e}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub active: bool,
}

/// What to start: one track, or a context (album, artist, playlist).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlayTarget {
    Track(String),
    Context(String),
}

/// `spotify:<kind>:<id>` for an item's kind (`Context` has no URI of its own: the id is already one).
pub fn uri(kind: Kind, id: &str) -> String {
    let k = match kind {
        Kind::Track => "track",
        Kind::Artist => "artist",
        Kind::Album => "album",
        Kind::Context => return id.to_string(),
    };
    format!("spotify:{k}:{id}")
}

// ── Parsers ───────────────────────────────────────────────────────────────────

/// The smallest image at least 64 px wide; failing that the largest; failing that "".
fn pick_art(images: &Value) -> String {
    let list: Vec<(Option<u64>, &str)> = images
        .as_array()
        .map(|a| a.iter().filter_map(|i| Some((i["width"].as_u64(), i["url"].as_str()?))).collect())
        .unwrap_or_default();
    let big_enough = list.iter().filter(|(w, _)| w.is_some_and(|w| w >= MIN_ART_WIDTH)).min_by_key(|(w, _)| *w);
    let pick = big_enough.or_else(|| list.iter().max_by_key(|(w, _)| *w)).map(|(_, u)| *u);
    pick.unwrap_or("").to_string()
}

fn artist_names(artists: &Value) -> String {
    artists
        .as_array()
        .map(|a| a.iter().filter_map(|x| x["name"].as_str()).collect::<Vec<_>>().join(", "))
        .unwrap_or_default()
}

fn item(kind: Kind, v: &Value, subtitle: String, art_url: String) -> Option<Item> {
    Some(Item {
        source: Source::Spotify,
        kind,
        id: v["id"].as_str()?.to_string(),
        title: v["name"].as_str()?.to_string(),
        subtitle,
        art_url,
    })
}

fn track_item(v: &Value, art: Option<String>) -> Option<Item> {
    let art = art.unwrap_or_else(|| pick_art(&v["album"]["images"]));
    item(Kind::Track, v, artist_names(&v["artists"]), art)
}

fn items(page: &Value) -> &[Value] {
    page["items"].as_array().map(Vec::as_slice).unwrap_or(&[])
}

pub fn parse_search(json: &Value) -> Vec<Item> {
    let tracks = items(&json["tracks"]).iter().filter_map(|t| track_item(t, None));
    let artists = items(&json["artists"]).iter().filter_map(parse_artist);
    tracks.chain(artists).collect()
}

pub fn parse_artist(a: &Value) -> Option<Item> {
    item(Kind::Artist, a, String::new(), pick_art(&a["images"]))
}

pub fn parse_artist_albums(json: &Value) -> Vec<Item> {
    items(json)
        .iter()
        .filter_map(|a| item(Kind::Album, a, artist_names(&a["artists"]), pick_art(&a["images"])))
        .collect()
}

/// Accepts the album object (with its `tracks` page) or a bare tracks page.
pub fn parse_album_tracks(json: &Value, album_art: &str) -> Vec<Item> {
    let page = if json["tracks"].is_object() { &json["tracks"] } else { json };
    items(page).iter().filter_map(|t| track_item(t, Some(album_art.to_string()))).collect()
}

pub fn parse_devices(json: &Value) -> Vec<Device> {
    json["devices"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|d| {
                    Some(Device {
                        id: d["id"].as_str()?.to_string(),
                        name: d["name"].as_str().unwrap_or("").to_string(),
                        kind: d["type"].as_str().unwrap_or("").to_string(),
                        active: d["is_active"].as_bool().unwrap_or(false),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// "2026-11-08T15:04:05.123Z" to unix milliseconds.
fn played_at_ms(s: &str) -> Option<i64> {
    let secs = crate::tasks::iso_to_unix(s)?;
    let frac = s
        .split_once('.')
        .map(|(_, rest)| rest.chars().take_while(char::is_ascii_digit).take(3).collect::<String>());
    let ms = frac.map_or(0, |f| format!("{f:0<3}").parse::<i64>().unwrap_or(0));
    Some(secs * 1000 + ms)
}

pub fn parse_recent(json: &Value) -> Vec<HistoryEntry> {
    items(json)
        .iter()
        .filter_map(|e| {
            Some(HistoryEntry { item: track_item(&e["track"], None)?, at_ms: played_at_ms(e["played_at"].as_str()?)? })
        })
        .collect()
}

// ── Transport ─────────────────────────────────────────────────────────────────

fn http() -> Result<reqwest::Client, ApiError> {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(Duration::from_secs(8))
        .build()
        .map_err(|e| ApiError::Other(e.to_string()))
}

fn from_auth(e: AuthError) -> ApiError {
    match e {
        AuthError::LoginNeeded => ApiError::LoginNeeded,
        AuthError::Network(s) => ApiError::Other(s),
    }
}

/// The live token source: `force` drops the cached token first, so a 401 gets a real refresh.
async fn live_token(force: bool) -> Result<String, AuthError> {
    if force {
        spotify_auth::invalidate_cached_token();
    }
    spotify_auth::access_token().await
}

/// One authorized request. 401: refresh once and retry; a second 401 is `LoginNeeded`. 429 reads
/// `Retry-After`. 404 about a device is `NoDevice`. A 2xx without a body (204) gives `Value::Null`.
async fn call<T, Fut>(
    base: &str,
    tokens: &T,
    method: reqwest::Method,
    path: &str,
    body: Option<Value>,
) -> Result<Value, ApiError>
where
    T: Fn(bool) -> Fut,
    Fut: Future<Output = Result<String, AuthError>>,
{
    let client = http()?;
    let url = format!("{base}{path}");
    let mut force = false;
    loop {
        let token = tokens(force).await.map_err(from_auth)?;
        let mut req = client.request(method.clone(), &url).bearer_auth(token);
        if let Some(b) = &body {
            req = req.json(b);
        } else if method == reqwest::Method::PUT {
            req = req.header("Content-Length", "0");
        }
        let response = req.send().await.map_err(|e| ApiError::Other(e.to_string()))?;
        let status = response.status().as_u16();
        let retry_after = response
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.trim().parse::<u64>().ok());
        let bytes = crate::net::read_capped(response, MAX_ANSWER).await.map_err(ApiError::Other)?;
        match status {
            401 if !force => force = true,
            401 => return Err(ApiError::LoginNeeded),
            429 => return Err(ApiError::RateLimited { retry_after_s: retry_after.unwrap_or(1) }),
            200..=299 => return Ok(serde_json::from_slice(&bytes).unwrap_or(Value::Null)),
            404 if is_no_device(&bytes) => return Err(ApiError::NoDevice),
            s => return Err(ApiError::Other(format!("Spotify: HTTP {s}"))),
        }
    }
}

fn is_no_device(body: &[u8]) -> bool {
    let text = String::from_utf8_lossy(body).to_lowercase();
    text.contains("no_active_device") || text.contains("device not found")
}

// ── Search cache ──────────────────────────────────────────────────────────────

/// The last 20 searches by (query, offset), good for 60 s. Newest last.
#[derive(Default)]
struct SearchCache {
    entries: Vec<(String, u32, Instant, Vec<Item>)>,
}

impl SearchCache {
    fn get(&self, q: &str, offset: u32, now: Instant) -> Option<Vec<Item>> {
        self.entries
            .iter()
            .find(|(k, o, at, _)| k == q && *o == offset && now.saturating_duration_since(*at) < CACHE_TTL)
            .map(|e| e.3.clone())
    }

    fn put(&mut self, q: String, offset: u32, items: Vec<Item>, now: Instant) {
        self.entries.retain(|(k, o, _, _)| !(k == &q && *o == offset));
        self.entries.push((q, offset, now, items));
        if self.entries.len() > CACHE_ENTRIES {
            self.entries.remove(0);
        }
    }
}

static SEARCHES: Mutex<SearchCache> = Mutex::new(SearchCache { entries: Vec::new() });

// ── Endpoints ─────────────────────────────────────────────────────────────────

async fn search_at<T, Fut>(base: &str, tokens: &T, q: &str, offset: u32) -> Result<Vec<Item>, ApiError>
where
    T: Fn(bool) -> Fut,
    Fut: Future<Output = Result<String, AuthError>>,
{
    if let Some(hit) = SEARCHES.lock().ok().and_then(|c| c.get(q, offset, Instant::now())) {
        return Ok(hit);
    }
    let path = format!("/search?q={}&type=track%2Cartist&limit={SEARCH_LIMIT}&offset={offset}", pct_encode(q));
    let found = parse_search(&call(base, tokens, reqwest::Method::GET, &path, None).await?);
    if let Ok(mut c) = SEARCHES.lock() {
        c.put(q.to_string(), offset, found.clone(), Instant::now());
    }
    Ok(found)
}

async fn devices_at<T, Fut>(base: &str, tokens: &T) -> Result<Vec<Device>, ApiError>
where
    T: Fn(bool) -> Fut,
    Fut: Future<Output = Result<String, AuthError>>,
{
    Ok(parse_devices(&call(base, tokens, reqwest::Method::GET, "/me/player/devices", None).await?))
}

async fn play_at<T, Fut>(base: &str, tokens: &T, device_id: &str, target: &PlayTarget) -> Result<(), ApiError>
where
    T: Fn(bool) -> Fut,
    Fut: Future<Output = Result<String, AuthError>>,
{
    let body = match target {
        PlayTarget::Track(u) => json!({ "uris": [u] }),
        PlayTarget::Context(u) => json!({ "context_uri": u }),
    };
    let path = format!("/me/player/play?device_id={}", pct_encode(device_id));
    call(base, tokens, reqwest::Method::PUT, &path, Some(body)).await.map(drop)
}

async fn pause_at<T, Fut>(base: &str, tokens: &T) -> Result<(), ApiError>
where
    T: Fn(bool) -> Fut,
    Fut: Future<Output = Result<String, AuthError>>,
{
    call(base, tokens, reqwest::Method::PUT, "/me/player/pause", None).await.map(drop)
}

pub async fn search(q: &str, offset: u32) -> Result<Vec<Item>, ApiError> {
    search_at(API_BASE, &live_token, q, offset).await
}

/// Top tracks no longer exist for dev-mode apps: the artist's tracks come from a search on `artist:"<name>"`.
pub async fn artist_tracks(name: &str) -> Result<Vec<Item>, ApiError> {
    let q = format!("artist:\"{}\"", name.replace('"', ""));
    Ok(search(&q, 0).await?.into_iter().filter(|i| i.kind == Kind::Track).collect())
}

// No command needs the artist's own record yet: player_artist gets the name from the clicked item.
#[allow(dead_code)]
pub async fn artist(id: &str) -> Result<Item, ApiError> {
    let path = format!("/artists/{}", pct_encode(id));
    let json = call(API_BASE, &live_token, reqwest::Method::GET, &path, None).await?;
    parse_artist(&json).ok_or_else(|| ApiError::Other("Spotify: unexpected artist answer".into()))
}

pub async fn artist_albums(id: &str) -> Result<Vec<Item>, ApiError> {
    let path = format!("/artists/{}/albums?include_groups=album%2Csingle&limit=10", pct_encode(id));
    Ok(parse_artist_albums(&call(API_BASE, &live_token, reqwest::Method::GET, &path, None).await?))
}

pub async fn album_tracks(id: &str) -> Result<Vec<Item>, ApiError> {
    let path = format!("/albums/{}", pct_encode(id));
    let json = call(API_BASE, &live_token, reqwest::Method::GET, &path, None).await?;
    Ok(parse_album_tracks(&json, &pick_art(&json["images"])))
}

pub async fn devices() -> Result<Vec<Device>, ApiError> {
    devices_at(API_BASE, &live_token).await
}

pub async fn recently_played() -> Result<Vec<HistoryEntry>, ApiError> {
    let json = call(API_BASE, &live_token, reqwest::Method::GET, "/me/player/recently-played?limit=20", None).await?;
    Ok(parse_recent(&json))
}

pub async fn play(device_id: &str, target: &PlayTarget) -> Result<(), ApiError> {
    play_at(API_BASE, &live_token, device_id, target).await
}

pub async fn pause() -> Result<(), ApiError> {
    pause_at(API_BASE, &live_token).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    const SEARCH: &str = include_str!("fixtures/search.json");
    const ARTIST: &str = include_str!("fixtures/artist.json");
    const ARTIST_ALBUMS: &str = include_str!("fixtures/artist_albums.json");
    const ALBUM: &str = include_str!("fixtures/album.json");
    const DEVICES: &str = include_str!("fixtures/devices.json");
    const RECENT: &str = include_str!("fixtures/recent.json");

    fn json(s: &str) -> Value {
        serde_json::from_str(s).unwrap()
    }

    #[test]
    fn parse_search_returns_tracks_and_artists() {
        let items = parse_search(&json(SEARCH));
        assert_eq!(items.len(), 4);
        let t = &items[0];
        assert_eq!((t.source, t.kind), (Source::Spotify, Kind::Track));
        assert_eq!(t.id, "4Dvkj6JhhA12EX05fT7y2e");
        assert_eq!(t.title, "Karma Police");
        assert_eq!(t.subtitle, "Radiohead");
        // Smallest image that is at least 64 px wide.
        assert_eq!(t.art_url, "https://i.scdn.co/image/ab67616d00004851c8b4-64");
        assert_eq!(items[1].subtitle, "Radiohead, A. Guest");
        let a = &items[2];
        assert_eq!((a.kind, a.title.as_str()), (Kind::Artist, "Radiohead"));
        assert_eq!(a.id, "4Z8W4fKeB5YxbusRsdQVPb");
        assert_eq!(a.art_url, "https://i.scdn.co/image/ab6761610000f1-160");
    }

    #[test]
    fn parse_search_handles_missing_images() {
        let items = parse_search(&json(SEARCH));
        assert_eq!(items[1].art_url, "");
        assert_eq!(items[3].art_url, "");
        // No tracks / no artists keys at all.
        assert!(parse_search(&json("{}")).is_empty());
        // Only tiny images: the largest one.
        let tiny = json(r#"{"artists":{"items":[{"id":"x","name":"X","images":[{"url":"a","width":32},{"url":"b","width":48}]}]}}"#);
        assert_eq!(parse_search(&tiny)[0].art_url, "b");
        // Images without a width: still the first one rather than nothing.
        let nowidth = json(r#"{"artists":{"items":[{"id":"x","name":"X","images":[{"url":"a"}]}]}}"#);
        assert_eq!(parse_search(&nowidth)[0].art_url, "a");
    }

    #[test]
    fn parse_artist_ok() {
        let a = parse_artist(&json(ARTIST)).unwrap();
        assert_eq!((a.kind, a.id.as_str(), a.title.as_str()), (Kind::Artist, "4Z8W4fKeB5YxbusRsdQVPb", "Radiohead"));
        assert_eq!(a.art_url, "https://i.scdn.co/image/ab6761610000e5eb-640");
    }

    #[test]
    fn parse_artist_albums_ok() {
        let albums = parse_artist_albums(&json(ARTIST_ALBUMS));
        assert_eq!(albums.len(), 3);
        assert_eq!(albums[0].kind, Kind::Album);
        assert_eq!(albums[0].title, "OK Computer");
        assert_eq!(albums[0].id, "6dVIqQ8qmQ5GBnJ9shOYGE");
        assert_eq!(albums[0].subtitle, "Radiohead");
        assert_eq!(albums[0].art_url, "https://i.scdn.co/image/ab67616d00004851c8b4-64");
        assert_eq!(albums[1].art_url, "https://i.scdn.co/image/ab67616d00001e02kida-300");
        assert_eq!(albums[2].art_url, "");
    }

    #[test]
    fn parse_album_tracks_inherit_album_art() {
        let tracks = parse_album_tracks(&json(ALBUM), "ART");
        assert_eq!(tracks.len(), 2);
        assert!(tracks.iter().all(|t| t.kind == Kind::Track && t.art_url == "ART"));
        assert_eq!(tracks[1].title, "Paranoid Android");
        assert_eq!(tracks[1].id, "2EqlS6tkEnglzr7tkKAAYD");
        assert_eq!(tracks[1].subtitle, "Radiohead");
        // A bare tracks page (no album wrapper) parses the same.
        let page = json(ALBUM)["tracks"].clone();
        assert_eq!(parse_album_tracks(&page, "ART").len(), 2);
    }

    #[test]
    fn parse_devices_ok() {
        let d = parse_devices(&json(DEVICES));
        assert_eq!(d.len(), 2);
        assert_eq!(d[0], Device { id: "74ASZWbe4lXaubB36ztrGX".into(), name: "alan-lap".into(), kind: "Computer".into(), active: true });
        assert!(!d[1].active);
        assert_eq!(d[1].kind, "Smartphone");
    }

    #[test]
    fn parse_recent_ok() {
        let r = parse_recent(&json(RECENT));
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].item.title, "Karma Police");
        assert_eq!(r[0].item.kind, Kind::Track);
        assert_eq!(r[0].item.art_url, "https://i.scdn.co/image/ab67616d00004851c8b4-64");
        // 2026-11-08T15:04:05.123Z
        assert_eq!(r[0].at_ms, 1_794_150_245_123);
        assert_eq!(r[1].at_ms, 1_794_146_400_000);
        assert_eq!(r[1].item.art_url, "");
    }

    #[test]
    fn uris_follow_the_kind() {
        assert_eq!(uri(Kind::Track, "abc"), "spotify:track:abc");
        assert_eq!(uri(Kind::Artist, "abc"), "spotify:artist:abc");
        assert_eq!(uri(Kind::Album, "abc"), "spotify:album:abc");
    }

    // ── Fake server ───────────────────────────────────────────────────────────

    struct Fake {
        base: String,
        hits: Arc<AtomicUsize>,
        seen: Arc<Mutex<Vec<String>>>,
    }

    /// Answers the given (status, extra headers, body) one per connection, in order, then stops listening.
    fn fake(responses: Vec<(&'static str, &'static str, String)>) -> Fake {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let hits = Arc::new(AtomicUsize::new(0));
        let seen = Arc::new(Mutex::new(Vec::new()));
        let (h, s) = (hits.clone(), seen.clone());
        std::thread::spawn(move || {
            for (status, headers, body) in responses {
                let Ok((mut conn, _)) = listener.accept() else { return };
                let mut buf = [0u8; 8192];
                let n = conn.read(&mut buf).unwrap_or(0);
                s.lock().unwrap().push(String::from_utf8_lossy(&buf[..n]).into_owned());
                h.fetch_add(1, Ordering::SeqCst);
                let head = format!("HTTP/1.1 {status}\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n", body.len());
                let _ = conn.write_all(head.as_bytes());
                let _ = conn.write_all(body.as_bytes());
            }
        });
        Fake { base, hits, seen }
    }

    /// A token provider that never touches the keyring; counts how many times a refresh was forced.
    fn tokens(forced: Arc<AtomicUsize>) -> impl Fn(bool) -> std::future::Ready<Result<String, AuthError>> {
        move |force| {
            if force {
                forced.fetch_add(1, Ordering::SeqCst);
            }
            std::future::ready(Ok(if force { "fresh".to_string() } else { "stale".to_string() }))
        }
    }

    #[tokio::test]
    async fn unauthorized_after_refresh_reports_login_needed() {
        let srv = fake(vec![("401 Unauthorized", "", "{}".into()), ("401 Unauthorized", "", "{}".into())]);
        let forced = Arc::new(AtomicUsize::new(0));
        let r = devices_at(&srv.base, &tokens(forced.clone())).await;
        assert_eq!(r, Err(ApiError::LoginNeeded));
        assert_eq!(forced.load(Ordering::SeqCst), 1, "refresh exactly once");
        assert_eq!(srv.hits.load(Ordering::SeqCst), 2);
        let seen = srv.seen.lock().unwrap();
        assert!(seen[0].to_lowercase().contains("authorization: bearer stale"), "{}", seen[0]);
        assert!(seen[1].to_lowercase().contains("authorization: bearer fresh"), "{}", seen[1]);
    }

    #[tokio::test]
    async fn a_401_then_a_good_answer_succeeds_after_one_refresh() {
        let srv = fake(vec![("401 Unauthorized", "", "{}".into()), ("200 OK", "", DEVICES.into())]);
        let forced = Arc::new(AtomicUsize::new(0));
        let d = devices_at(&srv.base, &tokens(forced.clone())).await.unwrap();
        assert_eq!(d.len(), 2);
        assert_eq!(forced.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn rate_limited_reads_retry_after() {
        let srv = fake(vec![("429 Too Many Requests", "Retry-After: 7\r\n", "{}".into())]);
        let r = devices_at(&srv.base, &tokens(Arc::default())).await;
        assert_eq!(r, Err(ApiError::RateLimited { retry_after_s: 7 }));
        assert_eq!(srv.hits.load(Ordering::SeqCst), 1, "no retry on 429");
    }

    #[tokio::test]
    async fn login_needed_from_the_auth_layer_is_passed_on() {
        let srv = fake(vec![]);
        let tok = |_force: bool| std::future::ready(Err::<String, _>(AuthError::LoginNeeded));
        assert_eq!(devices_at(&srv.base, &tok).await, Err(ApiError::LoginNeeded));
        let tok = |_force: bool| std::future::ready(Err::<String, _>(AuthError::Network("down".into())));
        assert_eq!(devices_at(&srv.base, &tok).await, Err(ApiError::Other("down".into())));
    }

    #[tokio::test]
    async fn search_cache_hits_within_60s() {
        let srv = fake(vec![("200 OK", "", SEARCH.into())]);
        let tok = tokens(Arc::default());
        let first = search_at(&srv.base, &tok, "cache me please", 0).await.unwrap();
        // The fake only has one answer: a second network call would hang/fail.
        let second = search_at(&srv.base, &tok, "cache me please", 0).await.unwrap();
        assert_eq!(first, second);
        assert_eq!(first.len(), 4);
        assert_eq!(srv.hits.load(Ordering::SeqCst), 1);
        let req = srv.seen.lock().unwrap()[0].clone();
        assert!(req.starts_with("GET /search?q=cache%20me%20please&type=track%2Cartist&limit=10&offset=0 "), "{req}");
    }

    #[tokio::test]
    async fn search_cache_is_keyed_by_offset_and_expires() {
        let srv = fake(vec![("200 OK", "", SEARCH.into()), ("200 OK", "", SEARCH.into())]);
        let tok = tokens(Arc::default());
        search_at(&srv.base, &tok, "paged query", 0).await.unwrap();
        search_at(&srv.base, &tok, "paged query", 10).await.unwrap();
        assert_eq!(srv.hits.load(Ordering::SeqCst), 2);

        let mut cache = SearchCache::default();
        let t0 = std::time::Instant::now();
        cache.put("q".into(), 0, vec![], t0);
        assert!(cache.get("q", 0, t0 + Duration::from_secs(59)).is_some());
        assert!(cache.get("q", 0, t0 + Duration::from_secs(61)).is_none());
        for i in 0..25 {
            cache.put(format!("q{i}"), 0, vec![], t0);
        }
        assert!(cache.get("q0", 0, t0).is_none(), "oldest evicted past 20");
        assert!(cache.get("q24", 0, t0).is_some());
        assert_eq!(cache.entries.len(), 20);
    }

    #[tokio::test]
    async fn play_sends_uris_for_a_track_and_context_uri_for_a_context() {
        let srv = fake(vec![("204 No Content", "", String::new()), ("204 No Content", "", String::new())]);
        let tok = tokens(Arc::default());
        play_at(&srv.base, &tok, "dev 1", &PlayTarget::Track("spotify:track:abc".into())).await.unwrap();
        play_at(&srv.base, &tok, "dev 1", &PlayTarget::Context("spotify:album:xyz".into())).await.unwrap();
        let seen = srv.seen.lock().unwrap();
        assert!(seen[0].starts_with("PUT /me/player/play?device_id=dev%201 "), "{}", seen[0]);
        assert!(seen[0].ends_with(r#"{"uris":["spotify:track:abc"]}"#), "{}", seen[0]);
        assert!(seen[1].ends_with(r#"{"context_uri":"spotify:album:xyz"}"#), "{}", seen[1]);
    }

    #[tokio::test]
    async fn play_without_an_active_device_reports_no_device() {
        let body = r#"{"error":{"status":404,"message":"Device not found","reason":"NO_ACTIVE_DEVICE"}}"#;
        let srv = fake(vec![("404 Not Found", "", body.into())]);
        let r = play_at(&srv.base, &tokens(Arc::default()), "d", &PlayTarget::Track("spotify:track:a".into())).await;
        assert_eq!(r, Err(ApiError::NoDevice));
    }

    #[tokio::test]
    async fn pause_is_a_put_and_other_errors_carry_the_status() {
        let srv = fake(vec![("204 No Content", "", String::new()), ("500 Internal Server Error", "", "{}".into())]);
        let tok = tokens(Arc::default());
        pause_at(&srv.base, &tok).await.unwrap();
        assert!(srv.seen.lock().unwrap()[0].starts_with("PUT /me/player/pause "));
        match pause_at(&srv.base, &tok).await {
            Err(ApiError::Other(m)) => assert!(m.contains("500"), "{m}"),
            other => panic!("{other:?}"),
        }
    }
}
