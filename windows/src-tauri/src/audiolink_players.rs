// The one thing the audio relay does to the computer's media players: pause the ones that are playing, with MPRIS
// `Pause` (never a toggle, so a player that is already paused is left alone).
//
// It does NOT use the players' status to know whether the computer is playing: some players only mirror another device
// (KDE Connect shows the phone's media here; Sonora shows what plays on the phone's Spotify) and report "playing"
// without making any sound on this computer. That is told by the sound itself (`audiolink::SoundWatch`).

use zbus::blocking::{fdo::DBusProxy, Connection, Proxy};

const PREFIX: &str = "org.mpris.MediaPlayer2.";
const PATH: &str = "/org/mpris/MediaPlayer2";
const PLAYER: &str = "org.mpris.MediaPlayer2.Player";

/// True for the MPRIS players that are only a mirror of the *phone's* media (KDE Connect / GSConnect show what the
/// phone plays here as a player). Pausing them would pause the phone.
pub fn is_mirror(bus_name: &str) -> bool {
    let n = bus_name.to_ascii_lowercase();
    n.contains("kdeconnect") || n.contains("gsconnect")
}

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

/// True when a player that is not a mirror reports "playing". Used together with real sound: it only makes the relay
/// notice sooner that this computer started playing.
pub fn non_mirror_playing() -> bool {
    let Ok(conn) = Connection::session() else { return false };
    player_names(&conn).iter().any(|n| status_of(&conn, n).as_deref() == Some("Playing"))
}

/// Pauses every player that is playing; returns the names of the ones it asked. A player that is paused, stopped or
/// gone is left alone.
pub fn pause_all_playing() -> Vec<String> {
    let Ok(conn) = Connection::session() else { return Vec::new() };
    let mut asked = Vec::new();
    for name in player_names(&conn) {
        if status_of(&conn, &name).as_deref() != Some("Playing") {
            continue;
        }
        if let Ok(player) = Proxy::new(&conn, name.as_str(), PATH, PLAYER) {
            if player.call_method("Pause", &()).is_ok() {
                asked.push(name.strip_prefix(PREFIX).unwrap_or(&name).to_string());
            }
        }
    }
    asked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_phones_mirror_is_not_a_player_of_this_computer() {
        assert!(is_mirror("org.mpris.MediaPlayer2.kdeconnect.mpris_47f59de111914df59ee2abdb5d537b3e"));
        assert!(is_mirror("org.mpris.MediaPlayer2.GSConnect.Something"));
        assert!(!is_mirror("org.mpris.MediaPlayer2.sonora"));
        assert!(!is_mirror("org.mpris.MediaPlayer2.brave.instance3255140"));
        assert!(!is_mirror("org.mpris.MediaPlayer2.vlc"));
    }
}
