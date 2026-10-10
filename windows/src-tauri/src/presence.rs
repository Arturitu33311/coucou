// Which device shows Mochi and Coucou's cards: the one in use. The rules are pure (`decide`, twin of
// the phone's Arbiter.kt: both read scripts/phone/presence-vectors.json), the rest is the live state
// they are fed with.
//
//  1. The presence server decides only when every device we can see reaches it and agrees on the
//     answer, and the answer is one of ours. (It only knows who sends it a heartbeat: if one of us
//     does not, its "active" would hide the other for no reason.)
//  2. Otherwise the devices that can hear each other decide among themselves: the one with a screen
//     on that was touched most recently, with a margin so a stray touch does not flip the island.
//  3. A device that hears nobody and is not told by the server shows everything.
//
// Nothing here changes what is waiting for an answer: only the painting follows `active`.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};

pub const PHONE: &str = "s21";
pub const LAPTOP: &str = "laptop";
const KNOWN: [&str; 2] = [PHONE, LAPTOP];

/// A peer silent for longer than this is not there.
pub const FRESH_MS: u64 = 15_000;
/// How much more recently another device must have been touched to take the island from the one that has it.
pub const HYSTERESIS_S: u64 = 8;
/// A week: past that an idle value means nothing, and an absurd one must not outlast a real one.
pub const MAX_IDLE_S: u64 = 7 * 24 * 3600;
/// How long the server's answer counts: the heartbeat is every 30 s, with one missed.
const SERVER_TTL: Duration = Duration::from_secs(75);
/// News is an edge, not a state: only the beat that carries it counts.
const NEWS_WINDOW: Duration = Duration::from_secs(3);

/// What one device says about itself. `idle_s` is seconds since its last input.
#[derive(Clone, Debug, PartialEq)]
pub struct Beat {
    pub device: String,
    pub idle_s: u64,
    pub screen_on: bool,
    pub server_ok: bool,
    pub server_active: Option<String>,
}

/// A peer's last beat and how long ago it arrived (its idle has kept running since).
#[derive(Clone, Debug)]
pub struct Seen {
    pub beat: Beat,
    pub age_ms: u64,
}

/// Who should show Mochi and Coucou: one of the ids in play.
pub fn decide(me: &Beat, peers: &[Seen], previous: Option<&str>) -> String {
    let fresh: Vec<&Seen> = peers.iter().filter(|p| p.age_ms <= FRESH_MS).collect();

    // 1. The server, only when all of us can see it and agree.
    if me.server_ok {
        if let Some(named) = me.server_active.as_deref() {
            if KNOWN.contains(&named)
                && fresh.iter().all(|p| p.beat.server_ok && p.beat.server_active.as_deref() == Some(named))
            {
                return named.to_string();
            }
        }
    }

    // 2. The ones that hear each other.
    struct Cand<'a> {
        id: &'a str,
        idle: u64,
        screen: bool,
    }
    let mut group = vec![Cand { id: &me.device, idle: me.idle_s, screen: me.screen_on }];
    group.extend(fresh.iter().map(|p| Cand { id: &p.beat.device, idle: p.beat.idle_s + p.age_ms / 1000, screen: p.beat.screen_on }));
    let any_awake = group.iter().any(|c| c.screen);
    let awake: Vec<&Cand> = group.iter().filter(|c| c.screen || !any_awake).collect();
    let best = awake.iter().min_by(|a, b| a.idle.cmp(&b.idle).then(a.id.cmp(b.id))).expect("the group has at least this device");
    if let Some(held) = awake.iter().find(|c| Some(c.id) == previous) {
        if held.idle <= best.idle + HYSTERESIS_S {
            return held.id.to_string();
        }
    }
    best.id.to_string()
}

// ── Live state ───────────────────────────────────────────────────────────────────

struct Peer {
    beat: Beat,
    music: bool,
    news: bool,
    at: Instant,
}

#[derive(Default)]
struct Live {
    peers: HashMap<String, Peer>,
    /// Own input idle at `idle_at`, and whether the screen is on (not locked).
    idle_s: u64,
    idle_at: Option<Instant>,
    locked: bool,
    server: Option<(String, Instant)>,
    /// The presence server's address and key are in the keyring (so it is worth asking).
    server_configured: bool,
    previous: Option<String>,
    /// What the page was last told.
    sent: Option<(bool, bool, bool)>,
    /// This device is playing something (the page says so).
    music: bool,
    /// Claude Code is rate-limited here (the page says so): the phone's Mochi is tired too.
    limit: bool,
    /// Something just arrived here (a notification): the phone's Mochi is startled until then.
    news_until: Option<Instant>,
    active: bool,
    peer_music: bool,
    peer_news: bool,
}

static LIVE: Mutex<Option<Live>> = Mutex::new(None);

/// Wakes the clock when something changes who is there (a phone arrives, the server answers).
static WAKE: tokio::sync::Notify = tokio::sync::Notify::const_new();

fn with<R>(f: impl FnOnce(&mut Live) -> R) -> R {
    let mut g = LIVE.lock().unwrap_or_else(|e| e.into_inner());
    let live = g.get_or_insert_with(|| Live { active: true, ..Live::default() });
    f(live)
}

impl Live {
    fn my_beat(&self) -> Beat {
        let ticking = self.idle_at.map(|t| t.elapsed().as_secs()).unwrap_or(0);
        let server = self.server.as_ref().filter(|(_, at)| at.elapsed() <= SERVER_TTL);
        Beat {
            device: LAPTOP.to_string(),
            idle_s: (self.idle_s + ticking).min(MAX_IDLE_S),
            screen_on: !self.locked,
            server_ok: server.is_some(),
            server_active: server.map(|(a, _)| a.clone()).filter(|a| !a.is_empty()),
        }
    }

    /// Recomputes and returns the payload for the page if it changed.
    fn recompute(&mut self) -> Option<Value> {
        let now = Instant::now();
        let seen: Vec<Seen> = self
            .peers
            .values()
            .map(|p| Seen { beat: p.beat.clone(), age_ms: now.duration_since(p.at).as_millis() as u64 })
            .collect();
        let me = self.my_beat();
        let winner = decide(&me, &seen, self.previous.as_deref());
        self.previous = Some(winner.clone());
        self.active = winner == LAPTOP;
        let alive = |p: &Peer| now.duration_since(p.at) <= Duration::from_millis(FRESH_MS);
        self.peer_music = self.peers.values().any(|p| alive(p) && p.music);
        self.peer_news = self.peers.values().any(|p| alive(p) && p.news && now.duration_since(p.at) <= NEWS_WINDOW);
        let now_state = (self.active, self.peer_music, self.peer_news);
        if self.sent == Some(now_state) {
            return None;
        }
        self.sent = Some(now_state);
        Some(self.payload())
    }

    fn payload(&self) -> Value {
        json!({ "active": self.active, "peerMusic": self.peer_music, "peerNews": self.peer_news })
    }
}

fn publish(app: &AppHandle, payload: Option<Value>) {
    if let Some(p) = payload {
        let _ = app.emit("presence", p);
    }
}

/// What the page asks at start.
pub fn state() -> Value {
    with(|l| {
        l.recompute();
        l.payload()
    })
}

/// A beat from the phone arrived over the link.
pub fn on_peer(app: &AppHandle, beat: Beat, music: bool, news: bool) {
    // The first beat of a link: own idle may never have been read (the clock sleeps while alone), and
    // the reply must not claim "just used". One quick read, here.
    #[cfg(target_os = "linux")]
    if with(|l| l.idle_at.is_none() || l.peers.is_empty()) {
        let (idle, locked) = (read_idle_s(), read_locked());
        with(|l| {
            if let Some(idle) = idle {
                l.idle_s = idle;
                l.idle_at = Some(Instant::now());
            }
            l.locked = locked;
        });
    }
    let out = with(|l| {
        let first = !l.peers.contains_key(&beat.device);
        l.peers.insert(beat.device.clone(), Peer { beat, music, news, at: Instant::now() });
        if first {
            WAKE.notify_one();
        }
        l.recompute()
    });
    publish(app, out);
}

/// The link of that device ended: it is not there until it says so again.
pub fn on_link_down(app: &AppHandle, device: &str) {
    let out = with(|l| {
        l.peers.remove(device);
        l.recompute()
    });
    publish(app, out);
}

/// This device's own beat, for the reply to a phone's beat.
pub fn reply_line() -> Value {
    with(|l| {
        let b = l.my_beat();
        l.recompute();
        json!({
            "t": "peer",
            "device": b.device,
            "idle": b.idle_s,
            "screen_on": b.screen_on,
            "server_ok": b.server_ok,
            "server_active": b.server_active,
            "music": l.music,
            "news": l.news_until.map(|t| Instant::now() < t).unwrap_or(false),
            "limit": l.limit,
            "active": l.active,
        })
    })
}

/// The page tells what this device is playing (the dance follows it on the other one).
pub fn set_local_music(app: &AppHandle, music: bool) {
    let (out, changed) = with(|l| {
        let changed = l.music != music;
        l.music = music;
        (l.recompute(), changed)
    });
    publish(app, out);
    // The phone hears it now, not at its next beat.
    if changed {
        crate::remote::publish_peer();
    }
}

/// The page says whether Claude Code is rate-limited here: the phone's Mochi follows.
pub fn set_local_limit(limited: bool) {
    let changed = with(|l| {
        let changed = l.limit != limited;
        l.limit = limited;
        changed
    });
    if changed {
        crate::remote::publish_peer();
    }
}

/// Something arrived on this device (a notification): the phone's Mochi is startled too.
pub fn set_local_news() {
    with(|l| l.news_until = Some(Instant::now() + NEWS_WINDOW));
    crate::remote::publish_peer();
}

// ── What the phone may say ─────────────────────────────────────────────────────────

/// Reads a phone's beat. Only state: the device must be the phone, the server answer one of our ids,
/// the idle bounded. Anything else about it is ignored.
pub fn parse_peer(v: &Value) -> Option<(Beat, bool, bool)> {
    if v.get("device").and_then(Value::as_str) != Some(PHONE) {
        return None;
    }
    let idle = v.get("idle").and_then(Value::as_u64).unwrap_or(0).min(MAX_IDLE_S);
    let server_active = v
        .get("server_active")
        .and_then(Value::as_str)
        .filter(|s| KNOWN.contains(s))
        .map(str::to_string);
    let flag = |k: &str, d: bool| v.get(k).and_then(Value::as_bool).unwrap_or(d);
    Some((
        Beat {
            device: PHONE.to_string(),
            idle_s: idle,
            screen_on: flag("screen_on", true),
            server_ok: flag("server_ok", false),
            server_active,
        },
        flag("music", false),
        flag("news", false),
    ))
}

// ── Own signals ───────────────────────────────────────────────────────────────────

/// Seconds since the last keyboard or pointer input, from GNOME's idle monitor.
#[cfg(target_os = "linux")]
fn read_idle_s() -> Option<u64> {
    let out = std::process::Command::new("gdbus")
        .args([
            "call", "--session", "--dest", "org.gnome.Mutter.IdleMonitor",
            "--object-path", "/org/gnome/Mutter/IdleMonitor/Core",
            "--method", "org.gnome.Mutter.IdleMonitor.GetIdletime",
        ])
        .output()
        .ok()?;
    parse_idle_ms(&String::from_utf8_lossy(&out.stdout)).map(|ms| ms / 1000)
}

/// "(uint64 13650,)" → 13650.
pub fn parse_idle_ms(text: &str) -> Option<u64> {
    text.split(|c: char| !c.is_ascii_digit()).filter(|s| !s.is_empty()).last()?.parse().ok()
}

/// Is the session locked (logind's LockedHint)?
#[cfg(target_os = "linux")]
fn read_locked() -> bool {
    let id = std::env::var("XDG_SESSION_ID").unwrap_or_else(|_| "auto".into());
    std::process::Command::new("loginctl")
        .args(["show-session", &id, "-p", "LockedHint", "--value"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "yes")
        .unwrap_or(false)
}

/// The presence server's heartbeat, only if its address and key are in the keyring. `Some(active)`
/// is its answer (empty: it answered without naming anyone); None: not reachable or not set up.
async fn server_heartbeat(idle_s: u64, screen_on: bool) -> Option<String> {
    let url = crate::secrets::get("presence-url")?;
    let key = crate::secrets::get("presence-key")?;
    let client = reqwest::Client::builder().timeout(Duration::from_secs(5)).build().ok()?;
    let resp = client
        .post(format!("{}/api/presence/heartbeat", url.trim_end_matches('/')))
        .header("X-Mochi-Key", key)
        .json(&json!({ "device": LAPTOP, "idle_seconds": idle_s, "screen_on": screen_on }))
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let body: Value = resp.json().await.ok()?;
    Some(body.get("active_device").and_then(Value::as_str).unwrap_or("").to_string())
}

/// Starts the clock: own idle and lock every 2 s (which also expires peers that went quiet), and the
/// server heartbeat every 30 s when it is configured. Safe to call once at start.
pub fn start(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            // Nobody to arbitrate with (no phone on the link, no server set up): this device is the
            // one in use whatever its idle says, so the clock only ticks now and then.
            let alone = with(|l| l.peers.is_empty() && !l.server_configured);
            #[cfg(target_os = "linux")]
            if !alone {
                let read = tauri::async_runtime::spawn_blocking(|| (read_idle_s(), read_locked())).await;
                if let Ok((idle, locked)) = read {
                    // Touched since the last look (its idle fell): the phone hears it now, so Mochi
                    // follows the person at once instead of at the phone's next beat.
                    let touched = with(|l| {
                        let before = l.my_beat().idle_s;
                        let fell = idle.map_or(false, |i| i + 1 < before);
                        if let Some(idle) = idle {
                            l.idle_s = idle;
                            l.idle_at = Some(Instant::now());
                        }
                        l.locked = locked;
                        fell
                    });
                    if touched {
                        crate::remote::publish_peer();
                    }
                }
            }
            let out = with(|l| l.recompute());
            publish(&app, out);
            let wait = if alone { Duration::from_secs(10) } else { Duration::from_secs(2) };
            // A phone arriving wakes the clock at once (on_peer notifies).
            let _ = tokio::time::timeout(wait, WAKE.notified()).await;
        }
    });
    // The server is its own loop: a slow or absent one must not hold up the clock above.
    tauri::async_runtime::spawn(async move {
        loop {
            let (idle, screen_on) = with(|l| {
                let b = l.my_beat();
                (b.idle_s, b.screen_on)
            });
            let configured = crate::secrets::present("presence-url") && crate::secrets::present("presence-key");
            let answer = if configured { server_heartbeat(idle, screen_on).await } else { None };
            with(|l| {
                l.server_configured = configured;
                l.server = answer.map(|a| (a, Instant::now()));
            });
            if configured {
                WAKE.notify_one();
            }
            tokio::time::sleep(Duration::from_secs(30)).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn beat(o: &Value) -> Beat {
        Beat {
            device: o["device"].as_str().unwrap().to_string(),
            idle_s: o["idle"].as_u64().unwrap(),
            screen_on: o["screen_on"].as_bool().unwrap(),
            server_ok: o["server_ok"].as_bool().unwrap(),
            server_active: o["server_active"].as_str().map(str::to_string),
        }
    }

    /// The same file the phone's ArbiterTest reads: both sides must agree on every case.
    #[test]
    fn the_shared_vectors_all_hold() {
        let doc: Value = serde_json::from_str(include_str!("../../scripts/phone/presence-vectors.json")).unwrap();
        let cases = doc["cases"].as_array().unwrap();
        assert!(cases.len() >= 15);
        for c in cases {
            let peers: Vec<Seen> = c["peers"]
                .as_array()
                .unwrap()
                .iter()
                .map(|p| Seen { beat: beat(p), age_ms: p["age_ms"].as_u64().unwrap() })
                .collect();
            let previous = c["previous"].as_str();
            let got = decide(&beat(&c["me"]), &peers, previous);
            assert_eq!(got, c["expect"].as_str().unwrap(), "{}", c["name"]);
        }
    }

    #[test]
    fn a_phone_beat_is_state_and_bounded() {
        let ok = parse_peer(&json!({"device":"s21","idle":12,"screen_on":false,"server_ok":true,"server_active":"s21","music":true,"news":false})).unwrap();
        assert_eq!(ok.0.idle_s, 12);
        assert!(!ok.0.screen_on && ok.0.server_ok && ok.1 && !ok.2);
        assert_eq!(ok.0.server_active.as_deref(), Some("s21"));
        // Only the phone speaks on this link; the laptop's own id is not accepted from outside.
        assert!(parse_peer(&json!({"device":"laptop","idle":0})).is_none());
        assert!(parse_peer(&json!({"idle":0})).is_none());
        // An id we do not know is no server answer; an absurd idle is clamped.
        let odd = parse_peer(&json!({"device":"s21","idle":u64::MAX,"server_active":"vga"})).unwrap();
        assert_eq!(odd.0.server_active, None);
        assert_eq!(odd.0.idle_s, MAX_IDLE_S);
    }

    #[test]
    fn gnome_idle_output_is_read() {
        assert_eq!(parse_idle_ms("(uint64 13650,)\n"), Some(13650));
        assert_eq!(parse_idle_ms("garbage"), None);
    }

    #[test]
    fn a_silent_phone_stops_counting_on_its_own_clock() {
        // No beats at all: the laptop shows (alone, no server).
        let me = Beat { device: LAPTOP.into(), idle_s: 600, screen_on: true, server_ok: false, server_active: None };
        assert_eq!(decide(&me, &[], None), LAPTOP);
        let stale = Seen { beat: Beat { device: PHONE.into(), idle_s: 0, screen_on: true, server_ok: false, server_active: None }, age_ms: FRESH_MS + 1 };
        assert_eq!(decide(&me, &[stale], Some(PHONE)), LAPTOP);
    }
}
