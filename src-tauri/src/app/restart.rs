//! Restart audio (Voicemeeter's "Restart Audio Engine"): rebuild every device, app session, the processed
//! mic and call mix, and reconnect the headphones. Every trigger ends up here.

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{AppHandle, Manager};

use super::sink::push_notice;
use crate::{
    error::Result,
    headset::{model::HeadsetCommand, HeadsetHandle},
    worker::WorkerHandle,
};

/// Launch flags that ask a running Resonance to restart audio (like `Voicemeeter.exe -r`).
pub const RESTART_ARGS: [&str; 2] = ["-r", "--restart-audio"];

pub fn wants_restart(args: &[String]) -> bool {
    args.iter().any(|a| RESTART_ARGS.contains(&a.as_str()))
}

/// Set while a restart is running, so repeated triggers (key repeat, tray clicks, the watchdog) don't pile up.
static RESTARTING: AtomicBool = AtomicBool::new(false);

struct RestartGuard;

impl RestartGuard {
    fn acquire() -> Option<Self> {
        RESTARTING.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire).ok().map(|_| Self)
    }
}

impl Drop for RestartGuard {
    fn drop(&mut self) {
        RESTARTING.store(false, Ordering::Release);
    }
}

pub async fn restart_audio(app: &AppHandle) -> Result<()> {
    let Some(_guard) = RestartGuard::acquire() else {
        tracing::debug!("restart audio already in progress");
        return Ok(());
    };
    let worker = app.state::<WorkerHandle>().inner().clone();
    let headset = app.state::<HeadsetHandle>().inner().clone();
    let result = worker.restart().await;
    // The headphones reconnect even if the audio side couldn't be rebuilt.
    let _ = headset.command(HeadsetCommand::Retry).await;
    if let Err(e) = &result {
        push_notice(app, crate::error::AudioError::Unavailable(format!("Couldn't restart audio: {e}")));
    }
    result
}

/// For triggers that can't wait (tray menu, a second launch with `-r`, the control pipe).
pub fn spawn_restart(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let _ = restart_audio(&app).await;
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_the_restart_flags() {
        let args = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(wants_restart(&args(&["resonance.exe", "-r"])));
        assert!(wants_restart(&args(&["resonance.exe", "--restart-audio"])));
        assert!(!wants_restart(&args(&["resonance.exe", "--minimized"])));
        assert!(!wants_restart(&args(&["resonance.exe"])));
    }

    #[test]
    fn only_one_restart_runs_at_a_time() {
        let first = RestartGuard::acquire().expect("first restart starts");
        assert!(RestartGuard::acquire().is_none(), "a second one is refused while the first runs");
        drop(first);
        assert!(RestartGuard::acquire().is_some(), "the guard clears when the restart ends");
    }
}
