use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use crate::{
    model::{Command, HotkeyMode, MicConfig},
    worker::WorkerHandle,
};

pub fn hotkey_action(mode: HotkeyMode, pressed: bool) -> Option<Command> {
    match (mode, pressed) {
        (HotkeyMode::Toggle, true) => Some(Command::ToggleMicMute),
        (HotkeyMode::Toggle, false) => None,
        (HotkeyMode::PushToTalk, p) => Some(Command::SetMicMute { muted: !p }),
        (HotkeyMode::PushToMute, p) => Some(Command::SetMicMute { muted: p }),
    }
}

/// Replaces the registered mic hotkey. Validates by registering; returns a readable error.
pub fn apply(app: &AppHandle, cfg: &MicConfig) -> Result<(), String> {
    let gs = app.global_shortcut();
    gs.unregister_all().map_err(|e| format!("Couldn't clear hotkeys: {e}"))?;
    let Some(hk) = cfg.hotkey.as_deref() else { return Ok(()) };
    let shortcut: Shortcut = hk.parse().map_err(|e| format!("Invalid hotkey \"{hk}\": {e}"))?;
    let worker = app.state::<WorkerHandle>().inner().clone();
    let mode = cfg.mode;
    gs.on_shortcut(shortcut, move |_app, _shortcut, event| {
        if let Some(cmd) = hotkey_action(mode, event.state() == ShortcutState::Pressed) {
            worker.send(cmd);
        }
    })
    .map_err(|e| format!("Couldn't register \"{hk}\" (is another app using it?): {e}"))?;
    if mode == HotkeyMode::PushToTalk {
        app.state::<WorkerHandle>().send(Command::SetMicMute { muted: true });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_modes_and_key_states_to_commands() {
        assert_eq!(hotkey_action(HotkeyMode::Toggle, true), Some(Command::ToggleMicMute));
        assert_eq!(hotkey_action(HotkeyMode::Toggle, false), None);
        assert_eq!(hotkey_action(HotkeyMode::PushToTalk, true), Some(Command::SetMicMute { muted: false }));
        assert_eq!(hotkey_action(HotkeyMode::PushToTalk, false), Some(Command::SetMicMute { muted: true }));
        assert_eq!(hotkey_action(HotkeyMode::PushToMute, true), Some(Command::SetMicMute { muted: true }));
        assert_eq!(hotkey_action(HotkeyMode::PushToMute, false), Some(Command::SetMicMute { muted: false }));
    }
}
