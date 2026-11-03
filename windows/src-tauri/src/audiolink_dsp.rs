//! Loudness for the audio relay: a slow automatic gain followed by a look-ahead limiter. Pure (no I/O).
//!
//! Why: the relay used to add a fixed +6 dB and bend everything above 70 % of full scale with a per-sample `tanh`.
//! A desktop plays quietly into its own speakers while music on the phone is mastered near full scale, so the PC's
//! sound arrived too low; raising the fixed gain pushed more samples into the bend and the result distorted.
//! A fixed gain cannot be right for both a quiet video call and a loud track.
//!
//! What it does now:
//!  1. Slow automatic gain: it follows the loudness IN dB, at a limited speed (`LEVEL_SLEW_DB_PER_S`), and lifts what
//!     arrives quiet towards `TARGET_DB`; it never attenuates, never lifts more than `MAX_GAIN_DB`, and holds still on
//!     silence (so it never boosts noise). It starts at +6 dB. Following dB at a limited speed (a log-domain average)
//!     makes it robust to short loud events: a slam or an explosion moves the estimate by a dB or two instead of
//!     dominating it, so the quiet speech after it is NOT ducked (an earlier version measured plain RMS power and
//!     dropped quiet speech by 8 dB for seconds after any loud sound, which sounded like the audio cutting out).
//!     The peaks of such events are the limiter's job.
//!  2. Look-ahead limiter: sees `LOOKAHEAD` frames ahead and lowers the gain *smoothly before* a peak arrives, so
//!     nothing exceeds `CEILING`. A smoothly varying gain does not add harmonics the way clipping or a waveshaper does.
//!
//! Format: 16-bit little-endian stereo interleaved, 48 kHz. State carries over from one chunk to the next and the
//! result does not depend on how the stream is cut into chunks (the automatic gain is updated every `BLOCK` frames).
//! The output is delayed by `LOOKAHEAD - 1` frames (about 5 ms); its first frames are silence.

use std::collections::VecDeque;

pub const RATE: f32 = 48_000.0;
const CHANNELS: usize = 2;
/// Loudest sample the limiter lets through, as a fraction of full scale (about -0.45 dBFS).
pub const CEILING: f32 = 0.95;
/// Frames the limiter looks ahead (5 ms).
const LOOKAHEAD: usize = 240;
/// The loudness the automatic gain aims for, in dBFS of RMS.
pub const TARGET_DB: f32 = -17.0;
/// The most the automatic gain will lift anything, in dB.
pub const MAX_GAIN_DB: f32 = 18.0;
/// Below this RMS (dBFS) the signal counts as silence: the automatic gain holds still.
const GATE_DB: f32 = -55.0;
/// How fast the loudness estimate may move, in dB per second, up or down. Slow on purpose (see the module docs).
const LEVEL_SLEW_DB_PER_S: f32 = 4.0;
/// How fast the applied gain follows what the estimate asks for, in dB per second.
const GAIN_SLEW_DB_PER_S: f32 = 6.0;
/// Where the automatic gain starts: what the relay used to add as a fixed gain, so it never starts worse than before.
const INITIAL_GAIN_DB: f32 = 6.0;
/// The automatic gain is updated every this many frames (1.33 ms).
const BLOCK: usize = 64;

pub struct Leveler {
    // Automatic gain.
    /// Short-term power (about 60 ms), the raw material of the dB estimate.
    mean_square: f32,
    ms_alpha: f32,
    /// The loudness estimate in dBFS, moved at a limited speed; None until the first sound above the gate.
    level_db: Option<f32>,
    gain_db: f32,
    gain_lin: f32,
    block_frames: usize,
    // Look-ahead limiter. All queues hold one entry per frame.
    delay: VecDeque<[f32; CHANNELS]>,
    need_window: VecDeque<(u64, f32)>, // (frame index, needed gain) — a monotonic deque for the sliding minimum
    min_ring: VecDeque<f32>,           // the last LOOKAHEAD window-minimums, averaged for a smooth attack
    min_sum: f32,
    limit_gain: f32,
    release_coef: f32,
    frame_no: u64,
}

impl Leveler {
    pub fn new() -> Self {
        Leveler {
            mean_square: 0.0,
            ms_alpha: 1.0 - (-1.0 / (0.06 * RATE)).exp(),
            level_db: None,
            gain_db: INITIAL_GAIN_DB,
            gain_lin: 1.995_262_3, // 10^(6/20)
            block_frames: 0,
            delay: VecDeque::with_capacity(LOOKAHEAD),
            need_window: VecDeque::new(),
            min_ring: VecDeque::new(),
            min_sum: 0.0,
            limit_gain: 1.0,
            release_coef: 1.0 - (-1.0 / (0.10 * RATE)).exp(),
            frame_no: 0,
        }
    }

    /// The gain the automatic stage is applying right now, in dB (tests only).
    #[cfg(test)]
    pub fn auto_gain_db(&self) -> f32 {
        self.gain_db
    }

    /// Processes `chunk` in place (16-bit LE stereo). A trailing partial frame is left untouched.
    pub fn process(&mut self, chunk: &mut [u8]) {
        for frame in chunk.chunks_exact_mut(CHANNELS * 2) {
            let l = i16::from_le_bytes([frame[0], frame[1]]) as f32 / 32768.0;
            let r = i16::from_le_bytes([frame[2], frame[3]]) as f32 / 32768.0;
            let out = self.frame([l, r]);
            for (c, v) in out.iter().enumerate() {
                let s = (v * 32768.0).round().clamp(-32767.0, 32767.0) as i16;
                frame[c * 2..c * 2 + 2].copy_from_slice(&s.to_le_bytes());
            }
        }
    }

    fn frame(&mut self, x: [f32; CHANNELS]) -> [f32; CHANNELS] {
        // 1. Automatic gain. The loudness is measured on the INPUT, the gain is applied per frame.
        let ms = (x[0] * x[0] + x[1] * x[1]) * 0.5;
        self.mean_square += self.ms_alpha * (ms - self.mean_square);
        self.block_frames += 1;
        if self.block_frames >= BLOCK {
            self.block_frames = 0;
            self.update_auto_gain();
        }
        let boosted = [x[0] * self.gain_lin, x[1] * self.gain_lin];

        // 2. Look-ahead limiter. `need` is the gain this frame alone would need to stay under the ceiling.
        let peak = boosted[0].abs().max(boosted[1].abs());
        let need = if peak > CEILING { CEILING / peak } else { 1.0 };
        let n = self.frame_no;
        self.frame_no += 1;
        while self.need_window.back().is_some_and(|&(_, v)| v >= need) {
            self.need_window.pop_back();
        }
        self.need_window.push_back((n, need));
        while self.need_window.front().is_some_and(|&(i, _)| i + (LOOKAHEAD as u64) <= n) {
            self.need_window.pop_front();
        }
        let window_min = self.need_window.front().map_or(1.0, |&(_, v)| v);
        // Smooth the attack: the average of the last LOOKAHEAD window-minimums. Every one of those windows contains
        // the frame being output, so the average can never be above what that frame needs.
        self.min_ring.push_back(window_min);
        self.min_sum += window_min;
        if self.min_ring.len() > LOOKAHEAD {
            self.min_sum -= self.min_ring.pop_front().unwrap_or(0.0);
        }
        let attack_gain = self.min_sum / self.min_ring.len() as f32;
        // Release: gain comes back up slowly (100 ms), goes down at once (the average is already smooth).
        self.limit_gain = if attack_gain < self.limit_gain {
            attack_gain
        } else {
            self.limit_gain + (attack_gain - self.limit_gain) * self.release_coef
        };

        self.delay.push_back(boosted);
        if self.delay.len() < LOOKAHEAD {
            return [0.0; CHANNELS]; // filling the look-ahead: the first ~5 ms are silence
        }
        let d = self.delay.pop_front().unwrap_or([0.0; CHANNELS]);
        [d[0] * self.limit_gain, d[1] * self.limit_gain]
    }

    fn update_auto_gain(&mut self) {
        let short_db = 10.0 * self.mean_square.max(1e-12).log10();
        if short_db < GATE_DB {
            return; // silence: hold everything, never raise the gain on noise
        }
        let dt = BLOCK as f32 / RATE;
        // The estimate chases the short-term level at a limited speed. The first sound sets it at once.
        let level = match self.level_db {
            None => short_db,
            Some(l) => {
                let step = LEVEL_SLEW_DB_PER_S * dt;
                l + (short_db - l).clamp(-step, step)
            }
        };
        self.level_db = Some(level);
        let desired = (TARGET_DB - level).clamp(0.0, MAX_GAIN_DB);
        let step = GAIN_SLEW_DB_PER_S * dt;
        self.gain_db += (desired - self.gain_db).clamp(-step, step);
        self.gain_lin = 10f32.powf(self.gain_db / 20.0);
    }
}

impl Default for Leveler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(amp: f32, hz: f32, secs: f32, start: usize) -> Vec<u8> {
        let n = (secs * RATE) as usize;
        let mut out = Vec::with_capacity(n * 4);
        for i in start..start + n {
            let v = (amp * (2.0 * std::f32::consts::PI * hz * i as f32 / RATE).sin() * 32768.0).round() as i16;
            out.extend_from_slice(&v.to_le_bytes());
            out.extend_from_slice(&v.to_le_bytes());
        }
        out
    }

    fn left(bytes: &[u8]) -> Vec<f32> {
        bytes.chunks_exact(4).map(|f| i16::from_le_bytes([f[0], f[1]]) as f32 / 32768.0).collect()
    }

    fn rms_db(x: &[f32]) -> f32 {
        10.0 * (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).max(1e-12).log10()
    }

    fn run(l: &mut Leveler, mut bytes: Vec<u8>, chunk_bytes: usize) -> Vec<u8> {
        for c in bytes.chunks_mut(chunk_bytes) {
            l.process(c);
        }
        bytes
    }

    #[test]
    fn a_quiet_source_is_lifted_towards_the_target_without_clipping() {
        let mut l = Leveler::new();
        let out = left(&run(&mut l, tone(0.03, 440.0, 14.0, 0), 3840)); // about -33 dBFS RMS
        let tail = &out[out.len() - 48_000..];
        let level = rms_db(tail);
        assert!((level - TARGET_DB).abs() < 1.5, "settled at {level:.1} dB, wanted {TARGET_DB}");
        assert!(out.iter().all(|v| v.abs() <= CEILING + 0.001));
    }

    #[test]
    fn the_automatic_gain_never_exceeds_its_maximum() {
        let mut l = Leveler::new();
        run(&mut l, tone(0.002, 440.0, 40.0, 0), 3840); // very quiet but above the gate? -57 dBFS RMS: held
        assert!(l.auto_gain_db() <= MAX_GAIN_DB + 0.001);
        let mut l = Leveler::new();
        run(&mut l, tone(0.006, 440.0, 40.0, 0), 3840); // -49 dBFS RMS: wants +32, gets the cap
        assert!((l.auto_gain_db() - MAX_GAIN_DB).abs() < 0.01, "{}", l.auto_gain_db());
    }

    #[test]
    fn a_loud_source_is_not_attenuated_and_never_clips() {
        let mut l = Leveler::new();
        let input = tone(0.99, 220.0, 6.0, 0);
        let out = left(&run(&mut l, input.clone(), 3840));
        assert!(out.iter().all(|v| v.abs() <= CEILING + 0.001), "peak {}", out.iter().fold(0f32, |m, v| m.max(v.abs())));
        assert!(rms_db(&out[48_000..]) > -4.5, "a loud passage must stay loud: {:.1} dB", rms_db(&out[48_000..]));
        assert!(l.auto_gain_db() < 0.01, "already loud: no lift, got {}", l.auto_gain_db());
    }

    #[test]
    fn a_sudden_burst_is_caught_before_it_arrives() {
        let mut l = Leveler::new();
        let mut input = tone(0.05, 300.0, 3.0, 0);
        input.extend(tone(1.0, 300.0, 0.05, 144_000)); // full-scale burst after 3 s
        input.extend(tone(0.05, 300.0, 1.0, 146_400));
        let out = left(&run(&mut l, input, 3840));
        assert!(out.iter().all(|v| v.abs() <= CEILING + 0.001), "peak {}", out.iter().fold(0f32, |m, v| m.max(v.abs())));
    }

    #[test]
    fn limiting_does_not_distort_a_steady_tone() {
        // A tone at 0.99 is limited to 0.95: the gain is a smooth 0.96, so the result must still be a clean sine.
        let mut l = Leveler::new();
        let out = left(&run(&mut l, tone(0.99, 1000.0, 3.0, 0), 3840));
        let seg = &out[96_000..96_000 + 48_000]; // exactly 1000 cycles
        let (mut s, mut c) = (0.0f64, 0.0f64);
        for (i, v) in seg.iter().enumerate() {
            let ph = 2.0 * std::f64::consts::PI * 1000.0 * (i + 96_000) as f64 / RATE as f64;
            s += *v as f64 * ph.sin();
            c += *v as f64 * ph.cos();
        }
        let n = seg.len() as f64;
        let (a, b) = (2.0 * s / n, 2.0 * c / n);
        let resid: f64 = seg
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let ph = 2.0 * std::f64::consts::PI * 1000.0 * (i + 96_000) as f64 / RATE as f64;
                let e = *v as f64 - (a * ph.sin() + b * ph.cos());
                e * e
            })
            .sum::<f64>()
            / n;
        let fund = (a * a + b * b) / 2.0;
        let thd_db = 10.0 * (resid / fund).max(1e-20).log10();
        assert!(thd_db < -45.0, "distortion {thd_db:.1} dB relative to the tone");
    }

    #[test]
    fn silence_stays_silent_and_does_not_boost_noise() {
        let mut l = Leveler::new();
        let out = run(&mut l, vec![0u8; 3840 * 200], 3840);
        assert!(out.iter().all(|&b| b == 0));
        let hiss = tone(0.0008, 5000.0, 5.0, 0); // about -65 dBFS: under the gate
        run(&mut l, hiss, 3840);
        assert!((l.auto_gain_db() - INITIAL_GAIN_DB).abs() < 0.01, "noise must not be lifted: {}", l.auto_gain_db());
    }

    #[test]
    fn the_result_does_not_depend_on_how_the_stream_is_cut() {
        let input = {
            let mut v = tone(0.04, 330.0, 2.0, 0);
            v.extend(tone(0.9, 330.0, 0.3, 96_000));
            v.extend(tone(0.04, 330.0, 1.0, 110_400));
            v
        };
        let a = run(&mut Leveler::new(), input.clone(), 3840);
        let b = run(&mut Leveler::new(), input, 12); // three frames at a time
        assert_eq!(a, b);
    }

    /// RMS in dB of `secs` of the left channel starting at `from_s`.
    fn window_db(out: &[f32], from_s: f32, secs: f32) -> f32 {
        let a = (from_s * RATE) as usize;
        rms_db(&out[a..a + (secs * RATE) as usize])
    }

    #[test]
    fn quiet_speech_is_not_ducked_after_a_loud_event() {
        // The symptom reported on the relay: "it cuts a lot". Quiet speech-level sound with a short loud event
        // (a door slam, an explosion, a notification) every few seconds. Right after each event the speech must
        // not fall away: that sounds like the sound cutting out.
        let mut l = Leveler::new();
        let mut input = Vec::new();
        let mut t0 = 0usize;
        let mut push = |amp: f32, secs: f32, input: &mut Vec<u8>| {
            input.extend(tone(amp, 300.0, secs, t0));
            t0 += (secs * RATE) as usize;
        };
        push(0.03, 8.0, &mut input);          // settle on quiet speech-level sound (8 s)
        push(0.8, 0.3, &mut input);           // loud event, 8.0 - 8.3 s
        push(0.03, 6.0, &mut input);          // speech again
        let out = left(&run(&mut l, input, 3840));
        let before = window_db(&out, 7.0, 1.0);
        for (after_s, label) in [(8.5, "0.2 s"), (9.0, "0.7 s"), (10.0, "1.7 s"), (11.0, "2.7 s")] {
            let now = window_db(&out, after_s, 0.2);
            assert!(before - now < 3.0, "speech {label} after the event is {:.1} dB below its level before ({before:.1} -> {now:.1})", before - now);
        }
    }

    #[test]
    fn gain_recovers_after_a_loud_passage_within_a_few_seconds() {
        let mut l = Leveler::new();
        let mut input = tone(0.02, 400.0, 5.0, 0);
        input.extend(tone(1.0, 400.0, 0.5, 240_000));
        input.extend(tone(0.02, 400.0, 6.0, 264_000));
        let out = left(&run(&mut l, input, 3840));
        // After the burst, the quiet part must again be lifted (not stuck low): compare the last second to the settled level.
        let last = rms_db(&out[out.len() - 48_000..]);
        assert!(last > TARGET_DB - 3.0, "still low after recovery: {last:.1} dB");
    }
}
