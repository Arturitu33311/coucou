// The desktop player's window: created once, hidden, then only shown and hidden.

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{AppHandle, Emitter, Manager, WebviewWindow};

use super::{geometry_scales_to_logical, primary_monitor_rect, Geometry, MonitorInfo};

pub const LABEL: &str = "desktop-player";

/// Event the island follows: whether the player's window really is on screen
/// (made and mapped), so the player shortcut, the magnifier and `--player` go to
/// it only then, and to the notch otherwise. A name of its own: Desktop Mochi's
/// window has its own `desktop-visible`, which the island also hears.
pub const SHOWN_EVENT: &str = "desktop-player-shown";

static SHOWN: AtomicBool = AtomicBool::new(false);

/// Whether the window is on screen now (always false where there is none).
pub fn is_shown() -> bool {
    SHOWN.load(Ordering::Relaxed)
}

/// Records what is on screen and tells the island when that changed.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn announce(app: &AppHandle, shown: bool) {
    if SHOWN.swap(shown, Ordering::Relaxed) != shown {
        let _ = app.emit(SHOWN_EVENT, shown);
    }
}

pub fn window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window(LABEL)
}

/// Every monitor, the primary one marked (Tauri's list carries no such flag:
/// it is matched against `primary_monitor` by place and size).
fn monitors(app: &AppHandle) -> Vec<MonitorInfo> {
    let primary = app.primary_monitor().ok().flatten().map(|m| (*m.position(), *m.size()));
    let Ok(list) = app.available_monitors() else { return Vec::new() };
    list.iter()
        .map(|m| {
            let (p, s) = (*m.position(), *m.size());
            MonitorInfo {
                x: p.x,
                y: p.y,
                w: s.width,
                h: s.height,
                scale: m.scale_factor(),
                primary: primary == Some((p, s)),
            }
        })
        .collect()
}

/// The primary monitor's geometry, in logical pixels.
pub fn primary_geometry(app: &AppHandle) -> Option<Geometry> {
    primary_monitor_rect(&monitors(app)).map(|m| geometry_scales_to_logical(&m))
}

/// Shows the window on the primary monitor, over the wallpaper; it is made
/// (hidden) the first time. Where it cannot be made it stays off. Any thread.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))] // called by the watcher, Linux only
pub fn show(app: &AppHandle) {
    #[cfg(target_os = "linux")]
    {
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || linux::show(&handle));
        // after it on the main thread: the video window shows, then the player is raised over it
        crate::desktop_player::video::follow_desktop(app, true);
    }
    #[cfg(not(target_os = "linux"))]
    let _ = app;
}

/// Hides the window and lets go of the keyboard. Any thread.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn hide(app: &AppHandle) {
    #[cfg(target_os = "linux")]
    {
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || linux::hide(&handle));
        crate::desktop_player::video::follow_desktop(app, false);
    }
    #[cfg(not(target_os = "linux"))]
    let _ = app;
}

/// Puts the window back over the other desktop-level windows, for when a
/// wallpaper app maps its own desktop window on top of it. Any thread.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn raise(app: &AppHandle) {
    #[cfg(target_os = "linux")]
    {
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || linux::raise(&handle));
    }
    #[cfg(not(target_os = "linux"))]
    let _ = app;
}

/// Keyboard on or off (the search). Any thread.
pub fn set_focused(app: &AppHandle, focused: bool) {
    #[cfg(target_os = "linux")]
    {
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || linux::set_focused(&handle, focused));
    }
    #[cfg(not(target_os = "linux"))]
    let _ = (app, focused);
}

#[cfg(target_os = "linux")]
mod linux {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    use tauri::{AppHandle, LogicalPosition, LogicalSize, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

    use super::{announce, primary_geometry, window, LABEL};
    use crate::platform;

    /// Set once making the window failed or the display is not X11: never tried again.
    static UNAVAILABLE: AtomicBool = AtomicBool::new(false);
    /// The single later try, for a first show made before the monitors were known.
    static RETRIED: AtomicBool = AtomicBool::new(false);
    const RETRY_AFTER: Duration = Duration::from_secs(2);

    fn page_url(app: &AppHandle) -> WebviewUrl {
        #[cfg(dev)]
        if let Some(mut base) = app.config().build.dev_url.clone() {
            base.set_path("/desktop.html");
            return WebviewUrl::External(base);
        }
        let _ = app;
        WebviewUrl::App("desktop.html".into())
    }

    pub fn ensure(app: &AppHandle) -> Option<WebviewWindow> {
        if let Some(win) = window(app) {
            return Some(win);
        }
        if UNAVAILABLE.load(Ordering::Relaxed) {
            return None;
        }
        if !platform::desktop_player_supported() {
            UNAVAILABLE.store(true, Ordering::Relaxed);
            crate::log::line("desktop player: not an X11 session, off");
            return None;
        }
        let Some(geo) = primary_geometry(app) else {
            // Not cached: the monitors may simply not be known yet.
            crate::log::line("desktop player: no monitor geometry yet, window not made");
            return None;
        };
        let built = WebviewWindowBuilder::new(app, LABEL, page_url(app))
            .additional_browser_args(crate::BROWSER_ARGS)
            .title("Coucou desktop")
            .position(geo.x, geo.y)
            .inner_size(geo.w, geo.h)
            // GTK won't size a non-resizable window below its natural size (see island::apply_geometry).
            .resizable(true)
            .decorations(false)
            .transparent(true)
            .shadow(false)
            // Under every normal window: never always-on-top.
            .always_on_top(false)
            .skip_taskbar(true)
            .visible_on_all_workspaces(true)
            .focused(false)
            .focusable(false)
            .maximizable(false)
            .minimizable(false)
            .closable(false)
            .disable_drag_drop_handler()
            .visible(false)
            .build();
        match built {
            Ok(win) if platform::prepare_desktop_player(&win) => {
                crate::log::line(format!("desktop player: window {}x{} at {},{}", geo.w, geo.h, geo.x, geo.y));
                Some(win)
            }
            Ok(win) => {
                let _ = win.destroy();
                UNAVAILABLE.store(true, Ordering::Relaxed);
                crate::log::line("desktop player: no GTK window, off");
                None
            }
            Err(err) => {
                UNAVAILABLE.store(true, Ordering::Relaxed);
                crate::log::line(format!("desktop player window failed: {err}"));
                None
            }
        }
    }

    pub fn show(app: &AppHandle) {
        let Some(win) = ensure(app) else {
            announce(app, false);
            // No monitor geometry yet (the session is still starting): one more try a little later.
            if !UNAVAILABLE.load(Ordering::Relaxed) && !RETRIED.swap(true, Ordering::Relaxed) {
                let app = app.clone();
                gtk::glib::timeout_add_local_once(RETRY_AFTER, move || {
                    if crate::desktop_player::visibility::current() {
                        show(&app);
                    }
                });
            }
            return;
        };
        // The primary monitor may have changed since the window was made.
        if let Some(geo) = primary_geometry(app) {
            let _ = win.set_position(LogicalPosition::new(geo.x, geo.y));
            let _ = win.set_size(LogicalSize::new(geo.w, geo.h));
        }
        if win.show().is_err() {
            announce(app, false);
            return;
        }
        // Mapping raises it (prepare_desktop_player); shown already, this does.
        platform::raise_desktop_player(&win);
        announce(app, true);
    }

    pub fn hide(app: &AppHandle) {
        announce(app, false);
        let Some(win) = window(app) else { return };
        platform::set_desktop_player_keyboard(&win, false);
        let _ = win.hide();
    }

    pub fn raise(app: &AppHandle) {
        let Some(win) = window(app) else { return };
        if win.is_visible().unwrap_or(false) {
            platform::raise_desktop_player(&win);
        }
    }

    pub fn set_focused(app: &AppHandle, focused: bool) {
        let Some(win) = window(app) else { return };
        if focused && !win.is_visible().unwrap_or(false) {
            return;
        }
        platform::set_desktop_player_keyboard(&win, focused);
    }
}
