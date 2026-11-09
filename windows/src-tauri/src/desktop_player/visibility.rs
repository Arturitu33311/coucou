// "Desktop in view": true when the current workspace has no visible normal
// window of anyone else's, so the desktop player may show itself.
//
// X11 only, event-driven (EWMH): one blocking thread selects PropertyChange on
// the root and on each client, rereads _NET_CLIENT_LIST_STACKING,
// _NET_CURRENT_DESKTOP and each client's state when something changes, and
// shows or hides the player's window (window.rs) only when the answer flips.
// No polling. It also puts the player back over a wallpaper app's desktop
// window that comes up above it (the spike: Hidamari restarting).
//
// What the island routes on is not this answer but whether the window really is
// on screen (window.rs, `desktop-player-shown`): the window may not exist
// (Wayland under XWayland, no GTK window, no monitor yet) while this says true.
// Nothing is emitted from here: Desktop Mochi's window owns `desktop-visible`.

use std::sync::atomic::{AtomicBool, Ordering};

/// What matters of one top-level window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Win {
    pub id: u32,
    /// `_NET_WM_DESKTOP`; 0xFFFFFFFF means every workspace; None = unknown (treated as sticky).
    pub desktop: Option<u32>,
    /// `_NET_WM_STATE_HIDDEN` (minimized).
    pub hidden: bool,
    /// An ordinary window (not a dock, desktop, notification, utility...).
    pub normal: bool,
    /// Belongs to this process (island, Mochi, the player itself).
    pub ours: bool,
    /// `_NET_WM_WINDOW_TYPE_DESKTOP`: a desktop-level window (the player, a wallpaper app's).
    pub desktop_type: bool,
}

const ALL_DESKTOPS: u32 = 0xFFFF_FFFF;

/// True when no `normal && !hidden && !ours` window is on `current` (or on all workspaces).
pub fn desktop_visible(wins: &[Win], current: u32) -> bool {
    !wins.iter().any(|w| {
        w.normal && !w.hidden && !w.ours && w.desktop.map_or(true, |d| d == current || d == ALL_DESKTOPS)
    })
}

/// True when someone else's desktop-type window sits above ours in `stacking`
/// (bottom to top, as `_NET_CLIENT_LIST_STACKING`). False while ours is not there.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn foreign_desktop_above(stacking: &[Win]) -> bool {
    let Some(mine) = stacking.iter().rposition(|w| w.ours && w.desktop_type) else { return false };
    stacking[mine + 1..].iter().any(|w| w.desktop_type && !w.ours)
}

/// Raise only when a foreign desktop window has just come above ours: once per
/// time it happens, so a wallpaper app that raises itself back cannot make a loop,
/// and a raise that does nothing (no xdotool) is not retried on every event.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn should_raise(was_above: bool, is_above: bool) -> bool {
    is_above && !was_above
}

static VISIBLE: AtomicBool = AtomicBool::new(false);

/// The last answer the watcher computed (false until it has run, or off X11).
/// Not whether the window is on screen: see `window::is_shown`.
pub fn current() -> bool {
    VISIBLE.load(Ordering::Relaxed)
}

/// The webview pulls this at startup: the first event fires before its listener exists.
#[tauri::command]
pub fn desktop_is_visible() -> bool {
    current()
}

/// Start the watcher on a Linux X11 session; nothing anywhere else.
pub fn start(app: tauri::AppHandle) {
    #[cfg(target_os = "linux")]
    {
        let wayland = std::env::var("XDG_SESSION_TYPE").map(|v| v.eq_ignore_ascii_case("wayland")).unwrap_or(false);
        if wayland || std::env::var_os("DISPLAY").is_none() {
            return;
        }
        let _ = std::thread::Builder::new().name("desktop-visible".into()).spawn(move || {
            if let Err(e) = linux::run(&app) {
                crate::log::line(format!("desktop-visible watcher stopped: {e}"));
            }
        });
    }
    #[cfg(not(target_os = "linux"))]
    let _ = app;
}

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use crate::desktop_player::window;
    use std::collections::HashSet;
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{AtomEnum, ChangeWindowAttributesAux, ConnectionExt, EventMask, Window};
    use x11rb::protocol::Event;
    use x11rb::rust_connection::RustConnection;

    type R<T> = Result<T, Box<dyn std::error::Error>>;

    pub struct Atoms {
        stacking: u32,
        current_desktop: u32,
        wm_desktop: u32,
        wm_state: u32,
        hidden: u32,
        wtype: u32,
        t_normal: u32,
        t_dialog: u32,
        t_desktop: u32,
        pid: u32,
    }

    fn atom(c: &RustConnection, n: &str) -> R<u32> {
        Ok(c.intern_atom(false, n.as_bytes())?.reply()?.atom)
    }

    fn atoms(c: &RustConnection) -> R<Atoms> {
        Ok(Atoms {
            stacking: atom(c, "_NET_CLIENT_LIST_STACKING")?,
            current_desktop: atom(c, "_NET_CURRENT_DESKTOP")?,
            wm_desktop: atom(c, "_NET_WM_DESKTOP")?,
            wm_state: atom(c, "_NET_WM_STATE")?,
            hidden: atom(c, "_NET_WM_STATE_HIDDEN")?,
            wtype: atom(c, "_NET_WM_WINDOW_TYPE")?,
            t_normal: atom(c, "_NET_WM_WINDOW_TYPE_NORMAL")?,
            t_dialog: atom(c, "_NET_WM_WINDOW_TYPE_DIALOG")?,
            t_desktop: atom(c, "_NET_WM_WINDOW_TYPE_DESKTOP")?,
            pid: atom(c, "_NET_WM_PID")?,
        })
    }

    fn cards(c: &RustConnection, w: Window, prop: u32, kind: AtomEnum, max: u32) -> Vec<u32> {
        c.get_property(false, w, prop, kind, 0, max)
            .ok()
            .and_then(|r| r.reply().ok())
            .and_then(|r| r.value32().map(|v| v.collect()))
            .unwrap_or_default()
    }

    fn read_win(c: &RustConnection, a: &Atoms, id: Window, me: u32) -> Win {
        let desktop = cards(c, id, a.wm_desktop, AtomEnum::CARDINAL, 1).first().copied();
        let hidden = cards(c, id, a.wm_state, AtomEnum::ATOM, 64).contains(&a.hidden);
        let types = cards(c, id, a.wtype, AtomEnum::ATOM, 16);
        let normal = types.is_empty() || types.iter().any(|t| *t == a.t_normal || *t == a.t_dialog);
        let ours = cards(c, id, a.pid, AtomEnum::CARDINAL, 1).first() == Some(&me);
        let desktop_type = types.contains(&a.t_desktop);
        Win { id, desktop, hidden, normal, ours, desktop_type }
    }

    /// (desktop in view, a foreign desktop window above ours)
    fn compute(c: &RustConnection, a: &Atoms, root: Window, watched: &mut HashSet<Window>) -> (bool, bool) {
        let current = cards(c, root, a.current_desktop, AtomEnum::CARDINAL, 1).first().copied().unwrap_or(0);
        let me = std::process::id();
        let mut wins = Vec::new();
        for id in cards(c, root, a.stacking, AtomEnum::WINDOW, 4096) {
            if watched.insert(id) {
                // State changes (minimize, move to another workspace) arrive as PropertyNotify on the client.
                let _ = c.change_window_attributes(id, &ChangeWindowAttributesAux::new().event_mask(EventMask::PROPERTY_CHANGE));
            }
            wins.push(read_win(c, a, id, me));
        }
        (desktop_visible(&wins, current), foreign_desktop_above(&wins))
    }

    pub fn run(app: &tauri::AppHandle) -> R<()> {
        let (c, screen) = x11rb::connect(None)?;
        let root = c.setup().roots[screen].root;
        let a = atoms(&c)?;
        c.change_window_attributes(root, &ChangeWindowAttributesAux::new().event_mask(EventMask::PROPERTY_CHANGE))?;
        c.flush()?;

        let mut watched = HashSet::new();
        let (mut last, mut above) = compute(&c, &a, root, &mut watched);
        publish(app, last);
        loop {
            let mut relevant = false;
            let mut next = Some(c.wait_for_event()?);
            // Drain what is already queued so a burst costs one recompute.
            while let Some(ev) = next {
                if let Event::PropertyNotify(p) = ev {
                    relevant |= if p.window == root {
                        p.atom == a.stacking || p.atom == a.current_desktop
                    } else {
                        p.atom == a.wm_state || p.atom == a.wm_desktop || p.atom == a.wtype
                    };
                }
                next = c.poll_for_event()?;
            }
            if !relevant {
                continue;
            }
            // Forget windows that left the list so the set doesn't grow forever.
            let live: HashSet<Window> = cards(&c, root, a.stacking, AtomEnum::WINDOW, 4096).into_iter().collect();
            watched.retain(|w| live.contains(w));
            let (now, now_above) = compute(&c, &a, root, &mut watched);
            if now != last {
                last = now;
                publish(app, now);
            } else if now && should_raise(above, now_above) {
                window::raise(app);
            }
            above = now_above;
        }
    }

    /// Records the answer and shows or hides the window to match (a show raises it too).
    fn publish(app: &tauri::AppHandle, v: bool) {
        VISIBLE.store(v, Ordering::Relaxed);
        if v {
            window::show(app);
        } else {
            window::hide(app);
        }
    }

    #[cfg(test)]
    mod live {
        /// Read-only look at the real session: `cargo test live_current -- --ignored --nocapture`.
        #[test]
        #[ignore]
        fn live_current() {
            let (c, screen) = x11rb::connect(None).unwrap();
            let root = c.setup().roots[screen].root;
            let a = super::atoms(&c).unwrap();
            let mut w = std::collections::HashSet::new();
            println!("(desktop_visible, foreign_desktop_above) = {:?}", super::compute(&c, &a, root, &mut w));
        }
        use x11rb::connection::Connection;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(id: u32, desktop: Option<u32>) -> Win {
        Win { id, desktop, hidden: false, normal: true, ours: false, desktop_type: false }
    }

    fn desk(id: u32, ours: bool) -> Win {
        Win { id, desktop: Some(ALL_DESKTOPS), hidden: false, normal: false, ours, desktop_type: true }
    }

    #[test]
    fn desktop_hidden_when_a_window_is_visible() {
        assert!(!desktop_visible(&[w(1, Some(0))], 0));
    }

    #[test]
    fn empty_workspace_shows_desktop() {
        assert!(desktop_visible(&[], 0));
        assert!(desktop_visible(&[w(1, Some(1))], 0));
    }

    #[test]
    fn minimized_windows_dont_count() {
        let mut x = w(1, Some(0));
        x.hidden = true;
        assert!(desktop_visible(&[x], 0));
    }

    #[test]
    fn sticky_windows_count() {
        assert!(!desktop_visible(&[w(1, Some(0xFFFF_FFFF))], 3));
    }

    #[test]
    fn our_own_windows_and_docks_dont_count() {
        let mut island = w(1, Some(0));
        island.ours = true;
        let mut plank = w(2, Some(0xFFFF_FFFF));
        plank.normal = false;
        assert!(desktop_visible(&[island, plank], 0));
    }

    #[test]
    fn a_wallpaper_window_above_ours_is_seen() {
        // bottom to top: our player, then Hidamari's window mapped again on top
        assert!(foreign_desktop_above(&[desk(1, true), desk(2, false)]));
        // raised back: ours on top
        assert!(!foreign_desktop_above(&[desk(2, false), desk(1, true)]));
    }

    #[test]
    fn nothing_to_raise_without_our_window_or_for_normal_windows() {
        assert!(!foreign_desktop_above(&[desk(2, false)]));
        assert!(!foreign_desktop_above(&[]));
        // a normal window above ours is meant to be there
        assert!(!foreign_desktop_above(&[desk(1, true), w(3, Some(0))]));
        // our other windows (island, Mochi) are not desktop windows of someone else
        let mut island = w(4, Some(0));
        island.ours = true;
        assert!(!foreign_desktop_above(&[desk(1, true), island]));
    }

    #[test]
    fn raise_only_when_a_wallpaper_window_has_just_come_above() {
        assert!(should_raise(false, true));
        // still above after our raise (it raised itself back, or no xdotool): no loop
        assert!(!should_raise(true, true));
        assert!(!should_raise(true, false));
        assert!(!should_raise(false, false));
    }
}
