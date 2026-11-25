// Music handoff: you are listening on this computer, you pick up the phone and stay on it, and the song
// (or video) follows you.
//
// What happens, in order:
//  1. The presence clock says a phone has been the device in use for a while (`dwell`), this computer
//     has been left alone, and something is playing here through Bluetooth headphones.
//  2. This computer offers it to that phone: a card with the title and where you were. Nothing is
//     done to the music yet. ("auto" skips the tap, see below.)
//  3. The phone taps "continue": it can only answer the offer that is waiting, like it answers a
//     permission request. Then this computer pauses that player (never a toggle) and lets go of the
//     headphones (BlueZ `Disconnect`, then a short block so they do not come straight back here), and
//     the phone opens the content once it has the headphones.
//
// The rules that decide whether to offer are pure (`due`, `classify`, `check_accept`) and unit-tested.
// Bluetooth is only touched at the moment of an accepted offer (or an "auto" one), never to poll.
//
// Limits of this first version, on purpose:
//  - Spotify (through Sonora) opens the track on the phone and shows where you were; Spotify's
//    deep links cannot seek. Videos (YouTube) open at the same second.
//  - A headphone that cannot do multipoint (the Bose QC Earbuds II) has to be released here and
//    connected there: the phone does that by itself when it can, otherwise it opens Bluetooth settings.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tauri::{AppHandle, Manager};

use crate::music::Track;
use crate::presence::{self, Standing};

/// How long a card waits for its tap.
const OFFER_TTL: Duration = Duration::from_secs(120);
/// After an offer ends (accepted, expired), no new one for this long.
const COOLDOWN: Duration = Duration::from_secs(300);
/// The clock calls `tick` four times a second; the checks run at most this often.
const CHECK_EVERY: Duration = Duration::from_secs(3);
/// How long the headphones stay blocked here after being let go, so they connect to the phone instead.
const BT_HOLD: Duration = Duration::from_secs(20);
/// The phone must have been touched this recently (seconds) to count as "in use".
const PHONE_TOUCH_S: u64 = 45;
/// A video needs the phone to hold the island this many times longer: a film on the screen is watched
/// without touching anything, and a glance at the phone must not take it away.
const VIDEO_DWELL_FACTOR: u64 = 2;

// ── Settings ─────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Off,
    Ask,
    Auto,
}

impl Mode {
    pub fn parse(s: &str) -> Mode {
        match s {
            "ask" => Mode::Ask,
            "auto" => Mode::Auto,
            _ => Mode::Off,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Cfg {
    pub mode: Mode,
    pub dwell_s: u64,
}

fn config(app: &AppHandle) -> Cfg {
    app.try_state::<crate::Shared>()
        .map(|s| {
            let st = s.settings.lock().unwrap();
            Cfg { mode: Mode::parse(&st.music_handoff), dwell_s: st.handoff_dwell_s.clamp(15, 1800) }
        })
        .unwrap_or(Cfg { mode: Mode::Off, dwell_s: 90 })
}

// ── What is playing, and how the phone can open it ───────────────────────────

/// What the phone is asked to open. Only https addresses of the services below ever leave this
/// computer; the phone checks them again.
#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    /// A Spotify track or episode page.
    Track(String),
    /// A video page, already at the second you were at.
    Video(String),
    /// Spotify's search for "title artist": the song could not be pinned to an id.
    Search(String),
    /// Something we cannot open on a phone.
    Nothing,
}

impl Target {
    pub fn kind(&self) -> &'static str {
        match self {
            Target::Track(_) => "track",
            Target::Video(_) => "video",
            Target::Search(_) => "search",
            Target::Nothing => "none",
        }
    }

    pub fn uri(&self) -> &str {
        match self {
            Target::Track(u) | Target::Video(u) | Target::Search(u) => u,
            Target::Nothing => "",
        }
    }
}

fn id_ok(id: &str, len: std::ops::RangeInclusive<usize>, dash: bool) -> bool {
    len.contains(&id.len()) && id.chars().all(|c| c.is_ascii_alphanumeric() || (dash && (c == '-' || c == '_')))
}

/// A Spotify id: 22 letters and digits.
pub fn valid_spotify_id(id: &str) -> bool {
    id_ok(id, 22..=22, false)
}

fn percent_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// The value of one query parameter, without decoding it (ids are plain).
fn query_param<'a>(query: &'a str, key: &str) -> Option<&'a str> {
    query.split('&').find_map(|kv| kv.strip_prefix(key).and_then(|r| r.strip_prefix('=')))
}

/// Reads an address the player published (`xesam:url`). Only the pages we know how to open.
fn from_url(url: &str, pos_s: i64) -> Target {
    let Some(rest) = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://")) else { return Target::Nothing };
    let end = rest.find(|c| c == '/' || c == '?' || c == '#').unwrap_or(rest.len());
    let host = rest[..end].to_ascii_lowercase();
    let tail = &rest[end..];
    let (path, query) = {
        let tail = tail.split('#').next().unwrap_or("");
        match tail.split_once('?') {
            Some((p, q)) => (p, q),
            None => (tail, ""),
        }
    };
    let host = host.strip_prefix("www.").or_else(|| host.strip_prefix("m.")).unwrap_or(&host);
    let t = pos_s.max(0);
    match host {
        "youtube.com" | "music.youtube.com" => match query_param(query, "v") {
            Some(v) if id_ok(v, 11..=11, true) && path.starts_with("/watch") => {
                Target::Video(format!("https://www.youtube.com/watch?v={v}&t={t}s"))
            }
            _ => Target::Nothing,
        },
        "youtu.be" => match path.trim_start_matches('/').split('/').next() {
            Some(v) if id_ok(v, 11..=11, true) => Target::Video(format!("https://www.youtube.com/watch?v={v}&t={t}s")),
            _ => Target::Nothing,
        },
        "open.spotify.com" => {
            let mut segs = path.trim_start_matches('/').split('/');
            match (segs.next(), segs.next()) {
                (Some(kind @ ("track" | "episode")), Some(id)) if valid_spotify_id(id) => {
                    Target::Track(format!("https://open.spotify.com/{kind}/{id}"))
                }
                _ => Target::Nothing,
            }
        }
        _ => Target::Nothing,
    }
}

fn is_sonora(t: &Track) -> bool {
    t.player.eq_ignore_ascii_case("sonora") || t.identity.eq_ignore_ascii_case("sonora")
}

fn is_spotify(t: &Track) -> bool {
    t.player.to_ascii_lowercase().contains("spotify") || t.identity.to_ascii_lowercase().contains("spotify")
}

/// What to open on the phone for this track. `sonora_id` is the Spotify id found for it in Sonora's
/// own history, when it could be.
pub fn classify(track: &Track, sonora_id: Option<&str>) -> Target {
    let pos_s = track.position_ms / 1000;
    let by_url = from_url(track.url.trim(), pos_s);
    if by_url != Target::Nothing {
        return by_url;
    }
    if is_sonora(track) || is_spotify(track) {
        if let Some(id) = sonora_id.filter(|id| valid_spotify_id(id)) {
            return Target::Track(format!("https://open.spotify.com/track/{id}"));
        }
        let q = format!("{} {}", track.title, track.artists.first().map(String::as_str).unwrap_or("")).trim().to_string();
        if !q.is_empty() {
            return Target::Search(format!("https://open.spotify.com/search/{}", percent_encode(&q)));
        }
    }
    Target::Nothing
}

/// Sonora keeps what it played in a sqlite file; the newest Spotify row with this title (and first
/// artist) has the track's id. That file is another program's private schema, so every failure is
/// silent and the caller falls back to a search link. Read-only, through python's own sqlite module
/// (there is no sqlite client on this machine and no need to link one for this).
fn sonora_spotify_id(title: &str, artist: &str) -> Option<String> {
    const PY: &str = r#"
import sqlite3, sys, os
db = os.path.expanduser("~/.var/app/io.github.nolight132.sonora/data/sonora/state.sqlite")
title, artist = sys.argv[1], sys.argv[2]
c = sqlite3.connect("file:" + db + "?mode=ro", uri=True, timeout=1)
for tid, artists in c.execute("select track_id, artists from plays where provider='spotify' and name=? order by played_at desc limit 20", (title,)):
    if not artist or artist.lower() in (artists or "").lower():
        print(tid); break
"#;
    let out = std::process::Command::new("timeout")
        .args(["3", "python3", "-I", "-c", PY, title, artist])
        .output()
        .ok()?;
    let id = String::from_utf8_lossy(&out.stdout).trim().to_string();
    valid_spotify_id(&id).then_some(id)
}

// ── When to offer ────────────────────────────────────────────────────────────

/// Is the person on a phone, and have they been for long enough? Cheap: only what the clock knows.
pub fn due(cfg: &Cfg, st: &Standing, dwell_s: u64) -> bool {
    cfg.mode != Mode::Off
        && presence::is_phone(&st.winner)
        && st.winner_fresh
        && st.winner_screen_on
        && st.winner_idle_s <= PHONE_TOUCH_S
        && st.held_s >= dwell_s
        // This computer left alone for the whole stay (or, if that is long, at least a minute).
        && st.my_idle_s >= dwell_s.min(60)
}

// ── The offer ────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
struct Offer {
    id: String,
    /// The phone the card went to.
    device: String,
    created: Instant,
    accepted: bool,
    player: String,
    title: String,
    #[allow(dead_code)]
    target: Target,
}

/// Can this phone take this offer now? It must be the card that is waiting, for that phone, not
/// answered, not expired, and the person must still be on that phone.
fn check_accept(o: &Offer, id: &str, device: &str, now: Instant, winner: &str) -> Result<(), String> {
    if o.id != id {
        return Err("that offer is not the one waiting".into());
    }
    if o.device != device {
        return Err("that offer is for another phone".into());
    }
    if o.accepted {
        return Err("already taken".into());
    }
    if now.duration_since(o.created) > OFFER_TTL {
        return Err("the offer expired".into());
    }
    if winner != device {
        return Err("you are back on the computer".into());
    }
    Ok(())
}

struct State {
    offer: Option<Offer>,
    last_end: Option<Instant>,
    last_check: Option<Instant>,
    busy: bool,
    seq: u64,
}

static STATE: Mutex<State> = Mutex::new(State { offer: None, last_end: None, last_check: None, busy: false, seq: 0 });

fn state() -> std::sync::MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}

fn log(msg: impl AsRef<str>) {
    crate::log::line(format!("handoff: {}", msg.as_ref()));
}

/// Called by the presence clock. Does nothing unless the setting is on and a phone has held the
/// island for a while; the looks at the player and Bluetooth happen off the clock's thread.
pub fn tick(app: &AppHandle) {
    let cfg = config(app);
    if cfg.mode == Mode::Off {
        return;
    }
    {
        let mut s = state();
        if s.busy {
            return;
        }
        if let Some(o) = &s.offer {
            if !o.accepted && o.created.elapsed() > OFFER_TTL {
                log(format!("offer {} expired", o.id));
                s.offer = None;
                s.last_end = Some(Instant::now());
            }
            return;
        }
        if s.last_check.is_some_and(|t| t.elapsed() < CHECK_EVERY) || s.last_end.is_some_and(|t| t.elapsed() < COOLDOWN) {
            return;
        }
        s.last_check = Some(Instant::now());
    }
    let standing = presence::standing();
    if !due(&cfg, &standing, cfg.dwell_s) || !crate::remote::has_listeners() {
        return;
    }
    state().busy = true;
    tauri::async_runtime::spawn_blocking(move || {
        offer_now(&cfg, &standing);
        state().busy = false;
    });
}

/// Looks at what plays and where the headphones are, and if it is worth it, puts the card out.
fn offer_now(cfg: &Cfg, standing: &Standing) {
    let Some(track) = crate::music::read_now() else { return };
    if track.status != "Playing" {
        return;
    }
    let sonora_id = if is_sonora(&track) || is_spotify(&track) {
        sonora_spotify_id(&track.title, track.artists.first().map(String::as_str).unwrap_or(""))
    } else {
        None
    };
    let target = classify(&track, sonora_id.as_deref());
    if target == Target::Nothing {
        return;
    }
    // A video is watched, not touched: it needs the phone to have held the island longer.
    if matches!(target, Target::Video(_)) && !due(cfg, standing, cfg.dwell_s * VIDEO_DWELL_FACTOR) {
        return;
    }
    // Nothing to carry over if the sound is not in headphones: speakers stay where they are.
    let Some(headset) = bluetooth::connected_headset() else { return };

    let id = {
        let mut s = state();
        s.seq += 1;
        format!("h-{}-{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0), s.seq)
    };
    let offer = Offer {
        id: id.clone(),
        device: standing.winner.clone(),
        created: Instant::now(),
        accepted: false,
        player: track.player.clone(),
        title: track.title.clone(),
        target: target.clone(),
    };
    let line = offer_line(&id, &standing.winner, &target, &track, &headset.name, cfg.mode == Mode::Auto);
    state().offer = Some(offer.clone());
    crate::remote::publish_line(&line);
    log(format!("offered \"{}\" ({}) to {} via {}", track.title, target.kind(), standing.winner, headset.name));
    if cfg.mode == Mode::Auto {
        state().offer.as_mut().map(|o| o.accepted = true);
        run(offer);
    }
}

/// The card the phone shows. The same keys on both sides (wire-fixtures.json).
fn offer_line(id: &str, device: &str, target: &Target, track: &Track, headset: &str, auto: bool) -> Value {
    json!({
        "t": "handoff",
        "id": id,
        "device": device,
        "kind": target.kind(),
        "uri": target.uri(),
        "title": track.title,
        "artist": track.artists.first().cloned().unwrap_or_default(),
        "player": track.identity,
        "pos_ms": track.position_ms,
        "length_ms": track.length_ms,
        "headset": headset,
        "auto": auto,
        "ttl_s": OFFER_TTL.as_secs(),
    })
}

/// How it went, for the phone's card.
fn done_line(id: &str, device: &str, ok: bool, why: &str, headset: Option<&str>) -> Value {
    json!({ "t": "handoff_done", "id": id, "device": device, "ok": ok, "why": why, "headset": headset })
}

/// The phone tapped the card. Checked, then done on its own thread; the phone is told how it went.
pub fn accept(device: &str, id: &str) -> Result<(), String> {
    let standing = presence::standing();
    let offer = {
        let mut s = state();
        let o = s.offer.as_mut().ok_or_else(|| "no offer is waiting".to_string())?;
        check_accept(o, id, device, Instant::now(), &standing.winner)?;
        o.accepted = true;
        o.clone()
    };
    tauri::async_runtime::spawn_blocking(move || run(offer));
    Ok(())
}

/// Pauses the player, lets go of the headphones, tells the phone.
fn run(offer: Offer) {
    let result = execute(&offer);
    {
        let mut s = state();
        s.offer = None;
        s.last_end = Some(Instant::now());
    }
    let (ok, why, headset) = match &result {
        Ok(h) => (true, String::new(), h.clone()),
        Err(e) => (false, e.clone(), None),
    };
    log(format!("offer {}: {}", offer.id, if ok { "done".to_string() } else { format!("failed ({why})") }));
    crate::remote::publish_line(&done_line(&offer.id, &offer.device, ok, &why, headset.as_deref()));
}

fn execute(offer: &Offer) -> Result<Option<String>, String> {
    // The song may have changed while the card waited: then the card is about something else.
    let now = crate::music::read_now().ok_or("nothing is playing any more")?;
    if now.player != offer.player || now.title != offer.title {
        return Err("the song changed".into());
    }
    let Some(headset) = bluetooth::connected_headset() else { return Err("the headphones are not connected here any more".into()) };
    crate::music::set_playing(&offer.player, false)?;
    match bluetooth::release(&headset) {
        Ok(()) => Ok(Some(headset.name)),
        Err(e) => {
            // The music must not stay stopped for nothing.
            let _ = crate::music::set_playing(&offer.player, true);
            Err(format!("could not let go of the headphones ({e})"))
        }
    }
}

/// At start: a block this program put on the headphones and never lifted (it was closed in between)
/// must not outlive it.
pub fn recover() {
    bluetooth::recover();
}

// ── Bluetooth (BlueZ over the system bus) ────────────────────────────────────

#[derive(Clone, Debug)]
pub struct Headset {
    pub path: String,
    pub name: String,
}

#[cfg(target_os = "linux")]
mod bluetooth {
    use super::*;
    use zbus::blocking::{fdo::ObjectManagerProxy, Connection, Proxy};
    use zbus::zvariant::OwnedValue;

    const SINK: &str = "0000110b-0000-1000-8000-00805f9b34fb";
    const DEVICE: &str = "org.bluez.Device1";

    fn marker() -> std::path::PathBuf {
        crate::platform::local_dir().join("bt-hold.json")
    }

    fn device_proxy<'a>(conn: &'a Connection, path: &'a str) -> Result<Proxy<'a>, String> {
        Proxy::new(conn, "org.bluez", path, DEVICE).map_err(|e| e.to_string())
    }

    /// The Bluetooth audio device that is connected right now (headphones, speaker), if any.
    pub fn connected_headset() -> Option<Headset> {
        let conn = Connection::system().ok()?;
        let om = ObjectManagerProxy::builder(&conn).destination("org.bluez").ok()?.path("/").ok()?.build().ok()?;
        let objects = om.get_managed_objects().ok()?;
        objects.into_iter().find_map(|(path, ifaces)| {
            let props = ifaces.iter().find(|(name, _)| name.as_str() == DEVICE)?.1;
            let get = |k: &str| -> Option<OwnedValue> { props.get(k)?.try_clone().ok() };
            if !bool::try_from(get("Connected")?).ok()? {
                return None;
            }
            let uuids = Vec::<String>::try_from(get("UUIDs")?).ok()?;
            if !uuids.iter().any(|u| u.eq_ignore_ascii_case(SINK)) {
                return None;
            }
            let name = get("Alias").or_else(|| get("Name")).and_then(|v| String::try_from(v).ok()).unwrap_or_else(|| "headphones".into());
            Some(Headset { path: path.to_string(), name })
        })
    }

    /// Lets go of the headphones: disconnect, then block them for a few seconds so they do not come
    /// straight back here and the phone gets them. The block is always lifted: by a timer, and at the
    /// next start if this program dies before the timer does.
    pub fn release(h: &Headset) -> Result<(), String> {
        let conn = Connection::system().map_err(|e| e.to_string())?;
        device_proxy(&conn, &h.path)?.call_method("Disconnect", &()).map_err(|e| e.to_string())?;
        match device_proxy(&conn, &h.path).and_then(|p| p.set_property("Blocked", true).map_err(|e| e.to_string())) {
            Ok(()) => {
                let _ = std::fs::create_dir_all(crate::platform::local_dir());
                let _ = std::fs::write(marker(), json!({ "path": h.path }).to_string());
                let path = h.path.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(BT_HOLD);
                    unblock(&path);
                });
            }
            Err(e) => log(format!("not blocked ({e}); they were disconnected anyway")),
        }
        Ok(())
    }

    fn unblock(path: &str) {
        if let Ok(conn) = Connection::system() {
            match device_proxy(&conn, path).and_then(|p| p.set_property("Blocked", false).map_err(|e| e.to_string())) {
                Ok(()) => {
                    let _ = std::fs::remove_file(marker());
                }
                Err(e) => log(format!("could not lift the block on {path}: {e}")),
            }
        }
    }

    pub fn recover() {
        let Ok(text) = std::fs::read_to_string(marker()) else { return };
        if let Some(path) = serde_json::from_str::<Value>(&text).ok().and_then(|v| v["path"].as_str().map(str::to_string)) {
            // Only a BlueZ device path: whatever is in the file is not trusted.
            if path.starts_with("/org/bluez/hci") && path.chars().all(|c| c.is_ascii_alphanumeric() || c == '/' || c == '_') {
                log(format!("lifting a block left from last time on {path}"));
                unblock(&path);
                return;
            }
        }
        let _ = std::fs::remove_file(marker());
    }
}

#[cfg(not(target_os = "linux"))]
mod bluetooth {
    use super::*;
    pub fn connected_headset() -> Option<Headset> {
        None
    }
    pub fn release(_: &Headset) -> Result<(), String> {
        Err("not supported on this platform".into())
    }
    pub fn recover() {}
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn track(player: &str, title: &str, url: &str, pos_ms: i64) -> Track {
        Track {
            player: player.into(), identity: player.into(), status: "Playing".into(), title: title.into(),
            artists: vec!["Fuerza Regida".into()], album: "A".into(), length_ms: 176_000, art_url: String::new(),
            url: url.into(), position_ms: pos_ms, position_at: 0, can_seek: true, can_next: true, can_previous: true, volume: -1.0,
        }
    }

    fn standing(winner: &str, held: u64, my_idle: u64, phone_idle: u64) -> Standing {
        Standing {
            winner: winner.into(), held_s: held, my_idle_s: my_idle, winner_idle_s: phone_idle,
            winner_fresh: true, winner_screen_on: true,
        }
    }

    const ASK: Cfg = Cfg { mode: Mode::Ask, dwell_s: 90 };

    #[test]
    fn the_offer_waits_for_the_phone_to_hold_the_island_long_enough() {
        assert!(due(&ASK, &standing("s22", 90, 120, 5), 90));
        // Not long enough yet.
        assert!(!due(&ASK, &standing("s22", 89, 120, 5), 90));
        // The person is at the computer.
        assert!(!due(&ASK, &standing("laptop", 600, 0, 0), 90));
        // The computer was touched a moment ago: not "left alone".
        assert!(!due(&ASK, &standing("s22", 100, 10, 5), 90));
        // The phone is on, but nobody touches it: it is in a pocket or on a table.
        assert!(!due(&ASK, &standing("s22", 300, 300, 200), 90));
        // Off means off.
        assert!(!due(&Cfg { mode: Mode::Off, dwell_s: 90 }, &standing("s22", 900, 900, 1), 90));
        // A silent or dark phone is not in use.
        let mut dark = standing("s22", 300, 300, 5);
        dark.winner_screen_on = false;
        assert!(!due(&ASK, &dark, 90));
        let mut gone = standing("s22", 300, 300, 5);
        gone.winner_fresh = false;
        assert!(!due(&ASK, &gone, 90));
    }

    #[test]
    fn a_video_needs_the_phone_for_twice_as_long() {
        let st = standing("s22", 100, 200, 5);
        assert!(due(&ASK, &st, ASK.dwell_s));
        assert!(!due(&ASK, &st, ASK.dwell_s * VIDEO_DWELL_FACTOR));
    }

    #[test]
    fn youtube_pages_open_at_the_same_second_in_every_spelling() {
        let want = Target::Video("https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=151s".into());
        for url in [
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ&list=PL1&t=30s",
            "https://m.youtube.com/watch?v=dQw4w9WgXcQ",
            "https://music.youtube.com/watch?v=dQw4w9WgXcQ&si=abc",
            "https://youtu.be/dQw4w9WgXcQ?si=abc",
        ] {
            assert_eq!(classify(&track("firefox", "x", url, 151_900), None), want, "{url}");
        }
        // Not a video page, or not an id.
        for url in ["https://www.youtube.com/", "https://www.youtube.com/watch?v=short", "https://www.youtube.com/@canal", "https://youtube.com.evil.example/watch?v=dQw4w9WgXcQ"] {
            assert_eq!(classify(&track("firefox", "x", url, 0), None), Target::Nothing, "{url}");
        }
    }

    #[test]
    fn only_services_we_know_ever_leave_the_computer() {
        assert_eq!(classify(&track("firefox", "x", "https://example.com/video.mp4", 0), None), Target::Nothing);
        assert_eq!(classify(&track("firefox", "x", "file:///home/a/song.mp3", 0), None), Target::Nothing);
        assert_eq!(classify(&track("firefox", "x", "javascript:alert(1)", 0), None), Target::Nothing);
        assert_eq!(classify(&track("vlc", "x", "", 0), None), Target::Nothing);
        // Spotify's own page, as Spotify's player publishes it.
        assert_eq!(
            classify(&track("spotify", "x", "https://open.spotify.com/track/6OwjpJLqayrcTuzBvxoGMd", 0), None),
            Target::Track("https://open.spotify.com/track/6OwjpJLqayrcTuzBvxoGMd".into())
        );
        assert_eq!(classify(&track("spotify", "x", "https://open.spotify.com/track/../../x", 0), None).kind(), "search");
    }

    #[test]
    fn sonora_songs_open_by_id_or_by_search() {
        let t = track("sonora", "JESUS", "", 20_000);
        assert_eq!(
            classify(&t, Some("6OwjpJLqayrcTuzBvxoGMd")),
            Target::Track("https://open.spotify.com/track/6OwjpJLqayrcTuzBvxoGMd".into())
        );
        // No id (the history had no match) or a bad one: a search, never a broken link.
        for id in [None, Some("nope"), Some("6OwjpJLqayrcTuzBvxoGM/")] {
            assert_eq!(classify(&t, id), Target::Search("https://open.spotify.com/search/JESUS%20Fuerza%20Regida".into()), "{id:?}");
        }
    }

    #[test]
    fn a_card_can_only_be_taken_by_its_phone_once_while_it_is_fresh_and_you_are_still_there() {
        let now = Instant::now();
        let o = Offer {
            id: "h-1".into(), device: "s22".into(), created: now, accepted: false, player: "sonora".into(),
            title: "JESUS".into(), target: Target::Nothing,
        };
        assert!(check_accept(&o, "h-1", "s22", now, "s22").is_ok());
        assert!(check_accept(&o, "h-2", "s22", now, "s22").is_err(), "another id");
        assert!(check_accept(&o, "h-1", "s21", now, "s21").is_err(), "the other phone");
        assert!(check_accept(&o, "h-1", "s22", now + OFFER_TTL + Duration::from_secs(1), "s22").is_err(), "expired");
        assert!(check_accept(&o, "h-1", "s22", now, "laptop").is_err(), "back at the computer");
        let mut taken = o.clone();
        taken.accepted = true;
        assert!(check_accept(&taken, "h-1", "s22", now, "s22").is_err(), "twice");
    }

    #[test]
    fn the_settings_words_are_read_and_anything_else_is_off() {
        assert_eq!(Mode::parse("ask"), Mode::Ask);
        assert_eq!(Mode::parse("auto"), Mode::Auto);
        assert_eq!(Mode::parse("on"), Mode::Off);
        assert_eq!(Mode::parse(""), Mode::Off);
    }

    #[test]
    fn spotify_ids_are_22_letters_and_digits() {
        assert!(valid_spotify_id("6OwjpJLqayrcTuzBvxoGMd"));
        assert!(!valid_spotify_id("6OwjpJLqayrcTuzBvxoGM"));
        assert!(!valid_spotify_id("6OwjpJLqayrcTuzBvxoG-d"));
        assert!(!valid_spotify_id(""));
    }

    /// The lines both ends write and read: same file as the phone's WireFixturesTest.
    #[test]
    fn the_wire_fixtures_for_the_handoff_have_the_keys_the_phone_reads() {
        let doc: Value = serde_json::from_str(include_str!("../../scripts/phone/wire-fixtures.json")).unwrap();
        let keys = |v: &Value| {
            let mut k: Vec<String> = v.as_object().unwrap().keys().cloned().collect();
            k.sort();
            k
        };
        let t = track("sonora", "JESUS", "", 151_000);
        let target = Target::Track("https://open.spotify.com/track/6OwjpJLqayrcTuzBvxoGMd".into());
        let line = offer_line("h-1", "s22", &target, &t, "Bose QC Earbuds II", false);
        assert_eq!(keys(&line), keys(&doc["laptop_to_phone"]["handoff"]));
        assert_eq!(keys(&done_line("h-1", "s22", true, "", Some("Bose"))), keys(&doc["laptop_to_phone"]["handoff_done"]));
        // What the phone sends is read as the answer to an offer, and only with an id.
        let none = std::collections::HashMap::new();
        let accept = doc["phone_to_laptop"]["handoff_accept"].to_string();
        assert_eq!(crate::remote::parse_client_line(&accept, &none), crate::remote::Action::HandoffAccept { id: "h-1".into() });
        let bad = r#"{"op":"handoff_accept"}"#;
        assert!(matches!(crate::remote::parse_client_line(bad, &none), crate::remote::Action::Reject(_)));
    }

    /// Read-only look at this machine (never pauses or disconnects anything):
    /// `cargo test --lib handoff::tests::live_probe -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn live_probe() {
        let track = crate::music::read_now();
        println!("playing: {:?}", track.as_ref().map(|t| (&t.player, &t.title, &t.status, t.position_ms, &t.url)));
        if let Some(t) = &track {
            let id = sonora_spotify_id(&t.title, t.artists.first().map(String::as_str).unwrap_or(""));
            println!("sonora id: {id:?}");
            println!("target: {:?}", classify(t, id.as_deref()));
        }
        println!("headset: {:?}", bluetooth::connected_headset());
    }
}
