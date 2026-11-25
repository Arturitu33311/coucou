// The audio relay on the computer. The phone holds the Bluetooth headphones; what plays here is sent to it
// over a private Unix socket (`$XDG_RUNTIME_DIR/coucou-audio.sock`), which the phone reaches through ssh
// with a key that may run exactly one command, `coucou-hook --audio` (a plain pipe to this socket). Nothing
// listens on the network, and the phone's other key (the Claude Code one) cannot reach this socket.
//
// The decisions (when to move the sound to the silent sink, when to stream, what to pause) are not made
// here but in `audiolink_logic`, the same function the phone runs. This file is the plumbing: it owns the
// socket, turns what the phone says into events, and carries out the actions the logic returns.
//
// What the phone may say is as narrow as `audiolink_wire` lets it: a heartbeat, or where the headphones
// are, whether a call is on and whether it is playing media. Anything else, or a line that is too long,
// drops the connection. A phone that says nothing for three seconds is gone.
//
// Off until the user turns it on (Settings → Phone); nothing runs, nothing is polled, until then.

#[cfg(target_os = "linux")]
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
#[cfg(target_os = "linux")]
use std::sync::mpsc::Sender;
#[cfg(target_os = "linux")]
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use tauri::AppHandle;
#[cfg(target_os = "linux")]
use crate::audiolink_logic::{step, Action, Event, State};
use crate::audiolink_wire::PhoneMsg;

/// The phone is gone after this long without a line from it.
pub const SILENCE: Duration = Duration::from_secs(3);

#[cfg(target_os = "linux")]
static ENABLED: AtomicBool = AtomicBool::new(false);
#[cfg(target_os = "linux")]
static STARTED: AtomicBool = AtomicBool::new(false);
#[cfg(target_os = "linux")]
static CHECKED: AtomicBool = AtomicBool::new(false);
#[cfg(target_os = "linux")]
static NEXT_ID: AtomicU64 = AtomicU64::new(1);
#[cfg(target_os = "linux")]
static EVENTS: OnceLock<Sender<Event>> = OnceLock::new();
#[cfg(target_os = "linux")]
static CURRENT: Mutex<Slot<Conn>> = Mutex::new(Slot::new());

/// One connection holds the slot at a time; a newer one takes it over and the older one is closed.
pub struct Slot<T> {
    current: Option<(u64, T)>,
}

impl<T> Slot<T> {
    pub const fn new() -> Self {
        Slot { current: None }
    }

    /// Puts `conn` in the slot as connection `id`; returns the one it replaced, which the caller closes.
    pub fn take_over(&mut self, id: u64, conn: T) -> Option<T> {
        self.current.replace((id, conn)).map(|(_, old)| old)
    }

    /// The connection `id` ended. True when it was the current one (a replaced connection ending is not news).
    pub fn release(&mut self, id: u64) -> bool {
        if self.current.as_ref().is_some_and(|(cur, _)| *cur == id) {
            self.current = None;
            true
        } else {
            false
        }
    }

    /// True when connection `id` is the one in the slot.
    pub fn is_current(&self, id: u64) -> bool {
        self.current.as_ref().is_some_and(|(cur, _)| *cur == id)
    }

    /// Empties the slot, handing back whoever was in it.
    pub fn take(&mut self) -> Option<T> {
        self.current.take().map(|(_, c)| c)
    }
}

/// Why a connection's reading side ended.
#[derive(Debug, PartialEq, Eq)]
pub enum End {
    /// Nothing arrived for `SILENCE`.
    Silent,
    /// The phone hung up.
    Closed,
    /// A line that is too long or that the protocol does not know.
    Bad,
}

/// Reads the phone's lines until it goes quiet, hangs up or says something the protocol does not allow.
pub async fn pump<R>(rd: &mut R, silence: Duration, mut on_msg: impl FnMut(PhoneMsg)) -> End
where
    R: tokio::io::AsyncRead + Unpin,
{
    use tokio::io::AsyncReadExt;

    use crate::audiolink_wire::{parse_phone_line, MAX_ART_LINE, MAX_LINE, MAX_NOW_LINE};

    let mut buf: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 1024];
    loop {
        let n = match tokio::time::timeout(silence, rd.read(&mut chunk)).await {
            Err(_) => return End::Silent,
            Ok(Err(_)) | Ok(Ok(0)) => return End::Closed,
            Ok(Ok(n)) => n,
        };
        buf.extend_from_slice(&chunk[..n]);
        while let Some(nl) = buf.iter().position(|b| *b == b'\n') {
            let line: Vec<u8> = buf.drain(..=nl).collect();
            let Ok(text) = std::str::from_utf8(&line[..nl]) else { return End::Bad };
            if text.trim().is_empty() {
                continue;
            }
            match parse_phone_line(text) {
                Ok(msg) => on_msg(msg),
                Err(_) => return End::Bad,
            }
        }
        // No newline yet and already longer than its kind of line may be: a flood, not a message. Only the two
        // now-playing ops may pass 512 bytes, and they say so first (`op` is the first key).
        let allowed = if buf.starts_with(br#"{"op":"art""#) {
            MAX_ART_LINE
        } else if buf.starts_with(br#"{"op":"now""#) {
            MAX_NOW_LINE
        } else {
            MAX_LINE
        };
        if buf.len() > allowed {
            return End::Bad;
        }
    }
}

#[cfg(target_os = "linux")]
struct Conn {
    /// Frames to write to the phone. Full means the phone is slow: frames are dropped, never queued without bound.
    out: tokio::sync::mpsc::Sender<Vec<u8>>,
    /// A second handle on the socket, to close it from outside.
    kill: std::os::unix::net::UnixStream,
}

#[cfg(target_os = "linux")]
fn send_event(ev: Event) {
    if let Some(tx) = EVENTS.get() {
        let _ = tx.send(ev);
    }
}

/// Writes one audio frame to the phone if one is connected. False when nobody is there or it is too slow
/// (the frame is dropped: audio is never queued without bound, and never takes the slots kept for control).
#[cfg(target_os = "linux")]
pub fn send_frame(bytes: Vec<u8>) -> bool {
    match CURRENT.lock().unwrap().current.as_ref() {
        Some((_, c)) => try_audio(&c.out, bytes),
        None => false,
    }
}

/// Writes a control frame (pause, swap prompt). False only when nobody is connected or even the reserve is full.
#[cfg(target_os = "linux")]
pub fn send_control(bytes: Vec<u8>) -> bool {
    match CURRENT.lock().unwrap().current.as_ref() {
        Some((_, c)) => c.out.try_send(bytes).is_ok(),
        None => false,
    }
}

/// Turns the audio relay on or off; the socket is served from the first time it is turned on.
#[cfg(target_os = "linux")]
pub fn sync_enabled(app: &AppHandle, on: bool) {
    // Whatever the setting says now, a crash while it was on must not leave the computer silent.
    if !CHECKED.swap(true, Ordering::SeqCst) {
        crate::audiolink_sink::restore_if_stale();
    }
    ENABLED.store(on, Ordering::Relaxed);
    if on && !STARTED.swap(true, Ordering::SeqCst) {
        let (tx, rx) = std::sync::mpsc::channel::<Event>();
        let _ = EVENTS.set(tx);
        let engine_app = app.clone();
        std::thread::spawn(move || engine(engine_app, rx));
        std::thread::spawn(autopc_watch);
        let app = app.clone();
        tauri::async_runtime::spawn(async move { serve(app).await });
    }
    send_event(Event::Enabled(on));
    if !on {
        if let Some(c) = CURRENT.lock().unwrap().take() {
            let _ = c.kill.shutdown(std::net::Shutdown::Both);
        }
        crate::phonenow::clear();
    }
}

#[cfg(not(target_os = "linux"))]
pub fn sync_enabled(_app: &AppHandle, _on: bool) {}

/// How long the headphones may be missing from the phone (a Bluetooth blip) before the computer gives its
/// speakers back. The sound stays on the silent sink meanwhile.
#[cfg(target_os = "linux")]
pub const GRACE: Duration = Duration::from_secs(5);

/// True when the wait before giving the speakers back must start now: the relay just fell into `Hold` (or was
/// already held while the phone still had the headphones) and the headphones are neither on the phone nor on
/// the computer.
#[cfg(target_os = "linux")]
pub fn needs_grace(prev: &State, next: &State) -> bool {
    use crate::audiolink_logic::{Mode, Where};
    next.mode == Mode::Hold
        && !matches!(next.headphones, Where::Phone | Where::Pc)
        && (prev.mode != Mode::Hold || prev.headphones == Where::Phone)
}

/// Tells grace timers apart: each wait that starts gets a number, and anything that ends the grace situation (the
/// headphones came back, the relay went off) makes every earlier number stale, so an old timer firing late can
/// never cut a newer blip short.
#[cfg(target_os = "linux")]
pub struct GraceClock {
    gen: u64,
}

#[cfg(target_os = "linux")]
impl GraceClock {
    pub const fn new() -> Self {
        GraceClock { gen: 0 }
    }

    /// Call after every step. `Some(n)` means: start a timer that, when it ends, asks `is_current(n)`.
    pub fn update(&mut self, prev: &State, next: &State) -> Option<u64> {
        use crate::audiolink_logic::{Mode, Where};
        if needs_grace(prev, next) {
            self.gen += 1;
            return Some(self.gen);
        }
        // Still held with the headphones gone: the running wait stays valid. Anything else ends the situation.
        let still_waiting = next.mode == Mode::Hold && !matches!(next.headphones, Where::Phone | Where::Pc);
        if !still_waiting {
            self.gen += 1;
        }
        None
    }

    /// True when timer `n` is still the one that counts.
    pub fn is_current(&self, n: u64) -> bool {
        self.gen == n
    }
}

#[cfg(target_os = "linux")]
static GRACE_CLOCK: Mutex<GraceClock> = Mutex::new(GraceClock::new());

/// The events a phone's state line stands for, in the order that avoids a pointless flip: a call is applied
/// before the headphones, so "headphones on the phone, call on" lands in `Hold` without first starting the relay.
pub fn events_of(msg: &PhoneMsg) -> Vec<crate::audiolink_logic::Event> {
    use crate::audiolink_logic::Event;
    match msg {
        PhoneMsg::Heartbeat | PhoneMsg::Caps | PhoneMsg::Now { .. } | PhoneMsg::Art { .. } => Vec::new(),
        PhoneMsg::State { headphones, call, phone_playing } => {
            vec![Event::Call(*call), Event::Headphones(*headphones), Event::PhoneMedia(*phone_playing)]
        }
    }
}

/// Room in the phone's queue. A part of it is kept for control frames (pause, swap prompt), which must not be
/// lost when the network is slow and audio is piling up.
pub const OUT_CAPACITY: usize = 64;
pub const CONTROL_RESERVE: usize = 16;

/// Queues an audio frame unless the queue is down to the control reserve.
pub fn try_audio(tx: &tokio::sync::mpsc::Sender<Vec<u8>>, bytes: Vec<u8>) -> bool {
    tx.capacity() > CONTROL_RESERVE && tx.try_send(bytes).is_ok()
}

/// Largest sample (absolute value) still counted as silence.
pub const SILENCE_PEAK: i16 = 8;
/// How many silent 20 ms chunks are still sent after the last loud one (one second).
pub const SILENCE_TAIL: u32 = 50;

/// True when every 16-bit little-endian sample in the chunk is within `SILENCE_PEAK`.
pub fn is_silent(chunk: &[u8]) -> bool {
    chunk.chunks_exact(2).all(|b| i16::from_le_bytes([b[0], b[1]]).unsigned_abs() <= SILENCE_PEAK as u16)
}

/// Lets sound through, and a short tail of silence after it, so a quiet laptop costs the network nothing.
pub struct SilenceGate {
    quiet: u32,
}

impl SilenceGate {
    pub const fn new() -> Self {
        SilenceGate { quiet: 0 }
    }

    /// True when this chunk should be sent.
    pub fn pass(&mut self, chunk: &[u8]) -> bool {
        if is_silent(chunk) {
            self.quiet = self.quiet.saturating_add(1);
            self.quiet <= SILENCE_TAIL
        } else {
            self.quiet = 0;
            true
        }
    }
}

// Loudness for what goes to the phone (automatic gain + look-ahead limiter) lives in `audiolink_dsp`.

/// Sound that goes on this long counts as "this computer is playing" (a ping or a click is shorter).
pub const PLAYING_AFTER_MS: u64 = 3_000;
/// With a media player reporting "playing" as well, a sound counts as playing after only this long.
pub const FAST_AFTER_MS: u64 = 600;
/// Silence up to this long inside a sound does not end it (music and speech have gaps).
pub const SOUND_GAP_MS: u64 = 1_000;
/// This long without sound and the computer is no longer playing.
pub const PLAYING_ENDS_MS: u64 = 3_000;

// The relay tells "the computer is playing" from the sound itself and then asks the phone to pause its music. It cannot
// tell whose sound it is, so Coucou's own (an agent that finished, a notification, « OK Coucou »'s spoken answer) paused
// the phone's music. Coucou says when it makes a sound and for how long; that stretch does not count for this decision
// (the sound still goes to the phone like any other).

/// Longest own sound that is believed, and the tail left after it (room for the output's own delay).
const OWN_SOUND_MAX_MS: u64 = 20_000;
const OWN_SOUND_TAIL_MS: u64 = 400;

static OWN_SOUND_UNTIL: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);

/// When the current own-sound stretch ends if one of `ms` starts at `now`: never earlier than what was already declared.
fn own_sound_end(current: Option<std::time::Instant>, now: std::time::Instant, ms: u64) -> std::time::Instant {
    let until = now + std::time::Duration::from_millis(ms.min(OWN_SOUND_MAX_MS) + OWN_SOUND_TAIL_MS);
    current.map_or(until, |c| c.max(until))
}

/// Coucou is about to make a sound of `ms` milliseconds.
pub fn own_sound(ms: u64) {
    let mut g = OWN_SOUND_UNTIL.lock().unwrap();
    *g = Some(own_sound_end(*g, std::time::Instant::now(), ms));
}

/// Whether the sound heard now is Coucou's own.
fn own_sound_now() -> bool {
    OWN_SOUND_UNTIL.lock().unwrap().is_some_and(|t| std::time::Instant::now() < t)
}

/// The island's sounds (an agent finished, a notification…) say here how long they last.
#[tauri::command]
pub fn audiolink_own_sound(ms: u64) {
    own_sound(ms);
}

/// Whether this computer is really playing, told by the sound itself, chunk by chunk. A media player's own status
/// cannot be trusted for this: some players only mirror another device (Sonora shows what plays on the phone's
/// Spotify, KDE Connect shows the phone's media) and report "playing" without making any sound here.
pub struct SoundWatch {
    playing: bool,
    /// When the current sound (gaps under `SOUND_GAP_MS` included) began.
    since: Option<u64>,
    last_loud: Option<u64>,
    /// Whether the player check was already made for the current sound (it is asked once per sound).
    hint_asked: bool,
}

impl SoundWatch {
    pub const fn new() -> Self {
        SoundWatch { playing: false, since: None, last_loud: None, hint_asked: false }
    }

    pub fn is_playing(&self) -> bool {
        self.playing
    }

    /// One 20 ms chunk at `now_ms`; `loud` is whether it holds sound. Returns the new state when it changes.
    /// `player_playing` says whether a media player that makes sound here reports "playing"; it is only called
    /// (once per sound) when the sound has gone on `FAST_AFTER_MS`, and then the computer counts as playing at once
    /// instead of after `PLAYING_AFTER_MS`. A mirror (Sonora, KDE Connect) makes no sound, so it cannot trigger this.
    pub fn feed(&mut self, now_ms: u64, loud: bool, player_playing: impl FnOnce() -> bool) -> Option<bool> {
        if loud {
            // A silence longer than the allowed gap ended the previous sound: this chunk starts a new one.
            if self.last_loud.map_or(true, |l| now_ms - l > SOUND_GAP_MS) {
                self.since = Some(now_ms);
                self.hint_asked = false;
            }
            self.last_loud = Some(now_ms);
            // How long it has gone on is measured to its last chunk with sound, so two short sounds never add up.
            if !self.playing {
                let lasted = now_ms - self.since.unwrap_or(now_ms);
                let confirmed = lasted >= PLAYING_AFTER_MS
                    || (lasted >= FAST_AFTER_MS && !std::mem::replace(&mut self.hint_asked, true) && player_playing());
                if confirmed {
                    self.playing = true;
                    return Some(true);
                }
            }
        } else if self.playing && self.last_loud.map_or(true, |l| now_ms - l >= PLAYING_ENDS_MS) {
            self.playing = false;
            self.since = None;
            return Some(false);
        }
        None
    }
}

/// What to tell the person when the phone could not be reached to ask it for its part of a swap.
pub fn swap_note(to: &SwapTo, delivered: bool) -> Option<&'static str> {
    if delivered {
        return None;
    }
    Some(match to {
        SwapTo::Pc => {
            "The phone is not connected to the laptop, so it could not be asked: disconnect the headphones on the phone yourself. \
             The laptop keeps trying to connect them for a minute."
        }
        SwapTo::Phone => {
            "The headphones are let go, but the phone could not be asked: connect them on the phone yourself."
        }
    })
}

/// Where the headphones are being moved to by hand.
#[derive(Debug, PartialEq, Eq)]
pub enum SwapTo {
    Pc,
    Phone,
}

impl SwapTo {
    pub fn parse(s: &str) -> Option<SwapTo> {
        match s {
            "pc" => Some(SwapTo::Pc),
            "phone" => Some(SwapTo::Phone),
            _ => None,
        }
    }
}

/// What the "phone is gone" watcher knows (see `audiolink_autopc`): updated after every step of the state machine.
#[cfg(target_os = "linux")]
static AUTOPC: Mutex<crate::audiolink_autopc::Tracker> = Mutex::new(crate::audiolink_autopc::Tracker::new());

/// Milliseconds on a clock that only this module compares with itself.
#[cfg(target_os = "linux")]
fn mono_ms() -> u64 {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    START.get_or_init(std::time::Instant::now).elapsed().as_millis() as u64
}

/// The user's switch for bringing the headphones back by themselves: on unless this file exists.
#[cfg(target_os = "linux")]
fn autopc_enabled() -> bool {
    !crate::platform::config_dir().join("audiolink-autopc.off").exists()
}

/// Every few seconds: if the phone is gone, bring the headphones to this computer (the same thing as the manual
/// "Headphones to this computer", see `audiolink_autopc` for when).
#[cfg(target_os = "linux")]
fn autopc_watch() {
    loop {
        std::thread::sleep(Duration::from_secs(5));
        if !ENABLED.load(Ordering::Relaxed) {
            continue;
        }
        let go = {
            let mut t = AUTOPC.lock().unwrap();
            let now = mono_ms();
            let go = t.should_try(autopc_enabled(), SWAPPING.load(Ordering::Relaxed), now);
            if go {
                t.tried(now);
            }
            go
        };
        if go {
            crate::log::line("audiolink: the phone is gone, bringing the headphones to this computer");
            // The phone cannot be asked to let go (it is off), so the answer is always the "could not tell the phone"
            // note: nothing to show. The connecting itself runs on its own thread.
            let _ = swap("pc");
        }
    }
}

/// True while a manual swap is in progress: the speakers must not take over in the middle of it.
#[cfg(target_os = "linux")]
static SWAPPING: AtomicBool = AtomicBool::new(false);

/// Waits out the grace period (and any swap in progress), then says it is over.
#[cfg(target_os = "linux")]
fn wait_grace(n: u64) {
    std::thread::sleep(GRACE);
    while SWAPPING.load(Ordering::Relaxed) {
        std::thread::sleep(Duration::from_millis(500));
    }
    // A wait that was cancelled or replaced in the meantime must not end anything.
    if GRACE_CLOCK.lock().unwrap().is_current(n) {
        send_event(Event::GraceExpired);
    }
}

/// Moves the headphones by hand. To the computer: the phone is asked to let go (a notification, because Android
/// will not do it silently) and the computer keeps trying to connect them for a minute. Back to the phone: the
/// computer lets go and the phone is asked to connect them.
#[cfg(target_os = "linux")]
fn swap(to: &str) -> Result<(), String> {
    use crate::audiolink_bt::{bluez_connect, bluez_disconnect, find_headphones_mac};
    use crate::audiolink_wire::{encode, FrameKind, SWAP_CONNECT, SWAP_RELEASE};
    use crate::log;

    if !ENABLED.load(Ordering::Relaxed) {
        return Err("turn on Phone headphones first".into());
    }
    let target = SwapTo::parse(to).ok_or("unknown target")?;
    let mac = find_headphones_mac().ok_or("no paired Bluetooth headphones found (is Bluetooth on?)")?;
    match target {
        SwapTo::Pc => {
            if SWAPPING.swap(true, Ordering::SeqCst) {
                return Err("a swap is already in progress".into());
            }
            let delivered = send_control(encode(FrameKind::SwapPrompt, &[SWAP_RELEASE]));
            std::thread::spawn(move || {
                for _ in 0..20 {
                    if bluez_connect(&mac).is_ok() {
                        log::line("audiolink: the headphones are on this computer");
                        send_event(Event::Headphones(crate::audiolink_logic::Where::Pc));
                        // The relay's restore puts the speakers back as the default; the person asked for the
                        // headphones, so give PipeWire a moment to make their sink, then point the sound at it.
                        std::thread::sleep(Duration::from_millis(700));
                        if !crate::audiolink_sink::default_to_bluez(&mac) {
                            log::line("audiolink: the headphones' sink did not show up; the sound stays where it was");
                        }
                        break;
                    }
                    std::thread::sleep(Duration::from_secs(3));
                }
                SWAPPING.store(false, Ordering::SeqCst);
            });
            swap_note(&SwapTo::Pc, delivered).map_or(Ok(()), |n| Err(n.to_string()))
        }
        SwapTo::Phone => {
            // Whatever is playing here would come out of the speakers the moment the headphones let go.
            crate::audiolink_players::pause_all_playing();
            bluez_disconnect(&mac)?;
            send_event(Event::Headphones(crate::audiolink_logic::Where::None));
            let delivered = send_control(encode(FrameKind::SwapPrompt, &[SWAP_CONNECT]));
            swap_note(&SwapTo::Phone, delivered).map_or(Ok(()), |n| Err(n.to_string()))
        }
    }
}

/// "Headphones to this computer" / "back to the phone", from Settings.
#[tauri::command]
pub async fn audiolink_swap(to: String) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        tauri::async_runtime::spawn_blocking(move || swap(&to)).await.map_err(|e| e.to_string())?
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = to;
        Err("the audio relay is only on Linux".into())
    }
}

/// The one thread that owns the state: events in, actions out, in order.
#[cfg(target_os = "linux")]
fn engine(app: AppHandle, rx: std::sync::mpsc::Receiver<Event>) {
    use crate::log;
    let mut state = State::default();
    while let Ok(ev) = rx.recv() {
        log::line(format!("audiolink: event {ev:?} (was {:?})", state.mode));
        let (next, actions) = step(&state, ev);
        if next.mode != state.mode {
            log::line(format!("audiolink: {:?} -> {:?}", state.mode, next.mode));
        }
        let wait = GRACE_CLOCK.lock().unwrap().update(&state, &next);
        if let Some(n) = wait {
            std::thread::spawn(move || wait_grace(n));
        }
        state = next;
        AUTOPC.lock().unwrap().observe(&state, mono_ms());
        for a in actions {
            apply(&app, a);
        }
    }
}

/// Process id of the `parec` that is feeding the phone, if one is running.
#[cfg(target_os = "linux")]
static CAPTURE_PID: Mutex<Option<u32>> = Mutex::new(None);

/// Carries out what the logic asked for.
#[cfg(target_os = "linux")]
fn apply(_app: &AppHandle, action: Action) {
    use crate::{audiolink_sink as sink, log};

    log::line(format!("audiolink: {action:?}"));
    match action {
        Action::SinkVirtual => match sink::sink_virtual() {
            Ok(()) => sink::spawn_watchdog(),
            Err(e) => log::line(format!("audiolink: no silent sink ({e})")),
        },
        Action::SinkRestore => {
            if let Err(e) = sink::sink_restore() {
                log::line(format!("audiolink: restore: {e}"));
            }
            sink::stop_watchdog();
        }
        Action::RelayStart => start_capture(),
        Action::RelayStop => stop_capture(),
        // Off the engine thread: a player that does not answer must not hold up the stream.
        Action::PausePc => {
            std::thread::spawn(|| {
                let names = crate::audiolink_players::pause_all_playing();
                crate::log::line(format!("audiolink: paused {} player(s) here: {}", names.len(), names.join(", ")));
            });
        }
        // The phone cannot tell when the computer starts playing, so the computer asks it to pause.
        Action::PausePhone => {
            send_control(crate::audiolink_wire::encode(crate::audiolink_wire::FrameKind::PausePhone, &[]));
        }
    }
}

/// Reads the silent sink's monitor and hands 20 ms chunks to the phone, on its own thread.
#[cfg(target_os = "linux")]
fn start_capture() {
    use crate::audiolink_sink::{capture_start, CHUNK};
    use crate::audiolink_wire::{encode, FrameKind};

    if CAPTURE_PID.lock().unwrap().is_some() {
        return;
    }
    let mut capture = match capture_start() {
        Ok(c) => c,
        Err(e) => {
            crate::log::line(format!("audiolink: no capture ({e})"));
            return;
        }
    };
    let pid = capture.pid();
    *CAPTURE_PID.lock().unwrap() = Some(pid);
    std::thread::spawn(move || {
        let mut chunk = [0u8; CHUNK];
        let mut gate = SilenceGate::new();
        let mut watch = SoundWatch::new();
        let mut leveler = crate::audiolink_dsp::Leveler::new();
        let started = std::time::Instant::now();
        // A blocking read: it sleeps until parec has sound to give, and ends when parec is stopped. Silence
        // is not sent (past a short tail): a quiet laptop costs the network and the phone nothing.
        while let Ok(true) = capture.read_chunk(&mut chunk) {
            // Whether this computer is playing is told by the sound itself, not by a player's status (see SoundWatch).
            if let Some(playing) = watch.feed(started.elapsed().as_millis() as u64, !is_silent(&chunk) && !own_sound_now(), crate::audiolink_players::non_mirror_playing) {
                send_event(Event::PcMedia(playing));
            }
            if gate.pass(&chunk) {
                let mut out = chunk;
                leveler.process(&mut out);
                send_frame(encode(FrameKind::Audio, &out));
            }
        }
        if watch.is_playing() {
            send_event(Event::PcMedia(false));
        }
        let mut current = CAPTURE_PID.lock().unwrap();
        if *current == Some(pid) {
            *current = None;
        }
    });
}

#[cfg(target_os = "linux")]
fn stop_capture() {
    if let Some(pid) = CAPTURE_PID.lock().unwrap().take() {
        // parec ends, the blocked read returns, the thread finishes.
        unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM) };
    }
}

/// The audio socket, next to the phone link's.
#[cfg(target_os = "linux")]
pub fn socket_path() -> Option<std::path::PathBuf> {
    crate::platform::relay_socket_path().map(|p| p.with_file_name("coucou-audio.sock"))
}

#[cfg(target_os = "linux")]
async fn serve(app: AppHandle) {
    use std::os::unix::fs::PermissionsExt;
    use tokio::net::UnixListener;

    use crate::log;

    let Some(path) = socket_path() else {
        log::line("audiolink: no private runtime directory");
        STARTED.store(false, Ordering::SeqCst);
        return;
    };
    if path.exists() {
        if std::os::unix::net::UnixStream::connect(&path).is_ok() {
            log::line("audiolink: another Coucou already serves it");
            return;
        }
        let _ = std::fs::remove_file(&path);
    }
    let listener = match UnixListener::bind(&path) {
        Ok(l) => l,
        Err(e) => {
            log::line(format!("audiolink: cannot open the socket ({e})"));
            STARTED.store(false, Ordering::SeqCst);
            return;
        }
    };
    let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    log::line("audiolink: ready");
    let uid = unsafe { libc::getuid() };
    loop {
        let Ok((stream, _)) = listener.accept().await else {
            tokio::time::sleep(Duration::from_millis(200)).await;
            continue;
        };
        if !matches!(stream.peer_cred(), Ok(c) if c.uid() == uid) {
            continue;
        }
        tauri::async_runtime::spawn(client(app.clone(), stream));
    }
}

#[cfg(target_os = "linux")]
async fn client(_app: AppHandle, stream: tokio::net::UnixStream) {
    use tokio::io::AsyncWriteExt;

    use crate::audiolink_wire::{encode, FrameKind, HELLO_JSON};
    use crate::log;

    if !ENABLED.load(Ordering::Relaxed) {
        return;
    }
    let Ok(std_stream) = stream.into_std() else { return };
    let Ok(kill) = std_stream.try_clone() else { return };
    let Ok(stream) = tokio::net::UnixStream::from_std(std_stream) else { return };
    let (mut rd, mut wr) = stream.into_split();
    let (out, mut out_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(OUT_CAPACITY);

    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let older = CURRENT.lock().unwrap().take_over(id, Conn { out, kill });
    if let Some(old) = older {
        let _ = old.kill.shutdown(std::net::Shutdown::Both);
    }
    log::line("audiolink: a phone connected");

    // Writes what is handed to it; when nothing is for a second, a heartbeat, so the phone knows we live.
    let writer = tauri::async_runtime::spawn(async move {
        if wr.write_all(&encode(FrameKind::Hello, HELLO_JSON.as_bytes())).await.is_err() {
            return;
        }
        loop {
            let bytes = match tokio::time::timeout(Duration::from_secs(1), out_rx.recv()).await {
                Ok(Some(b)) => b,
                Ok(None) => break,
                Err(_) => encode(FrameKind::Heartbeat, &[]),
            };
            if wr.write_all(&bytes).await.is_err() || wr.flush().await.is_err() {
                break;
            }
        }
        let _ = wr.shutdown().await;
    });

    send_event(Event::Link(true));
    let end = pump(&mut rd, SILENCE, |msg| {
        // What the phone plays, for the island and the wallpaper. Only the current connection speaks for the phone.
        if matches!(msg, PhoneMsg::Now { .. } | PhoneMsg::Art { .. } | PhoneMsg::Caps) && CURRENT.lock().unwrap().is_current(id) {
            if msg == PhoneMsg::Caps {
                crate::phonenow::phone_can_read(true);
            } else {
                crate::phonenow::apply(&msg);
            }
        }
        for ev in events_of(&msg) {
            send_event(ev);
        }
    })
    .await;
    writer.abort();
    // A replaced connection ending is not the phone leaving: only the current one speaks for the link.
    if CURRENT.lock().unwrap().release(id) {
        crate::phonenow::clear();
        send_event(Event::Link(false));
    }
    log::line(format!("audiolink: the phone's connection ended ({end:?})"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audiolink_logic::Where;
    use tokio::io::AsyncWriteExt;

    fn run<F: std::future::Future>(f: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread().enable_time().build().unwrap().block_on(f)
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_stale_grace_timer_cannot_cut_a_newer_blip_short() {
        let mut clock = GraceClock::new();
        let first = clock.update(&relaying(), &held(Where::None)).expect("a blip starts a wait");
        // They come back: the wait no longer counts.
        assert_eq!(clock.update(&held(Where::None), &relaying()), None);
        assert!(!clock.is_current(first));
        // They drop again: a new wait, and the first timer firing now is ignored.
        let second = clock.update(&relaying(), &held(Where::None)).expect("a second blip starts a second wait");
        assert_ne!(first, second);
        assert!(!clock.is_current(first));
        assert!(clock.is_current(second));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn the_wait_stays_valid_while_the_headphones_stay_gone() {
        let mut clock = GraceClock::new();
        let n = clock.update(&relaying(), &held(Where::None)).unwrap();
        assert_eq!(clock.update(&held(Where::None), &held(Where::None)), None);
        assert!(clock.is_current(n));
    }

    #[test]
    fn a_state_line_applies_the_call_before_the_headphones() {
        use crate::audiolink_logic::Event;
        let msg = PhoneMsg::State { headphones: Where::Phone, call: true, phone_playing: false };
        assert_eq!(events_of(&msg), vec![Event::Call(true), Event::Headphones(Where::Phone), Event::PhoneMedia(false)]);
        assert_eq!(events_of(&PhoneMsg::Heartbeat), vec![]);
    }

    #[test]
    fn audio_cannot_use_the_slots_kept_for_control_frames() {
        let (tx, _rx) = tokio::sync::mpsc::channel::<Vec<u8>>(OUT_CAPACITY);
        let audio = (0..OUT_CAPACITY).take_while(|_| try_audio(&tx, vec![0])).count();
        assert_eq!(audio, OUT_CAPACITY - CONTROL_RESERVE);
        // A pause or a swap prompt still gets through while audio is backed up.
        let control = (0..CONTROL_RESERVE).take_while(|_| tx.try_send(vec![1]).is_ok()).count();
        assert_eq!(control, CONTROL_RESERVE);
    }

    #[test]
    fn silence_is_samples_within_the_peak() {
        assert!(is_silent(&[0u8; 3840]));
        let faint: Vec<u8> = (0..1920).flat_map(|i| (if i % 2 == 0 { 3i16 } else { -3i16 }).to_le_bytes()).collect();
        assert!(is_silent(&faint));
        let loud: Vec<u8> = (0..1920).flat_map(|_| 1000i16.to_le_bytes()).collect();
        assert!(!is_silent(&loud));
        let mut one = vec![0u8; 3840];
        one[1000..1002].copy_from_slice(&(-900i16).to_le_bytes());
        assert!(!is_silent(&one), "a single loud sample is sound");
    }

    #[test]
    fn the_gate_sends_a_tail_of_silence_then_nothing() {
        let quiet = [0u8; 3840];
        let loud: Vec<u8> = (0..1920).flat_map(|_| 1000i16.to_le_bytes()).collect();
        let mut gate = SilenceGate::new();
        assert!(gate.pass(&loud));
        let sent = (0..200).filter(|_| gate.pass(&quiet)).count();
        assert_eq!(sent as u32, SILENCE_TAIL);
        assert!(!gate.pass(&quiet));
        // Sound again: through, and the tail starts over.
        assert!(gate.pass(&loud));
        assert!(gate.pass(&quiet));
    }

    fn samples(chunk: &[u8]) -> Vec<i16> {
        chunk.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect()
    }

    fn bytes(samples: &[i16]) -> Vec<u8> {
        samples.iter().flat_map(|s| s.to_le_bytes()).collect()
    }

    /// Feeds `ms` of chunks (20 ms apart) from `from`, `loud` or silent; returns every change reported.
    fn feed_for(w: &mut SoundWatch, from: u64, ms: u64, loud: bool) -> Vec<(u64, bool)> {
        feed_for_hint(w, from, ms, loud, &|| false)
    }

    /// Like `feed_for`, with a media player check (`hint`) available to the watcher.
    fn feed_for_hint(w: &mut SoundWatch, from: u64, ms: u64, loud: bool, hint: &dyn Fn() -> bool) -> Vec<(u64, bool)> {
        (0..=ms / 20).filter_map(|i| w.feed(from + i * 20, loud, hint).map(|b| (from + i * 20, b))).collect()
    }

    #[test]
    fn a_player_that_says_it_is_playing_makes_it_count_after_a_fraction_of_a_second() {
        let mut w = SoundWatch::new();
        let changes = feed_for_hint(&mut w, 0, 2_000, true, &|| true);
        assert_eq!(changes, vec![(FAST_AFTER_MS, true)]);
    }

    #[test]
    fn without_a_player_saying_so_it_still_takes_three_seconds() {
        let mut w = SoundWatch::new();
        let changes = feed_for_hint(&mut w, 0, 4_000, true, &|| false);
        assert_eq!(changes, vec![(PLAYING_AFTER_MS, true)]);
    }

    #[test]
    fn the_player_is_checked_once_per_sound_and_never_for_a_blip() {
        let calls = std::cell::Cell::new(0);
        let ask = || {
            calls.set(calls.get() + 1);
            false
        };
        let mut w = SoundWatch::new();
        feed_for_hint(&mut w, 0, 500, true, &ask); // shorter than FAST_AFTER_MS: no check at all
        assert_eq!(calls.get(), 0);
        let mut w = SoundWatch::new();
        feed_for_hint(&mut w, 0, 2_500, true, &ask); // a long sound: one check, not one per chunk
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn a_new_sound_after_a_pause_is_checked_again() {
        let calls = std::cell::Cell::new(0);
        let ask = || {
            calls.set(calls.get() + 1);
            false
        };
        let mut w = SoundWatch::new();
        feed_for_hint(&mut w, 0, 1_500, true, &ask);
        feed_for_hint(&mut w, 1_520, 1_500, false, &ask); // more than a second of silence ends it
        feed_for_hint(&mut w, 3_040, 1_500, true, &ask);
        assert_eq!(calls.get(), 2);
    }

    #[test]
    fn a_ping_or_a_click_is_not_the_computer_playing() {
        let mut w = SoundWatch::new();
        assert!(feed_for(&mut w, 0, 2_900, true).is_empty());
        assert!(feed_for(&mut w, 2_920, 8_000, false).is_empty());
    }

    #[test]
    fn sound_that_goes_on_makes_the_computer_playing_exactly_once() {
        let mut w = SoundWatch::new();
        let changes = feed_for(&mut w, 0, 10_000, true);
        assert_eq!(changes, vec![(PLAYING_AFTER_MS, true)]);
    }

    #[test]
    fn small_gaps_inside_music_do_not_break_it() {
        let mut w = SoundWatch::new();
        let mut changes = Vec::new();
        // 800 ms of sound, 200 ms of silence, over and over: music with breathing room.
        for second in 0..6u64 {
            changes.extend(feed_for(&mut w, second * 1_000, 780, true));
            changes.extend(feed_for(&mut w, second * 1_000 + 800, 180, false));
        }
        assert_eq!(changes.len(), 1);
        assert!(changes[0].1 && changes[0].0 <= 3_200, "{changes:?}");
    }

    #[test]
    fn two_short_sounds_with_a_pause_between_are_not_one_long_one() {
        let mut w = SoundWatch::new();
        let mut changes = feed_for(&mut w, 0, 2_000, true);
        changes.extend(feed_for(&mut w, 2_020, 1_200, false)); // a gap of more than a second ends the first sound
        changes.extend(feed_for(&mut w, 3_240, 2_000, true));
        assert!(changes.is_empty(), "{changes:?}");
    }

    #[test]
    fn playing_ends_after_three_seconds_of_silence() {
        let mut w = SoundWatch::new();
        feed_for(&mut w, 0, 4_000, true);
        assert!(feed_for(&mut w, 4_020, 2_900, false).is_empty(), "less than three seconds of silence is still playing");
        let ended = feed_for(&mut w, 6_940, 400, false);
        assert_eq!(ended.len(), 1);
        assert!(!ended[0].1);
    }

    #[test]
    fn a_ping_after_playing_ended_does_not_start_it_again() {
        let mut w = SoundWatch::new();
        feed_for(&mut w, 0, 4_000, true);
        feed_for(&mut w, 4_020, 3_100, false);
        assert!(feed_for(&mut w, 7_140, 600, true).is_empty());
    }

    #[test]
    fn a_swap_says_so_when_the_phone_could_not_be_asked() {
        assert_eq!(swap_note(&SwapTo::Pc, true), None);
        assert_eq!(swap_note(&SwapTo::Phone, true), None);
        assert!(swap_note(&SwapTo::Pc, false).unwrap().contains("disconnect"));
        assert!(swap_note(&SwapTo::Phone, false).unwrap().contains("connect"));
    }

    #[test]
    fn swap_target_is_pc_or_phone_and_nothing_else() {
        assert_eq!(SwapTo::parse("pc"), Some(SwapTo::Pc));
        assert_eq!(SwapTo::parse("phone"), Some(SwapTo::Phone));
        assert_eq!(SwapTo::parse("PC"), None);
        assert_eq!(SwapTo::parse("pc; reboot"), None);
        assert_eq!(SwapTo::parse(""), None);
    }

    #[cfg(target_os = "linux")]
    fn held(headphones: Where) -> State {
        State { mode: crate::audiolink_logic::Mode::Hold, headphones, ..State::default() }
    }

    #[cfg(target_os = "linux")]
    fn relaying() -> State {
        State { mode: crate::audiolink_logic::Mode::Relay, headphones: Where::Phone, ..State::default() }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn the_wait_starts_when_the_headphones_leave_the_phone() {
        assert!(needs_grace(&relaying(), &held(Where::None)));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn the_wait_is_not_started_twice() {
        // Already held with the headphones gone: another event in the same state must not stack a second timer.
        assert!(!needs_grace(&held(Where::None), &held(Where::None)));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_hold_with_the_headphones_still_on_the_phone_has_no_wait() {
        // A lost link or a call: the speakers stay silent for as long as it lasts.
        assert!(!needs_grace(&relaying(), &held(Where::Phone)));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn the_wait_starts_when_a_held_relay_loses_the_headphones() {
        assert!(needs_grace(&held(Where::Phone), &held(Where::None)));
    }

    #[test]
    fn second_connection_replaces_first() {
        let mut slot: Slot<&str> = Slot::new();
        assert_eq!(slot.take_over(1, "first"), None);
        assert_eq!(slot.take_over(2, "second"), Some("first"));
        // The first one ending later is not the phone leaving.
        assert!(!slot.release(1));
        assert!(slot.release(2));
        assert!(!slot.release(2));
    }

    #[test]
    fn silent_connection_times_out() {
        let end = run(async {
            let (mut phone, mut laptop) = tokio::io::duplex(1024);
            phone.write_all(b"{\"op\":\"hb\"}\n").await.unwrap();
            let mut seen = Vec::new();
            let end = pump(&mut laptop, Duration::from_millis(150), |m| seen.push(m)).await;
            assert_eq!(seen, vec![PhoneMsg::Heartbeat]);
            end
        });
        assert_eq!(end, End::Silent);
    }

    #[test]
    fn a_hung_up_connection_ends_closed_after_its_last_line() {
        let (end, seen) = run(async {
            let (mut phone, mut laptop) = tokio::io::duplex(1024);
            phone
                .write_all(b"{\"op\":\"state\",\"headphones\":\"phone\",\"call\":false,\"phone_playing\":true}\n")
                .await
                .unwrap();
            drop(phone);
            let mut seen = Vec::new();
            let end = pump(&mut laptop, Duration::from_secs(5), |m| seen.push(m)).await;
            (end, seen)
        });
        assert_eq!(end, End::Closed);
        assert_eq!(seen, vec![PhoneMsg::State { headphones: Where::Phone, call: false, phone_playing: true }]);
    }

    #[test]
    fn a_line_that_never_ends_is_dropped_not_buffered() {
        let end = run(async {
            let (mut phone, mut laptop) = tokio::io::duplex(8192);
            phone.write_all(&vec![b'x'; 2000]).await.unwrap();
            pump(&mut laptop, Duration::from_secs(5), |_| {}).await
        });
        assert_eq!(end, End::Bad);
    }

    #[test]
    fn a_line_the_protocol_does_not_know_drops_the_connection() {
        let end = run(async {
            let (mut phone, mut laptop) = tokio::io::duplex(1024);
            phone.write_all(b"{\"op\":\"format_disk\"}\n").await.unwrap();
            pump(&mut laptop, Duration::from_secs(5), |_| panic!("no message should come out")).await
        });
        assert_eq!(end, End::Bad);
    }
}
