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
/// How far ahead to seek (f64 bits, s), learnt from how late the video lands after a seek.
static LEAD: AtomicU64 = AtomicU64::new(0);
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
/// The last sync seeked: its leftover teaches LEAD.
static JUST_SEEKED: AtomicBool = AtomicBool::new(false);

/// Told to the page when the video went away on its own (the stream failed): it shows its scene again.
pub const GONE_EVENT: &str = "desktop-video-gone";

/// Off by less than this, the video is left alone (a seek costs a visible hitch).
pub const IN_SYNC_S: f64 = 0.25;
const MAX_LEAD_S: f64 = 1.5;

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

/// The seek target when the video at `video_s` is off from `want_s` by more than IN_SYNC_S, aimed `lead_s`
/// ahead (a seek lands that late); None when it is close enough.
pub fn correction(video_s: f64, want_s: f64, lead_s: f64) -> Option<f64> {
    ((want_s - video_s).abs() > IN_SYNC_S).then(|| want_s + lead_s)
}

/// The next lead after a seek left the video `behind_s` behind (negative: ahead).
pub fn learn_lead(lead_s: f64, behind_s: f64) -> f64 {
    (lead_s + behind_s * 0.7).clamp(0.0, MAX_LEAD_S)
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
        let found = match videod::find(title.trim(), &artists, length).await {
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
        JUST_SEEKED.store(false, Ordering::SeqCst);
        let start = video_time_for(position_now(position_s, playing, asked), video.offset) + load(&LEAD);
        match mpv::play_video(&video.url, wid, start).await {
            Ok(()) if current() => {
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
        let wanted = move || SEQ.load(Ordering::SeqCst) == seq && ACTIVE.load(Ordering::SeqCst);
        if mpv::video_gone(wanted).await && SEQ.load(Ordering::SeqCst) == seq {
            crate::log::line("desktop video: the stream stopped, back to the scene");
            ACTIVE.store(false, Ordering::SeqCst);
            show_window(&app, false);
            let _ = app.emit(GONE_EVENT, ());
        }
    }

    pub async fn pause(paused: bool) {
        if ACTIVE.load(Ordering::SeqCst) {
            mpv::video_pause(paused).await;
        }
    }

    pub async fn sync(position_s: f64) {
        if !ACTIVE.load(Ordering::SeqCst) || !position_s.is_finite() {
            return;
        }
        let Some(at) = mpv::video_time().await else { return };
        let want = video_time_for(position_s, load(&OFFSET));
        if JUST_SEEKED.swap(false, Ordering::SeqCst) {
            store(&LEAD, learn_lead(load(&LEAD), want - at));
        }
        if let Some(to) = correction(at, want, load(&LEAD)) {
            JUST_SEEKED.store(true, Ordering::SeqCst);
            mpv::video_seek(to).await;
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
    fn only_a_real_drift_seeks() {
        assert_eq!(correction(10.0, 10.2, 0.0), None, "a fifth of a second off: left alone");
        assert_eq!(correction(10.0, 14.0, 0.3), Some(14.3), "a late start: aimed a little ahead");
        assert_eq!(correction(40.0, 14.0, 0.0), Some(14.0), "ahead (the song was rewound): back");
    }

    #[test]
    fn the_lead_learns_from_late_seeks_and_stays_bounded() {
        let l = learn_lead(0.0, 0.6);
        assert!(l > 0.3 && l < 0.6);
        assert_eq!(learn_lead(1.4, 3.0), MAX_LEAD_S);
        assert_eq!(learn_lead(0.1, -1.0), 0.0);
    }
}
