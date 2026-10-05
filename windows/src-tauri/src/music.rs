// The Music pill's backend: what is playing, on any MPRIS player (Spotify,
// Firefox, VLC, Brave, …), plus album art and synchronised lyrics.
//
// MPRIS is the D-Bus standard Linux media players speak, and the same source
// the "Dynamic Music Pill" GNOME extension reads. It is event driven:
// PropertiesChanged / Seeked / NameOwnerChanged wake one thread, which reads the
// players and emits a `music` event to the island only when something changed.
// While nothing happens it costs nothing; while a track plays, one read of the
// Position every ten seconds keeps the island's own clock honest (the island
// extrapolates between reads, so lyrics and the seek bar don't need polling).
//
// Network: everything here that leaves the machine — lyrics from lrclib.net, album
// art from an http(s) URL the player gave us — happens only after the user turns
// on "Fetch lyrics and album art online" in Settings (CLAUDE.md: network calls
// only to services the user configured). Art that is a local file is always read.
//
// Linux only: other platforms have no MPRIS, and get an empty pill.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

const MAX_ART_BYTES: usize = 6 << 20;
const MAX_LYRICS_CACHE: usize = 64;
/// How close lrclib's duration must be to the track's for a search hit to count.
const MAX_DURATION_GAP_S: f64 = 5.0;
const USER_AGENT: &str = concat!(
    "Coucou/",
    env!("CARGO_PKG_VERSION"),
    " (https://github.com/Louis-CFM/coucou)"
);

// ── What the island receives ──────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    /// Bus name after `org.mpris.MediaPlayer2.` (`spotify`, `firefox.instance123`…).
    pub player: String,
    /// The player's own name for itself ("Spotify"), or `player` when it has none.
    pub identity: String,
    /// `Playing`, `Paused` or `Stopped`.
    pub status: String,
    pub title: String,
    pub artists: Vec<String>,
    pub album: String,
    pub length_ms: i64,
    pub art_url: String,
    /// Where the track was `position_at` (epoch ms). The island extrapolates from here.
    pub position_ms: i64,
    pub position_at: i64,
    pub can_seek: bool,
    pub can_next: bool,
    pub can_previous: bool,
    /// The player's own volume, 0.0–1.0, or -1.0 when it has none (not every player
    /// implements `Volume`). The wheel on the pill can change it.
    pub volume: f64,
}

impl Track {
    /// True when nothing but the position differs — so a re-read that finds the
    /// same song at the expected spot is not worth an event.
    fn same_as(&self, other: &Track, now_ms: i64) -> bool {
        let mut a = self.clone();
        let mut b = other.clone();
        let expected = a.expected_position(now_ms);
        a.position_ms = 0;
        a.position_at = 0;
        b.position_ms = 0;
        b.position_at = 0;
        a == b && (other.position_ms - expected).abs() <= SEEK_TOLERANCE_MS
    }

    fn expected_position(&self, now_ms: i64) -> i64 {
        if self.status == "Playing" {
            self.position_ms + (now_ms - self.position_at).max(0)
        } else {
            self.position_ms
        }
    }
}

/// A re-read further than this from where the clock says we should be is a seek.
const SEEK_TOLERANCE_MS: i64 = 1500;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Line {
    /// When the line starts, in ms from the start of the track.
    pub t: i64,
    pub text: String,
}

// ── Pure helpers (no D-Bus, no network: these are the tested part) ────────────

/// `[mm:ss.xx]` stamps (one or several per line) → sorted lines. Empty lyrics
/// lines (instrumental gaps) are dropped, like the GNOME extension does.
pub fn parse_lrc(text: &str) -> Vec<Line> {
    let mut lines = Vec::new();
    for raw in text.lines() {
        let mut rest = raw.trim_start();
        let mut stamps = Vec::new();
        while let Some(stripped) = rest.strip_prefix('[') {
            let Some(end) = stripped.find(']') else { break };
            match parse_stamp(&stripped[..end]) {
                Some(ms) => stamps.push(ms),
                // `[ar:Artist]`, `[ti:Title]`… are tags, not timings.
                None => break,
            }
            rest = &stripped[end + 1..];
        }
        let words = rest.trim();
        if words.is_empty() {
            continue;
        }
        for t in stamps {
            lines.push(Line { t, text: words.to_string() });
        }
    }
    lines.sort_by_key(|l| l.t);
    lines
}

fn parse_stamp(s: &str) -> Option<i64> {
    let (min, rest) = s.split_once(':')?;
    let (sec, frac) = match rest.split_once(['.', ':']) {
        Some((sec, frac)) => (sec, frac),
        None => (rest, "0"),
    };
    let min: i64 = min.parse().ok()?;
    let sec: i64 = sec.parse().ok()?;
    if frac.is_empty() || !frac.bytes().all(|b| b.is_ascii_digit()) || sec >= 60 {
        return None;
    }
    // ".5" is 500 ms, ".05" 50 ms, ".055" 55 ms: the digits are a decimal fraction.
    let digits: String = frac.chars().take(3).collect();
    let ms = digits.parse::<i64>().ok()? * 10_i64.pow(3 - digits.len() as u32);
    Some(min * 60_000 + sec * 1000 + ms)
}

#[derive(Debug, Clone, Deserialize)]
pub struct LrcItem {
    #[serde(default)]
    pub id: Option<i64>,
    #[serde(default)]
    pub duration: Option<f64>,
    #[serde(default, rename = "syncedLyrics")]
    pub synced_lyrics: Option<String>,
}

/// Which script the lyrics should be in when lrclib has several versions of a song
/// (the original Japanese, a romanised one…): the GNOME extension's "lyrics
/// language preference", with the same three choices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LyricsPref {
    Any,
    /// The song's own script (Japanese, Korean, Chinese…).
    Original,
    /// Latin letters (romanised or translated).
    Latin,
}

impl LyricsPref {
    pub fn parse(s: &str) -> Self {
        match s {
            "original" => Self::Original,
            "latin" => Self::Latin,
            _ => Self::Any,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Script {
    Original,
    Latin,
    Unknown,
}

fn is_cjk(c: char) -> bool {
    matches!(c as u32, 0x3040..=0x9FFF | 0xAC00..=0xD7AF)
}

/// Same rule as the extension: look at the first fifteen lines; more than 15 % CJK
/// characters is "original", more than 40 % Latin letters is "latin".
fn script_of(lines: &[Line]) -> Script {
    let sample: String = lines.iter().take(15).map(|l| l.text.as_str()).collect::<Vec<_>>().join(" ");
    let (mut cjk, mut latin, mut total) = (0usize, 0usize, 0usize);
    for c in sample.chars().filter(|c| !c.is_whitespace()) {
        total += 1;
        if is_cjk(c) {
            cjk += 1;
        } else if c.is_ascii_alphabetic() {
            latin += 1;
        }
    }
    if total == 0 {
        Script::Unknown
    } else if cjk as f64 / total as f64 > 0.15 {
        Script::Original
    } else if latin as f64 / total as f64 > 0.4 {
        Script::Latin
    } else {
        Script::Unknown
    }
}

/// How well a candidate's script matches the preference: 2 = what was asked for,
/// 1 = something else, 0 = can't tell (or no preference).
fn script_score(lines: &[Line], pref: LyricsPref) -> i32 {
    let script = script_of(lines);
    match pref {
        LyricsPref::Any => 0,
        LyricsPref::Original => match script {
            Script::Original => 2,
            Script::Unknown => 0,
            Script::Latin => 1,
        },
        LyricsPref::Latin => match script {
            Script::Latin => 2,
            Script::Unknown => 0,
            Script::Original => 1,
        },
    }
}

/// Picks the best synced lyrics: the exact match lrclib returned if it has any,
/// otherwise the search hit whose duration is closest to the track's (and within
/// a few seconds — a live version of the same song is not the same lyrics). The
/// script preference outranks the duration, as in the extension.
pub fn choose_lyrics(
    exact: Option<LrcItem>,
    hits: Vec<LrcItem>,
    duration_s: f64,
    pref: LyricsPref,
) -> Option<LrcItem> {
    let mut pool: Vec<LrcItem> = Vec::new();
    if let Some(e) = exact {
        pool.push(e);
    }
    for h in hits {
        let gap = (h.duration.unwrap_or(0.0) - duration_s).abs();
        if gap < MAX_DURATION_GAP_S && !pool.iter().any(|p| p.id.is_some() && p.id == h.id) {
            pool.push(h);
        }
    }
    let score = |i: &LrcItem| -> f64 {
        let lines = parse_lrc(i.synced_lyrics.as_deref().unwrap_or(""));
        let gap = (i.duration.unwrap_or(0.0) - duration_s).abs();
        f64::from(script_score(&lines, pref)) * 1000.0 - gap
    };
    pool.into_iter()
        .filter(|i| i.synced_lyrics.as_deref().is_some_and(|s| !s.trim().is_empty()))
        .max_by(|a, b| score(a).partial_cmp(&score(b)).unwrap_or(std::cmp::Ordering::Equal))
}

/// `file:///home/a/My%20Music/cover.png` → `/home/a/My Music/cover.png`.
pub fn file_url_to_path(url: &str) -> Option<String> {
    let rest = url.strip_prefix("file://")?;
    let rest = rest.strip_prefix("localhost").unwrap_or(rest);
    if !rest.starts_with('/') {
        return None;
    }
    let bytes = rest.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// The MIME type of an image by its first bytes, or None when it isn't one we
/// show. Checked on content, never on the file name: any local process can claim
/// to be a player and point `artUrl` anywhere.
pub fn sniff_image(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        Some("image/png")
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if bytes.starts_with(b"GIF8") {
        Some("image/gif")
    } else if bytes.len() > 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

/// What the island is told about the player that should be on screen.
#[derive(Debug, Clone)]
pub struct Candidate {
    pub name: String,
    pub status: String,
    pub has_track: bool,
}

/// Which of several players to show: the one playing (the one we already showed,
/// if several are), else the one we showed last, else any with a track.
pub fn pick_player(players: &[Candidate], previous: Option<&str>) -> Option<usize> {
    let playing: Vec<usize> = (0..players.len()).filter(|&i| players[i].status == "Playing").collect();
    if let Some(prev) = previous {
        if let Some(&i) = playing.iter().find(|&&i| players[i].name == prev) {
            return Some(i);
        }
    }
    if let Some(&i) = playing.first() {
        return Some(i);
    }
    if let Some(prev) = previous {
        if let Some(i) = (0..players.len()).find(|&i| players[i].name == prev && players[i].has_track) {
            return Some(i);
        }
    }
    (0..players.len()).find(|&i| players[i].has_track)
}

// ── Settings gate ─────────────────────────────────────────────────────────────

fn online_allowed(app: &AppHandle) -> bool {
    // Tray → Pause means no network calls at all, like the integration pollers.
    if PAUSED.load(Ordering::Relaxed) {
        return false;
    }
    app.try_state::<crate::Shared>()
        .map(|s| s.settings.lock().unwrap().music_online)
        .unwrap_or(false)
}

fn lyrics_pref(app: &AppHandle) -> LyricsPref {
    app.try_state::<crate::Shared>()
        .map(|s| LyricsPref::parse(&s.settings.lock().unwrap().music_lyrics_language))
        .unwrap_or(LyricsPref::Any)
}

// ── Album art ─────────────────────────────────────────────────────────────────

/// The art as a `data:` URL, so the island can read its pixels (for the colour)
/// without the canvas being tainted. Local files always; http(s) only when the
/// user allowed online fetches.
#[tauri::command]
pub async fn music_art(app: AppHandle, url: String) -> Result<String, String> {
    let bytes = if let Some(path) = file_url_to_path(&url) {
        read_capped(&path).await?
    } else if url.starts_with("https://") || url.starts_with("http://") {
        if !online_allowed(&app) {
            return Err("online art is off".into());
        }
        fetch_capped(&url).await?
    } else {
        return Err("unsupported art URL".into());
    };
    let mime = sniff_image(&bytes).ok_or("not an image")?;
    Ok(format!("data:{mime};base64,{}", crate::claude::base64_for(&bytes)))
}

async fn read_capped(path: &str) -> Result<Vec<u8>, String> {
    let path = path.to_string();
    tokio::task::spawn_blocking(move || {
        let meta = std::fs::metadata(&path).map_err(|e| e.to_string())?;
        if !meta.is_file() || meta.len() as usize > MAX_ART_BYTES {
            return Err("art file is not a small regular file".to_string());
        }
        std::fs::read(&path).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

fn http() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(Duration::from_secs(8))
        .build()
        .map_err(|e| e.to_string())
}

async fn fetch_capped(url: &str) -> Result<Vec<u8>, String> {
    let response = http()?.get(url).send().await.map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("art: HTTP {}", response.status().as_u16()));
    }
    if response.content_length().is_some_and(|n| n as usize > MAX_ART_BYTES) {
        return Err("art is too large".into());
    }
    let bytes = response.bytes().await.map_err(|e| e.to_string())?;
    if bytes.len() > MAX_ART_BYTES {
        return Err("art is too large".into());
    }
    Ok(bytes.to_vec())
}

// ── Lyrics (lrclib.net) ───────────────────────────────────────────────────────

static LYRICS_CACHE: Mutex<Option<HashMap<String, Option<Vec<Line>>>>> = Mutex::new(None);

/// Synchronised lyrics for a track, or `None` when lrclib has none. Cached per
/// track for the life of the app so replaying a song costs no request.
#[tauri::command]
pub async fn music_lyrics(
    app: AppHandle,
    title: String,
    artist: String,
    album: String,
    duration_ms: i64,
) -> Result<Option<Vec<Line>>, String> {
    if !online_allowed(&app) {
        return Err("online lyrics are off".into());
    }
    if title.trim().is_empty() || duration_ms <= 0 {
        return Ok(None);
    }
    let pref = lyrics_pref(&app);
    // The preference is part of the key: changing it must not serve the old pick.
    let key = format!("{title}\u{1f}{artist}\u{1f}{duration_ms}\u{1f}{pref:?}");
    if let Some(hit) = LYRICS_CACHE.lock().unwrap().as_ref().and_then(|c| c.get(&key)) {
        return Ok(hit.clone());
    }

    let duration_s = duration_ms as f64 / 1000.0;
    let client = http()?;
    let exact: Option<LrcItem> = match client
        .get("https://lrclib.net/api/get")
        .query(&[
            ("track_name", title.as_str()),
            ("artist_name", artist.as_str()),
            ("album_name", album.as_str()),
            ("duration", &(duration_ms / 1000).to_string()),
        ])
        .send()
        .await
    {
        Ok(r) if r.status().is_success() => r.json().await.ok(),
        _ => None,
    };
    let hits: Vec<LrcItem> = match client
        .get("https://lrclib.net/api/search")
        .query(&[("q", format!("{title} {artist}").trim())])
        .send()
        .await
    {
        Ok(r) if r.status().is_success() => r.json().await.unwrap_or_default(),
        _ => Vec::new(),
    };

    let lines = choose_lyrics(exact, hits, duration_s, pref)
        .and_then(|i| i.synced_lyrics)
        .map(|s| parse_lrc(&s))
        .filter(|l| !l.is_empty());

    let mut cache = LYRICS_CACHE.lock().unwrap();
    let map = cache.get_or_insert_with(HashMap::new);
    if map.len() >= MAX_LYRICS_CACHE {
        map.clear();
    }
    map.insert(key, lines.clone());
    Ok(lines)
}

// ── State shared with the commands ────────────────────────────────────────────

static ENABLED: AtomicBool = AtomicBool::new(false);
static PAUSED: AtomicBool = AtomicBool::new(false);
static LAST: Mutex<Option<Track>> = Mutex::new(None);

/// Tray → Pause. While paused the players are left alone and nothing is fetched;
/// on resume the current track is read again straight away.
pub fn set_paused(_app: &AppHandle, paused: bool) {
    PAUSED.store(paused, Ordering::Relaxed);
    if paused {
        // Paused means nothing is listened to either.
        crate::bars::shutdown();
        return;
    }
    // A track may have changed while paused: forget what we showed, so the next
    // read is reported even if it looks the same as the stale one.
    *LAST.lock().unwrap() = None;
    #[cfg(target_os = "linux")]
    mpris::wake();
}

/// What the island last was told, for a page that has just loaded.
#[tauri::command]
pub fn music_state() -> Option<Track> {
    LAST.lock().unwrap().clone()
}

// ── Linux: MPRIS ──────────────────────────────────────────────────────────────

#[cfg(target_os = "linux")]
mod mpris {
    use super::*;
    use std::sync::mpsc::{self, RecvTimeoutError, Sender};
    use std::sync::OnceLock;
    use std::time::{SystemTime, UNIX_EPOCH};

    use tauri::Emitter;
    use zbus::blocking::{fdo::DBusProxy, Connection, MessageIterator, Proxy};
    use zbus::zvariant::OwnedValue;
    use zbus::MatchRule;

    use crate::island::WINDOW_LABEL;
    use crate::log;

    const PREFIX: &str = "org.mpris.MediaPlayer2.";
    const PATH: &str = "/org/mpris/MediaPlayer2";
    const PLAYER: &str = "org.mpris.MediaPlayer2.Player";
    const ROOT: &str = "org.mpris.MediaPlayer2";

    static STARTED: AtomicBool = AtomicBool::new(false);
    static WAKE: OnceLock<Sender<()>> = OnceLock::new();
    static ACTIVE: Mutex<Option<String>> = Mutex::new(None);

    pub fn wake() {
        if let Some(tx) = WAKE.get() {
            let _ = tx.send(());
        }
    }

    pub fn now_ms() -> i64 {
        SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
    }

    /// Switches the watcher on or off. The first time it goes on, it starts; after
    /// that the thread just ignores events while it is off.
    pub fn set_enabled(app: &AppHandle, on: bool) {
        ENABLED.store(on, Ordering::Relaxed);
        if !on {
            *LAST.lock().unwrap() = None;
            crate::bars::shutdown();
            return;
        }
        if STARTED.swap(true, Ordering::SeqCst) {
            if let Some(tx) = WAKE.get() {
                let _ = tx.send(());
            }
            return;
        }
        let app = app.clone();
        std::thread::Builder::new()
            .name("coucou-mpris".into())
            .spawn(move || run(app))
            .ok();
    }

    fn run(app: AppHandle) {
        let conn = match Connection::session() {
            Ok(c) => c,
            Err(err) => {
                log::line(format!("music: no session bus ({err}) — the Music pill stays empty"));
                STARTED.store(false, Ordering::SeqCst);
                return;
            }
        };
        let (tx, rx) = mpsc::channel::<()>();
        let _ = WAKE.set(tx.clone());

        // Three signals tell us everything that matters. Each blocks on its own
        // thread and only nudges the worker; the worker re-reads the players.
        let rules = [
            MatchRule::builder()
                .msg_type(zbus::message::Type::Signal)
                .interface("org.freedesktop.DBus.Properties")
                .and_then(|b| b.member("PropertiesChanged"))
                .and_then(|b| b.path(PATH))
                .map(|b| b.build()),
            MatchRule::builder()
                .msg_type(zbus::message::Type::Signal)
                .interface(PLAYER)
                .and_then(|b| b.member("Seeked"))
                .map(|b| b.build()),
            MatchRule::builder()
                .msg_type(zbus::message::Type::Signal)
                .sender("org.freedesktop.DBus")
                .and_then(|b| b.interface("org.freedesktop.DBus"))
                .and_then(|b| b.member("NameOwnerChanged"))
                .and_then(|b| b.arg0ns(ROOT))
                .map(|b| b.build()),
        ];
        for rule in rules.into_iter().flatten() {
            let conn = conn.clone();
            let tx = tx.clone();
            std::thread::spawn(move || {
                if let Ok(iter) = MessageIterator::for_match_rule(rule, &conn, Some(64)) {
                    for _ in iter {
                        if tx.send(()).is_err() {
                            break;
                        }
                    }
                }
            });
        }

        // Players that were already running when we started never send a signal:
        // read them once now, then follow the signals.
        let mut playing = false;
        if ENABLED.load(Ordering::Relaxed) && !PAUSED.load(Ordering::Relaxed) {
            let track = snapshot(&conn);
            playing = track.as_ref().is_some_and(|t| t.status == "Playing");
            publish(&app, track);
        }
        loop {
            // A bare timeout is the resync: while playing, re-read the position now
            // and then. Paused or idle, the thread sleeps until a signal.
            let wait = if playing { Duration::from_secs(10) } else { Duration::from_secs(300) };
            match rx.recv_timeout(wait) {
                Ok(()) => {
                    // A track change fires several signals at once: take them as one.
                    while rx.recv_timeout(Duration::from_millis(80)).is_ok() {}
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
            if !ENABLED.load(Ordering::Relaxed) || PAUSED.load(Ordering::Relaxed) {
                playing = false;
                continue;
            }
            let track = snapshot(&conn);
            playing = track.as_ref().is_some_and(|t| t.status == "Playing");
            publish(&app, track);
        }
    }

    fn publish(app: &AppHandle, track: Option<Track>) {
        let now = now_ms();
        {
            let mut last = LAST.lock().unwrap();
            let unchanged = match (&*last, &track) {
                (None, None) => true,
                (Some(a), Some(b)) => a.same_as(b, now),
                _ => false,
            };
            if unchanged {
                return;
            }
            *last = track.clone();
        }
        let _ = app.emit_to(WINDOW_LABEL, "music", track);
    }

    fn players(conn: &Connection) -> Vec<String> {
        DBusProxy::new(conn)
            .ok()
            .and_then(|p| p.list_names().ok())
            .map(|names| {
                names
                    .into_iter()
                    .map(|n| n.to_string())
                    .filter(|n| n.starts_with(PREFIX))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn proxy<'a>(conn: &'a Connection, name: &'a str, interface: &'a str) -> Option<Proxy<'a>> {
        Proxy::new(conn, name, PATH, interface).ok()
    }

    fn text(v: &OwnedValue) -> Option<String> {
        String::try_from(v.try_clone().ok()?).ok()
    }

    fn texts(v: &OwnedValue) -> Vec<String> {
        let Ok(copy) = v.try_clone() else { return Vec::new() };
        match Vec::<String>::try_from(copy) {
            Ok(list) => list,
            // A few players send the artist as a plain string, not a list.
            Err(_) => text(v).into_iter().collect(),
        }
    }

    fn integer(v: &OwnedValue) -> Option<i64> {
        let copy = v.try_clone().ok()?;
        i64::try_from(copy.try_clone().ok()?)
            .ok()
            .or_else(|| u64::try_from(copy.try_clone().ok()?).ok().map(|n| n as i64))
            .or_else(|| i32::try_from(copy.try_clone().ok()?).ok().map(i64::from))
            .or_else(|| u32::try_from(copy).ok().map(i64::from))
    }

    struct Read {
        name: String,
        status: String,
        track: Track,
    }

    fn read(conn: &Connection, name: &str) -> Option<Read> {
        let player = proxy(conn, name, PLAYER)?;
        let status: String = player.get_property("PlaybackStatus").ok()?;
        let meta: HashMap<String, OwnedValue> = player.get_property("Metadata").unwrap_or_default();
        let position_us: i64 = player.get_property("Position").unwrap_or(0);
        let flag = |p: &str| player.get_property::<bool>(p).unwrap_or(false);
        let identity = proxy(conn, name, ROOT)
            .and_then(|r| r.get_property::<String>("Identity").ok())
            .filter(|s| !s.is_empty());
        let short = name.strip_prefix(PREFIX).unwrap_or(name).to_string();

        let track = Track {
            identity: identity.unwrap_or_else(|| short.clone()),
            player: short,
            status: status.clone(),
            title: meta.get("xesam:title").and_then(text).unwrap_or_default(),
            artists: meta.get("xesam:artist").map(texts).unwrap_or_default(),
            album: meta.get("xesam:album").and_then(text).unwrap_or_default(),
            length_ms: meta.get("mpris:length").and_then(integer).unwrap_or(0) / 1000,
            art_url: meta.get("mpris:artUrl").and_then(text).unwrap_or_default(),
            position_ms: (position_us / 1000).max(0),
            position_at: now_ms(),
            can_seek: flag("CanSeek"),
            can_next: flag("CanGoNext"),
            can_previous: flag("CanGoPrevious"),
            volume: player.get_property::<f64>("Volume").unwrap_or(-1.0),
        };
        Some(Read { name: name.to_string(), status, track })
    }

    fn snapshot(conn: &Connection) -> Option<Track> {
        let mut reads: Vec<Read> = players(conn).iter().filter_map(|n| read(conn, n)).collect();
        let previous = ACTIVE.lock().unwrap().clone();
        let candidates: Vec<Candidate> = reads
            .iter()
            .map(|r| Candidate {
                name: r.name.clone(),
                status: r.status.clone(),
                has_track: !r.track.title.is_empty(),
            })
            .collect();
        let chosen = pick_player(&candidates, previous.as_deref())?;
        let read = reads.swap_remove(chosen);
        *ACTIVE.lock().unwrap() = Some(read.name);
        Some(read.track)
    }

    /// PlayPause / Next / Previous, or a seek to an absolute position.
    pub fn control(action: &str, value_ms: Option<i64>) -> Result<(), String> {
        let name = ACTIVE.lock().unwrap().clone().ok_or("no player")?;
        let conn = Connection::session().map_err(|e| e.to_string())?;
        let player = proxy(&conn, &name, PLAYER).ok_or("player is gone")?;
        let result = match action {
            "playpause" => player.call_method("PlayPause", &()).map(|_| ()),
            "next" => player.call_method("Next", &()).map(|_| ()),
            "previous" => player.call_method("Previous", &()).map(|_| ()),
            // Middle-click on the pill: bring the player's own window forward.
            "raise" => proxy(&conn, &name, ROOT).ok_or("player is gone")?.call_method("Raise", &()).map(|_| ()),
            // The player's own volume, sent as thousandths (0–1000) in `value_ms`.
            "volume" => {
                let permille = value_ms.ok_or("volume needs a level")?.clamp(0, 1000);
                player.set_property("Volume", permille as f64 / 1000.0).map_err(zbus::Error::from)
            }
            "seek" => {
                // `Seek` takes an offset, which works on every player; SetPosition
                // needs a track id some players don't give. Offset = target − now.
                let target_us = value_ms.ok_or("seek needs a position")?.max(0) * 1000;
                let now_us: i64 = player.get_property("Position").unwrap_or(0);
                player.call_method("Seek", &(target_us - now_us)).map(|_| ())
            }
            other => return Err(format!("unknown action {other}")),
        };
        result.map_err(|e| e.to_string())?;
        if let Some(tx) = WAKE.get() {
            let _ = tx.send(());
        }
        Ok(())
    }
}

/// Starts or stops watching the players, following the Music pill's switch.
#[cfg(target_os = "linux")]
pub fn sync_enabled(app: &AppHandle, on: bool) {
    mpris::set_enabled(app, on);
}

#[cfg(not(target_os = "linux"))]
pub fn sync_enabled(_app: &AppHandle, on: bool) {
    ENABLED.store(on, Ordering::Relaxed);
}

#[tauri::command]
pub async fn music_control(action: String, value_ms: Option<i64>) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        tokio::task::spawn_blocking(move || mpris::control(&action, value_ms))
            .await
            .map_err(|e| e.to_string())?
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (action, value_ms);
        Err("no media players on this platform".into())
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lrc_lines_are_timed_in_milliseconds_and_sorted() {
        let lines = parse_lrc("[00:10.50] second\n[00:01.05]first\n[01:02.003]third");
        assert_eq!(
            lines,
            vec![
                Line { t: 1_050, text: "first".into() },
                Line { t: 10_500, text: "second".into() },
                Line { t: 62_003, text: "third".into() },
            ]
        );
    }

    #[test]
    fn lrc_ignores_tags_and_empty_lines_and_expands_repeated_stamps() {
        let lrc = "[ar:Someone]\n[ti:Song]\n[00:00.00]\n[00:05.00][00:15.00]chorus\n\nplain text\n";
        let lines = parse_lrc(lrc);
        assert_eq!(
            lines,
            vec![
                Line { t: 5_000, text: "chorus".into() },
                Line { t: 15_000, text: "chorus".into() },
            ]
        );
    }

    #[test]
    fn lrc_fraction_digits_are_a_decimal_fraction() {
        assert_eq!(parse_stamp("00:01.5"), Some(1_500));
        assert_eq!(parse_stamp("00:01.05"), Some(1_050));
        assert_eq!(parse_stamp("00:01.055"), Some(1_055));
        assert_eq!(parse_stamp("ar:Someone"), None);
        assert_eq!(parse_stamp("00:75.00"), None);
    }

    fn item(id: i64, duration: f64, lyrics: Option<&str>) -> LrcItem {
        LrcItem { id: Some(id), duration: Some(duration), synced_lyrics: lyrics.map(String::from) }
    }

    #[test]
    fn the_closest_duration_wins_and_unsynced_or_far_hits_are_out() {
        let hits = vec![
            item(1, 200.0, Some("[00:01.00]far")),         // 20 s off: a different cut
            item(2, 181.0, Some("[00:01.00]close")),       // 1 s off
            item(3, 180.5, None),                           // closest, but no synced lyrics
            item(4, 183.0, Some("[00:01.00]less close")),  // 3 s off
        ];
        let best = choose_lyrics(None, hits, 180.0, LyricsPref::Any).unwrap();
        assert_eq!(best.id, Some(2));
    }

    #[test]
    fn an_exact_match_with_lyrics_beats_nothing_and_search_does_not_repeat_it() {
        let exact = item(9, 180.0, Some("[00:01.00]exact"));
        let hits = vec![item(9, 180.0, Some("[00:01.00]exact")), item(5, 184.0, Some("[00:01.00]other"))];
        assert_eq!(choose_lyrics(Some(exact), hits, 180.0, LyricsPref::Any).unwrap().id, Some(9));
        assert!(choose_lyrics(None, vec![item(1, 300.0, Some("[00:01.00]x"))], 180.0, LyricsPref::Any).is_none());
        assert!(choose_lyrics(None, Vec::new(), 180.0, LyricsPref::Any).is_none());
    }

    #[test]
    fn the_script_preference_outranks_the_duration() {
        // Same song, two versions: the original Japanese is closer in length, the
        // romanised one is 3 s off.
        let japanese = item(1, 180.0, Some("[00:01.00]夜に駆ける\n[00:05.00]沈むように溶けてゆくように"));
        let romaji = item(2, 183.0, Some("[00:01.00]Yoru ni kakeru\n[00:05.00]Shizumu you ni tokete yuku you ni"));
        let both = || vec![japanese.clone(), romaji.clone()];

        assert_eq!(choose_lyrics(None, both(), 180.0, LyricsPref::Any).unwrap().id, Some(1));
        assert_eq!(choose_lyrics(None, both(), 180.0, LyricsPref::Original).unwrap().id, Some(1));
        assert_eq!(choose_lyrics(None, both(), 180.0, LyricsPref::Latin).unwrap().id, Some(2));
        // With no version in the wanted script, the closest one still wins.
        assert_eq!(
            choose_lyrics(None, vec![japanese.clone()], 180.0, LyricsPref::Latin).unwrap().id,
            Some(1)
        );
    }

    #[test]
    fn the_preference_words_map_and_unknown_means_any() {
        assert_eq!(LyricsPref::parse("latin"), LyricsPref::Latin);
        assert_eq!(LyricsPref::parse("original"), LyricsPref::Original);
        assert_eq!(LyricsPref::parse("any"), LyricsPref::Any);
        assert_eq!(LyricsPref::parse("whatever"), LyricsPref::Any);
    }

    #[test]
    fn file_urls_become_paths() {
        assert_eq!(file_url_to_path("file:///tmp/a.png").as_deref(), Some("/tmp/a.png"));
        assert_eq!(
            file_url_to_path("file:///home/a/My%20Music/%C3%B1.png").as_deref(),
            Some("/home/a/My Music/ñ.png")
        );
        assert_eq!(file_url_to_path("file://localhost/tmp/a.png").as_deref(), Some("/tmp/a.png"));
        assert_eq!(file_url_to_path("https://x/y.png"), None);
        assert_eq!(file_url_to_path("file://relative/path"), None);
        // A truncated escape must not read past the end.
        assert_eq!(file_url_to_path("file:///tmp/a%2"), Some("/tmp/a%2".to_string()));
    }

    #[test]
    fn images_are_recognised_by_content_not_by_name() {
        assert_eq!(sniff_image(&[0x89, b'P', b'N', b'G', 0, 0]), Some("image/png"));
        assert_eq!(sniff_image(&[0xFF, 0xD8, 0xFF, 0xE0]), Some("image/jpeg"));
        assert_eq!(sniff_image(b"RIFF\0\0\0\0WEBPVP8 "), Some("image/webp"));
        assert_eq!(sniff_image(b"-----BEGIN OPENSSH PRIVATE KEY-----"), None);
        assert_eq!(sniff_image(b"root:x:0:0"), None);
    }

    fn cand(name: &str, status: &str, has_track: bool) -> Candidate {
        Candidate { name: name.into(), status: status.into(), has_track }
    }

    #[test]
    fn the_player_that_is_playing_is_shown_and_the_current_one_is_kept() {
        let players = [cand("a", "Paused", true), cand("b", "Playing", true), cand("c", "Playing", true)];
        assert_eq!(pick_player(&players, None), Some(1));
        // Two are playing: don't flip away from the one already on screen.
        assert_eq!(pick_player(&players, Some("c")), Some(2));
        // The one on screen paused and another plays: follow the music.
        assert_eq!(pick_player(&[cand("a", "Paused", true), cand("b", "Playing", true)], Some("a")), Some(1));
        // Nothing plays: stay on the last one shown, else any with a track.
        let idle = [cand("a", "Paused", true), cand("b", "Paused", true)];
        assert_eq!(pick_player(&idle, Some("b")), Some(1));
        assert_eq!(pick_player(&idle, None), Some(0));
        assert_eq!(pick_player(&[cand("a", "Stopped", false)], None), None);
        assert_eq!(pick_player(&[], Some("a")), None);
    }

    fn track(status: &str, pos: i64, at: i64) -> Track {
        Track {
            player: "p".into(), identity: "P".into(), status: status.into(), title: "T".into(),
            artists: vec!["A".into()], album: "B".into(), length_ms: 200_000, art_url: String::new(),
            position_ms: pos, position_at: at, can_seek: true, can_next: true, can_previous: true, volume: 0.5,
        }
    }

    #[test]
    fn a_reread_at_the_expected_spot_is_not_news_but_a_seek_or_a_new_title_is() {
        let before = track("Playing", 10_000, 1_000_000);
        // 5 s later the clock says 15 s: finding it at 15.4 s is just drift.
        assert!(before.same_as(&track("Playing", 15_400, 1_005_000), 1_005_000));
        // Found at 60 s: somebody scrubbed.
        assert!(!before.same_as(&track("Playing", 60_000, 1_005_000), 1_005_000));
        // Paused: the position must not move.
        let paused = track("Paused", 10_000, 1_000_000);
        assert!(paused.same_as(&track("Paused", 10_000, 1_090_000), 1_090_000));
        assert!(!paused.same_as(&track("Paused", 30_000, 1_090_000), 1_090_000));
        // Anything but the position changing is an event.
        let mut other = track("Playing", 15_000, 1_005_000);
        other.title = "Another".into();
        assert!(!before.same_as(&other, 1_005_000));
        assert!(!before.same_as(&track("Paused", 15_000, 1_005_000), 1_005_000));
    }
}
