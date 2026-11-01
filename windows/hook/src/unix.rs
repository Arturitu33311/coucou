//! The Linux transport: a Unix socket in the user's runtime directory.
//!
//! `$XDG_RUNTIME_DIR` is private to the user (mode 0700), so nobody else can
//! even reach the socket. We still check, once connected, that the process on
//! the other end runs as us — the same promise the Windows relay makes with the
//! pipe server's SID, for the same price.

use std::io;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::io::{AsRawFd, FromRawFd};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::CONNECT_TIMEOUT;

/// `$XDG_RUNTIME_DIR/coucou.sock`, or `/run/user/<uid>/coucou.sock` when the
/// variable is missing (a hook started from a stripped-down environment). The
/// directory must be ours and closed to everyone else, or there is no relay.
/// Must match `platform::relay_socket_path()` in the app exactly.
fn socket_path() -> Option<PathBuf> {
    private_runtime_dir().map(|dir| dir.join("coucou.sock"))
}

/// The user's runtime directory, if it is ours and closed to everyone else.
fn private_runtime_dir() -> Option<PathBuf> {
    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| PathBuf::from(format!("/run/user/{}", unsafe { libc::getuid() })));
    is_private_dir(&dir).then_some(dir)
}

/// A real directory (not a symlink), owned by us, no access for group or others.
fn is_private_dir(dir: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    std::fs::symlink_metadata(dir)
        .map(|m| {
            m.file_type().is_dir() && m.uid() == unsafe { libc::getuid() } && m.mode() & 0o077 == 0
        })
        .unwrap_or(false)
}

/// Opens the socket. Retries only while the app's backlog is full: any other
/// error — no socket, nobody listening — means there is nothing to talk to, and
/// waiting would only delay Claude Code.
pub fn connect() -> Option<UnixStream> {
    let path = socket_path()?;
    let deadline = Instant::now() + CONNECT_TIMEOUT;
    loop {
        match try_connect(&path) {
            // Somebody else's server on our socket gets nothing from us.
            Ok(stream) => return server_is_same_user(&stream).then_some(stream),
            Err(err) => {
                if err.raw_os_error() != Some(libc::EAGAIN) || Instant::now() >= deadline {
                    return None;
                }
                std::thread::sleep(Duration::from_millis(15));
            }
        }
    }
}

/// A non-blocking connect, so a full backlog answers EAGAIN at once instead of
/// parking us until the app gets round to accepting. Blocking again afterwards:
/// the main thread's budget bounds every read and write.
fn try_connect(path: &Path) -> io::Result<UnixStream> {
    let fd = unsafe {
        libc::socket(libc::AF_UNIX, libc::SOCK_STREAM | libc::SOCK_CLOEXEC | libc::SOCK_NONBLOCK, 0)
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // Owns the descriptor from here on, so every early return closes it.
    let stream = unsafe { UnixStream::from_raw_fd(fd) };

    let mut addr: libc::sockaddr_un = unsafe { std::mem::zeroed() };
    addr.sun_family = libc::AF_UNIX as libc::sa_family_t;
    let bytes = path.as_os_str().as_bytes();
    // Room for the terminating NUL, which zeroed() already put there.
    if bytes.len() >= addr.sun_path.len() {
        return Err(io::Error::from(io::ErrorKind::InvalidInput));
    }
    for (dst, src) in addr.sun_path.iter_mut().zip(bytes) {
        *dst = *src as libc::c_char;
    }

    let rc = unsafe {
        libc::connect(
            fd,
            &addr as *const libc::sockaddr_un as *const libc::sockaddr,
            std::mem::size_of::<libc::sockaddr_un>() as libc::socklen_t,
        )
    };
    if rc != 0 {
        return Err(io::Error::last_os_error());
    }
    stream.set_nonblocking(false)?;
    Ok(stream)
}

/// True when the process serving the socket runs as the same user we do.
/// A failure to answer is treated as "not ours", as on Windows.
fn server_is_same_user(stream: &UnixStream) -> bool {
    let mut cred: libc::ucred = unsafe { std::mem::zeroed() };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    let rc = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            &mut cred as *mut libc::ucred as *mut libc::c_void,
            &mut len,
        )
    };
    rc == 0
        && len as usize == std::mem::size_of::<libc::ucred>()
        && cred.uid == unsafe { libc::getuid() }
}

/// `coucou-hook --remote`: what a phone's ssh key is allowed to run, and the only thing it can run.
/// It joins the ssh channel (stdin and stdout) to Coucou's phone-link socket, line for line, and
/// does nothing else: no shell, no arguments, no file. If Coucou is closed, or the phone link is
/// off, it says so in one line and ends.
pub fn run_remote() -> ! {
    pipe_to("coucou-remote.sock")
}

/// `coucou-hook --audio`: what the phone's second ssh key (audio only) is allowed to run. Same pipe as
/// `--remote`, to the audio relay's own socket, so losing that key loses the audio and nothing else.
pub fn run_audio() -> ! {
    pipe_to("coucou-audio.sock")
}

/// `coucou-hook --audio-watchdog`: started by Coucou while the audio relay has the computer's output on
/// its silent sink, with a pipe on stdin that Coucou keeps open. It does nothing until that pipe closes
/// (Coucou quit, was killed, crashed) and then puts the output back where the marker file says it was.
/// It only ever runs `pactl`, with names it has checked.
pub fn run_audio_watchdog() -> ! {
    use std::io::Read;

    let mut sink = [0u8; 64];
    let mut stdin = io::stdin();
    while let Ok(n) = stdin.read(&mut sink) {
        if n == 0 {
            break;
        }
    }
    restore_audio_sink();
    std::process::exit(0)
}

const AUDIO_SINK: &str = "coucou_audiolink";

/// `<local dir>/audiolink-sink.json`; must match `settings::local_dir()` in the app.
fn audio_marker_path() -> Option<PathBuf> {
    let data = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))?;
    Some(data.join("coucou").join("audiolink-sink.json"))
}

fn pactl(args: &[&str]) -> String {
    std::process::Command::new("pactl")
        .args(args)
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default()
}

/// The first column of each line of a `pactl list short …` whose second column satisfies `pick`.
fn short_column(text: &str, pick: impl Fn(&str) -> bool) -> Vec<u32> {
    text.lines()
        .filter_map(|l| {
            let mut f = l.split('\t');
            let first: u32 = f.next()?.trim().parse().ok()?;
            pick(f.next()?).then_some(first)
        })
        .collect()
}

fn restore_audio_sink() {
    let Some(path) = audio_marker_path() else { return };
    let Ok(text) = std::fs::read_to_string(&path) else { return };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else { return };
    let Some(previous) = v["previous"]
        .as_str()
        .filter(|p| !p.is_empty() && *p != AUDIO_SINK && !p.chars().any(|c| c.is_whitespace() || c.is_control()))
    else {
        return;
    };
    let module = v["module"].as_u64().and_then(|m| u32::try_from(m).ok()).unwrap_or(0);

    // Where our sink is now: its streams are the ones to bring back.
    let ours = short_column(&pactl(&["list", "short", "sinks"]), |name| name == AUDIO_SINK).first().copied();
    pactl(&["set-default-sink", previous]);
    if let Some(sink) = ours {
        let inputs = pactl(&["list", "short", "sink-inputs"]);
        let on_ours: Vec<u32> = inputs
            .lines()
            .filter_map(|l| {
                let mut f = l.split('\t');
                let input: u32 = f.next()?.trim().parse().ok()?;
                (f.next()?.trim().parse::<u32>().ok()? == sink).then_some(input)
            })
            .collect();
        for input in on_ours {
            pactl(&["move-sink-input", &input.to_string(), previous]);
        }
    }
    // The module id may not have been written yet if the crash came in between: find it by its sink name.
    let module = if module > 0 {
        Some(module)
    } else {
        pactl(&["list", "short", "modules"])
            .lines()
            .find(|l| l.contains(&format!("sink_name={AUDIO_SINK}")))
            .and_then(|l| l.split('\t').next()?.trim().parse().ok())
    };
    if let Some(m) = module {
        pactl(&["unload-module", &m.to_string()]);
    }
    let _ = std::fs::remove_file(path);
}

fn pipe_to(socket_name: &str) -> ! {
    use std::io::{Read, Write};

    let path = private_runtime_dir().map(|d| d.join(socket_name));
    let stream = path.and_then(|p| UnixStream::connect(p).ok()).filter(server_is_same_user);
    let Some(stream) = stream else {
        println!("{{\"t\":\"offline\"}}");
        std::process::exit(0);
    };
    let Ok(mut to_app) = stream.try_clone() else { std::process::exit(1) };
    // Phone → Coucou. When the phone hangs up, say so to the app (it then closes its side).
    std::thread::spawn(move || {
        let mut chunk = [0u8; 4096];
        let mut stdin = std::io::stdin();
        while let Ok(n) = stdin.read(&mut chunk) {
            if n == 0 || to_app.write_all(&chunk[..n]).is_err() {
                break;
            }
        }
        let _ = to_app.shutdown(std::net::Shutdown::Write);
    });
    // Coucou → phone, flushed at every read: events must not sit in a buffer.
    let mut from_app = stream;
    let mut out = std::io::stdout();
    let mut chunk = [0u8; 4096];
    while let Ok(n) = from_app.read(&mut chunk) {
        if n == 0 || out.write_all(&chunk[..n]).is_err() || out.flush().is_err() {
            break;
        }
    }
    std::process::exit(0);
}
