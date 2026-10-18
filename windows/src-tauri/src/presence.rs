// Which device shows Mochi and Coucou's cards: the one in use. The rules are pure (`decide`, twin of
// the phone's Arbiter.kt: both read scripts/phone/presence-vectors.json), the rest is the live state
// they are fed with.
//
//  1. The presence server decides only when every device we can see reaches it and agrees on the
//     answer, and the answer is one of ours. (It only knows who sends it a heartbeat: if one of us
//     does not, its "active" would hide the other for no reason.) Its "vga" means nobody is in use:
//     Mochi goes to the panel, and no device here shows him, unless one of us with its screen on was
//     touched within the server's own limit (its answer is up to a heartbeat old): then rule 2 among those.
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

/// The phones that may join the link (each pairs with its own key and says which it is in its beats).
pub const PHONES: [&str; 2] = ["s21", "s22"];
pub const LAPTOP: &str = "laptop";
const KNOWN: [&str; 3] = [PHONES[0], PHONES[1], LAPTOP];

pub fn is_phone(id: &str) -> bool {
    PHONES.contains(&id)
}

/// A peer silent for longer than this is not there.
pub const FRESH_MS: u64 = 15_000;
/// The server's failsafe display: when it names this one, Mochi is on the VGA panel and on no device here.
pub const VGA: &str = "vga";
/// The server's own limit (mochi_presence.py IDLE_LIMIT): touched longer ago than this, a device is not in use.
pub const IDLE_LIMIT_S: u64 = 300;
/// What the laptop is called on the presence server (it says "pc"; here it is LAPTOP).
pub const SERVER_LAPTOP: &str = "pc";

/// The presence server's name for a device, in our ids ("pc" is the laptop). Pure.
pub fn from_server(name: &str) -> String {
    if name == SERVER_LAPTOP { LAPTOP.to_string() } else { name.to_string() }
}

/// How much more recently another device must have been touched to take the island from the one that has it.
pub const HYSTERESIS_S: u64 = 2;
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
    let mut in_use_only = false;
    if me.server_ok {
        if let Some(named) = me.server_active.as_deref() {
            let agreed = fresh.iter().all(|p| p.beat.server_ok && p.beat.server_active.as_deref() == Some(named));
            if agreed && KNOWN.contains(&named) {
                return named.to_string();
            }
            if agreed && named == VGA {
                // Nobody in use, says the server: the panel has him, unless one of us is in use right now.
                let touched = |idle: u64, screen: bool| screen && idle < IDLE_LIMIT_S;
                let anyone = touched(me.idle_s, me.screen_on)
                    || fresh.iter().any(|p| touched(p.beat.idle_s + p.age_ms / 1000, p.beat.screen_on));
                if !anyone {
                    return VGA.to_string();
                }
                in_use_only = true;
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
    if in_use_only {
        group.retain(|c| c.screen && c.idle < IDLE_LIMIT_S);
    }
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
    /// Jinx's background mood, as the presence server last said it (one of Mochi's emotes; None: calm or unknown).
    jinx_mood: Option<String>,
    /// What the page was last told.
    sent: Option<(bool, bool, bool, Option<String>, bool)>,
    /// This device is playing something (the page says so).
    music: bool,
    /// Claude Code is rate-limited here (the page says so): the phone's Mochi is tired too.
    limit: bool,
    /// Something just arrived here (a notification): the phone's Mochi is startled until then.
    news_until: Option<Instant>,
    active: bool,
    /// A phone holds Mochi: it is a live peer of the remote link, so it already gets the sessions from it.
    phone_holds: bool,
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

    /// Works out who has Mochi now and what the other device hears and sees. It does NOT say what the
    /// page was told: whoever has an app handle publishes the change with `recompute`. (A reader that
    /// settles without publishing, like the reply to a beat, must not use up the change: the page would
    /// never hear that the island is back.)
    fn settle(&mut self) {
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
        self.phone_holds = is_phone(&winner);
        let alive = |p: &Peer| now.duration_since(p.at) <= Duration::from_millis(FRESH_MS);
        self.peer_music = self.peers.values().any(|p| alive(p) && p.music);
        self.peer_news = self.peers.values().any(|p| alive(p) && p.news && now.duration_since(p.at) <= NEWS_WINDOW);
    }

    /// Settles, and returns the payload for the page if it differs from what the page was last told.
    fn recompute(&mut self) -> Option<Value> {
        self.settle();
        let now_state = (self.active, self.peer_music, self.peer_news, self.jinx_mood.clone(), self.phone_holds);
        if self.sent.as_ref() == Some(&now_state) {
            return None;
        }
        self.sent = Some(now_state);
        Some(self.payload())
    }

    fn payload(&self) -> Value {
        json!({ "active": self.active, "peerMusic": self.peer_music, "peerNews": self.peer_news, "jinxMood": self.jinx_mood, "phone": self.phone_holds })
    }
}

fn publish(app: &AppHandle, payload: Option<Value>) {
    if let Some(p) = payload {
        // One line per change of who has Mochi, so a migration that does not happen can be read off the log.
        let at_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        crate::log::line(format!("presence: {p} @{at_ms}ms"));
        let _ = app.emit("presence", p);
    }
}

/// What the page asks at start.
pub fn state() -> Value {
    with(|l| {
        l.settle();
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
        // Settle only: publishing the change to the page is the caller-with-a-handle's job (the clock),
        // and consuming it here is exactly what kept the island hidden after coming back.
        l.settle();
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

/// What every other phone on the link is told about this one: its beat, relayed by the laptop (the
/// phones are not connected to each other, so the laptop is how they hear one another and arbitrate
/// with the same inputs). `relay` marks it as not the laptop's own.
pub fn relay_line(beat: &Beat, music: bool, news: bool) -> Value {
    json!({
        "t": "peer",
        "device": beat.device,
        "idle": beat.idle_s,
        "screen_on": beat.screen_on,
        "server_ok": beat.server_ok,
        "server_active": beat.server_active,
        "music": music,
        "news": news,
        "relay": true,
    })
}

/// A phone's link ended: the other phones drop it at once instead of waiting for its beat to go stale.
pub fn gone_line(device: &str) -> Value {
    json!({ "t": "peer_gone", "device": device })
}

/// Reads a phone's beat. Only state: the device must be one of the phones, the server answer one of
/// our ids, the idle bounded. Anything else about it is ignored.
pub fn parse_peer(v: &Value) -> Option<(Beat, bool, bool)> {
    let device = v.get("device").and_then(Value::as_str).filter(|d| is_phone(d))?.to_string();
    let idle = v.get("idle").and_then(Value::as_u64).unwrap_or(0).min(MAX_IDLE_S);
    let server_active = v
        .get("server_active")
        .and_then(Value::as_str)
        .map(from_server)
        // One of ours, or the panel ("nobody in use"): what else a server says is no answer.
        .filter(|s| KNOWN.contains(&s.as_str()) || s == VGA);
    let flag = |k: &str, d: bool| v.get(k).and_then(Value::as_bool).unwrap_or(d);
    Some((
        Beat {
            device,
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

/// Seconds since the last keyboard or pointer input, from GNOME's idle monitor: asked over one kept
/// D-Bus connection (cheap enough to look four times a second). Where that is not there, `gdbus` is
/// tried at most every two seconds.
#[cfg(target_os = "linux")]
fn read_idle_s() -> Option<u64> {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::OnceLock;

    static CONN: OnceLock<Option<zbus::blocking::Connection>> = OnceLock::new();
    static LAST_FALLBACK_MS: AtomicU64 = AtomicU64::new(0);

    if let Some(conn) = CONN.get_or_init(|| zbus::blocking::Connection::session().ok()) {
        let reply = conn.call_method(
            Some("org.gnome.Mutter.IdleMonitor"),
            "/org/gnome/Mutter/IdleMonitor/Core",
            Some("org.gnome.Mutter.IdleMonitor"),
            "GetIdletime",
            &(),
        );
        if let Ok(msg) = reply {
            if let Ok(ms) = msg.body().deserialize::<u64>() {
                return Some(ms / 1000);
            }
        }
    }
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    if now_ms.saturating_sub(LAST_FALLBACK_MS.load(Ordering::Relaxed)) < 2_000 {
        return None;
    }
    LAST_FALLBACK_MS.store(now_ms, Ordering::Relaxed);
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

/// What the presence server answers a heartbeat with: who is in use (our ids; empty: nobody named) and
/// Jinx's background mood (`jinx_mood`, one of Mochi's emotes, or None when calm or not sent). Pure.
pub fn read_answer(body: &Value) -> (String, Option<String>) {
    let active = from_server(body.get("active_device").and_then(Value::as_str).unwrap_or(""));
    let mood = body.get("jinx_mood").and_then(|m| crate::jinx_gateway::valid_emote(m));
    (active, mood)
}

/// The presence server's heartbeat, only if its address and key are in the keyring. `Some(answer)`
/// is its answer (see `read_answer`); None: not reachable or not set up.
async fn server_heartbeat(idle_s: u64, screen_on: bool) -> Option<(String, Option<String>)> {
    let url = crate::secrets::get("presence-url")?;
    let key = crate::secrets::get("presence-key")?;
    let client = reqwest::Client::builder().timeout(Duration::from_secs(5)).build().ok()?;
    let resp = client
        .post(format!("{}/api/presence/heartbeat", url.trim_end_matches('/')))
        .header("X-Mochi-Key", key)
        // The server knows the laptop as "pc" (it refuses any other name for it).
        .json(&json!({ "device": SERVER_LAPTOP, "idle_seconds": idle_s, "screen_on": screen_on }))
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let body: Value = resp.json().await.ok()?;
    Some(read_answer(&body))
}

/// The inbox messages to show: newer than `after`, with text, from the last hour (as on the phone: older news
/// is not news). `(id, payload for the island)`. Pure.
pub fn new_messages(body: &Value, after: i64, now_s: i64) -> Vec<(i64, Value)> {
    let mut out = Vec::new();
    for m in body.get("messages").and_then(Value::as_array).into_iter().flatten() {
        let id = m.get("id").and_then(Value::as_i64).unwrap_or(0);
        let text = m.get("text").and_then(Value::as_str).unwrap_or("").trim();
        let ts = m.get("ts").and_then(Value::as_i64).unwrap_or(now_s);
        if id <= after || text.is_empty() || now_s - ts > 3600 {
            continue;
        }
        out.push((id, json!({
            "id": id,
            "text": text.chars().take(500).collect::<String>(),
            "kind": m.get("kind").and_then(Value::as_str).unwrap_or("info"),
            "source": m.get("source").and_then(Value::as_str).unwrap_or(""),
            "mood": m.get("mood").cloned().unwrap_or(Value::Null),
        })));
    }
    out.sort_by_key(|(id, _)| *id);
    out
}

static LAST_INBOX: Mutex<i64> = Mutex::new(0);

/// While this laptop is the device in use, what Jinx sent through the presence server (a reply to a message
/// written elsewhere, a notice, a question waiting for permission) goes to the island as `jinx-island`
/// events and is acked, so no other device shows it again. Never on another device's turn.
async fn inbox_round(app: &AppHandle) {
    let (Some(url), Some(key)) = (crate::secrets::get("presence-url"), crate::secrets::get("presence-key")) else {
        return;
    };
    let Ok(client) = reqwest::Client::builder().timeout(Duration::from_secs(5)).build() else { return };
    let base = url.trim_end_matches('/').to_string();
    let Ok(resp) = client.get(format!("{base}/api/mochi/inbox?since_id=0")).header("X-Mochi-Key", &key).send().await else {
        return;
    };
    let Ok(body) = resp.json::<Value>().await else { return };
    let after = *LAST_INBOX.lock().unwrap();
    let now_s = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    for (id, payload) in new_messages(&body, after, now_s) {
        let _ = app.emit_to(crate::island::WINDOW_LABEL, "jinx-island", payload);
        let _ = client
            .post(format!("{base}/api/mochi/ack"))
            .header("X-Mochi-Key", &key)
            .json(&json!({ "id": id, "by": SERVER_LAPTOP }))
            .send()
            .await;
        *LAST_INBOX.lock().unwrap() = id;
    }
}

/// Starts the clock: own idle and lock every 2 s (which also expires peers that went quiet), and the
/// server heartbeat every 30 s when it is configured. Safe to call once at start.
pub fn start(app: &AppHandle) {
    let inbox_app = app.clone();
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut tick: u64 = 0;
        loop {
            // Nobody to arbitrate with (no phone on the link, no server set up): this device is the
            // one in use whatever its idle says, so the clock only ticks now and then.
            let alone = with(|l| l.peers.is_empty() && !l.server_configured);
            #[cfg(target_os = "linux")]
            if !alone {
                // The idle monitor is asked over one kept D-Bus connection (no process per look), four
                // times a second; the lock hint is a process, so it is read every two seconds.
                let look_at_lock = tick % 8 == 0;
                let read = tauri::async_runtime::spawn_blocking(move || {
                    (read_idle_s(), if look_at_lock { Some(read_locked()) } else { None })
                })
                .await;
                if let Ok((idle, locked)) = read {
                    // Touched since the last look (its idle fell): the phone hears it now, so Mochi
                    // follows the person at once instead of at the phone's next beat.
                    let touched = with(|l| {
                        let before = l.my_beat().idle_s;
                        let fell = idle.map_or(false, |i| i < before);
                        if let Some(idle) = idle {
                            l.idle_s = idle;
                            l.idle_at = Some(Instant::now());
                        }
                        if let Some(locked) = locked {
                            l.locked = locked;
                        }
                        fell
                    });
                    if touched {
                        crate::remote::publish_peer();
                    }
                }
            }
            let out = with(|l| l.recompute());
            publish(&app, out);
            tick = tick.wrapping_add(1);
            let wait = if alone { Duration::from_secs(10) } else { Duration::from_millis(250) };
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
                // Her mood stays as it was while the server is out of reach: it changes slowly.
                if let Some((_, mood)) = &answer {
                    l.jinx_mood = mood.clone();
                }
                l.server = answer.map(|(a, _)| (a, Instant::now()));
            });
            if configured {
                WAKE.notify_one();
                // Jinx's messages are this laptop's to show only while it is the one in use.
                if with(|l| l.server.as_ref().map_or(false, |(a, _)| *a == from_server(SERVER_LAPTOP))) {
                    inbox_round(&inbox_app).await;
                }
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
        // Only phones speak on this link; the laptop's own id is not accepted from outside.
        assert!(parse_peer(&json!({"device":"laptop","idle":0})).is_none());
        assert!(parse_peer(&json!({"idle":0})).is_none());
        // The second phone is a phone; an unknown device is not.
        assert_eq!(parse_peer(&json!({"device":"s22","idle":3})).unwrap().0.device, "s22");
        assert!(parse_peer(&json!({"device":"s23","idle":3})).is_none());
        // An id we do not know is no server answer; an absurd idle is clamped.
        let odd = parse_peer(&json!({"device":"s21","idle":u64::MAX,"server_active":"tv"})).unwrap();
        assert_eq!(odd.0.server_active, None);
        assert_eq!(odd.0.idle_s, MAX_IDLE_S);
        // The panel is an answer ("nobody in use"), and the server's "pc" is the laptop.
        let vga = parse_peer(&json!({"device":"s21","idle":1,"server_active":"vga"})).unwrap();
        assert_eq!(vga.0.server_active.as_deref(), Some(VGA));
        let pc = parse_peer(&json!({"device":"s21","idle":1,"server_active":"pc"})).unwrap();
        assert_eq!(pc.0.server_active.as_deref(), Some(LAPTOP));
    }

    #[test]
    fn only_new_inbox_messages_with_text_are_shown_in_order() {
        let body = json!({"messages": [
            {"id": 7, "text": "b", "kind": "alert", "source": "jinx"},
            {"id": 5, "text": "a", "source": "jinx-chat"},
            {"id": 3, "text": "old"},
            {"id": 9, "text": "  "},
            {"id": 11, "text": "stale", "ts": 1000}
        ]});
        let got = new_messages(&body, 4, 10_000);
        assert_eq!(got.iter().map(|(id, _)| *id).collect::<Vec<_>>(), vec![5, 7]);
        assert_eq!(got[0].1["source"], "jinx-chat");
        assert_eq!(got[1].1["kind"], "alert");
        assert_eq!(got[0].1["kind"], "info");
    }

    #[test]
    fn the_server_answer_reads_its_names_and_her_mood() {
        assert_eq!(read_answer(&json!({"active_device":"pc","jinx_mood":"happy"})), ("laptop".into(), Some("happy".into())));
        assert_eq!(read_answer(&json!({"active_device":"vga","jinx_mood":null})), ("vga".into(), None));
        assert_eq!(read_answer(&json!({"active_device":"s22","jinx_mood":"Happy"})), ("s22".into(), None));
        assert_eq!(read_answer(&json!({})), (String::new(), None));
    }

    const PHONE: &str = "s21";

    fn phone_beat(idle: u64) -> Peer {
        Peer {
            beat: Beat { device: PHONE.into(), idle_s: idle, screen_on: true, server_ok: false, server_active: None },
            music: false,
            news: false,
            at: Instant::now(),
        }
    }

    /// The reported bug: the person used the phone (island leaves), then came back to the laptop, and
    /// the island stayed hidden because the reply to a beat had settled the change without publishing it.
    #[test]
    fn coming_back_to_the_laptop_is_published_even_if_a_reply_settled_it_first() {
        let mut l = Live { active: true, ..Live::default() };
        l.idle_s = 100;
        l.idle_at = Some(Instant::now());
        l.peers.insert(PHONE.into(), phone_beat(0));
        let gone = l.recompute().expect("the island leaves when the phone is used");
        assert_eq!(gone["active"], false);

        // Back at the laptop: its idle falls, the phone has been quiet for a while.
        l.idle_s = 0;
        l.idle_at = Some(Instant::now());
        l.peers.insert(PHONE.into(), phone_beat(40));
        // The reply to a beat settles first (no handle to publish with)...
        l.settle();
        assert!(l.active);
        // ...and the clock, which has the handle, must still have the change to publish.
        let back = l.recompute().expect("the page must still be told the island is back");
        assert_eq!(back["active"], true);
        // Nothing new: nothing to publish.
        assert!(l.recompute().is_none());
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
