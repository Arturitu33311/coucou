//! Choosing where Spotify plays: the headless spotifyd device "Coucou" first, the official app as fallback.
//! All side effects go through `Effects` so tests never touch systemd, flatpak, xdotool, the network or time.

use std::time::Duration;

use super::spotify_api::{self, ApiError, Device};

const COUCOU: &str = "Coucou";
const POLL_EVERY: Duration = Duration::from_millis(500);
const SERVICE_WAIT: Duration = Duration::from_secs(6);
const APP_WAIT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Choice {
    Coucou(String),
    App(String),
    None,
}

/// Prefers the device named "Coucou"; otherwise any `Computer` device (the official app).
pub fn choose(devices: &[Device]) -> Choice {
    if let Some(d) = devices.iter().find(|d| d.name == COUCOU) {
        return Choice::Coucou(d.id.clone());
    }
    match devices.iter().find(|d| d.kind == "Computer") {
        Some(d) => Choice::App(d.id.clone()),
        None => Choice::None,
    }
}

#[cfg(any(target_os = "linux", test))]
pub(crate) trait Effects {
    async fn devices(&mut self) -> Result<Vec<Device>, ApiError>;
    /// Start spotifyd (best effort, errors ignored).
    fn start_service(&mut self);
    /// Launch the official app detached (best effort).
    fn launch_app(&mut self);
    /// Minimize the app window without blocking (best effort).
    fn minimize(&mut self);
    async fn sleep(&mut self, d: Duration);
}

/// Polls `devices()` every `POLL_EVERY` for up to `total`; `want_coucou` limits the match to the Coucou device.
#[cfg(any(target_os = "linux", test))]
async fn poll<E: Effects>(e: &mut E, total: Duration, want_coucou: bool) -> Result<Option<(String, bool)>, ApiError> {
    let mut waited = Duration::ZERO;
    loop {
        match choose(&e.devices().await?) {
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
pub(crate) async fn ensure_device_with<E: Effects>(e: &mut E) -> Result<(String, bool), ApiError> {
    if let Choice::Coucou(id) = choose(&e.devices().await?) {
        return Ok((id, false));
    }
    e.start_service();
    if let Some(found) = poll(e, SERVICE_WAIT, true).await? {
        return Ok(found);
    }
    e.launch_app();
    e.minimize();
    match poll(e, APP_WAIT, false).await? {
        Some((id, _)) => Ok((id, true)),
        None => Err(ApiError::NoDevice),
    }
}

#[cfg(target_os = "linux")]
struct Real;

#[cfg(target_os = "linux")]
impl Effects for Real {
    async fn devices(&mut self) -> Result<Vec<Device>, ApiError> {
        spotify_api::devices().await
    }
    fn start_service(&mut self) {
        let _ = std::process::Command::new("systemctl")
            .args(["--user", "start", "coucou-spotifyd.service"])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
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
        // Waits (up to 8 s) for the window, then minimizes it. Missing xdotool or failure: carry on.
        std::thread::spawn(|| {
            let _ = std::process::Command::new("timeout")
                .args(["8", "xdotool", "search", "--sync", "--classname", "spotify", "windowminimize"])
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
        });
    }
    async fn sleep(&mut self, d: Duration) {
        tokio::time::sleep(d).await;
    }
}

#[cfg(target_os = "linux")]
pub async fn ensure_device() -> Result<(String, bool), ApiError> {
    ensure_device_with(&mut Real).await
}

/// Windows: no spotifyd or flatpak; use whatever device is already there.
#[cfg(not(target_os = "linux"))]
pub async fn ensure_device() -> Result<(String, bool), ApiError> {
    match choose(&spotify_api::devices().await?) {
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

    #[test]
    fn prefers_coucou_device() {
        let d = [dev("a", "alan-lap", "Computer"), dev("c", "Coucou", "Computer")];
        assert_eq!(choose(&d), Choice::Coucou("c".into()));
    }

    #[test]
    fn falls_back_to_app_when_coucou_device_missing() {
        let d = [dev("p", "Phone", "Smartphone"), dev("a", "alan-lap", "Computer")];
        assert_eq!(choose(&d), Choice::App("a".into()));
    }

    #[test]
    fn none_when_no_computer() {
        assert_eq!(choose(&[dev("p", "Phone", "Smartphone")]), Choice::None);
        assert_eq!(choose(&[]), Choice::None);
    }

    /// `devices()` answers come from a script; the last one repeats. Sleeping only counts virtual time.
    struct Fake {
        script: VecDeque<Vec<Device>>,
        slept: Duration,
        service_started: bool,
        app_launched: bool,
        minimized: bool,
    }

    impl Fake {
        fn new(script: Vec<Vec<Device>>) -> Self {
            Fake { script: script.into(), slept: Duration::ZERO, service_started: false, app_launched: false, minimized: false }
        }
    }

    impl Effects for Fake {
        async fn devices(&mut self) -> Result<Vec<Device>, ApiError> {
            Ok(if self.script.len() > 1 { self.script.pop_front().unwrap() } else { self.script[0].clone() })
        }
        fn start_service(&mut self) {
            self.service_started = true;
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
    }

    #[tokio::test]
    async fn starts_service_then_finds_coucou() {
        let mut f = Fake::new(vec![vec![], vec![], vec![dev("c", "Coucou", "Computer")]]);
        assert_eq!(ensure_device_with(&mut f).await, Ok(("c".into(), false)));
        assert!(f.service_started && !f.app_launched);
    }

    #[tokio::test]
    async fn uses_coucou_without_starting_anything() {
        let mut f = Fake::new(vec![vec![dev("c", "Coucou", "Computer")]]);
        assert_eq!(ensure_device_with(&mut f).await, Ok(("c".into(), false)));
        assert!(!f.service_started && !f.app_launched);
    }

    #[tokio::test]
    async fn uses_app_after_service_timeout() {
        // Coucou never shows up; the app appears only after it is launched.
        let mut script = vec![vec![]; 14];
        script.push(vec![dev("a", "alan-lap", "Computer")]);
        let mut f = Fake::new(script);
        assert_eq!(ensure_device_with(&mut f).await, Ok(("a".into(), true)));
        assert!(f.service_started && f.app_launched && f.minimized);
        assert!(f.slept >= SERVICE_WAIT);
    }

    #[tokio::test]
    async fn no_device_when_nothing_appears() {
        let mut f = Fake::new(vec![vec![]]);
        assert_eq!(ensure_device_with(&mut f).await, Err(ApiError::NoDevice));
        assert!(f.slept >= SERVICE_WAIT + APP_WAIT);
    }
}
