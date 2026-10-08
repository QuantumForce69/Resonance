#![cfg(windows)]
use resonance_lib::{backend::AudioBackend, model::*, win::WasapiBackend};

fn backend() -> WasapiBackend {
    WasapiBackend::new().expect("WASAPI backend")
}

#[test]
#[ignore]
fn enumerates_devices_and_sessions() {
    let mut b = backend();
    let s = b.snapshot().unwrap();
    assert!(s.devices.iter().any(|d| d.flow == Flow::Render), "no output devices");
    assert!(s.devices.iter().filter(|d| d.flow == Flow::Render && d.default_console).count() <= 1);
    for d in &s.devices {
        assert!(!d.id.is_empty() && !d.name.is_empty());
        assert!((0.0..=1.0).contains(&d.volume));
    }
    println!("{:#?}", s);
}

#[test]
#[ignore]
fn set_and_restore_default_output_volume() {
    let mut b = backend();
    let s = b.snapshot().unwrap();
    let d = s.devices.iter().find(|d| d.flow == Flow::Render && d.default_console).expect("default output").clone();
    let target = if d.volume > 0.5 { d.volume - 0.05 } else { d.volume + 0.05 };
    b.set_device_volume(&d.id, target).unwrap();
    let after = b.snapshot().unwrap().devices.into_iter().find(|x| x.id == d.id).unwrap().volume;
    b.set_device_volume(&d.id, d.volume).unwrap();
    assert!((after - target).abs() < 0.01, "expected {target}, got {after}");
}

#[test]
#[ignore]
fn toggle_and_restore_mic_mute() {
    let mut b = backend();
    let s = b.snapshot().unwrap();
    let Some(mic) = s
        .devices
        .iter()
        .find(|d| d.flow == Flow::Capture && d.default_console && d.state == DeviceState::Active)
        .cloned()
    else {
        eprintln!("no microphone; skipping");
        return;
    };
    b.set_device_mute(&mic.id, !mic.muted).unwrap();
    let after = b.snapshot().unwrap().devices.into_iter().find(|x| x.id == mic.id).unwrap().muted;
    b.set_device_mute(&mic.id, mic.muted).unwrap();
    assert_eq!(after, !mic.muted);
}

#[test]
#[ignore]
fn reads_peaks_without_panicking() {
    let mut b = backend();
    let s = b.snapshot().unwrap();
    let mic = s.devices.iter().find(|d| d.flow == Flow::Capture && d.default_console).map(|d| d.id.clone());
    let p = b.peaks(mic.as_deref());
    assert!((0.0..=1.0).contains(&p.master));
    b.stop_metering();
}

#[test]
#[ignore]
fn routing_is_available_on_this_machine() {
    assert!(backend().routing_available(), "IAudioPolicyConfigFactory failed to activate");
}

#[test]
#[ignore]
fn extracts_app_icons() {
    let icon = resonance_lib::win::exe_icon_png_base64(r"C:\Windows\explorer.exe").expect("explorer icon");
    assert!(icon.len() > 100);
}

#[test]
#[ignore]
fn mic_meter_opens_and_reads() {
    let mut b = backend();
    let s = b.snapshot().unwrap();
    let Some(mic) =
        s.devices.iter().find(|d| d.flow == Flow::Capture && d.default_console && d.state == DeviceState::Active)
    else {
        eprintln!("no microphone; skipping");
        return;
    };
    for _ in 0..10 {
        let p = b.peaks(Some(&mic.id));
        assert!((0.0..=1.0).contains(&p.mic));
        std::thread::sleep(std::time::Duration::from_millis(30));
    }
    println!("mic: {} boost: {:?}", mic.name, mic.boost);
    b.stop_metering();
}

/// A default-device change made by someone else (here: a second backend) must reach a backend
/// that caches the topology, through Windows' change notification. Uses the communications
/// microphone so nothing audible changes, and restores it.
#[test]
#[ignore]
fn external_default_change_is_noticed() {
    let mut watcher = backend();
    let mut other = backend();
    let s = watcher.snapshot().unwrap();
    let mics: Vec<_> = s.devices.iter().filter(|d| d.flow == Flow::Capture && d.state == DeviceState::Active).collect();
    let (Some(original), Some(target)) = (mics.iter().find(|d| d.default_comms), mics.iter().find(|d| !d.default_comms)) else {
        eprintln!("need two microphones; skipping");
        return;
    };
    let (original, target) = (original.id.clone(), target.id.clone());
    other.snapshot().unwrap();
    other.set_default_device(&target, Role::Communications).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(300));
    let seen = watcher.snapshot().unwrap().devices.into_iter().find(|d| d.id == target).unwrap().default_comms;
    other.set_default_device(&original, Role::Communications).unwrap();
    assert!(seen, "the caching backend did not notice the external default change");
}

/// Volume changes made by another program reach the backend through Windows' notifications even though it
/// no longer re-reads everything on every poll.
#[test]
#[ignore]
fn external_volume_changes_are_noticed() {
    let mut watcher = backend();
    let mut other = backend();
    let s = watcher.snapshot().unwrap();

    let out = s.devices.iter().find(|d| d.flow == Flow::Render && d.default_console).expect("default output").clone();
    let target = if out.volume > 0.5 { out.volume - 0.07 } else { out.volume + 0.07 };
    other.snapshot().unwrap();
    other.set_device_volume(&out.id, target).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(300));
    let seen = watcher.snapshot().unwrap().devices.into_iter().find(|d| d.id == out.id).unwrap().volume;
    other.set_device_volume(&out.id, out.volume).unwrap();
    assert!((seen - target).abs() < 0.01, "device volume: expected {target}, saw {seen}");

    if let Some(session) = s.sessions.iter().find(|x| !x.is_system).cloned() {
        let target = if session.volume > 0.5 { session.volume - 0.07 } else { session.volume + 0.07 };
        other.set_session_volume(&session.id, target).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(300));
        let seen = watcher.snapshot().unwrap().sessions.into_iter().find(|x| x.id == session.id).unwrap().volume;
        other.set_session_volume(&session.id, session.volume).unwrap();
        assert!((seen - target).abs() < 0.01, "session volume: expected {target}, saw {seen}");
    }
}
