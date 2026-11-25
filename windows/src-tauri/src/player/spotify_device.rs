//! Choosing where Spotify plays: the headless spotifyd device "Coucou" first, the official app as fallback.
//! All side effects go through `Effects` so tests never touch systemd, flatpak, xdotool, the network or time.

use std::time::Duration;
#[cfg(target_os = "linux")]
use std::time::Instant;

use super::spotify_api::{self, ApiError, Device};

const COUCOU: &str = "Coucou";
const POLL_EVERY: Duration = Duration::from_millis(500);
const SERVICE_WAIT: Duration = Duration::from_secs(6);
const APP_WAIT: Duration = Duration::from_secs(10);
/// How long `systemctl --user start` may take before it counts as failed.
#[cfg(target_os = "linux")]
const SERVICE_START_MAX: Duration = Duration::from_secs(3);
/// Once "Coucou" failed to appear, plays go straight to the app for this long (no 6 s wait each time).
#[cfg(any(target_os = "linux", test))]
const MISSING_FOR: Duration = Duration::from_secs(5 * 60);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Choice {
    Coucou(String),
    App(String),
    None,
}

/// Prefers the device named "Coucou"; otherwise the official app on this computer (Spotify desktop
/// names itself after the hostname), then an active `Computer`, then any `Computer` (maybe another PC).
pub fn choose(devices: &[Device], host: Option<&str>) -> Choice {
    if let Some(d) = devices.iter().find(|d| d.name == COUCOU) {
        return Choice::Coucou(d.id.clone());
    }
    let host = host.map(str::trim).filter(|h| !h.is_empty());
    let here = host.and_then(|h| devices.iter().find(|d| d.name.trim().eq_ignore_ascii_case(h)));
    let computer = |d: &&Device| d.kind == "Computer";
    let pick = here
        .or_else(|| devices.iter().filter(computer).find(|d| d.active))
        .or_else(|| devices.iter().find(computer));
    match pick {
        Some(d) => Choice::App(d.id.clone()),
        None => Choice::None,
    }
}

/// This machine's name, as the Spotify app calls its device.
pub fn hostname() -> Option<String> {
    #[cfg(target_os = "linux")]
    let name = {
        let mut buf = [0u8; 256];
        // SAFETY: the buffer is valid for its length; gethostname writes at most that many bytes.
        let r = unsafe { libc::gethostname(buf.as_mut_ptr() as *mut libc::c_char, buf.len()) };
        if r != 0 {
            return None;
        }
        let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
        String::from_utf8_lossy(&buf[..end]).into_owned()
    };
    #[cfg(not(target_os = "linux"))]
    let name = std::env::var("COMPUTERNAME").ok()?;
    let name = name.trim().to_string();
    (!name.is_empty()).then_some(name)
}

/// Whether "Coucou" was found missing less than `MISSING_FOR` ago.
#[cfg(any(target_os = "linux", test))]
fn still_missing(missing_since: Option<std::time::Instant>, now: std::time::Instant) -> bool {
    missing_since.is_some_and(|at| now.saturating_duration_since(at) < MISSING_FOR)
}

#[cfg(any(target_os = "linux", test))]
pub(crate) trait Effects {
    async fn devices(&mut self) -> Result<Vec<Device>, ApiError>;
    /// Start spotifyd; false when `systemctl --user start` failed (or could not run).
    async fn start_service(&mut self) -> bool;
    /// Launch the official app detached (best effort).
    fn launch_app(&mut self);
    /// Minimize the app window without blocking (best effort).
    fn minimize(&mut self);
    async fn sleep(&mut self, d: Duration);
    /// "Coucou" did not appear a short while ago (`MISSING_FOR`).
    fn coucou_missing(&mut self) -> bool;
    fn set_coucou_missing(&mut self, missing: bool);
}

/// Polls `devices()` every `POLL_EVERY` for up to `total`; `want_coucou` limits the match to the Coucou device.
#[cfg(any(target_os = "linux", test))]
async fn poll<E: Effects>(
    e: &mut E,
    host: Option<&str>,
    total: Duration,
    want_coucou: bool,
) -> Result<Option<(String, bool)>, ApiError> {
    let mut waited = Duration::ZERO;
    loop {
        match choose(&e.devices().await?, host) {
            Choice::Coucou(id) => return Ok(Some((id, false))),
            Choice::App(id) if !want_coucou => return Ok(Some((id, true))),
            _ => {}
        }
        if waited >= total {
            return Ok(None);
        }
        e.sleep(POLL_EVERY).await;
        waited += POLL_EVERY;
    }
}

#[cfg(any(target_os = "linux", test))]
pub(crate) async fn ensure_device_with<E: Effects>(e: &mut E, host: Option<&str>) -> Result<(String, bool), ApiError> {
    if let Choice::Coucou(id) = choose(&e.devices().await?, host) {
        e.set_coucou_missing(false);
        return Ok((id, false));
    }
    // spotifyd is started and waited for only when it was not just found missing, and only
    // when systemd took the start: a broken or absent unit costs no wait on every play.
    if !e.coucou_missing() {
        if e.start_service().await {
            if let Some(found) = poll(e, host, SERVICE_WAIT, true).await? {
                e.set_coucou_missing(false);
                return Ok(found);
            }
        }
        e.set_coucou_missing(true);
    }
    // The app may already be running: reuse it, never relaunch (that would raise its window).
    if let Choice::App(id) = choose(&e.devices().await?, host) {
        return Ok((id, true));
    }
    e.launch_app();
    e.minimize();
    match poll(e, host, APP_WAIT, false).await? {
        Some((id, _)) => Ok((id, true)),
        None => Err(ApiError::NoDevice),
    }
}

#[cfg(target_os = "linux")]
struct Real;

#[cfg(target_os = "linux")]
static COUCOU_MISSING_SINCE: std::sync::Mutex<Option<Instant>> = std::sync::Mutex::new(None);

#[cfg(target_os = "linux")]
impl Effects for Real {
    async fn devices(&mut self) -> Result<Vec<Device>, ApiError> {
        spotify_api::devices().await
    }
    async fn start_service(&mut self) -> bool {
        let start = tokio::process::Command::new("systemctl")
            .args(["--user", "start", "coucou-spotifyd.service"])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .status();
        // A unit stuck "activating" would otherwise hold every play (they are serialised) for
        // systemd's own start timeout: past a few seconds it counts as failed.
        matches!(tokio::time::timeout(SERVICE_START_MAX, start).await, Ok(Ok(s)) if s.success())
    }
    fn launch_app(&mut self) {
        use std::os::unix::process::CommandExt;
        let mut cmd = std::process::Command::new("flatpak");
        cmd.args(["run", "com.spotify.Client"])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        // Own session: the app must outlive the island.
        unsafe {
            cmd.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
        if let Ok(mut child) = cmd.spawn() {
            // Reap it when it exits so it never lingers as a zombie.
            std::thread::spawn(move || {
                let _ = child.wait();
            });
        }
    }
    fn minimize(&mut self) {
        minimize_app();
    }
    async fn sleep(&mut self, d: Duration) {
        tokio::time::sleep(d).await;
    }
    fn coucou_missing(&mut self) -> bool {
        let since = *COUCOU_MISSING_SINCE.lock().unwrap_or_else(|p| p.into_inner());
        still_missing(since, Instant::now())
    }
    fn set_coucou_missing(&mut self, missing: bool) {
        *COUCOU_MISSING_SINCE.lock().unwrap_or_else(|p| p.into_inner()) = missing.then(Instant::now);
    }
}

/// Waits (up to 8 s) for the Spotify app's window, then minimizes it, off the async runtime.
/// Missing xdotool or failure: carry on.
#[cfg(target_os = "linux")]
pub fn minimize_app() {
    std::thread::spawn(|| {
        let _ = std::process::Command::new("timeout")
            .args(["8", "xdotool", "search", "--sync", "--classname", "spotify", "windowminimize"])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    });
}

/// Windows: no xdotool; the app is left as it opens.
#[cfg(not(target_os = "linux"))]
pub fn minimize_app() {}

#[cfg(target_os = "linux")]
pub async fn ensure_device() -> Result<(String, bool), ApiError> {
    ensure_device_with(&mut Real, hostname().as_deref()).await
}

/// Windows: no spotifyd or flatpak; use whatever device is already there, this PC's app first.
#[cfg(not(target_os = "linux"))]
pub async fn ensure_device() -> Result<(String, bool), ApiError> {
    match choose(&spotify_api::devices().await?, hostname().as_deref()) {
        Choice::Coucou(id) => Ok((id, false)),
        Choice::App(id) => Ok((id, true)),
        Choice::None => Err(ApiError::NoDevice),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    fn dev(id: &str, name: &str, kind: &str) -> Device {
        Device { id: id.into(), name: name.into(), kind: kind.into(), active: false }
    }

    fn active(mut d: Device) -> Device {
        d.active = true;
        d
    }

    #[test]
    fn prefers_coucou_device() {
        let d = [dev("a", "alan-lap", "Computer"), dev("c", "Coucou", "Computer")];
        assert_eq!(choose(&d, Some("alan-lap")), Choice::Coucou("c".into()));
    }

    #[test]
    fn falls_back_to_app_when_coucou_device_missing() {
        let d = [dev("p", "Phone", "Smartphone"), dev("a", "alan-lap", "Computer")];
        assert_eq!(choose(&d, Some("alan-lap")), Choice::App("a".into()));
        assert_eq!(choose(&d, None), Choice::App("a".into()));
    }

    #[test]
    fn prefers_this_computer_over_another_one() {
        // The office PC is listed first and is even the active one: this machine still wins.
        let d = [active(dev("o", "OFFICE-PC", "Computer")), dev("a", "Alan-Lap", "Computer")];
        assert_eq!(choose(&d, Some("alan-lap")), Choice::App("a".into()), "hostname, any case");
        assert_eq!(choose(&d, Some(" ALAN-LAP ")), Choice::App("a".into()));
    }

    #[test]
    fn without_this_computer_prefers_the_active_one() {
        let d = [dev("o", "OFFICE-PC", "Computer"), active(dev("h", "HOME-PC", "Computer"))];
        assert_eq!(choose(&d, Some("alan-lap")), Choice::App("h".into()));
        assert_eq!(choose(&d, Some("")), Choice::App("h".into()));
        // An active phone is not a computer: the first computer then.
        let d = [active(dev("p", "Phone", "Smartphone")), dev("o", "OFFICE-PC", "Computer")];
        assert_eq!(choose(&d, Some("alan-lap")), Choice::App("o".into()));
    }

    #[test]
    fn none_when_no_computer() {
        assert_eq!(choose(&[dev("p", "Phone", "Smartphone")], Some("alan-lap")), Choice::None);
        assert_eq!(choose(&[], None), Choice::None);
    }

    #[test]
    fn hostname_is_not_empty_here() {
        assert!(hostname().is_some_and(|h| !h.trim().is_empty()));
    }

    #[test]
    fn missing_is_remembered_for_five_minutes() {
        let now = std::time::Instant::now();
        assert!(!still_missing(None, now));
        assert!(still_missing(Some(now), now));
        assert!(still_missing(Some(now), now + MISSING_FOR - Duration::from_secs(1)));
        assert!(!still_missing(Some(now), now + MISSING_FOR));
    }

    /// `devices()` answers come from a script; the last one repeats. Sleeping only counts virtual time.
    struct Fake {
        script: VecDeque<Vec<Device>>,
        slept: Duration,
        service_starts: bool,
        service_started: bool,
        app_launched: bool,
        minimized: bool,
        missing: bool,
    }

    impl Fake {
        fn new(script: Vec<Vec<Device>>) -> Self {
            Fake {
                script: script.into(),
                slept: Duration::ZERO,
                service_starts: true,
                service_started: false,
                app_launched: false,
                minimized: false,
                missing: false,
            }
        }
    }

    impl Effects for Fake {
        async fn devices(&mut self) -> Result<Vec<Device>, ApiError> {
            Ok(if self.script.len() > 1 { self.script.pop_front().unwrap() } else { self.script[0].clone() })
        }
        async fn start_service(&mut self) -> bool {
            self.service_started = true;
            self.service_starts
        }
        fn launch_app(&mut self) {
            self.app_launched = true;
        }
        fn minimize(&mut self) {
            self.minimized = true;
        }
        async fn sleep(&mut self, d: Duration) {
            self.slept += d;
        }
        fn coucou_missing(&mut self) -> bool {
            self.missing
        }
        fn set_coucou_missing(&mut self, missing: bool) {
            self.missing = missing;
        }
    }

    const HOST: Option<&str> = Some("alan-lap");

    #[tokio::test]
    async fn starts_service_then_finds_coucou() {
        let mut f = Fake::new(vec![vec![], vec![], vec![dev("c", "Coucou", "Computer")]]);
        assert_eq!(ensure_device_with(&mut f, HOST).await, Ok(("c".into(), false)));
        assert!(f.service_started && !f.app_launched && !f.missing);
    }

    #[tokio::test]
    async fn uses_coucou_without_starting_anything() {
        let mut f = Fake::new(vec![vec![dev("c", "Coucou", "Computer")]]);
        f.missing = true; // it came back: forgotten
        assert_eq!(ensure_device_with(&mut f, HOST).await, Ok(("c".into(), false)));
        assert!(!f.service_started && !f.app_launched && !f.missing);
    }

    #[tokio::test]
    async fn uses_app_after_service_timeout() {
        // Coucou never shows up; the app appears only after it is launched.
        let mut script = vec![vec![]; 15];
        script.push(vec![dev("a", "alan-lap", "Computer")]);
        let mut f = Fake::new(script);
        assert_eq!(ensure_device_with(&mut f, HOST).await, Ok(("a".into(), true)));
        assert!(f.service_started && f.app_launched && f.minimized);
        assert!(f.slept >= SERVICE_WAIT);
        assert!(f.missing, "the timeout is remembered");
    }

    #[tokio::test]
    async fn uses_running_app_without_launching() {
        let mut f = Fake::new(vec![vec![dev("a", "alan-lap", "Computer")]]);
        assert_eq!(ensure_device_with(&mut f, HOST).await, Ok(("a".into(), true)));
        assert!(f.service_started && !f.app_launched && !f.minimized);
        assert!(f.slept >= SERVICE_WAIT);
    }

    #[tokio::test]
    async fn failed_service_start_skips_the_wait() {
        let mut f = Fake::new(vec![vec![dev("a", "alan-lap", "Computer")]]);
        f.service_starts = false;
        assert_eq!(ensure_device_with(&mut f, HOST).await, Ok(("a".into(), true)));
        assert!(f.service_started && !f.app_launched);
        assert_eq!(f.slept, Duration::ZERO, "no 6 s poll when systemd refused the start");
        assert!(f.missing);
    }

    #[tokio::test]
    async fn recently_missing_goes_straight_to_the_app() {
        let mut f = Fake::new(vec![vec![dev("a", "alan-lap", "Computer")]]);
        f.missing = true;
        assert_eq!(ensure_device_with(&mut f, HOST).await, Ok(("a".into(), true)));
        assert!(!f.service_started, "spotifyd is not tried again within the window");
        assert_eq!(f.slept, Duration::ZERO);
        // Nothing running: the app is launched, still with no spotifyd wait.
        let mut script = vec![vec![]; 3];
        script.push(vec![dev("a", "alan-lap", "Computer")]);
        let mut f = Fake::new(script);
        f.missing = true;
        assert_eq!(ensure_device_with(&mut f, HOST).await, Ok(("a".into(), true)));
        assert!(!f.service_started && f.app_launched);
        assert!(f.slept < SERVICE_WAIT);
    }

    #[tokio::test]
    async fn no_device_when_nothing_appears() {
        let mut f = Fake::new(vec![vec![]]);
        assert_eq!(ensure_device_with(&mut f, HOST).await, Err(ApiError::NoDevice));
        assert!(f.slept >= SERVICE_WAIT + APP_WAIT);
    }
}
