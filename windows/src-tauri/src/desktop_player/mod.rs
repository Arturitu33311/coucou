use serde::Serialize;

pub mod visibility;
pub mod window;

#[allow(unused_imports)] // The window API for the watcher (plan Task 3) and the shortcut.
pub use window::{ensure, hide, raise, show, LABEL};

/// A monitor as the OS reports it: physical pixels, its scale factor, and
/// whether the system calls it the primary one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MonitorInfo {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
    pub scale: f64,
    pub primary: bool,
}

/// The window's place in logical pixels, plus the scale it was measured with.
#[derive(Serialize, Clone, Copy, Debug, PartialEq)]
pub struct Geometry {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub scale: f64,
}

/// The monitor the player goes on: the one marked primary, else the one at the
/// desktop's origin (where X11 puts the primary when none is set), else the
/// first one. None only without any monitor.
pub fn primary_monitor_rect(monitors: &[MonitorInfo]) -> Option<MonitorInfo> {
    monitors
        .iter()
        .find(|m| m.primary)
        .or_else(|| monitors.iter().find(|m| m.x == 0 && m.y == 0))
        .or_else(|| monitors.first())
        .copied()
}

/// Physical pixels to logical ones. A missing or nonsensical scale counts as 1.
pub fn geometry_scales_to_logical(m: &MonitorInfo) -> Geometry {
    let scale = if m.scale.is_finite() && m.scale > 0.0 { m.scale } else { 1.0 };
    Geometry {
        x: m.x as f64 / scale,
        y: m.y as f64 / scale,
        w: m.w as f64 / scale,
        h: m.h as f64 / scale,
        scale,
    }
}

/// Whether this session gets a desktop player: an X11 display, and not a
/// Wayland session (where GTK may still run through XWayland, as on GNOME —
/// the same Wayland test as `platform::global_shortcuts_blocked`).
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn desktop_player_allowed(session_type: &str, wayland_display: &str, gdk_is_x11: bool) -> bool {
    let wayland = !wayland_display.is_empty() || session_type.eq_ignore_ascii_case("wayland");
    gdk_is_x11 && !wayland
}

/// The primary monitor's geometry, in logical pixels.
#[tauri::command]
pub fn desktop_player_geometry(app: tauri::AppHandle) -> Option<Geometry> {
    window::primary_geometry(&app)
}

/// Gives the window the keyboard while the search is being typed in, and takes
/// it back afterwards. Does nothing where there is no desktop player.
#[tauri::command]
pub fn desktop_player_focus(app: tauri::AppHandle, focused: bool) {
    window::set_focused(&app, focused);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mon(x: i32, y: i32, w: u32, h: u32, scale: f64, primary: bool) -> MonitorInfo {
        MonitorInfo { x, y, w, h, scale, primary }
    }

    #[test]
    fn primary_monitor_rect_picks_the_one_marked_primary() {
        let list = [mon(0, 0, 1920, 1080, 1.0, false), mon(1920, 0, 2560, 1440, 1.0, true)];
        assert_eq!(primary_monitor_rect(&list), Some(list[1]));
    }

    #[test]
    fn primary_monitor_rect_falls_back_to_the_origin() {
        let list = [mon(1920, 0, 2560, 1440, 1.0, false), mon(0, 0, 1920, 1080, 1.0, false)];
        assert_eq!(primary_monitor_rect(&list), Some(list[1]));
    }

    #[test]
    fn primary_monitor_rect_falls_back_to_the_first() {
        let list = [mon(100, 50, 1280, 720, 1.0, false), mon(1380, 50, 1280, 720, 1.0, false)];
        assert_eq!(primary_monitor_rect(&list), Some(list[0]));
    }

    #[test]
    fn primary_monitor_rect_is_none_without_monitors() {
        assert_eq!(primary_monitor_rect(&[]), None);
    }

    #[test]
    fn geometry_scales_to_logical() {
        let g = super::geometry_scales_to_logical(&mon(2560, 0, 2560, 1600, 2.0, true));
        assert_eq!(g, Geometry { x: 1280.0, y: 0.0, w: 1280.0, h: 800.0, scale: 2.0 });
    }

    #[test]
    fn geometry_scales_to_logical_keeps_scale_one() {
        let g = super::geometry_scales_to_logical(&mon(0, 0, 1920, 1080, 1.0, true));
        assert_eq!(g, Geometry { x: 0.0, y: 0.0, w: 1920.0, h: 1080.0, scale: 1.0 });
    }

    #[test]
    fn desktop_player_allowed_only_in_an_x11_session() {
        assert!(desktop_player_allowed("x11", "", true));
        assert!(desktop_player_allowed("", "", true));
        // GNOME Wayland with GDK_BACKEND=x11: GDK is X11, the session is not.
        assert!(!desktop_player_allowed("wayland", "wayland-0", true));
        assert!(!desktop_player_allowed("Wayland", "", true));
        assert!(!desktop_player_allowed("x11", "wayland-0", true));
        assert!(!desktop_player_allowed("wayland", "wayland-0", false));
        assert!(!desktop_player_allowed("x11", "", false));
    }

    #[test]
    fn geometry_scales_to_logical_treats_a_bad_scale_as_one() {
        let g = super::geometry_scales_to_logical(&mon(0, 0, 1920, 1080, 0.0, true));
        assert_eq!(g.w, 1920.0);
        assert_eq!(g.scale, 1.0);
    }
}

