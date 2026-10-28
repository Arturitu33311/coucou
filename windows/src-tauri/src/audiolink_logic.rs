// The audio relay's brain, with no I/O: given what happened (headphones moved, the phone link came or
// went, someone started playing…) it says what the machine must do (move the sound to the silent sink,
// start or stop the stream, pause a player). The phone runs the same function in Kotlin
// (`AudioLinkLogic.kt`); both read `scripts/phone/audiolink-vectors.json`, so they cannot drift apart.
//
// Three modes. `Off`: the computer plays on its own speakers as it always did. `Relay`: the sound goes
// to a silent virtual sink and from there to the phone. `Hold`: the headphones are on the phone but the
// stream cannot run (link lost, a call, a short Bluetooth drop); the virtual sink stays the output, so
// nothing can come out of the speakers by surprise.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Off,
    Relay,
    Hold,
}

/// Which device holds the headphones right now, as far as we know.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Where {
    Phone,
    Pc,
    #[default]
    None,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    Enabled(bool),
    Headphones(Where),
    Link(bool),
    Call(bool),
    PhoneMedia(bool),
    PcMedia(bool),
    /// The short wait after the headphones disappeared from the phone is over.
    GraceExpired,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    SinkVirtual,
    SinkRestore,
    RelayStart,
    RelayStop,
    PausePc,
    PausePhone,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct State {
    pub mode: Mode,
    pub enabled: bool,
    pub headphones: Where,
    pub link_up: bool,
    pub call: bool,
    pub phone_playing: bool,
    pub pc_playing: bool,
}

impl State {
    fn armed(&self) -> bool {
        self.enabled && self.headphones == Where::Phone
    }

    fn ready(&self) -> bool {
        self.armed() && self.link_up && !self.call
    }
}

/// Takes one event in; returns the next state and what to do about it.
pub fn step(state: &State, ev: Event) -> (State, Vec<Action>) {
    let mut s = *state;
    match ev {
        Event::Enabled(on) => s.enabled = on,
        Event::Headphones(at) => s.headphones = at,
        Event::Link(up) => s.link_up = up,
        Event::Call(on) => s.call = on,
        Event::PhoneMedia(p) => s.phone_playing = p,
        Event::PcMedia(p) => s.pc_playing = p,
        Event::GraceExpired => {}
    }
    let mut out = Vec::new();
    match s.mode {
        Mode::Off => {
            if s.ready() {
                s.mode = Mode::Relay;
                out.extend([Action::SinkVirtual, Action::RelayStart]);
            } else if s.armed() {
                s.mode = Mode::Hold;
                out.extend([Action::SinkVirtual, Action::PausePc]);
            }
        }
        Mode::Relay => {
            if !s.enabled || s.headphones == Where::Pc {
                s.mode = Mode::Off;
                out.extend([Action::RelayStop, Action::SinkRestore]);
            } else if !s.ready() {
                s.mode = Mode::Hold;
                out.extend([Action::RelayStop, Action::PausePc]);
            }
        }
        Mode::Hold => {
            let gone = ev == Event::GraceExpired && s.headphones != Where::Phone;
            if !s.enabled || s.headphones == Where::Pc || gone {
                s.mode = Mode::Off;
                out.push(Action::SinkRestore);
            } else if s.ready() {
                s.mode = Mode::Relay;
                out.push(Action::RelayStart);
            }
        }
    }
    // Last one to start wins: what starts on one device pauses the other.
    match (ev, s.mode) {
        (Event::PhoneMedia(true), Mode::Relay) if s.pc_playing => out.push(Action::PausePc),
        (Event::PcMedia(true), Mode::Relay) if s.phone_playing => out.push(Action::PausePhone),
        (Event::PcMedia(true), Mode::Hold) => out.push(Action::PausePc),
        _ => {}
    }
    (s, out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    const VECTORS: &str = include_str!("../../scripts/phone/audiolink-vectors.json");

    fn event(v: &Value) -> Event {
        match v["t"].as_str().unwrap() {
            "enabled" => Event::Enabled(v["on"].as_bool().unwrap()),
            "headphones" => Event::Headphones(match v["at"].as_str().unwrap() {
                "phone" => Where::Phone,
                "pc" => Where::Pc,
                "none" => Where::None,
                other => panic!("unknown place {other}"),
            }),
            "link" => Event::Link(v["up"].as_bool().unwrap()),
            "call" => Event::Call(v["on"].as_bool().unwrap()),
            "phone_media" => Event::PhoneMedia(v["playing"].as_bool().unwrap()),
            "pc_media" => Event::PcMedia(v["playing"].as_bool().unwrap()),
            "grace_expired" => Event::GraceExpired,
            other => panic!("unknown event {other}"),
        }
    }

    fn mode_name(m: Mode) -> &'static str {
        match m {
            Mode::Off => "off",
            Mode::Relay => "relay",
            Mode::Hold => "hold",
        }
    }

    fn action_name(a: Action) -> &'static str {
        match a {
            Action::SinkVirtual => "sink_virtual",
            Action::SinkRestore => "sink_restore",
            Action::RelayStart => "relay_start",
            Action::RelayStop => "relay_stop",
            Action::PausePc => "pause_pc",
            Action::PausePhone => "pause_phone",
        }
    }

    #[test]
    fn every_shared_state_case_holds() {
        let doc: Value = serde_json::from_str(VECTORS).unwrap();
        let cases = doc["state_cases"].as_array().unwrap();
        assert!(cases.len() >= 10, "the vector file lost its cases");
        for case in cases {
            let name = case["name"].as_str().unwrap();
            let mut state = State::default();
            for (i, s) in case["steps"].as_array().unwrap().iter().enumerate() {
                let (next, actions) = step(&state, event(&s["event"]));
                state = next;
                let want: Vec<&str> = s["actions"].as_array().unwrap().iter().map(|a| a.as_str().unwrap()).collect();
                let got: Vec<&str> = actions.iter().map(|a| action_name(*a)).collect();
                assert_eq!(got, want, "{name}, step {i}: actions");
                assert_eq!(mode_name(state.mode), s["mode"].as_str().unwrap(), "{name}, step {i}: mode");
            }
        }
    }
}
