// What is capturing the microphone, in two readings of the same sound-server listing:
//
//   * dictation: Handy (the speech-to-text app) is recording, so the island says "Listening…";
//   * the microphone indicator: some other program is recording, so the island shows a small dot
//     with the program's name (the top bar, where GNOME puts it, is hidden on this desktop).
//
// Both come from the sound server's own announcements (`pactl subscribe`): the listing is read
// only when something about a capture changes. Nothing polls: while nobody starts or stops a
// capture the process is asleep in a read and costs nothing.
//
// This is a privacy indicator, so it is built not to be talked out of showing:
//   * the exclusions (Handy, Coucou's own bars, monitors) are not taken from what a program says
//     about itself. A program is "Handy" only if the process behind the stream (the pid the
//     server itself recorded for the connection, `pipewire.sec.pid`) really is Handy; a monitor is
//     a stream whose *source* is a sink's monitor, not one that claims `stream.monitor`;
//   * when the server cannot be read the last state is kept, never cleared;
//   * names are what a program calls itself, so they are shown trimmed and without control
//     characters, as text.
//
// Only program names are sent to the page; no audio is ever opened or read here.

use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::island::WINDOW_LABEL;
use crate::log;

static STARTED: AtomicBool = AtomicBool::new(false);

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Dictation {
    pub recording: bool,
}

/// The programs recording the microphone, other than Handy.
#[derive(Serialize, Clone, Debug, PartialEq, Default)]
pub struct Mic {
    pub apps: Vec<String>,
}

/// What `pactl` said, three listings read together (all in the C locale, so the headings are the
/// same on every desktop language).
pub struct Snapshot {
    pub outputs: String,
    pub sources: String,
    pub clients: String,
}

/// Who runs a pid: the executable's file name, lower-cased.
pub type ExeOf<'a> = &'a dyn Fn(u32) -> Option<String>;

fn pactl(args: &[&str]) -> Option<String> {
    let out = Command::new("pactl")
        .args(args)
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// The three listings, or None if any of them cannot be read (then nothing is concluded).
pub fn snapshot() -> Option<Snapshot> {
    Some(Snapshot {
        outputs: pactl(&["list", "source-outputs"])?,
        sources: pactl(&["list", "sources", "short"])?,
        clients: pactl(&["list", "clients"])?,
    })
}

/// The real executable behind a pid (readable for the user's own processes), lower-cased.
pub fn exe_of_pid(pid: u32) -> Option<String> {
    let path = std::fs::read_link(format!("/proc/{pid}/exe")).ok()?;
    Some(path.file_name()?.to_string_lossy().to_lowercase())
}

/// One block of a `pactl list` (`Source Output #12`, `Client #3`).
struct Block<'a>(&'a str);

impl Block<'_> {
    fn id(&self) -> &str {
        self.0.lines().next().unwrap_or("").trim()
    }

    /// `key = "value"` in the properties.
    fn prop(&self, key: &str) -> Option<String> {
        self.0.lines().find_map(|l| {
            let (k, v) = l.trim().split_once('=')?;
            (k.trim() == key).then(|| v.trim().trim_matches('"').to_string())
        })
    }

    /// `Field: value` at the top of the block.
    fn field(&self, name: &str) -> Option<String> {
        self.0.lines().find_map(|l| {
            let (k, v) = l.trim().split_once(':')?;
            (k.trim() == name).then(|| v.trim().to_string())
        })
    }
}

fn blocks<'a>(listing: &'a str, heading: &str) -> Vec<Block<'a>> {
    listing.split(heading).skip(1).map(Block).collect()
}

/// A program's own name for itself, made safe to show: no control characters, one line, short.
pub fn clean_name(raw: &str) -> String {
    let one_line: String = raw.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    let squeezed = one_line.split_whitespace().collect::<Vec<_>>().join(" ");
    let cut: String = squeezed.chars().take(40).collect();
    if cut.is_empty() {
        "an app".to_string()
    } else {
        cut
    }
}

struct Stream {
    name: String,
    handy: bool,
    ours: bool,
    monitor: bool,
    corked: bool,
}

fn streams(snap: &Snapshot, exe_of: ExeOf) -> Vec<Stream> {
    streams_with(snap, exe_of, std::process::id())
}

/// `own_pid`: Coucou's own process. Its microphone use (Flow dictation, the Notetaker, « OK Coucou ») is not a
/// "program recording you": the pid is the server's, so a program cannot say it is Coucou.
fn streams_with(snap: &Snapshot, exe_of: ExeOf, own_pid: u32) -> Vec<Stream> {
    // Sources whose name ends in ".monitor" only listen to what a sink plays.
    let monitors: HashSet<String> = snap
        .sources
        .lines()
        .filter_map(|l| {
            let mut it = l.split('\t');
            let idx = it.next()?;
            let name = it.next()?;
            name.ends_with(".monitor").then(|| idx.trim().to_string())
        })
        .collect();
    // The pid the server recorded for each connection.
    let pids: HashMap<String, u32> = blocks(&snap.clients, "Client #")
        .iter()
        .filter_map(|b| {
            let pid = b.prop("pipewire.sec.pid").or_else(|| b.prop("application.process.id"))?.parse().ok()?;
            Some((b.id().to_string(), pid))
        })
        .collect();

    blocks(&snap.outputs, "Source Output #")
        .iter()
        .map(|b| {
            let name_claim = b.prop("application.name").unwrap_or_default().to_lowercase();
            let bin_claim = b.prop("application.process.binary").unwrap_or_default().to_lowercase();
            let id_claim = b.prop("application.id").unwrap_or_default();
            let pid = b.field("Client").and_then(|c| pids.get(&c).copied());
            let exe = pid.and_then(exe_of);
            let runs = |what: &str| exe.as_deref().is_some_and(|e| e.contains(what));
            let claims = |what: &str| name_claim.contains(what) || bin_claim == what;
            Stream {
                name: clean_name(&b.prop("application.name").or_else(|| b.prop("application.process.binary")).unwrap_or_default()),
                // Said to be Handy AND really Handy's process.
                handy: claims("handy") && runs("handy"),
                // Coucou's visualizer (it declares itself as pavucontrol): said so AND really cava.
                ours: ((claims("cava") || id_claim == "org.PulseAudio.pavucontrol") && runs("cava")) || pid == Some(own_pid),
                monitor: b.field("Source").is_some_and(|s| monitors.contains(&s)),
                corked: b.field("Corked").is_some_and(|c| c.eq_ignore_ascii_case("yes")),
            }
        })
        .collect()
}

/// Does the listing show Handy recording?
pub fn handy_recording(snap: &Snapshot, exe_of: ExeOf) -> bool {
    streams(snap, exe_of).iter().any(|s| s.handy && !s.monitor && !s.corked)
}

/// The programs (other than Handy and Coucou's own bars) that are recording the microphone, each
/// once, in the order the server lists them. A paused stream (a call tab left open, muted) and a
/// monitor are not recording.
pub fn mic_apps(snap: &Snapshot, exe_of: ExeOf) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for s in streams(snap, exe_of) {
        if s.handy || s.ours || s.monitor || s.corked {
            continue;
        }
        if !out.contains(&s.name) {
            out.push(s.name);
        }
    }
    out
}

/// The same reading on the real system, for Quick's warning (so it always agrees with the island).
pub fn mic_apps_now() -> Option<Vec<String>> {
    Some(mic_apps(&snapshot()?, &exe_of_pid))
}

/// Whether a line of `pactl subscribe` is about a capture starting, stopping or changing
/// (a stream being corked or uncorked is a "change"; a source disappearing is a "remove").
fn about_a_capture(line: &str) -> bool {
    line.contains("source-output") || line.contains("client")
}

/// Starts watching (once). Without `pactl` there is nothing to watch and it quietly gives up.
pub fn start(app: &AppHandle) {
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || watch(app));
}

#[derive(Default)]
struct Last {
    handy: bool,
    mic: Vec<String>,
}

fn watch(app: AppHandle) {
    let mut last = Last::default();
    loop {
        let child = Command::new("pactl")
            .arg("subscribe")
            .env("LC_ALL", "C")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn();
        let mut child = match child {
            Ok(c) => c,
            Err(_) => {
                log::line("capture indicators: pactl is not installed");
                STARTED.store(false, Ordering::SeqCst);
                return;
            }
        };
        if let Some(out) = child.stdout.take() {
            // A capture that was already on when the island started.
            look(&app, &mut last);
            for line in BufReader::new(out).lines().map_while(Result::ok) {
                if about_a_capture(&line) {
                    look(&app, &mut last);
                }
            }
        }
        let _ = child.wait();
        // The sound server went away (a restart, a logout): nothing can be recording through it.
        // Look again in a little while.
        publish(&app, &mut last, false, Vec::new());
        std::thread::sleep(std::time::Duration::from_secs(5));
    }
}

/// Reads the server; if it cannot be read, the last state stands (a failure is not "nobody records").
fn look(app: &AppHandle, last: &mut Last) {
    let Some(snap) = snapshot() else { return };
    let handy = handy_recording(&snap, &exe_of_pid);
    let mic = mic_apps(&snap, &exe_of_pid);
    publish(app, last, handy, mic);
}

/// Tells the page what changed since last time.
fn publish(app: &AppHandle, last: &mut Last, handy: bool, mic: Vec<String>) {
    if handy != last.handy {
        last.handy = handy;
        let _ = app.emit_to(WINDOW_LABEL, "dictation", Dictation { recording: handy });
    }
    if mic != last.mic {
        last.mic = mic.clone();
        let _ = app.emit_to(WINDOW_LABEL, "mic", Mic { apps: mic });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(outputs: &str) -> Snapshot {
        Snapshot {
            outputs: outputs.to_string(),
            // 56 is the microphone, 55 the monitor of the speakers.
            sources: "55\talsa_output.pci.analog-stereo.monitor\tPipeWire\ts32le 2ch 48000Hz\tSUSPENDED\n56\talsa_input.pci.analog-stereo\tPipeWire\ts32le 2ch 48000Hz\tSUSPENDED\n".to_string(),
            // Client 7 is really Handy, 8 really cava, 9 zoom, 10 chromium; 11 is a stranger.
            clients: ["7:100", "8:101", "9:102", "10:103", "11:104"]
                .iter()
                .map(|c| {
                    let (id, pid) = c.split_once(':').unwrap();
                    format!("Client #{id}\n\tDriver: PipeWire\n\tProperties:\n\t\tpipewire.sec.pid = \"{pid}\"\n\t\tapplication.process.id = \"1\"\n")
                })
                .collect(),
        }
    }

    /// What /proc says each pid really is.
    fn exe(pid: u32) -> Option<String> {
        match pid {
            100 => Some("handy".into()),
            101 => Some("cava".into()),
            102 => Some("zoom".into()),
            103 => Some("chromium".into()),
            104 => Some("evilapp".into()),
            _ => None,
        }
    }

    fn out(id: u32, client: u32, source: u32, corked: &str, props: &str) -> String {
        format!("Source Output #{id}\n\tDriver: PipeWire\n\tClient: {client}\n\tSource: {source}\n\tCorked: {corked}\n\tProperties:\n{props}")
    }

    const HANDY_P: &str = "\t\tapplication.name = \"Handy\"\n\t\tapplication.process.binary = \"handy\"\n";
    const CAVA_P: &str = "\t\tapplication.id = \"org.PulseAudio.pavucontrol\"\n\t\tapplication.name = \"cava\"\n\t\tapplication.process.binary = \"cava\"\n";
    const ZOOM_P: &str = "\t\tapplication.name = \"ZOOM VoiceEngine\"\n\t\tapplication.process.binary = \"zoom\"\n";
    const CHROME_P: &str = "\t\tapplication.name = \"Chromium input\"\n\t\tapplication.process.binary = \"chromium\"\n";

    #[test]
    fn only_handy_capturing_counts() {
        assert!(handy_recording(&snap(&out(1, 7, 56, "no", HANDY_P)), &exe));
        let both = out(1, 8, 56, "no", CAVA_P) + &out(2, 7, 56, "no", HANDY_P);
        assert!(handy_recording(&snap(&both), &exe), "among others");
        assert!(!handy_recording(&snap(&out(1, 8, 56, "no", CAVA_P)), &exe));
        assert!(!handy_recording(&snap(&out(1, 7, 55, "no", HANDY_P)), &exe), "its monitor only listens to what plays");
        assert!(!handy_recording(&snap(""), &exe));
    }

    #[test]
    fn the_microphone_indicator_names_the_other_programs() {
        assert_eq!(mic_apps(&snap(&out(1, 9, 56, "no", ZOOM_P)), &exe), vec!["ZOOM VoiceEngine"]);
        let quiet = out(1, 7, 56, "no", HANDY_P) + &out(2, 8, 56, "no", CAVA_P) + &out(3, 9, 55, "no", ZOOM_P);
        assert!(mic_apps(&snap(&quiet), &exe).is_empty(), "Handy, the bars and a monitor are not it");
        let two = out(1, 9, 56, "no", ZOOM_P) + &out(2, 10, 56, "no", CHROME_P);
        assert_eq!(mic_apps(&snap(&two), &exe), vec!["ZOOM VoiceEngine", "Chromium input"]);
        let twice = out(1, 10, 56, "no", CHROME_P) + &out(2, 10, 56, "no", CHROME_P);
        assert_eq!(mic_apps(&snap(&twice), &exe), vec!["Chromium input"], "each program once");
        assert!(mic_apps(&snap(""), &exe).is_empty());
    }

    #[test]
    fn coucou_dictating_is_not_a_program_recording_you() {
        // Client 9 (really "zoom") stands for Coucou's own process here.
        let s = snap(&out(1, 9, 56, "no", ZOOM_P));
        let mine = streams_with(&s, &exe, 102);
        assert!(mine.iter().all(|x| x.ours), "its own streams are ours");
        let other = streams_with(&s, &exe, 4242);
        assert!(other.iter().all(|x| !x.ours), "a stranger's are not");
        // And what a program says about itself changes nothing: it is the server's pid that counts.
        let liar = "\t\tapplication.name = \"Coucou\"\n\t\tapplication.process.binary = \"coucou-island\"\n";
        assert!(streams_with(&snap(&out(1, 11, 56, "no", liar)), &exe, 4242).iter().all(|x| !x.ours));
    }

    #[test]
    fn a_paused_stream_is_not_recording() {
        let idle = out(1, 10, 56, "yes", CHROME_P);
        assert!(mic_apps(&snap(&idle), &exe).is_empty(), "a call tab that is open but muted keeps an idle stream");
        assert_eq!(mic_apps(&snap(&(idle + &out(2, 9, 56, "no", ZOOM_P))), &exe), vec!["ZOOM VoiceEngine"]);
    }

    #[test]
    fn a_program_cannot_talk_its_way_out_of_the_indicator() {
        // A stranger (client 11, really "evilapp") that says it is Handy, cava, pavucontrol and a monitor.
        let liar = "\t\tapplication.name = \"Handy\"\n\t\tapplication.process.binary = \"cava\"\n\t\tapplication.id = \"org.PulseAudio.pavucontrol\"\n\t\tnode.passive = \"true\"\n\t\tstream.monitor = \"true\"\n";
        let s = snap(&out(1, 11, 56, "no", liar));
        assert_eq!(mic_apps(&s, &exe), vec!["Handy"], "still counted, under the name it gave");
        assert!(!handy_recording(&s, &exe), "and it does not make the island say Handy listens");
        // Claiming another program's pid in its own properties changes nothing: the server's pid rules.
        let borrowed = format!("{liar}\t\tapplication.process.id = \"100\"\n");
        assert_eq!(mic_apps(&snap(&out(1, 11, 56, "no", &borrowed)), &exe).len(), 1);
        // A stream whose client the server does not list (or whose process is gone) is not excluded.
        assert_eq!(mic_apps(&snap(&out(1, 99, 56, "no", HANDY_P)), &exe), vec!["Handy"]);
    }

    /// Prints how the REAL sound server's streams are classified (read-only):
    /// `cargo test --lib real_capture_probe -- --ignored --nocapture` while something records.
    #[test]
    #[ignore]
    fn real_capture_probe() {
        let snap = snapshot().expect("pactl");
        for s in streams(&snap, &exe_of_pid) {
            println!("stream {:?} handy={} ours={} monitor={} corked={}", s.name, s.handy, s.ours, s.monitor, s.corked);
        }
        println!("mic_apps = {:?}, handy = {}", mic_apps(&snap, &exe_of_pid), handy_recording(&snap, &exe_of_pid));
    }

    #[test]
    fn names_are_made_safe_to_show() {
        assert_eq!(clean_name("  Zoom \n\u{1b}[31mred"), "Zoom [31mred");
        assert_eq!(clean_name("\u{0}\u{7}"), "an app");
        assert_eq!(clean_name(""), "an app");
        assert_eq!(clean_name(&"x".repeat(100)).chars().count(), 40);
    }

    #[test]
    fn only_capture_announcements_are_looked_at() {
        assert!(about_a_capture("Event 'new' on source-output #77"));
        assert!(about_a_capture("Event 'remove' on source-output #77"));
        assert!(about_a_capture("Event 'change' on source-output #77"));
        assert!(!about_a_capture("Event 'change' on sink #3"));
        assert!(!about_a_capture("Event 'change' on server"));
    }
}
