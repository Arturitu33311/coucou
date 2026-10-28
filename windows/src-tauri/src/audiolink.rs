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

    use crate::audiolink_wire::{parse_phone_line, MAX_LINE};

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
        // No newline yet and already longer than any line may be: a flood, not a message.
        if buf.len() > MAX_LINE {
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

/// Writes one frame to the phone if one is connected. False when nobody is there or it is too slow.
#[cfg(target_os = "linux")]
pub fn send_frame(bytes: Vec<u8>) -> bool {
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
        let app = app.clone();
        tauri::async_runtime::spawn(async move { serve(app).await });
    }
    send_event(Event::Enabled(on));
    if !on {
        if let Some(c) = CURRENT.lock().unwrap().take() {
            let _ = c.kill.shutdown(std::net::Shutdown::Both);
        }
    }
}

#[cfg(not(target_os = "linux"))]
pub fn sync_enabled(_app: &AppHandle, _on: bool) {}

/// The one thread that owns the state: events in, actions out, in order.
#[cfg(target_os = "linux")]
fn engine(app: AppHandle, rx: std::sync::mpsc::Receiver<Event>) {
    use crate::log;
    let mut state = State::default();
    while let Ok(ev) = rx.recv() {
        let (next, actions) = step(&state, ev);
        if next.mode != state.mode {
            log::line(format!("audiolink: {:?} -> {:?}", state.mode, next.mode));
        }
        state = next;
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
        // Pausing the players is Task 5; pausing the phone is the phone's own business.
        Action::PausePc | Action::PausePhone => {}
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
        // A blocking read: it sleeps until parec has sound to give, and ends when parec is stopped.
        while let Ok(true) = capture.read_chunk(&mut chunk) {
            send_frame(encode(FrameKind::Audio, &chunk));
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
    let (out, mut out_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(64);

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
        if let PhoneMsg::State { headphones, call, phone_playing } = msg {
            send_event(Event::Headphones(headphones));
            send_event(Event::Call(call));
            send_event(Event::PhoneMedia(phone_playing));
        }
    })
    .await;
    writer.abort();
    // A replaced connection ending is not the phone leaving: only the current one speaks for the link.
    if CURRENT.lock().unwrap().release(id) {
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
