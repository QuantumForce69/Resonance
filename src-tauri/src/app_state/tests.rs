use super::*;
use crate::{fake::FakeBackend, settings::DevicePrefs};

fn fake() -> FakeBackend {
    let mut out1 = FakeBackend::device("out1", Flow::Render);
    out1.default_console = true;
    out1.default_comms = true;
    let out2 = FakeBackend::device("out2", Flow::Render);
    let mut mic = FakeBackend::device("mic1", Flow::Capture);
    mic.default_console = true;
    mic.boost = Some(Boost { db: 0.0, min: 0.0, max: 30.0, step: 10.0 });
    FakeBackend {
        devices: vec![out1, out2, mic],
        sessions: vec![
            FakeBackend::session("s1", 10, Some("spotify.exe")),
            FakeBackend::session("c1", 20, Some("chrome.exe")),
            FakeBackend::session("c2", 21, Some("chrome.exe")),
        ],
        routing: true,
        ..Default::default()
    }
}

fn core_with(settings: Settings) -> AppCore<FakeBackend> {
    let mut c = AppCore::new(fake(), settings);
    c.refresh().unwrap();
    c
}

fn app<'a>(c: &'a AppCore<FakeBackend>, key: &str) -> &'a App {
    c.state().apps.iter().find(|a| a.key == key).unwrap()
}

#[test]
fn refresh_builds_state_with_device_prefs_and_sorting() {
    let mut s = Settings::default();
    s.devices.insert("out2".into(), DevicePrefs { alias: Some("Desk".into()), hidden: true });
    let c = core_with(s);
    let st = c.state();
    assert!(st.routing_available);
    let flows: Vec<_> = st.devices.iter().map(|d| d.flow).collect();
    assert_eq!(flows, vec![Flow::Render, Flow::Render, Flow::Capture]);
    let desk = st.devices.iter().find(|d| d.id == "out2").unwrap();
    assert_eq!(desk.name, "Desk");
    assert_eq!(desk.original_name, "Device out2");
    assert!(desk.hidden);
    assert_eq!(st.apps.len(), 2);
}

#[test]
fn refresh_reports_change_only_when_state_differs() {
    let mut c = core_with(Settings::default());
    assert!(!c.refresh().unwrap());
    c.backend_mut().devices[0].volume = 0.9;
    assert!(c.refresh().unwrap());
}

#[test]
fn saved_prefs_are_applied_to_new_sessions() {
    let mut s = Settings::default();
    s.apps.insert("spotify.exe".into(), AppPrefs { volume: 0.25, muted: true });
    let mut c = core_with(s);
    assert_eq!(app(&c, "spotify.exe").volume, 0.25);
    assert!(app(&c, "spotify.exe").muted);
    // a new chrome session appears after the user set chrome to 0.4
    c.apply(Command::SetAppVolume { key: "chrome.exe".into(), volume: 0.4 }).unwrap();
    c.refresh().unwrap();
    c.backend_mut().sessions.push(FakeBackend::session("c3", 22, Some("chrome.exe")));
    c.refresh().unwrap();
    let c3 = c.backend_mut().sessions.iter().find(|s| s.id == "c3").unwrap().volume;
    assert!((c3 - 0.4).abs() < 1e-6);
}

#[test]
fn existing_sessions_are_not_reapplied_every_refresh() {
    let mut c = core_with(Settings::default());
    c.backend_mut().calls.clear();
    // an external app changes spotify's volume; we must not fight it
    c.backend_mut().sessions[0].volume = 0.7;
    c.refresh().unwrap();
    assert!(c.backend_mut().calls.iter().all(|call| !call.starts_with("session_volume")));
    assert_eq!(c.settings().apps["spotify.exe"].volume, 0.7);
    assert!(c.take_settings_dirty());
    assert!(!c.take_settings_dirty());
}

#[test]
fn pid_keys_are_never_remembered() {
    let mut f = fake();
    f.sessions.push(FakeBackend::session("x", 99, None));
    let mut c = AppCore::new(f, Settings::default());
    c.refresh().unwrap();
    assert!(c.settings().apps.keys().all(|k| !k.starts_with("pid:")));
}

#[test]
fn set_app_volume_hits_every_session_and_clamps_without_resnapshot() {
    let mut c = core_with(Settings::default());
    let before = c.backend_mut().snapshots;
    c.apply(Command::SetAppVolume { key: "chrome.exe".into(), volume: 1.7 }).unwrap();
    assert_eq!(c.backend_mut().snapshots, before, "volume changes patch state directly");
    assert!(c.backend_mut().calls.contains(&"session_volume c1 1.00".to_string()));
    assert!(c.backend_mut().calls.contains(&"session_volume c2 1.00".to_string()));
    assert_eq!(app(&c, "chrome.exe").volume, 1.0);
}

#[test]
fn invalid_volume_is_rejected() {
    let mut c = core_with(Settings::default());
    let err = c.apply(Command::SetAppVolume { key: "chrome.exe".into(), volume: f32::NAN }).unwrap_err();
    assert_eq!(err.code(), "invalid");
}

#[test]
fn unknown_app_is_not_found() {
    let mut c = core_with(Settings::default());
    let err = c.apply(Command::SetAppMute { key: "nope.exe".into(), muted: true }).unwrap_err();
    assert_eq!(err.code(), "notFound");
}

#[test]
fn set_app_output_routes_every_pid_and_refreshes() {
    let mut c = core_with(Settings::default());
    c.apply(Command::SetAppOutput { key: "chrome.exe".into(), device_id: Some("out2".into()) }).unwrap();
    assert!(c.backend_mut().calls.contains(&"output 20 Some(\"out2\")".to_string()));
    assert!(c.backend_mut().calls.contains(&"output 21 Some(\"out2\")".to_string()));
    assert_eq!(app(&c, "chrome.exe").output.as_deref(), Some("out2"));
}

#[test]
fn set_app_output_fails_cleanly_without_routing_or_with_unknown_device() {
    let mut c = core_with(Settings::default());
    let err = c.apply(Command::SetAppOutput { key: "chrome.exe".into(), device_id: Some("zzz".into()) }).unwrap_err();
    assert_eq!(err.code(), "notFound");
    c.backend_mut().routing = false;
    let err = c.apply(Command::SetAppOutput { key: "chrome.exe".into(), device_id: None }).unwrap_err();
    assert_eq!(err.code(), "unavailable");
}

#[test]
fn set_default_device_refreshes_flags() {
    let mut c = core_with(Settings::default());
    c.apply(Command::SetDefaultDevice { id: "out2".into(), role: Role::Console }).unwrap();
    let out2 = c.state().devices.iter().find(|d| d.id == "out2").unwrap();
    assert!(out2.default_console);
    assert!(!out2.default_comms);
}

#[test]
fn mic_mute_toggle_and_set() {
    let mut c = core_with(Settings::default());
    assert_eq!(c.mic_muted(), Some(false));
    c.apply(Command::ToggleMicMute).unwrap();
    assert_eq!(c.mic_muted(), Some(true));
    assert!(c.backend_mut().calls.contains(&"device_mute mic1 true".to_string()));
    c.apply(Command::SetMicMute { muted: false }).unwrap();
    assert_eq!(c.mic_muted(), Some(false));
}

#[test]
fn mic_commands_fail_without_a_default_mic() {
    let mut f = fake();
    f.devices.retain(|d| d.flow == Flow::Render);
    let mut c = AppCore::new(f, Settings::default());
    c.refresh().unwrap();
    assert_eq!(c.mic_muted(), None);
    assert_eq!(c.apply(Command::ToggleMicMute).unwrap_err().code(), "notFound");
}

#[test]
fn mic_boost_is_clamped_to_range() {
    let mut c = core_with(Settings::default());
    c.apply(Command::SetMicBoost { id: "mic1".into(), db: 99.0 }).unwrap();
    assert!(c.backend_mut().calls.contains(&"boost mic1 30.0".to_string()));
    let mic = c.state().devices.iter().find(|d| d.id == "mic1").unwrap();
    assert_eq!(mic.boost.as_ref().unwrap().db, 30.0);
    let err = c.apply(Command::SetMicBoost { id: "out1".into(), db: 10.0 }).unwrap_err();
    assert_eq!(err.code(), "unavailable");
}

#[test]
fn alias_is_trimmed_and_blank_clears_it() {
    let mut c = core_with(Settings::default());
    c.take_settings_dirty();
    c.apply(Command::SetDeviceAlias { id: "out1".into(), alias: Some("  Headphones ".into()) }).unwrap();
    assert_eq!(c.state().devices.iter().find(|d| d.id == "out1").unwrap().name, "Headphones");
    assert!(c.take_settings_dirty());
    c.apply(Command::SetDeviceAlias { id: "out1".into(), alias: Some("   ".into()) }).unwrap();
    let d = c.state().devices.iter().find(|d| d.id == "out1").unwrap();
    assert_eq!(d.name, "Device out1");
    assert_eq!(d.alias, None);
}

#[test]
fn unplugged_devices_stay_in_state() {
    let mut c = core_with(Settings::default());
    c.backend_mut().devices[1].state = DeviceState::Unplugged;
    c.refresh().unwrap();
    let d = c.state().devices.iter().find(|d| d.id == "out2").unwrap();
    assert_eq!(d.state, DeviceState::Unplugged);
    assert_eq!(c.state().devices.last().unwrap().flow, Flow::Capture);
}

#[test]
fn config_commands_update_settings_and_state() {
    let mut c = core_with(Settings::default());
    c.take_settings_dirty();
    let cfg = MicConfig { hotkey: None, mode: HotkeyMode::PushToTalk };
    c.apply(Command::SetMicConfig { config: cfg.clone() }).unwrap();
    assert_eq!(c.state().mic, cfg);
    assert_eq!(c.settings().mic, cfg);
    let ui = UiSettings { theme: Theme::Dark, start_minimized: true, ..Default::default() };
    c.apply(Command::SetUiSettings { ui: ui.clone() }).unwrap();
    assert_eq!(c.state().ui, ui);
    c.apply(Command::ResetAppMemory).unwrap();
    assert!(c.settings().apps.is_empty());
    assert!(c.take_settings_dirty());
}

#[test]
fn meters_aggregate_max_peak_per_app() {
    let mut c = core_with(Settings::default());
    let f = c.backend_mut();
    f.peaks.sessions.insert("c1".into(), 0.2);
    f.peaks.sessions.insert("c2".into(), 0.6);
    f.peaks.master = 0.5;
    f.peaks.mic = 0.1;
    let m = c.meters();
    assert_eq!(m.apps["chrome.exe"], 0.6);
    assert_eq!(m.apps["spotify.exe"], 0.0);
    assert_eq!(m.master, 0.5);
    assert_eq!(m.mic, 0.1);
    assert!(c.backend_mut().calls.contains(&"peaks Some(\"mic1\")".to_string()));
}

#[test]
fn snapshot_errors_propagate_and_keep_old_state() {
    let mut c = core_with(Settings::default());
    let before = c.state().clone();
    c.backend_mut().fail_next_snapshot = true;
    assert!(c.refresh().is_err());
    assert_eq!(c.state(), &before);
}

#[test]
fn restart_audio_is_not_an_app_core_command() {
    let mut c = core_with(Settings::default());
    assert_eq!(c.apply(Command::RestartAudio).unwrap_err().code(), "invalid");
}
