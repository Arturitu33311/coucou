// What the audio relay needs to know about the computer's media players, and the one thing it does to them.
//
// Knows: whether any MPRIS player is `Playing`, told to the relay only when that changes. It listens for the
// players' `PropertiesChanged` and for players appearing or leaving, and sleeps in between (nothing is polled).
// Does: pause the players that are playing, with MPRIS `Pause` (never a toggle, so a player that is already
// paused is left alone).
//
// This watches on its own rather than through the Music pill's watcher (`music.rs`), which only runs while
// that pill is on and publishes to the island: the relay must work with the pill off, and must not change
// what the pill does.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use zbus::blocking::{fdo::DBusProxy, Connection, MessageIterator, Proxy};
use zbus::MatchRule;

use crate::log;

const PREFIX: &str = "org.mpris.MediaPlayer2.";
const PATH: &str = "/org/mpris/MediaPlayer2";
const PLAYER: &str = "org.mpris.MediaPlayer2.Player";
const ROOT: &str = "org.mpris.MediaPlayer2";

static STARTED: AtomicBool = AtomicBool::new(false);

// ── Pure parts ─────────────────────────────────────────────────────────────────

/// True when at least one status is exactly `Playing`.
pub fn any_playing(statuses: &[&str]) -> bool {
    statuses.iter().any(|s| *s == "Playing")
}

/// Turns a stream of "is anything playing" readings into changes only. Starts as "nothing playing".
#[derive(Default)]
pub struct Edge {
    last: bool,
}

impl Edge {
    /// `Some(now)` when `now` differs from the last reading, otherwise `None`.
    pub fn feed(&mut self, now: bool) -> Option<bool> {
        (now != std::mem::replace(&mut self.last, now)).then_some(now)
    }
}

/// True for the MPRIS players that are only a mirror of the *phone's* media (KDE Connect / GSConnect show what the
/// phone plays here as a player). They are not sound made on this computer: counting them would make the phone's own
/// music look like the computer's, and pausing them would pause the phone.
pub fn is_mirror(bus_name: &str) -> bool {
    let n = bus_name.to_ascii_lowercase();
    n.contains("kdeconnect") || n.contains("gsconnect")
}

// ── D-Bus ──────────────────────────────────────────────────────────────────────

fn player_names(conn: &Connection) -> Vec<String> {
    DBusProxy::new(conn)
        .ok()
        .and_then(|p| p.list_names().ok())
        .map(|names| {
            names.into_iter().map(|n| n.to_string()).filter(|n| n.starts_with(PREFIX) && !is_mirror(n)).collect()
        })
        .unwrap_or_default()
}

fn status_of(conn: &Connection, name: &str) -> Option<String> {
    Proxy::new(conn, name, PATH, PLAYER).ok()?.get_property::<String>("PlaybackStatus").ok()
}

fn anything_playing(conn: &Connection) -> bool {
    let statuses: Vec<String> = player_names(conn).iter().filter_map(|n| status_of(conn, n)).collect();
    any_playing(&statuses.iter().map(String::as_str).collect::<Vec<_>>())
}

/// Starts watching (once). `on_change(true)` when something starts playing, `on_change(false)` when nothing
/// is left playing.
pub fn start(on_change: fn(bool)) {
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    let spawned = std::thread::Builder::new().name("coucou-audiolink-mpris".into()).spawn(move || {
        let conn = match Connection::session() {
            Ok(c) => c,
            Err(e) => {
                log::line(format!("audiolink: no session bus ({e}): the computer's players are not watched"));
                STARTED.store(false, Ordering::SeqCst);
                return;
            }
        };
        let (tx, rx) = mpsc::channel::<()>();
        let rules = [
            MatchRule::builder()
                .msg_type(zbus::message::Type::Signal)
                .interface("org.freedesktop.DBus.Properties")
                .and_then(|b| b.member("PropertiesChanged"))
                .and_then(|b| b.path(PATH))
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
        let mut edge = Edge::default();
        if let Some(now) = edge.feed(anything_playing(&conn)) {
            on_change(now);
        }
        // Sleeps until a player says something; a burst of signals counts as one.
        while rx.recv().is_ok() {
            while rx.recv_timeout(Duration::from_millis(80)).is_ok() {}
            if let Some(now) = edge.feed(anything_playing(&conn)) {
                on_change(now);
            }
        }
    });
    if spawned.is_err() {
        STARTED.store(false, Ordering::SeqCst);
    }
}

/// Pauses every player that is playing; returns how many it asked. A player that is paused, stopped or gone
/// is left alone.
pub fn pause_all_playing() -> usize {
    let Ok(conn) = Connection::session() else { return 0 };
    let mut asked = 0;
    for name in player_names(&conn) {
        if status_of(&conn, &name).as_deref() != Some("Playing") {
            continue;
        }
        if let Ok(player) = Proxy::new(&conn, name.as_str(), PATH, PLAYER) {
            if player.call_method("Pause", &()).is_ok() {
                asked += 1;
            }
        }
    }
    asked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn any_playing_is_true_only_for_playing() {
        assert!(!any_playing(&["Paused", "Stopped"]));
        assert!(any_playing(&["Paused", "Playing"]));
        assert!(!any_playing(&[]));
        assert!(!any_playing(&["playing"]), "the status is matched exactly");
    }

    #[test]
    fn the_phones_mirror_is_not_a_player_of_this_computer() {
        assert!(is_mirror("org.mpris.MediaPlayer2.kdeconnect.mpris_47f59de111914df59ee2abdb5d537b3e"));
        assert!(is_mirror("org.mpris.MediaPlayer2.GSConnect.Something"));
        assert!(!is_mirror("org.mpris.MediaPlayer2.sonora"));
        assert!(!is_mirror("org.mpris.MediaPlayer2.brave.instance3255140"));
        assert!(!is_mirror("org.mpris.MediaPlayer2.vlc"));
    }

    #[test]
    fn playing_listener_fires_only_on_change() {
        let mut edge = Edge::default();
        let calls: Vec<bool> = [false, false, true, true, false].into_iter().filter_map(|r| edge.feed(r)).collect();
        assert_eq!(calls, vec![true, false]);
    }
}
