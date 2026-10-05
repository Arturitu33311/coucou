// The Music pill's real-time visualizer: the levels of whatever is playing, from
// `cava`, the same tool and the same settings the Dynamic Music Pill GNOME
// extension uses.
//
// cava reads the sound card's output monitor (PulseAudio / PipeWire) and prints
// the level of 64 frequency bands, 60 times a second, as 16-bit numbers on its
// stdout. Only those levels are used — nothing is recorded, stored or sent — and
// cava runs only while the island can be seen and a track is playing: the island
// asks for it with `music_bars(true)` and stops it with `music_bars(false)`.
//
// Each frame is normalised against a rolling maximum, so a quiet track and a loud
// one fill the bars alike, and ~30 of them a second go to the island as one
// small event of 64 bytes.

use serde::Serialize;

/// Bands cava is asked for. The island groups them into as many bars as it draws.
pub const BANDS: usize = 64;

/// What one event carries.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Bars {
    /// 0–255 per band.
    pub bars: Vec<u8>,
    /// True after about half a second of silence: the bars are flat and dim.
    pub silent: bool,
}

/// The state of the normalisation, carried from frame to frame.
#[derive(Debug, Clone)]
pub struct Normaliser {
    rolling_max: f64,
    silent_frames: u32,
}

impl Default for Normaliser {
    fn default() -> Self {
        Self { rolling_max: 2000.0, silent_frames: 0 }
    }
}

impl Normaliser {
    /// One cava frame (raw 16-bit levels) → bars, exactly as the extension does:
    /// the rolling maximum rises at once and falls slowly (98 % old, 2 % new), never
    /// below 5000, and 30 quiet frames in a row mean silence.
    pub fn frame(&mut self, levels: &[u16]) -> Bars {
        let current_max = levels.iter().copied().max().unwrap_or(0).max(1) as f64;
        if current_max < 100.0 {
            self.silent_frames = self.silent_frames.saturating_add(1);
        } else {
            self.silent_frames = 0;
        }
        let silent = self.silent_frames >= 30;
        if silent {
            self.rolling_max = 2000.0;
            return Bars { bars: vec![0; levels.len()], silent: true };
        }
        if current_max > self.rolling_max {
            self.rolling_max = current_max;
        } else {
            self.rolling_max = self.rolling_max * 0.98 + current_max * 0.02;
        }
        let safe_max = self.rolling_max.max(5000.0);
        let bars = levels
            .iter()
            .map(|&v| ((f64::from(v) / safe_max).min(1.0) * 255.0).round() as u8)
            .collect();
        Bars { bars, silent: false }
    }
}

/// The cava configuration: the extension's, band for band.
pub fn config() -> String {
    format!(
        "[general]\n\
         bars = {BANDS}\n\
         framerate = 60\n\
         autosens = 1\n\
         lower_cutoff_freq = 50\n\
         higher_cutoff_freq = 8000\n\
         [smoothing]\n\
         monstercat = 1.5\n\
         waves = 0\n\
         noise_reduction = 60\n\
         gravity = 140\n\
         [input]\n\
         method = pulse\n\
         source = auto\n\
         [output]\n\
         method = raw\n\
         bit_format = 16bit\n\
         channels = mono\n\
         raw_target = /dev/stdout\n"
    )
}

/// Takes complete frames off the front of `buf` (each `BANDS` little-endian u16)
/// and returns the last one, if any. Bytes of a half-read frame stay in `buf`.
pub fn take_last_frame(buf: &mut Vec<u8>) -> Option<Vec<u16>> {
    let frame_bytes = BANDS * 2;
    let frames = buf.len() / frame_bytes;
    if frames == 0 {
        return None;
    }
    let start = (frames - 1) * frame_bytes;
    let last = buf[start..start + frame_bytes]
        .chunks_exact(2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .collect();
    buf.drain(..frames * frame_bytes);
    Some(last)
}

// ── The process (Linux) ───────────────────────────────────────────────────────

#[cfg(target_os = "linux")]
mod process {
    use super::*;
    use std::io::Read;
    use std::process::{Child, Command, Stdio};
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    use tauri::{AppHandle, Emitter};

    use crate::island::WINDOW_LABEL;
    use crate::log;

    struct Running {
        child: Child,
        config: std::path::PathBuf,
    }

    static RUNNING: Mutex<Option<Running>> = Mutex::new(None);

    fn find_cava() -> Option<std::path::PathBuf> {
        std::env::var_os("PATH").and_then(|paths| {
            std::env::split_paths(&paths).map(|d| d.join("cava")).find(|p| p.is_file())
        })
    }

    /// Starts cava. `Ok(false)` when it is not installed (the island then falls back
    /// to its animated bars); `Ok(true)` when it is running, or already was.
    pub fn start(app: &AppHandle) -> Result<bool, String> {
        let mut running = RUNNING.lock().unwrap();
        if running.is_some() {
            return Ok(true);
        }
        let Some(cava) = find_cava() else { return Ok(false) };

        // The config goes in the private runtime directory when there is one.
        let dir = crate::platform::relay_socket_path()
            .and_then(|p| p.parent().map(std::path::Path::to_path_buf))
            .unwrap_or_else(std::env::temp_dir);
        let config = dir.join(format!("coucou-cava-{}.conf", std::process::id()));
        std::fs::write(&config, super::config()).map_err(|e| e.to_string())?;

        let mut child = Command::new(cava)
            .arg("-p")
            .arg(&config)
            // cava's own stream must not show up as an app that is playing sound.
            .env("PULSE_PROP", "application.id=org.PulseAudio.pavucontrol")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| e.to_string())?;
        let mut stdout = child.stdout.take().ok_or("cava has no stdout")?;
        log::line("cava started (real-time visualizer)");

        let app = app.clone();
        std::thread::Builder::new()
            .name("coucou-cava".into())
            .spawn(move || {
                let mut buf: Vec<u8> = Vec::with_capacity(BANDS * 2 * 8);
                let mut chunk = [0u8; 4096];
                let mut norm = Normaliser::default();
                let mut last_emit = Instant::now() - Duration::from_secs(1);
                loop {
                    match stdout.read(&mut chunk) {
                        Ok(0) | Err(_) => break, // cava was stopped
                        Ok(n) => buf.extend_from_slice(&chunk[..n]),
                    }
                    if let Some(levels) = take_last_frame(&mut buf) {
                        let bars = norm.frame(&levels);
                        // cava runs at 60 fps; the bars do not need more than ~30.
                        if last_emit.elapsed() >= Duration::from_millis(30) {
                            last_emit = Instant::now();
                            let _ = app.emit_to(WINDOW_LABEL, "music-bars", bars);
                        }
                    }
                }
            })
            .map_err(|e| e.to_string())?;

        *running = Some(Running { child, config });
        Ok(true)
    }

    pub fn stop() {
        if let Some(mut r) = RUNNING.lock().unwrap().take() {
            let _ = r.child.kill();
            let _ = r.child.wait();
            let _ = std::fs::remove_file(&r.config);
            log::line("cava stopped");
        }
    }
}

/// Switches cava on (when it is installed) or off. Returns whether it is running.
#[tauri::command]
pub fn music_bars(app: tauri::AppHandle, on: bool) -> Result<bool, String> {
    #[cfg(target_os = "linux")]
    {
        if on {
            process::start(&app)
        } else {
            process::stop();
            Ok(false)
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (app, on);
        Ok(false)
    }
}

/// Called when the app quits or the Music pill goes off.
pub fn shutdown() {
    #[cfg(target_os = "linux")]
    process::stop();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_loud_frame_fills_the_bars_against_the_floor_of_5000() {
        let mut n = Normaliser::default();
        let mut levels = vec![0u16; BANDS];
        levels[3] = 2500; // half of the 5000 floor
        levels[9] = 8000; // above it: the rolling maximum rises to this at once
        let out = n.frame(&levels);
        assert!(!out.silent);
        assert_eq!(out.bars[9], 255);
        // 2500 against max(rolling 8000, 5000) = 8000
        assert_eq!(out.bars[3], (2500.0f64 / 8000.0 * 255.0).round() as u8);
        assert_eq!(out.bars[0], 0);
    }

    #[test]
    fn a_quiet_track_still_scales_against_the_5000_floor_not_its_own_peak() {
        let mut n = Normaliser::default();
        let levels = vec![1000u16; BANDS];
        let out = n.frame(&levels);
        // rolling max starts at 2000 and falls toward 1000; the floor is 5000
        assert_eq!(out.bars[0], (1000.0f64 / 5000.0 * 255.0).round() as u8);
    }

    #[test]
    fn thirty_quiet_frames_in_a_row_mean_silence_and_flat_bars() {
        let mut n = Normaliser::default();
        let quiet = vec![10u16; BANDS];
        let mut last = n.frame(&quiet);
        for _ in 0..29 {
            last = n.frame(&quiet);
        }
        assert!(last.silent);
        assert!(last.bars.iter().all(|&b| b == 0));
        // Any real sound ends it at once.
        let loud = vec![3000u16; BANDS];
        assert!(!n.frame(&loud).silent);
    }

    #[test]
    fn only_the_last_complete_frame_is_used_and_a_partial_one_waits() {
        let frame = |v: u16| -> Vec<u8> { (0..BANDS).flat_map(|_| v.to_le_bytes()).collect() };
        let mut buf = frame(111);
        buf.extend(frame(222));
        buf.extend(&frame(333)[..40]); // a frame cut in the middle
        let last = take_last_frame(&mut buf).unwrap();
        assert!(last.iter().all(|&v| v == 222));
        assert_eq!(buf.len(), 40, "the half frame must stay for the next read");
        assert!(take_last_frame(&mut buf).is_none());
        buf.extend(&frame(333)[40..]);
        assert!(take_last_frame(&mut buf).unwrap().iter().all(|&v| v == 333));
    }

    #[test]
    fn the_config_is_the_extensions() {
        let c = config();
        assert!(c.contains("bars = 64") && c.contains("framerate = 60"));
        assert!(c.contains("monstercat = 1.5") && c.contains("noise_reduction = 60"));
        assert!(c.contains("method = raw") && c.contains("channels = mono") && c.contains("bit_format = 16bit"));
    }
}
