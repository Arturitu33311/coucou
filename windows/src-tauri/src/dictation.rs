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
// Only program names are sent to the page; no audio is ever opened or read here.

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

/// One `Source Output` block of `pactl list source-outputs`.
struct Block<'a>(&'a str);

impl Block<'_> {
    fn lower(&self) -> String {
        self.0.to_lowercase()
    }

    /// The value of `key = "value"` in the block's properties.
    fn prop(&self, key: &str) -> Option<String> {
        self.0.lines().find_map(|l| {
            let (k, v) = l.trim().split_once('=')?;
            (k.trim() == key).then(|| v.trim().trim_matches('"').to_string())
        })
    }

    fn is_handy(&self) -> bool {
        let b = self.lower();
        b.contains("application.process.binary = \"handy\"") || b.contains("application.name = \"handy\"")
    }

    /// Coucou's own real-time bars (declared as pavucontrol so the desktop shows no indicator).
    fn is_ours(&self) -> bool {
        let b = self.lower();
        self.0.contains("org.PulseAudio.pavucontrol")
            || b.contains("application.process.binary = \"cava\"")
            || b.contains("application.name = \"cava\"")
    }

    /// A monitor only listens to what is playing; it is not a microphone.
    fn is_monitor(&self) -> bool {
        self.0.contains("node.passive = \"true\"") || self.0.contains("stream.monitor = \"true\"")
    }

    /// A paused stream: many apps (a browser with a call tab, Discord) keep one open and idle.
    fn is_corked(&self) -> bool {
        self.0.lines().any(|l| l.trim().eq_ignore_ascii_case("corked: yes"))
    }

    fn app_name(&self) -> String {
        let raw = self
            .prop("application.name")
            .or_else(|| self.prop("application.process.binary"))
            .unwrap_or_default();
        let clean = raw.trim();
        if clean.is_empty() {
            "an app".to_string()
        } else {
            clean.to_string()
        }
    }
}

fn blocks(listing: &str) -> impl Iterator<Item = Block<'_>> {
    listing.split("Source Output #").skip(1).map(Block)
}

/// Does the listing show Handy recording?
pub fn handy_recording(listing: &str) -> bool {
    blocks(listing).any(|b| b.is_handy() && !b.is_monitor() && !b.is_corked())
}

/// The programs (other than Handy and Coucou's own bars) that are recording the microphone,
/// each once, in the order the server lists them.
pub fn mic_apps(listing: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for b in blocks(listing) {
        if b.is_handy() || b.is_ours() || b.is_monitor() || b.is_corked() {
            continue;
        }
        let name = b.app_name();
        if !out.contains(&name) {
            out.push(name);
        }
    }
    out
}

/// Whether a line of `pactl subscribe` is about a capture starting, stopping or changing
/// (a stream being corked or uncorked is a "change").
fn about_a_capture(line: &str) -> bool {
    line.contains("source-output")
}

fn listing() -> Option<String> {
    Command::new("pactl")
        .args(["list", "source-outputs"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
}

/// Starts watching (once). Without `pactl` there is nothing to watch and it quietly gives up.
pub fn start(app: &AppHandle) {
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || watch(app));
}

struct Last {
    handy: bool,
    mic: Vec<String>,
}

fn watch(app: AppHandle) {
    let mut last = Last { handy: false, mic: Vec::new() };
    loop {
        let child = Command::new("pactl")
            .arg("subscribe")
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
            report(&app, &mut last, listing().as_deref().unwrap_or(""));
            for line in BufReader::new(out).lines().map_while(Result::ok) {
                if about_a_capture(&line) {
                    report(&app, &mut last, listing().as_deref().unwrap_or(""));
                }
            }
        }
        let _ = child.wait();
        // The sound server went away (a restart, a logout): look again in a little while.
        report(&app, &mut last, "");
        std::thread::sleep(std::time::Duration::from_secs(5));
    }
}

/// Tells the page what changed since last time.
fn report(app: &AppHandle, last: &mut Last, listing: &str) {
    let handy = handy_recording(listing);
    if handy != last.handy {
        last.handy = handy;
        let _ = app.emit_to(WINDOW_LABEL, "dictation", Dictation { recording: handy });
    }
    let mic = mic_apps(listing);
    if mic != last.mic {
        last.mic = mic.clone();
        let _ = app.emit_to(WINDOW_LABEL, "mic", Mic { apps: mic });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HANDY: &str = "Source Output #77\n\tDriver: PipeWire\n\tCorked: no\n\tProperties:\n\t\tapplication.name = \"Handy\"\n\t\tapplication.process.binary = \"handy\"\n\t\tnode.name = \"alsa_capture.handy\"\n";
    const CAVA: &str = "Source Output #78\n\tProperties:\n\t\tapplication.name = \"cava\"\n\t\tapplication.process.binary = \"cava\"\n";
    const PAVU: &str = "Source Output #81\n\tProperties:\n\t\tapplication.id = \"org.PulseAudio.pavucontrol\"\n\t\tapplication.name = \"Peak detect\"\n";
    const HANDY_MONITOR: &str = "Source Output #79\n\tProperties:\n\t\tapplication.name = \"Handy\"\n\t\tstream.monitor = \"true\"\n";
    const OTHER: &str = "Source Output #80\n\tProperties:\n\t\tapplication.name = \"Handyman Planner\"\n\t\tapplication.process.binary = \"handyman\"\n";
    const ZOOM: &str = "Source Output #90\n\tDriver: PipeWire\n\tCorked: no\n\tProperties:\n\t\tapplication.name = \"ZOOM VoiceEngine\"\n\t\tapplication.process.binary = \"zoom\"\n";
    const MEET_IDLE: &str = "Source Output #91\n\tDriver: PipeWire\n\tCorked: yes\n\tProperties:\n\t\tapplication.name = \"Chromium input\"\n\t\tapplication.process.binary = \"chromium\"\n";
    const MEET_LIVE: &str = "Source Output #92\n\tDriver: PipeWire\n\tCorked: no\n\tProperties:\n\t\tapplication.name = \"Chromium input\"\n\t\tapplication.process.binary = \"chromium\"\n";
    const NAMELESS: &str = "Source Output #93\n\tCorked: no\n\tProperties:\n\t\tmedia.name = \"x\"\n";

    #[test]
    fn only_handy_capturing_counts() {
        assert!(handy_recording(HANDY));
        assert!(handy_recording(&format!("{CAVA}{HANDY}")), "among others");
        assert!(!handy_recording(CAVA));
        assert!(!handy_recording(HANDY_MONITOR), "a monitor only listens to what plays");
        assert!(!handy_recording(OTHER), "another program with handy in its name");
        assert!(!handy_recording(""));
    }

    #[test]
    fn the_microphone_indicator_names_the_other_programs() {
        assert_eq!(mic_apps(ZOOM), vec!["ZOOM VoiceEngine"]);
        assert_eq!(mic_apps(&format!("{HANDY}{CAVA}{PAVU}{HANDY_MONITOR}")), Vec::<String>::new(), "Handy, the bars and monitors are not it");
        assert_eq!(mic_apps(&format!("{ZOOM}{MEET_LIVE}")), vec!["ZOOM VoiceEngine", "Chromium input"]);
        assert_eq!(mic_apps(&format!("{MEET_LIVE}{MEET_LIVE}")), vec!["Chromium input"], "each program once");
        assert_eq!(mic_apps(OTHER), vec!["Handyman Planner"]);
        assert_eq!(mic_apps(NAMELESS), vec!["an app"]);
        assert!(mic_apps("").is_empty());
    }

    #[test]
    fn a_paused_stream_is_not_recording() {
        assert!(mic_apps(MEET_IDLE).is_empty(), "a call tab that is open but muted keeps an idle stream");
        assert_eq!(mic_apps(&format!("{MEET_IDLE}{ZOOM}")), vec!["ZOOM VoiceEngine"]);
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
