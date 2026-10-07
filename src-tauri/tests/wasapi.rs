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
fn icons_and_accent() {
    let icon = resonance_lib::win::exe_icon_png_base64(r"C:\Windows\explorer.exe").expect("explorer icon");
    assert!(icon.len() > 100);
    let accent = resonance_lib::win::accent_hex();
    assert!(accent.starts_with('#') && accent.len() == 7, "{accent}");
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
