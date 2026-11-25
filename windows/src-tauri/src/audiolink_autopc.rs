//! Bringing the headphones back to the computer on their own when the phone is gone (switched off, out of reach).
//!
//! Without it, when the phone dies the relay falls silent, the speakers come back, and the headphones stay on a dead
//! phone until someone moves them by hand ("Headphones to this computer"). The computer already knows how (`swap("pc")`:
//! BlueZ connect with retries, then the sound is pointed at them); this only decides WHEN to do it, so it is pure.
//!
//! It tries when the link to the phone has been down for `PHONE_GONE_AFTER_MS` and the relay is not carrying sound, the
//! headphones are not already here, and it is not already trying. Attempts are spaced out (sooner while this computer is
//! playing sound, which is when the headphones are wanted) and stop after `MAX_ATTEMPTS` per absence; the phone coming
//! back (the link up again) starts the count over. `~/.config/coucou/audiolink-autopc.off` turns the whole thing off.

use crate::audiolink_logic::{Mode, State, Where};

/// A link that has been down this long is a phone that is gone, not a Wi-Fi blip.
pub const PHONE_GONE_AFTER_MS: u64 = 20_000;
/// Between attempts while nothing is playing here.
pub const RETRY_IDLE_MS: u64 = 120_000;
/// Between attempts while this computer is playing sound.
pub const RETRY_PLAYING_MS: u64 = 30_000;
/// Attempts (each one tries to connect for about a minute) per absence of the phone.
pub const MAX_ATTEMPTS: u32 = 6;

#[derive(Debug, Clone)]
pub struct Tracker {
    mode: Mode,
    headphones: Where,
    pc_playing: bool,
    /// When the link went down (ms on the caller's clock); None while it is up or not yet observed.
    link_down_since: Option<u64>,
    attempts: u32,
    last_attempt: Option<u64>,
}

impl Tracker {
    pub const fn new() -> Self {
        Tracker {
            mode: Mode::Off,
            headphones: Where::None,
            pc_playing: false,
            link_down_since: None,
            attempts: 0,
            last_attempt: None,
        }
    }

    /// Call after every step of the relay's state machine.
    pub fn observe(&mut self, s: &State, now_ms: u64) {
        self.mode = s.mode;
        self.headphones = s.headphones;
        self.pc_playing = s.pc_playing;
        if s.link_up {
            self.link_down_since = None;
            self.attempts = 0;
            self.last_attempt = None;
        } else if self.link_down_since.is_none() {
            self.link_down_since = Some(now_ms);
        }
    }

    /// Whether to try to bring the headphones here now. `auto_on` is the user's switch, `swapping` says a swap is
    /// already in progress.
    pub fn should_try(&self, auto_on: bool, swapping: bool, now_ms: u64) -> bool {
        if !auto_on || swapping {
            return false;
        }
        let Some(since) = self.link_down_since else { return false };
        if self.mode == Mode::Relay || self.headphones == Where::Pc {
            return false;
        }
        if now_ms.saturating_sub(since) < PHONE_GONE_AFTER_MS || self.attempts >= MAX_ATTEMPTS {
            return false;
        }
        let gap = if self.pc_playing { RETRY_PLAYING_MS } else { RETRY_IDLE_MS };
        self.last_attempt.map_or(true, |t| now_ms.saturating_sub(t) >= gap)
    }

    /// Call when an attempt starts.
    pub fn tried(&mut self, now_ms: u64) {
        self.attempts += 1;
        self.last_attempt = Some(now_ms);
    }
}

impl Default for Tracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(mode: Mode, headphones: Where, link_up: bool, pc_playing: bool) -> State {
        State { mode, headphones, link_up, pc_playing, ..State::default() }
    }

    /// A tracker that saw the link go down at `t`.
    fn gone(t: u64, headphones: Where, playing: bool) -> Tracker {
        let mut k = Tracker::new();
        k.observe(&state(Mode::Off, headphones, false, playing), t);
        k
    }

    #[test]
    fn waits_until_the_phone_is_really_gone() {
        let k = gone(1_000, Where::None, false);
        assert!(!k.should_try(true, false, 1_000 + PHONE_GONE_AFTER_MS - 1));
        assert!(k.should_try(true, false, 1_000 + PHONE_GONE_AFTER_MS));
    }

    #[test]
    fn nothing_before_the_first_observation() {
        assert!(!Tracker::new().should_try(true, false, 1_000_000));
    }

    #[test]
    fn a_working_relay_or_headphones_already_here_mean_no() {
        let mut k = Tracker::new();
        k.observe(&state(Mode::Relay, Where::Phone, false, false), 0);
        assert!(!k.should_try(true, false, 60_000));
        let k = gone(0, Where::Pc, false);
        assert!(!k.should_try(true, false, 60_000));
    }

    #[test]
    fn the_switch_and_a_swap_in_progress_stop_it() {
        let k = gone(0, Where::None, false);
        assert!(!k.should_try(false, false, 60_000));
        assert!(!k.should_try(true, true, 60_000));
    }

    #[test]
    fn attempts_are_spaced_sooner_while_the_pc_plays() {
        let mut idle = gone(0, Where::None, false);
        idle.tried(30_000);
        assert!(!idle.should_try(true, false, 30_000 + RETRY_IDLE_MS - 1));
        assert!(idle.should_try(true, false, 30_000 + RETRY_IDLE_MS));
        let mut playing = gone(0, Where::None, true);
        playing.tried(30_000);
        assert!(!playing.should_try(true, false, 30_000 + RETRY_PLAYING_MS - 1));
        assert!(playing.should_try(true, false, 30_000 + RETRY_PLAYING_MS));
    }

    #[test]
    fn it_gives_up_after_the_attempt_limit_until_the_phone_comes_back() {
        let mut k = gone(0, Where::None, true);
        let mut now = 30_000;
        for _ in 0..MAX_ATTEMPTS {
            assert!(k.should_try(true, false, now));
            k.tried(now);
            now += RETRY_PLAYING_MS;
        }
        assert!(!k.should_try(true, false, now + 10_000_000));
        // The phone comes back: the count starts over for the next absence.
        k.observe(&state(Mode::Relay, Where::Phone, true, false), now);
        k.observe(&state(Mode::Off, Where::None, false, true), now + 100_000);
        assert!(!k.should_try(true, false, now + 100_000 + PHONE_GONE_AFTER_MS - 1));
        assert!(k.should_try(true, false, now + 100_000 + PHONE_GONE_AFTER_MS));
    }

    #[test]
    fn the_clock_of_the_absence_starts_when_the_link_went_down_not_at_each_event() {
        let mut k = Tracker::new();
        k.observe(&state(Mode::Off, Where::None, false, false), 1_000);
        k.observe(&state(Mode::Off, Where::None, false, true), 15_000); // another event, same absence
        assert!(k.should_try(true, false, 1_000 + PHONE_GONE_AFTER_MS));
    }
}
