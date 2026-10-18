//! Restart audio (Voicemeeter's "Restart Audio Engine"): rebuild every device, app session, the processed
//! mic and call mix, and reconnect the headphones. The audio worker owns the restart (`Command::RestartAudio`);
//! the event sink reconnects the headphones and reports a failure afterwards.

use tauri::{AppHandle, Manager};

use crate::{model::Command, worker::WorkerHandle};

/// Launch flags that ask a running Resonance to restart audio (like `Voicemeeter.exe -r`).
pub const RESTART_ARGS: [&str; 2] = ["-r", "--restart-audio"];

pub fn wants_restart(args: &[String]) -> bool {
    args.iter().any(|a| RESTART_ARGS.contains(&a.as_str()))
}

/// For triggers that can't wait (tray menu, a second launch with `-r`).
pub fn spawn_restart(app: &AppHandle) {
    app.state::<WorkerHandle>().send(Command::RestartAudio);
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
}
