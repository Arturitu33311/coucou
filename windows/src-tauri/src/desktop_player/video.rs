// The desktop player's background video: the playing song's music video (found on YouTube Music by
// "artist title"), played without sound by a second mpv inside a window of our own that sits at desktop
// level just under the desktop player's window. That window is transparent where the page shows no
// scene (class `has-video`), so GNOME's compositor lays the page over the video: WebKit (run without
// compositing) never decodes or repaints a frame of it.
use std::sync::atomic::{AtomicBool, AtomicU64};

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
/// Bumped by every request: an older one that finishes late changes nothing.
static SEQ: AtomicU64 = AtomicU64::new(0);
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
/// A video is loaded in the video mpv (the window may be shown with the desktop player).
static ACTIVE: AtomicBool = AtomicBool::new(false);

/// Shows the music video for `title` by `artist`, from `position_s` seconds in; false when there is
/// none (or `title` is empty: the video stops). The page shows its own scene while this is false.
#[tauri::command]
pub async fn desktop_video(app: tauri::AppHandle, title: String, artist: String, position_s: f64) -> bool {
    imp::play(app, title, artist, position_s).await
}

/// Pauses or resumes the video: the song paused, or the desktop player covered.
#[tauri::command]
pub async fn desktop_video_pause(paused: bool) {
    imp::pause(paused).await
}

/// The desktop player was shown (true) or hidden: the video window goes with it.
pub fn follow_desktop(app: &tauri::AppHandle, shown: bool) {
    imp::follow_desktop(app, shown)
}

#[cfg(target_os = "linux")]
mod imp {
    use super::{ACTIVE, SEQ};
    use crate::player::{mpv, ytmusic};
    use gtk::prelude::*;
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::sync::atomic::Ordering;
    use std::sync::Mutex;
    use tauri::AppHandle;

    thread_local! {
        /// The video window (GTK: main thread only), made on first use and kept for the session.
        static WINDOW: RefCell<Option<gtk::Window>> = const { RefCell::new(None) };
    }

    /// "artist title" → its video id (None: YouTube Music has none), for the session.
    static FOUND: Mutex<Option<HashMap<String, Option<String>>>> = Mutex::new(None);

    async fn find(query: &str) -> Option<String> {
        if let Some(hit) = FOUND.lock().ok()?.get_or_insert_with(HashMap::new).get(query) {
            return hit.clone();
        }
        // A failure (no network) is not remembered: the next song asks again.
        let id = ytmusic::video(query).await.map_err(|e| eprintln!("desktop video: search failed: {e}")).ok()?;
        FOUND.lock().ok()?.get_or_insert_with(HashMap::new).insert(query.to_string(), id.clone());
        id
    }

    pub async fn play(app: AppHandle, title: String, artist: String, position_s: f64) -> bool {
        let seq = SEQ.fetch_add(1, Ordering::SeqCst) + 1;
        let current = || SEQ.load(Ordering::SeqCst) == seq;
        let stop = |app: &AppHandle| {
            ACTIVE.store(false, Ordering::SeqCst);
            show_window(app, false);
        };
        if title.trim().is_empty() {
            stop(&app);
            mpv::video_stop().await;
            return false;
        }
        let query = format!("{} {}", artist.trim(), title.trim());
        let Some(id) = find(&query).await else {
            if current() {
                stop(&app);
                mpv::video_stop().await;
            }
            return false;
        };
        if !current() {
            return false;
        }
        let Some(wid) = window_xid(&app) else { return false };
        let start = if position_s.is_finite() && position_s > 0.0 { position_s as u64 } else { 0 };
        match mpv::play_video(&id, wid, start).await {
            Ok(()) if current() => {
                ACTIVE.store(true, Ordering::SeqCst);
                show_window(&app, crate::desktop_player::window::is_shown());
                true
            }
            Ok(()) => false,
            Err(e) => {
                crate::log::line(format!("desktop video: {query}: {e}"));
                if current() {
                    stop(&app);
                }
                false
            }
        }
    }

    pub async fn pause(paused: bool) {
        if ACTIVE.load(Ordering::SeqCst) {
            mpv::video_pause(paused).await;
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
                    w.realize();
                    w
                });
                win.window().and_then(|g| g.downcast::<gdkx11::X11Window>().ok()).map(|g| g.xid())
            });
            let _ = tx.send(xid);
        })
        .ok()?;
        rx.recv_timeout(std::time::Duration::from_secs(3)).ok().flatten()
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
    pub async fn play(_app: tauri::AppHandle, _title: String, _artist: String, _position_s: f64) -> bool {
        false
    }
    pub async fn pause(_paused: bool) {}
    pub fn follow_desktop(_app: &tauri::AppHandle, _shown: bool) {}
}
