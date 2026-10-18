use std::collections::HashMap;

use super::*;
use crate::{
    engine::{EngineStatus, SourceSpec},
    fake::{FakeBackend, FakeEngine},
};

/// Speakers, a mic, VB-Cable; Spotify (pid 10), Chrome (pid 20 and its child 21) and system sounds.
fn backend() -> FakeBackend {
    let mut out1 = FakeBackend::device("out1", Flow::Render);
    out1.default_console = true;
    let mut mic1 = FakeBackend::named("mic1", Flow::Capture, "Desk Mic");
    mic1.default_console = true;
    mic1.default_comms = true;
    let mut system = FakeBackend::session("sys", 0, None);
    system.is_system = true;
    FakeBackend {
        devices: vec![
            out1,
            mic1,
            FakeBackend::named("cin", Flow::Render, "CABLE Input (VB-Audio Virtual Cable)"),
            FakeBackend::named("cout", Flow::Capture, "CABLE Output (VB-Audio Virtual Cable)"),
        ],
        sessions: vec![
            FakeBackend::session("s1", 10, Some("spotify.exe")),
            FakeBackend::session("c1", 20, Some("chrome.exe")),
            FakeBackend::session("c2", 21, Some("chrome.exe")),
            system,
        ],
        parents: HashMap::from([(21, 20), (20, 4), (10, 4)]),
        ..Default::default()
    }
}

fn core_with(settings: Settings) -> (AppCore<FakeBackend>, FakeEngine) {
    let engine = FakeEngine::default();
    let mut c = AppCore::new(backend(), settings);
    c.set_engine(Box::new(engine.clone()));
    c.refresh().unwrap();
    (c, engine)
}

fn processing(c: &mut AppCore<FakeBackend>, on: bool) {
    let mut p = c.settings().processing.clone();
    p.enabled = on;
    c.apply(Command::SetProcessing { processing: p }).unwrap();
}

fn send(c: &mut AppCore<FakeBackend>, key: &str, level: Option<f32>) -> Result<()> {
    c.apply(Command::SetAppSend { key: key.into(), level })
}

fn spec(key: &str, pid: u32, level: f32) -> SourceSpec {
    SourceSpec { key: key.into(), pid, level }
}

fn app_send(c: &AppCore<FakeBackend>, key: &str) -> Option<f32> {
    c.state().apps.iter().find(|a| a.key == key).unwrap().send
}

#[test]
fn root_pids_skips_children_of_listed_processes() {
    let parents = HashMap::from([(21, 20), (20, 4), (30, 31), (31, 30)]);
    assert_eq!(root_pids(&[20, 21], &parents), vec![20]);
    assert_eq!(root_pids(&[21], &parents), vec![21]);
    assert_eq!(root_pids(&[21, 20], &HashMap::new()), vec![21, 20]);
    assert_eq!(root_pids(&[30, 31], &parents), vec![30], "a parent loop keeps one process");
}

#[test]
fn sending_an_app_captures_it_while_processing_runs() {
    let (mut c, engine) = core_with(Settings::default());
    processing(&mut c, true);
    send(&mut c, "spotify.exe", Some(0.5)).unwrap();
    assert_eq!(engine.log().sources, vec![spec("spotify.exe", 10, 0.5)]);
    assert_eq!(app_send(&c, "spotify.exe"), Some(0.5));
    assert_eq!(c.settings().call_sends.get("spotify.exe"), Some(&0.5));
    assert!(c.take_settings_dirty());
}

#[test]
fn child_processes_are_not_captured_twice() {
    let (mut c, engine) = core_with(Settings::default());
    processing(&mut c, true);
    send(&mut c, "chrome.exe", Some(0.7)).unwrap();
    assert_eq!(engine.log().sources, vec![spec("chrome.exe", 20, 0.7)]);
}

#[test]
fn sends_wait_for_processing_and_stop_with_it() {
    let (mut c, engine) = core_with(Settings::default());
    send(&mut c, "spotify.exe", Some(0.5)).unwrap();
    assert!(engine.log().sources.is_empty());
    processing(&mut c, true);
    assert_eq!(engine.log().sources, vec![spec("spotify.exe", 10, 0.5)]);
    processing(&mut c, false);
    assert!(engine.log().sources.is_empty());
    assert_eq!(app_send(&c, "spotify.exe"), Some(0.5), "the send itself is kept");
}

#[test]
fn level_changes_and_unsending_update_the_engine() {
    let (mut c, engine) = core_with(Settings::default());
    processing(&mut c, true);
    send(&mut c, "spotify.exe", Some(0.5)).unwrap();
    send(&mut c, "spotify.exe", Some(0.2)).unwrap();
    assert_eq!(engine.log().sources, vec![spec("spotify.exe", 10, 0.2)]);
    send(&mut c, "spotify.exe", None).unwrap();
    assert!(engine.log().sources.is_empty());
    assert_eq!(app_send(&c, "spotify.exe"), None);
    let updates = engine.log().source_updates;
    c.refresh().unwrap();
    assert_eq!(engine.log().source_updates, updates, "nothing changed, nothing sent");
}

#[test]
fn system_sounds_cant_be_sent_and_levels_are_checked() {
    let (mut c, _) = core_with(Settings::default());
    assert_eq!(send(&mut c, "#system", Some(0.5)).unwrap_err().code(), "invalid");
    assert_eq!(send(&mut c, "spotify.exe", Some(f32::NAN)).unwrap_err().code(), "invalid");
    send(&mut c, "spotify.exe", Some(3.0)).unwrap();
    assert_eq!(app_send(&c, "spotify.exe"), Some(1.0));
}

#[test]
fn an_app_that_stops_playing_drops_out_and_reconnects() {
    let (mut c, engine) = core_with(Settings::default());
    processing(&mut c, true);
    send(&mut c, "spotify.exe", Some(0.5)).unwrap();
    c.backend_mut().sessions.retain(|s| s.pid != 10);
    c.refresh().unwrap();
    assert!(engine.log().sources.is_empty());
    c.backend_mut().sessions.push(FakeBackend::session("s9", 11, Some("spotify.exe")));
    c.refresh().unwrap();
    assert_eq!(engine.log().sources, vec![spec("spotify.exe", 11, 0.5)]);
}

#[test]
fn a_capture_that_fails_becomes_a_notice() {
    let (mut c, engine) = core_with(Settings::default());
    processing(&mut c, true);
    engine.log().fail_pids = vec![10];
    send(&mut c, "spotify.exe", Some(0.5)).unwrap();
    assert!(engine.log().sources.is_empty());
    let notices = c.take_notices();
    assert_eq!(notices.len(), 1);
    assert!(notices[0].to_string().contains("spotify.exe"));
    c.refresh().unwrap();
    assert!(c.take_notices().is_empty(), "not retried every refresh");
}

#[test]
fn saved_sends_are_restored() {
    let mut s = Settings::default();
    s.call_sends.insert("spotify.exe".into(), 0.4);
    s.processing.enabled = true;
    let (c, engine) = core_with(s);
    assert_eq!(app_send(&c, "spotify.exe"), Some(0.4));
    assert_eq!(engine.log().sources, vec![spec("spotify.exe", 10, 0.4)]);
}

#[test]
fn an_engine_restart_reopens_the_sources() {
    let (mut c, engine) = core_with(Settings::default());
    processing(&mut c, true);
    send(&mut c, "spotify.exe", Some(0.5)).unwrap();
    engine.log().failure = Some("unplugged".into());
    c.refresh().unwrap();
    assert!(matches!(c.state().processing.status, EngineStatus::Paused { .. }));
    assert!(engine.log().sources.is_empty());
    for _ in 0..super::processing::RETRY_REFRESHES {
        c.refresh().unwrap();
    }
    assert_eq!(c.state().processing.status, EngineStatus::Running);
    assert_eq!(engine.log().sources, vec![spec("spotify.exe", 10, 0.5)]);
}

#[test]
fn the_view_says_whether_apps_can_be_sent() {
    let (mut c, engine) = core_with(Settings::default());
    assert!(c.state().processing.call_available);
    engine.log().no_app_capture = true;
    c.refresh().unwrap();
    assert!(!c.state().processing.call_available);
}

#[test]
fn resonance_itself_is_never_listed_or_sent() {
    let mut b = backend();
    b.sessions.push(FakeBackend::session("me", std::process::id(), Some("resonance.exe")));
    let mut c = AppCore::new(b, Settings::default());
    c.refresh().unwrap();
    assert!(c.state().apps.iter().all(|a| a.key != "resonance.exe"));
}
