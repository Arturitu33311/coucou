// What the phone is playing, shown on this computer as an MPRIS player called `coucou.phone`.
//
// The phone tells us (audiolink `now` and `art` lines) its title, artist, album, state, position and cover. We publish
// that as a normal MPRIS player on the session bus, with `mpris:artUrl` pointing at a file we wrote, so the island
// (`music.rs`), the interactive wallpaper (`server.py`, via playerctl) and any other MPRIS client see the phone's music
// with its cover and need no change of their own. KDE Connect's mirror of the same music is hidden when this player
// exists (it has no cover and would show the track twice).
//
// The player is on the bus only while the phone has a title; the controls (play/pause, next, previous, seek) are
// forwarded to KDE Connect's mirror, the one thing that can reach the phone's player. It is not a sound source on this
// computer: `audiolink_players::is_mirror` keeps it out of every "is the computer playing?" decision.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Instant;

use zbus::blocking::{connection, Connection};
use zbus::zvariant::{ObjectPath, OwnedValue, Value};

use crate::audiolink_wire::PhoneMsg;

pub const BUS_NAME: &str = "org.mpris.MediaPlayer2.coucou.phone";
const PATH: &str = "/org/mpris/MediaPlayer2";

#[derive(Clone, Default)]
struct Now {
    title: String,
    artist: String,
    album: String,
    status: String,
    pos_ms: i64,
    len_ms: i64,
    at: Option<Instant>,
    /// `file://` URL of the cover, empty when there is none.
    art_url: String,
}

static NOW: Mutex<Option<Now>> = Mutex::new(None);
static BUS: Mutex<Option<Connection>> = Mutex::new(None);
/// `file://` URL of the current cover; empty when there is none.
static ART_URL: Mutex<String> = Mutex::new(String::new());

// ── The other way: what this computer plays, for the phone ─────────────────────────────────────────────────────────────

/// The phone said (`caps`) that it reads `PcNow` and `PcArt` frames.
static PHONE_READS: AtomicBool = AtomicBool::new(false);
/// The last cover frame and track frame, kept to hand to a phone that announces itself after they were made.
static LAST: Mutex<(Vec<u8>, Vec<u8>)> = Mutex::new((Vec::new(), Vec::new()));
/// Which cover `LAST` holds. Its lock also serialises the work (shrinking a cover takes a moment).
static SENT: Mutex<String> = Mutex::new(String::new());

/// The phone announced (or lost) the ability to show this computer's music.
pub fn phone_can_read(on: bool) {
    crate::log::line(format!("phonenow: the phone {} show this computer's music", if on { "can" } else { "cannot" }));
    PHONE_READS.store(on, Ordering::Relaxed);
    if on {
        let (art, now) = LAST.lock().unwrap().clone();
        for f in [art, now] {
            if !f.is_empty() {
                crate::audiolink::send_control(f);
            }
        }
    }
}

/// The cover as bytes small enough for a frame: a local file as it is when it is a JPEG that fits, else shrunk with
/// ImageMagick's `convert` if there is one; none otherwise (the phone then shows the title without a picture).
fn cover_for_phone(art_url: &str) -> Vec<u8> {
    use crate::audiolink_wire::MAX_PC_ART;
    let Some(path) = crate::music::file_url_to_path(art_url) else { return Vec::new() };
    let Ok(meta) = std::fs::metadata(&path) else { return Vec::new() };
    if !meta.is_file() || meta.len() > (6 << 20) {
        return Vec::new();
    }
    let Ok(bytes) = std::fs::read(&path) else { return Vec::new() };
    if crate::music::sniff_image(&bytes).is_none() {
        return Vec::new();
    }
    if bytes.len() <= MAX_PC_ART && bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return bytes;
    }
    for q in ["75", "50", "30"] {
        let out = std::process::Command::new("convert")
            .args([path.as_str(), "-resize", "320x320>", "-quality", q, "jpeg:-"])
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .output();
        if let Ok(o) = out {
            if o.status.success() && !o.stdout.is_empty() && o.stdout.len() <= MAX_PC_ART {
                return o.stdout;
            }
        }
    }
    Vec::new()
}

/// The track as JSON that is certain to fit a `PcNow` frame: control characters become spaces (escaped they would take
/// six bytes each) and the texts are cut shorter until the whole serialised text fits.
fn track_json(title: &str, artist: &str, album: &str, status: &str, pos: i64, len: i64) -> String {
    use crate::audiolink_wire::{clip, MAX_NOW_LINE};
    let flat = |s: &str| s.chars().map(|c| if c.is_control() { ' ' } else { c }).collect::<String>();
    let (title, artist, album) = (flat(title), flat(artist), flat(album));
    let mut cap = 200;
    loop {
        let j = serde_json::json!({
            "title": clip(&title, cap), "artist": clip(&artist, cap), "album": clip(&album, cap),
            "status": status, "pos": pos.max(0), "len": len.max(0),
        })
        .to_string();
        if j.len() <= MAX_NOW_LINE || cap == 0 {
            return j;
        }
        cap = cap * 3 / 4;
    }
}

/// What the island shows, told to the phone. The phone's own music (and its mirror) is not sent back to it.
pub fn share_pc_track(track: Option<&crate::music::Track>) {
    use crate::audiolink_wire::{encode, FrameKind};
    let own = track.filter(|t| !crate::audiolink_players::is_mirror(&format!("org.mpris.MediaPlayer2.{}", t.player)));
    let (json, art_url) = match own {
        Some(t) => {
            let now = crate::music::mpris::now_ms();
            let pos = if t.status == "Playing" { t.position_ms + (now - t.position_at).max(0) } else { t.position_ms };
            let status = if matches!(t.status.as_str(), "Playing" | "Paused") { t.status.as_str() } else { "Stopped" };
            (track_json(&t.title, &t.artists.join(", "), &t.album, status, pos, t.length_ms), t.art_url.clone())
        }
        None => (r#"{"title":"","artist":"","album":"","status":"Stopped","pos":0,"len":0}"#.to_string(), String::new()),
    };
    std::thread::spawn(move || {
        let mut sent = SENT.lock().unwrap();
        let art_changed = *sent != art_url;
        let mut last = LAST.lock().unwrap().clone();
        if art_changed {
            last.0 = encode(FrameKind::PcArt, &cover_for_phone(&art_url));
            *sent = art_url;
        }
        last.1 = encode(FrameKind::PcNow, json.as_bytes());
        if PHONE_READS.load(Ordering::Relaxed) {
            let a = art_changed && crate::audiolink::send_control(last.0.clone());
            let n = crate::audiolink::send_control(last.1.clone());
            crate::log::line(format!("phonenow: sent the computer's track to the phone (cover {a}, track {n}, {} B)", last.1.len()));
        }
        *LAST.lock().unwrap() = last;
    });
}

/// Where the covers are written: a folder the wallpaper's server is allowed to serve from.
fn art_dir() -> std::path::PathBuf {
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from).unwrap_or_else(std::env::temp_dir);
    home.join(".cache").join("coucou").join("phone-art")
}

/// Writes the cover to `<hash>.jpg` (the name is the cover's hash, so a client that caches by URL never sees stale art)
/// and removes the other covers. Returns its `file://` URL, or empty when there is no usable image.
fn store_art(hash: &str, image: &[u8]) -> String {
    let dir = art_dir();
    if image.is_empty() || crate::music::sniff_image(image).is_none() || std::fs::create_dir_all(&dir).is_err() {
        clear_art_files(&dir, None);
        return String::new();
    }
    let name = format!("{hash}.jpg");
    let path = dir.join(&name);
    if !path.exists() && std::fs::write(&path, image).is_err() {
        return String::new();
    }
    clear_art_files(&dir, Some(&name));
    format!("file://{}", path.display())
}

fn clear_art_files(dir: &std::path::Path, keep: Option<&str>) {
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            if keep.map_or(true, |k| e.file_name() != k) {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
}

/// What the phone said. Called for the current connection only.
pub fn apply(msg: &PhoneMsg) {
    match msg {
        PhoneMsg::Now { title, artist, album, status, pos_ms, len_ms } => {
            {
                let mut now = NOW.lock().unwrap();
                *now = Some(Now {
                    title: title.clone(),
                    artist: artist.clone(),
                    album: album.clone(),
                    status: status.clone(),
                    pos_ms: *pos_ms,
                    len_ms: *len_ms,
                    at: Some(Instant::now()),
                    art_url: String::new(),
                });
            }
            sync_bus();
        }
        PhoneMsg::Art { hash, image } => {
            let url = store_art(hash, image);
            // The cover comes before the track it belongs to, so it is kept on its own.
            *ART_URL.lock().unwrap() = url;
            sync_bus();
        }
        _ => {}
    }
}

/// The phone is gone: nothing is playing there as far as we can tell.
pub fn clear() {
    PHONE_READS.store(false, Ordering::Relaxed);
    *NOW.lock().unwrap() = None;
    ART_URL.lock().unwrap().clear();
    clear_art_files(&art_dir(), None);
    sync_bus();
}

/// Puts the player on the bus while there is a title and takes it off when there is none, then tells the island.
fn sync_bus() {
    let want = NOW.lock().unwrap().as_ref().is_some_and(|n| !n.title.is_empty());
    {
        let mut bus = BUS.lock().unwrap();
        if want && bus.is_none() {
            match connection::Builder::session()
                .and_then(|b| b.serve_at(PATH, Root))
                .and_then(|b| b.serve_at(PATH, Player))
                .and_then(|b| b.name(BUS_NAME))
                .and_then(|b| b.build())
            {
                Ok(c) => *bus = Some(c),
                Err(e) => crate::log::line(format!("phonenow: could not publish the phone's player: {e}")),
            }
        } else if !want {
            // Dropping the connection takes the name off the bus.
            *bus = None;
        }
    }
    crate::music::mpris::wake();
}

fn track() -> Option<Now> {
    let mut n = NOW.lock().unwrap().clone()?;
    n.art_url = ART_URL.lock().unwrap().clone();
    Some(n)
}

/// Sends a MPRIS call to the KDE Connect mirror of the phone's player, if there is one.
fn forward(method: &'static str, offset_us: Option<i64>) {
    std::thread::spawn(move || {
        let Ok(conn) = Connection::session() else { return };
        let Ok(proxy) = zbus::blocking::fdo::DBusProxy::new(&conn) else { return };
        let Ok(names) = proxy.list_names() else { return };
        for n in names.into_iter().map(|n| n.to_string()).filter(|n| n.starts_with("org.mpris.MediaPlayer2.") && crate::audiolink_players::is_kdeconnect(n)) {
            let Ok(p) = zbus::blocking::Proxy::new(&conn, n.as_str(), PATH, "org.mpris.MediaPlayer2.Player") else { continue };
            let _ = match offset_us {
                Some(o) => p.call_method(method, &(o,)).map(|_| ()),
                None => p.call_method(method, &()).map(|_| ()),
            };
        }
    });
}

struct Root;

#[zbus::interface(name = "org.mpris.MediaPlayer2")]
impl Root {
    fn raise(&self) {}
    fn quit(&self) {}
    #[zbus(property)]
    fn can_quit(&self) -> bool {
        false
    }
    #[zbus(property)]
    fn can_raise(&self) -> bool {
        false
    }
    #[zbus(property)]
    fn has_track_list(&self) -> bool {
        false
    }
    #[zbus(property)]
    fn identity(&self) -> String {
        "Phone".into()
    }
    #[zbus(property)]
    fn supported_uri_schemes(&self) -> Vec<String> {
        Vec::new()
    }
    #[zbus(property)]
    fn supported_mime_types(&self) -> Vec<String> {
        Vec::new()
    }
}

struct Player;

#[zbus::interface(name = "org.mpris.MediaPlayer2.Player")]
impl Player {
    fn play_pause(&self) {
        forward("PlayPause", None);
    }
    fn play(&self) {
        forward("Play", None);
    }
    fn pause(&self) {
        forward("Pause", None);
    }
    fn stop(&self) {
        forward("Stop", None);
    }
    fn next(&self) {
        forward("Next", None);
    }
    fn previous(&self) {
        forward("Previous", None);
    }
    fn seek(&self, offset: i64) {
        forward("Seek", Some(offset));
    }
    #[zbus(property)]
    fn playback_status(&self) -> String {
        track().map(|n| n.status).filter(|s| !s.is_empty()).unwrap_or_else(|| "Stopped".into())
    }
    #[zbus(property)]
    fn metadata(&self) -> HashMap<String, OwnedValue> {
        metadata_of(&track().unwrap_or_default())
    }
    #[zbus(property)]
    fn position(&self) -> i64 {
        track().map(|n| position_ms(&n) * 1000).unwrap_or(0)
    }
    #[zbus(property)]
    fn volume(&self) -> f64 {
        1.0
    }
    #[zbus(property)]
    fn can_control(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn can_play(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn can_pause(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn can_go_next(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn can_go_previous(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn can_seek(&self) -> bool {
        false
    }
}

/// Where the phone's track is now: the position it reported plus the time since, while it is playing.
fn position_ms(n: &Now) -> i64 {
    let since = n.at.map(|t| t.elapsed().as_millis() as i64).unwrap_or(0);
    let p = if n.status == "Playing" { n.pos_ms + since } else { n.pos_ms };
    if n.len_ms > 0 { p.min(n.len_ms) } else { p }
}

fn owned(v: Value<'_>) -> Option<OwnedValue> {
    OwnedValue::try_from(v).ok()
}

fn metadata_of(n: &Now) -> HashMap<String, OwnedValue> {
    let mut m = HashMap::new();
    if let Some(id) = ObjectPath::try_from("/org/coucou/phone/track").ok().and_then(|p| owned(Value::from(p))) {
        m.insert("mpris:trackid".into(), id);
    }
    let mut put = |k: &str, v: Value<'_>| {
        if let Some(o) = owned(v) {
            m.insert(k.into(), o);
        }
    };
    put("xesam:title", Value::from(n.title.clone()));
    put("xesam:album", Value::from(n.album.clone()));
    put("xesam:artist", Value::from(if n.artist.is_empty() { Vec::<String>::new() } else { vec![n.artist.clone()] }));
    put("mpris:length", Value::from(n.len_ms * 1000));
    if !n.art_url.is_empty() {
        put("mpris:artUrl", Value::from(n.art_url.clone()));
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_position_runs_only_while_playing_and_stops_at_the_end() {
        let mut n = Now { status: "Playing".into(), pos_ms: 1_000, len_ms: 1_500, at: Some(Instant::now() - std::time::Duration::from_secs(2)), ..Now::default() };
        assert_eq!(position_ms(&n), 1_500);
        n.status = "Paused".into();
        assert_eq!(position_ms(&n), 1_000);
    }

    #[test]
    fn a_hostile_title_still_fits_a_frame() {
        let nasty = "\u{1}\"\\".repeat(500);
        let j = track_json(&nasty, &nasty, &nasty, "Playing", 5, 9);
        assert!(j.len() <= crate::audiolink_wire::MAX_NOW_LINE, "{}", j.len());
        assert!(serde_json::from_str::<serde_json::Value>(&j).is_ok());
    }

    #[test]
    fn metadata_carries_the_cover_only_when_there_is_one() {
        let mut n = Now { title: "Song".into(), artist: "Band".into(), ..Now::default() };
        assert!(!metadata_of(&n).contains_key("mpris:artUrl"));
        n.art_url = "file:///tmp/x.jpg".into();
        let m = metadata_of(&n);
        assert!(m.contains_key("mpris:artUrl") && m.contains_key("xesam:title") && m.contains_key("mpris:trackid"));
    }
}
