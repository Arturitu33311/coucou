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
    is_kdeconnect(bus_name) || is_phone_player(bus_name)
}

/// KDE Connect's / GSConnect's mirror of the phone's media.
pub fn is_kdeconnect(bus_name: &str) -> bool {
    let n = bus_name.to_ascii_lowercase();
    n.contains("kdeconnect") || n.contains("gsconnect")
}

/// Coucou's own player for what the phone plays (`phonenow.rs`). It is never sound made here: if it counted, phone
/// music would look like the computer playing and the relay would pause the phone.
pub fn is_phone_player(bus_name: &str) -> bool {
    bus_name.to_ascii_lowercase().starts_with("org.mpris.mediaplayer2.coucou.phone")
}

/// The names (lower case) of the programs that have a stream playing on this computer, from `pactl list sink-inputs`
/// (`LC_ALL=C`): each uncorked stream's `application.name` and `application.process.binary`.
pub fn playing_stream_names(pactl_sink_inputs: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut corked = true;
    let mut names: Vec<String> = Vec::new();
    for line in pactl_sink_inputs.lines().chain(std::iter::once("Sink Input #end")) {
        let t = line.trim();
        if t.starts_with("Sink Input") {
            if !corked {
                out.append(&mut names);
            }
            names.clear();
            corked = true;
        } else if let Some(v) = t.strip_prefix("Corked:") {
            corked = v.trim() == "yes";
        } else if let Some(v) = t.strip_prefix("application.name = ").or_else(|| t.strip_prefix("application.process.binary = ")) {
            names.push(v.trim().trim_matches('"').to_lowercase());
        }
    }
    out
}

/// Whether the MPRIS player `bus_name` is making the sound that streams called `streams` carry: its short name (the
/// first word after the MPRIS prefix: "brave", "sonora", "vlc"…) appears in the name of one of them. A player with no
/// stream here, such as Sonora showing what plays on the phone's Spotify, makes no sound on this computer.
pub fn makes_sound_here(bus_name: &str, streams: &[String]) -> bool {
    let short = bus_name.strip_prefix(PREFIX).unwrap_or(bus_name).split('.').next().unwrap_or("").to_lowercase();
    !short.is_empty() && streams.iter().any(|s| s.contains(&short))
}

/// What is playing on this computer right now, by program name.
fn current_streams() -> Vec<String> {
    std::process::Command::new("pactl")
        .args(["list", "sink-inputs"])
        .env("LC_ALL", "C")
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .map(|o| playing_stream_names(&String::from_utf8_lossy(&o.stdout)))
        .unwrap_or_default()
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

/// True when a player that is not a mirror reports "playing" and has a stream playing here. Used together with real
/// sound: it only makes the relay notice sooner that this computer started playing.
pub fn non_mirror_playing() -> bool {
    let Ok(conn) = Connection::session() else { return false };
    let streams = current_streams();
    player_names(&conn).iter().any(|n| status_of(&conn, n).as_deref() == Some("Playing") && makes_sound_here(n, &streams))
}

/// Pauses the players that are playing sound on this computer; returns the names of the ones it asked. A player that
/// is paused, stopped or gone is left alone, and so is one that reports "playing" without any stream here: that is a
/// mirror of another device (Sonora showing the phone's Spotify), and pausing it would pause the phone.
pub fn pause_all_playing() -> Vec<String> {
    let Ok(conn) = Connection::session() else { return Vec::new() };
    let streams = current_streams();
    let mut asked = Vec::new();
    for name in player_names(&conn) {
        if status_of(&conn, &name).as_deref() != Some("Playing") {
            continue;
        }
        if !makes_sound_here(&name, &streams) {
            crate::log::line(format!("audiolink: left {} alone: it plays nothing here (a mirror of another device?)", name));
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

    const SINK_INPUTS: &str = r#"Sink Input #230
	Driver: PipeWire
	Corked: no
	Properties:
		application.name = "coucou"
		application.process.binary = "coucou"
Sink Input #766
	Corked: no
	Properties:
		application.name = "VLC media player (LibVLC 3.0.23)"
		application.process.binary = "vlc"
Sink Input #900
	Corked: yes
	Properties:
		application.name = "Spotify"
Sink Input #901
	Corked: no
	Properties:
		application.name = "Brave"
		application.process.binary = "brave"
"#;

    #[test]
    fn only_streams_that_are_playing_count() {
        let names = playing_stream_names(SINK_INPUTS);
        assert!(names.contains(&"brave".to_string()));
        assert!(names.contains(&"vlc media player (libvlc 3.0.23)".to_string()));
        assert!(names.contains(&"vlc".to_string()));
        assert!(!names.iter().any(|n| n.contains("spotify")), "a paused (corked) stream is not sound: {names:?}");
        assert!(playing_stream_names("").is_empty());
    }

    #[test]
    fn a_player_with_no_stream_here_is_not_the_source_of_the_sound() {
        let streams = playing_stream_names(SINK_INPUTS);
        assert!(makes_sound_here("org.mpris.MediaPlayer2.brave.instance3255140", &streams));
        assert!(makes_sound_here("org.mpris.MediaPlayer2.vlc", &streams));
        // Sonora showing what plays on the phone's Spotify has no stream on this computer.
        assert!(!makes_sound_here("org.mpris.MediaPlayer2.sonora", &streams));
        assert!(!makes_sound_here("org.mpris.MediaPlayer2.spotify", &streams), "its stream is paused");
        assert!(!makes_sound_here("org.mpris.MediaPlayer2.brave", &[]));
    }

    #[test]
    fn a_player_that_plays_here_is_found_by_its_own_stream_name() {
        let streams = vec!["alsa plug-in [sonora]".to_string(), "coucou".to_string()];
        assert!(makes_sound_here("org.mpris.MediaPlayer2.sonora", &streams));
        assert!(makes_sound_here("org.mpris.MediaPlayer2.firefox.instance_1_9", &["firefox".to_string()]));
        // An empty name must not match everything.
        assert!(!makes_sound_here("org.mpris.MediaPlayer2.", &streams));
    }

    #[test]
    fn the_phones_mirror_is_not_a_player_of_this_computer() {
        assert!(is_mirror("org.mpris.MediaPlayer2.kdeconnect.mpris_47f59de111914df59ee2abdb5d537b3e"));
        assert!(is_mirror("org.mpris.MediaPlayer2.GSConnect.Something"));
        // Coucou's own player for the phone's music must never count as the computer playing (it would pause the phone).
        assert!(is_mirror("org.mpris.MediaPlayer2.coucou.phone"));
        assert!(!is_mirror("org.mpris.MediaPlayer2.coucou"));
        assert!(!is_mirror("org.mpris.MediaPlayer2.sonora"));
        assert!(!is_mirror("org.mpris.MediaPlayer2.brave.instance3255140"));
        assert!(!is_mirror("org.mpris.MediaPlayer2.vlc"));
    }
}
