use super::*;
use crate::{
    engine::ProcessingSettings,
    fake::{FakeBackend, FakeEngine},
};

fn named(id: &str, flow: Flow, name: &str) -> RawDevice {
    let mut d = FakeBackend::device(id, flow);
    d.name = name.into();
    d
}

/// Speakers, a desk mic (default for both roles), VB-Cable, and one Spotify session.
fn backend() -> FakeBackend {
    let mut out1 = FakeBackend::device("out1", Flow::Render);
    out1.default_console = true;
    out1.default_comms = true;
    let mut mic1 = named("mic1", Flow::Capture, "Desk Mic");
    mic1.default_console = true;
    mic1.default_comms = true;
    FakeBackend {
        devices: vec![
            out1,
            mic1,
            named("cin", Flow::Render, "CABLE Input (VB-Audio Virtual Cable)"),
            named("cout", Flow::Capture, "CABLE Output (VB-Audio Virtual Cable)"),
        ],
        sessions: vec![FakeBackend::session("s1", 10, Some("spotify.exe"))],
        ..Default::default()
    }
}

fn processing_on() -> (AppCore<FakeBackend>, FakeEngine) {
    let engine = FakeEngine::default();
    let mut c = AppCore::new(backend(), Settings::default());
    c.set_engine(Box::new(engine.clone()));
    c.refresh().unwrap();
    let mut p: ProcessingSettings = c.settings().processing.clone();
    p.enabled = true;
    c.apply(Command::SetProcessing { processing: p }).unwrap();
    c.refresh().unwrap();
    (c, engine)
}

#[test]
fn rebuild_succeeds_even_if_the_first_refresh_fails() {
    let (mut c, _) = processing_on();
    let mut fresh = backend();
    fresh.fail_next_snapshot = true;
    c.rebuild(fresh, Box::new(FakeEngine::default())).unwrap();
    assert!(c.refresh().is_ok(), "the next resync recovers");
}

#[test]
fn rebuild_stops_the_old_engine_and_starts_the_new_one() {
    let (mut c, old) = processing_on();
    assert!(old.0.lock().unwrap().running.is_some());
    let new = FakeEngine::default();
    c.rebuild(backend(), Box::new(new.clone())).unwrap();
    assert!(old.0.lock().unwrap().running.is_none(), "old engine stopped");
    assert_eq!(new.0.lock().unwrap().starts, 1, "new engine started");
    assert!(new.0.lock().unwrap().running.is_some());
}

#[test]
fn rebuild_keeps_the_default_mic_on_the_cable_and_the_restore_point() {
    let (mut c, _) = processing_on();
    let restore = c.settings().processing_restore.clone();
    assert!(restore.is_some());
    let mut fresh = backend();
    // Windows still has CABLE Output as default (the OS kept it; only Resonance's objects were rebuilt).
    for d in &mut fresh.devices {
        d.default_console = d.id == "cout";
        d.default_comms = d.id == "cout";
    }
    c.rebuild(fresh, Box::new(FakeEngine::default())).unwrap();
    assert_eq!(c.settings().processing_restore, restore, "restore point unchanged");
    let default_mic = c.backend_mut().devices.iter().find(|d| d.flow == Flow::Capture && d.default_console).unwrap().id.clone();
    assert_eq!(default_mic, "cout");
}

#[test]
fn rebuild_reopens_call_sources_in_the_new_engine() {
    let (mut c, _) = processing_on();
    let key = c.state().apps[0].key.clone();
    c.apply(Command::SetAppSend { key, level: Some(0.7) }).unwrap();
    c.refresh().unwrap();
    let new = FakeEngine::default();
    c.rebuild(backend(), Box::new(new.clone())).unwrap();
    assert_eq!(new.0.lock().unwrap().sources.len(), 1, "sent app captured again");
}

#[test]
fn rebuild_reapplies_saved_app_volumes() {
    let (mut c, _) = processing_on();
    let key = c.state().apps[0].key.clone();
    c.apply(Command::SetAppVolume { key, volume: 0.3 }).unwrap();
    c.refresh().unwrap();
    let mut fresh = backend();
    fresh.sessions[0].volume = 1.0; // the OS forgot, e.g. after a driver reset
    c.rebuild(fresh, Box::new(FakeEngine::default())).unwrap();
    assert!((c.state().apps[0].volume - 0.3).abs() < 1e-6, "saved volume re-applied");
}
