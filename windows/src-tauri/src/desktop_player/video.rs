// The desktop player's background video: the playing song's official music video (found and resolved by
// player::videod), played without sound by a second mpv inside a window of our own that sits at desktop
// level just under the desktop player's window. That window is transparent where the page shows no
// scene (class `has-video`), so GNOME's compositor lays the page over the video: WebKit (run without
// compositing) never decodes or repaints a frame of it.
//
// The video follows the song's clock: it starts where the song is when it is ready (not where the song was
// when it was asked for), shifted by the video's own intro (`offset`), and the page asks it to catch up every
// few seconds (`desktop_video_sync`). It only shows once it plays; while it loads, or if it fails, the page
// keeps its own scene — never a black window.
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
/// Bumped by every request: an older one that finishes late changes nothing.
static SEQ: AtomicU64 = AtomicU64::new(0);
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
/// A video is playing in the video mpv (the window may be shown with the desktop player).
static ACTIVE: AtomicBool = AtomicBool::new(false);
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
/// The playing video's intro (f64 bits): video time = song time + offset.
static OFFSET: AtomicU64 = AtomicU64::new(0);
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
/// The smoothed drift (f64 bits, s; + = the video is behind), NaN until the first reading after a load or seek.
static DRIFT: AtomicU64 = AtomicU64::new(0x7FF8_0000_0000_0000);

/// A stream that fails (YouTube answers the odd stream URL with 403, or drops it mid-song) is resolved again
/// and reloaded where the song is, this many times per song.
pub const RETRIES: u32 = 2;

/// The song a video plays for, kept to reload it when its stream fails, and where the song was last seen.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
#[derive(Clone)]
struct Playing {
    seq: u64,
    title: String,
    artists: Vec<String>,
    length: f64,
    retries: u32,
    /// The song's position at the last sync, and when it was read (to tell where it is now).
    pos_s: f64,
    at: std::time::Instant,
    playing: bool,
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
static PLAYING: std::sync::Mutex<Option<Playing>> = std::sync::Mutex::new(None);

/// Told to the page when the video went away on its own (the stream failed): it shows its scene again.
pub const GONE_EVENT: &str = "desktop-video-gone";
/// Told to the page when a dropped video is playing again: it lets it show through.
pub const BACK_EVENT: &str = "desktop-video-back";

/// Players report their position in jumps (Sonora's MPRIS position is ±0.7 s off between reads), so a single
/// reading is never acted on: the drift is smoothed, small drifts are closed by playing the video a little
/// faster or slower (no visible jump), and only a large one (a late start, a seek in the song) seeks.
pub const SEEK_OVER_S: f64 = 1.2;
/// Below this the video plays at normal speed.
pub const NUDGE_FROM_S: f64 = 0.3;
/// At most this much faster or slower (4 % is not seen nor, with no sound, heard).
pub const MAX_NUDGE: f64 = 0.04;
/// A seek on a network stream lands about this late.
pub const SEEK_LEAD_S: f64 = 0.3;

fn load(a: &AtomicU64) -> f64 {
    f64::from_bits(a.load(Ordering::SeqCst))
}

fn store(a: &AtomicU64, v: f64) {
    a.store(v.to_bits(), Ordering::SeqCst)
}

/// Where the video should be for the song at `song_s`, given the video's intro.
pub fn video_time_for(song_s: f64, offset_s: f64) -> f64 {
    (song_s + offset_s).max(0.0)
}

/// The smoothed drift after a new reading `d` (NaN `prev`: the first one counts alone).
pub fn smooth_drift(prev: f64, d: f64) -> f64 {
    if prev.is_nan() {
        d
    } else {
        0.5 * prev + 0.5 * d
    }
}

#[derive(Debug, PartialEq)]
pub enum Fix {
    /// Jump to this time (and play at normal speed).
    Seek(f64),
    /// Play at this speed.
    Speed(f64),
}

/// What to do with the video, given this reading's drift `d`, the smoothed one and where it should be.
pub fn fix(d: f64, smoothed: f64, want_s: f64) -> Fix {
    if d.abs() > 2.0 * SEEK_OVER_S || smoothed.abs() > SEEK_OVER_S {
        Fix::Seek(want_s + SEEK_LEAD_S)
    } else if smoothed.abs() > NUDGE_FROM_S {
        Fix::Speed(1.0 + (smoothed * 0.05).clamp(-MAX_NUDGE, MAX_NUDGE))
    } else {
        Fix::Speed(1.0)
    }
}

/// Shows the song's music video, from where the song is (`position_s` when asked, plus the time it took
/// while `playing`); false when there is none (or `title` is empty: the video stops). `length_s` is the
/// song's length (0 = unknown): a video that does not run on the song's timeline is not used.
#[tauri::command]
pub async fn desktop_video(
    app: tauri::AppHandle,
    title: String,
    artists: Vec<String>,
    length_s: f64,
    position_s: f64,
    playing: bool,
) -> bool {
    imp::play(app, title, artists, length_s, position_s, playing).await
}

/// Pauses or resumes the video: the song paused, or the desktop player covered.
#[tauri::command]
pub async fn desktop_video_pause(paused: bool) {
    imp::pause(paused).await
}

/// The song is at `position_s`: the video catches up if it drifted (a pause, a seek, a late start).
#[tauri::command]
pub async fn desktop_video_sync(position_s: f64) {
    imp::sync(position_s).await
}

/// The desktop player was shown (true) or hidden: the video window goes with it.
pub fn follow_desktop(app: &tauri::AppHandle, shown: bool) {
    imp::follow_desktop(app, shown)
}

#[cfg(target_os = "linux")]
mod imp {
    use super::*;
    use crate::player::{mpv, videod};
    use gtk::prelude::*;
    use std::cell::RefCell;
    use tauri::{AppHandle, Emitter};

    thread_local! {
        /// The video window (GTK: main thread only), made on first use and kept for the session.
        static WINDOW: RefCell<Option<gtk::Window>> = const { RefCell::new(None) };
    }

    fn position_now(position_s: f64, playing: bool, asked: std::time::Instant) -> f64 {
        let p = if position_s.is_finite() { position_s.max(0.0) } else { 0.0 };
        if playing {
            p + asked.elapsed().as_secs_f64()
        } else {
            p
        }
    }

    /// Where the song is now, from the last sync (or the request).
    fn song_now() -> Option<(f64, bool)> {
        let p = PLAYING.lock().ok()?.clone()?;
        Some((position_now(p.pos_s, p.playing, p.at), p.playing))
    }

    pub async fn play(
        app: AppHandle,
        title: String,
        artists: Vec<String>,
        length_s: f64,
        position_s: f64,
        playing: bool,
    ) -> bool {
        let asked = std::time::Instant::now();
        let seq = SEQ.fetch_add(1, Ordering::SeqCst) + 1;
        let current = move || SEQ.load(Ordering::SeqCst) == seq;
        // The old song's video goes at once: the page's scene stands in until the new one plays.
        ACTIVE.store(false, Ordering::SeqCst);
        show_window(&app, false);
        if title.trim().is_empty() {
            mpv::video_stop().await;
            return false;
        }
        let length = if length_s.is_finite() { length_s.max(0.0) } else { 0.0 };
        let found = match videod::find(title.trim(), &artists, length, false).await {
            Ok(v) => v,
            Err(e) => {
                crate::log::line(format!("desktop video: {title}: {e}"));
                None
            }
        };
        if !current() {
            return false;
        }
        let Some(video) = found else {
            mpv::video_stop().await;
            return false;
        };
        let Some(wid) = window_xid(&app) else { return false };
        store(&OFFSET, video.offset);
        store(&DRIFT, f64::NAN);
        if let Ok(mut p) = PLAYING.lock() {
            *p = Some(Playing {
                seq,
                title: title.trim().to_string(),
                artists: artists.clone(),
                length,
                retries: 0,
                pos_s: position_s,
                at: asked,
                playing,
            });
        }
        let start = video_time_for(position_now(position_s, playing, asked), video.offset) + SEEK_LEAD_S;
        let mut video = video;
        let mut opened = mpv::play_video(&video.url, wid, start).await;
        if opened.is_err() && current() {
            // the stream URL did not open (a 403 now and then): resolve a new one, once
            crate::log::line(format!("desktop video: {title}: the stream did not open, resolving it again"));
            if let Ok(Some(v)) = videod::find(title.trim(), &artists, length, true).await {
                if current() {
                    let start = video_time_for(position_now(position_s, playing, asked), v.offset) + SEEK_LEAD_S;
                    opened = mpv::play_video(&v.url, wid, start).await;
                    video = v;
                }
            }
        }
        match opened {
            Ok(()) if current() => {
                mpv::video_speed(1.0).await;
                if !playing {
                    mpv::video_pause(true).await;
                }
                ACTIVE.store(true, Ordering::SeqCst);
                show_window(&app, crate::desktop_player::window::is_shown());
                if !video.measured {
                    // shown from the song's own time; its real offset (a short intro) arrives later and the
                    // next sync moves the video there
                    let (t, a, id) = (title.trim().to_string(), artists.clone(), video.id.clone());
                    tauri::async_runtime::spawn(async move {
                        match videod::refine(&t, &a, length, &id).await {
                            Ok(Some(off)) if current() => store(&OFFSET, off),
                            Ok(_) => {}
                            Err(e) => crate::log::line(format!("desktop video: {t}: offset: {e}")),
                        }
                    });
                }
                tauri::async_runtime::spawn(watch(app, seq));
                true
            }
            Ok(()) => false,
            Err(e) => {
                crate::log::line(format!("desktop video: {title}: {e}"));
                if current() {
                    mpv::video_stop().await;
                }
                false
            }
        }
    }

    /// Hides the window and tells the page when the video fails on its own, so it never sits there black.
    async fn watch(app: AppHandle, seq: u64) {
        let current = move || SEQ.load(Ordering::SeqCst) == seq;
        loop {
            let wanted = move || current() && ACTIVE.load(Ordering::SeqCst);
            if !mpv::video_gone(wanted).await || !current() {
                return;
            }
            ACTIVE.store(false, Ordering::SeqCst);
            show_window(&app, false);
            let _ = app.emit(GONE_EVENT, ());
            // a dropped stream: resolve it again and carry on where the song is (the scene meanwhile)
            let Some(p) = PLAYING.lock().ok().and_then(|g| g.clone()).filter(|p| p.seq == seq) else { return };
            if p.retries >= RETRIES {
                crate::log::line("desktop video: the stream stopped again, the scene stays");
                return;
            }
            if let Ok(mut g) = PLAYING.lock() {
                if let Some(q) = g.as_mut().filter(|q| q.seq == seq) {
                    q.retries += 1;
                }
            }
            crate::log::line(format!("desktop video: {}: the stream stopped, resolving it again", p.title));
            let Ok(Some(v)) = videod::find(&p.title, &p.artists, p.length, true).await else { return };
            let Some(wid) = window_xid(&app) else { return };
            let Some((pos, playing)) = song_now() else { return };
            if !current() {
                return;
            }
            store(&OFFSET, v.offset);
            store(&DRIFT, f64::NAN);
            if mpv::play_video(&v.url, wid, video_time_for(pos, v.offset) + SEEK_LEAD_S).await.is_err() || !current() {
                return;
            }
            if !playing {
                mpv::video_pause(true).await;
            }
            ACTIVE.store(true, Ordering::SeqCst);
            show_window(&app, crate::desktop_player::window::is_shown());
            let _ = app.emit(BACK_EVENT, ());
        }
    }

    pub async fn pause(paused: bool) {
        if ACTIVE.load(Ordering::SeqCst) {
            mpv::video_pause(paused).await;
        }
    }

    pub async fn sync(position_s: f64) {
        if !position_s.is_finite() {
            return;
        }
        if let Ok(mut g) = PLAYING.lock() {
            if let Some(p) = g.as_mut() {
                p.pos_s = position_s;
                p.at = std::time::Instant::now();
                p.playing = true;
            }
        }
        if !ACTIVE.load(Ordering::SeqCst) {
            return;
        }
        let Some(at) = mpv::video_time().await else { return };
        let want = video_time_for(position_s, load(&OFFSET));
        let d = want - at;
        let smoothed = smooth_drift(load(&DRIFT), d);
        match fix(d, smoothed, want) {
            Fix::Seek(to) => {
                store(&DRIFT, f64::NAN);
                mpv::video_speed(1.0).await;
                mpv::video_seek(to).await;
            }
            Fix::Speed(speed) => {
                store(&DRIFT, smoothed);
                mpv::video_speed(speed).await;
            }
        }
    }

    pub fn follow_desktop(app: &AppHandle, shown: bool) {
        if ACTIVE.load(Ordering::SeqCst) {
            show_window(app, shown);
        }
        if !shown {
            tauri::async_runtime::spawn(pause(true));
        }
    }

    /// The video window's X id, making the window (hidden, desktop level, the primary monitor's size)
    /// on the main thread the first time.
    fn window_xid(app: &AppHandle) -> Option<u64> {
        let geo = crate::desktop_player::window::primary_geometry(app)?;
        let (tx, rx) = std::sync::mpsc::channel();
        let app2 = app.clone();
        app.run_on_main_thread(move || {
            let xid = WINDOW.with(|cell| {
                let mut slot = cell.borrow_mut();
                let win = slot.get_or_insert_with(|| {
                    let w = gtk::Window::new(gtk::WindowType::Toplevel);
                    w.set_title("Coucou video");
                    w.set_type_hint(gtk::gdk::WindowTypeHint::Desktop);
                    w.set_skip_taskbar_hint(true);
                    w.set_skip_pager_hint(true);
                    w.set_accept_focus(false);
                    w.set_decorated(false);
                    w.set_app_paintable(true);
                    w.move_(geo.x as i32, geo.y as i32);
                    w.set_default_size(geo.w as i32, geo.h as i32);
                    // GNOME stacks a window that maps above the desktop-level ones already there: once this
                    // one is really mapped, the player goes back over it (raising before that loses the race).
                    let app3 = app2.clone();
                    w.connect_map_event(move |_, _| {
                        crate::desktop_player::window::raise(&app3);
                        gtk::glib::Propagation::Proceed
                    });
                    w.realize();
                    w
                });
                win.window().and_then(|g| g.downcast::<gdkx11::X11Window>().ok()).map(|g| g.xid())
            });
            let _ = tx.send(xid);
        })
        .ok()?;
        let xid = rx.recv_timeout(std::time::Duration::from_secs(3)).ok().flatten()?;
        // The stacking watcher must know it: the video window may never stay above the player.
        crate::desktop_player::visibility::set_video_xid(xid as u32);
        Some(xid)
    }

    /// Shows the video window under the desktop player (then raises the player over it), or hides it.
    fn show_window(app: &AppHandle, shown: bool) {
        let app2 = app.clone();
        let _ = app.run_on_main_thread(move || {
            WINDOW.with(|cell| {
                let Some(win) = cell.borrow().clone() else { return };
                if shown {
                    win.show();
                    crate::desktop_player::window::raise(&app2);
                } else {
                    win.hide();
                }
            });
        });
    }
}

#[cfg(not(target_os = "linux"))]
mod imp {
    pub async fn play(
        _app: tauri::AppHandle,
        _title: String,
        _artists: Vec<String>,
        _length_s: f64,
        _position_s: f64,
        _playing: bool,
    ) -> bool {
        false
    }
    pub async fn pause(_paused: bool) {}
    pub async fn sync(_position_s: f64) {}
    pub fn follow_desktop(_app: &tauri::AppHandle, _shown: bool) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_video_runs_on_the_songs_clock_plus_its_intro() {
        assert_eq!(video_time_for(30.0, 0.0), 30.0);
        assert_eq!(video_time_for(30.0, 22.6), 52.6);
        assert_eq!(video_time_for(-1.0, 0.0), 0.0);
    }

    #[test]
    fn a_late_start_or_a_seek_in_the_song_seeks() {
        assert_eq!(fix(3.6, smooth_drift(f64::NAN, 3.6), 40.0), Fix::Seek(40.0 + SEEK_LEAD_S), "3.6 s late");
        assert_eq!(fix(-60.0, smooth_drift(0.2, -60.0), 14.0), Fix::Seek(14.0 + SEEK_LEAD_S), "rewound");
    }

    #[test]
    fn jittery_readings_never_seek_and_small_drifts_only_nudge_the_speed() {
        // Sonora's position read live against a steady video: +0.21 +0.40 +1.09 +0.37 +0.61 -0.06 +0.26 +0.41
        let mut s = f64::NAN;
        for d in [-0.21, -0.40, -1.09, -0.37, -0.61, 0.06, -0.26, -0.41] {
            s = smooth_drift(s, d);
            match fix(d, s, 100.0) {
                Fix::Seek(_) => panic!("a jitter of {d} s made it seek"),
                Fix::Speed(v) => assert!((1.0 - MAX_NUDGE..=1.0 + MAX_NUDGE).contains(&v)),
            }
        }
        assert_eq!(fix(0.1, 0.1, 5.0), Fix::Speed(1.0), "in sync: normal speed");
        assert_eq!(fix(0.8, 0.8, 5.0), Fix::Speed(1.04), "behind: a little faster");
        assert_eq!(fix(-0.5, -0.5, 5.0), Fix::Speed(0.975), "ahead: a little slower");
    }
}
