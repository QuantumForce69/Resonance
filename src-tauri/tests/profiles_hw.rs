#![cfg(windows)]
//! Changes the default output briefly and puts it back. Quit the installed Resonance first. Run with:
//! cargo test --manifest-path src-tauri/Cargo.toml --test profiles_hw -- --ignored --nocapture
use resonance_lib::{app_state::AppCore, model::*, profile::PartChoice, settings::Settings, win::WasapiBackend};

fn default_output(core: &AppCore<WasapiBackend>) -> String {
    core.state().devices.iter().find(|d| d.flow == Flow::Render && d.default_console).unwrap().id.clone()
}

#[test]
#[ignore]
fn a_profile_restores_the_default_output_and_app_volumes() {
    let mut core = AppCore::new(WasapiBackend::new().unwrap(), Settings::default());
    core.refresh().unwrap();
    let original = default_output(&core);
    let other = core
        .state()
        .devices
        .iter()
        .find(|d| d.flow == Flow::Render && d.state == DeviceState::Active && d.id != original)
        .expect("needs a second output device")
        .id
        .clone();
    let app = core.state().apps.iter().find(|a| !a.is_system).map(|a| (a.key.clone(), a.volume));
    let parts = PartChoice { apps: true, default_output: true, ..Default::default() };
    core.apply(Command::SaveProfile { name: "Test".into(), parts, headset: None }).unwrap();

    core.apply(Command::SetDefaultDevice { id: other.clone(), role: Role::Console }).unwrap();
    if let Some((key, _)) = &app {
        core.apply(Command::SetAppVolume { key: key.clone(), volume: 0.31 }).unwrap();
    }
    core.apply(Command::ApplyProfile { id: "p1".into() }).unwrap();

    assert_eq!(default_output(&core), original);
    if let Some((key, volume)) = app {
        let now = core.state().apps.iter().find(|a| a.key == key).unwrap().volume;
        assert!((now - volume).abs() < 0.01, "{key}: {now} vs {volume}");
    }
    assert!(core.take_notices().is_empty());
}
