// Keep the computer from suspending while an agent works (AgentBar and LiveIsland both do this).
//
// It asks systemd-logind for a "sleep" inhibitor by running `systemd-inhibit … sleep 180` and holding
// that child. The island renews it every minute while an agent works; if Coucou dies, the lock
// still ends by itself within three minutes, so a crash can never keep the laptop awake for good.
// Only suspend is blocked ("sleep"): the screen may still blank and lock as usual.

use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

const HOLD_SECONDS: &str = "180";

static CHILD: Mutex<Option<Child>> = Mutex::new(None);

fn release_locked(slot: &mut Option<Child>) {
    if let Some(mut c) = slot.take() {
        let _ = c.kill();
        let _ = c.wait();
    }
}

/// Take (or renew) the lock when `on`, drop it when not. Errors are for the log: a machine without
/// systemd-inhibit simply stays as it was.
#[cfg(target_os = "linux")]
pub fn set(on: bool) -> Result<(), String> {
    let mut slot = CHILD.lock().map_err(|_| "lock poisoned".to_string())?;
    if !on {
        release_locked(&mut slot);
        return Ok(());
    }
    let fresh = Command::new("systemd-inhibit")
        .args([
            "--what=sleep",
            "--who=Coucou",
            "--why=An agent is working",
            "--mode=block",
            "sleep",
            HOLD_SECONDS,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("systemd-inhibit: {e}"))?;
    // logind refuses the lock (e.g. a session with no seat) by making systemd-inhibit exit at once.
    let mut fresh = fresh;
    std::thread::sleep(std::time::Duration::from_millis(150));
    if let Ok(Some(status)) = fresh.try_wait() {
        return Err(format!("the system refused the lock (systemd-inhibit exited with {status})"));
    }
    // The new lock is held before the old one is let go: there is no gap to suspend in.
    release_locked(&mut slot);
    *slot = Some(fresh);
    Ok(())
}

#[cfg(not(target_os = "linux"))]
pub fn set(on: bool) -> Result<(), String> {
    if !on {
        if let Ok(mut slot) = CHILD.lock() {
            release_locked(&mut slot);
        }
    }
    Err("Keeping the computer awake is only available on Linux for now".into())
}

/// Called when Coucou quits.
pub fn release() {
    if let Ok(mut slot) = CHILD.lock() {
        release_locked(&mut slot);
    }
}
