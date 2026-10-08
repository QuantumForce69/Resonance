use super::*;
use crate::{
    fake::{FakeBackend, FakeEngine},
    headset::part::HeadsetPart,
    profile::{AppOutput, PartChoice},
};

fn named(id: &str, flow: Flow, name: &str) -> RawDevice {
    let mut d = FakeBackend::device(id, flow);
    d.name = name.into();
    d
}

/// Speakers (default) and headphones, a mic, VB-Cable; Spotify (pid 10) and Chrome (pid 20).
fn backend() -> FakeBackend {
    let mut out1 = named("out1", Flow::Render, "Speakers");
    out1.default_console = true;
    out1.default_comms = true;
    let mut mic1 = named("mic1", Flow::Capture, "Desk Mic");
    mic1.default_console = true;
    mic1.default_comms = true;
    FakeBackend {
        devices: vec![
            out1,
            named("out2", Flow::Render, "Headphones"),
            mic1,
            named("cin", Flow::Render, "CABLE Input (VB-Audio Virtual Cable)"),
            named("cout", Flow::Capture, "CABLE Output (VB-Audio Virtual Cable)"),
        ],
        sessions: vec![
            FakeBackend::session("s1", 10, Some("spotify.exe")),
            FakeBackend::session("c1", 20, Some("chrome.exe")),
        ],
        routing: true,
        ..Default::default()
    }
}

fn core() -> (AppCore<FakeBackend>, FakeEngine) {
    let engine = FakeEngine::default();
    let mut c = AppCore::new(backend(), Settings::default());
    c.set_engine(Box::new(engine.clone()));
    c.refresh().unwrap();
    (c, engine)
}

fn parts(apps: bool, processing: bool, default_output: bool) -> PartChoice {
    PartChoice { apps, processing, headphones: false, default_output }
}

fn save(c: &mut AppCore<FakeBackend>, name: &str, choice: PartChoice) -> Result<()> {
    c.apply(Command::SaveProfile { name: name.into(), parts: choice, headset: None })
}

fn volume(c: &mut AppCore<FakeBackend>, session: &str) -> f32 {
    c.backend_mut().sessions.iter().find(|s| s.id == session).unwrap().volume
}

fn default_output(c: &mut AppCore<FakeBackend>) -> String {
    c.backend_mut().devices.iter().find(|d| d.flow == Flow::Render && d.default_console).unwrap().id.clone()
}

#[test]
fn saving_captures_only_the_chosen_parts() {
    let (mut c, _) = core();
    save(&mut c, " Work ", parts(true, false, true)).unwrap();
    let p = &c.state().profiles[0];
    assert_eq!((p.id.as_str(), p.name.as_str()), ("p1", "Work"));
    let apps = p.parts.apps.as_ref().unwrap();
    assert_eq!(apps["spotify.exe"].volume, 1.0);
    assert_eq!(apps["spotify.exe"].output, AppOutput::Default);
    assert_eq!(p.parts.default_output.as_deref(), Some("out1"));
    assert!(p.parts.processing.is_none() && p.parts.headphones.is_none());
    assert_eq!(c.state().active_profile.as_deref(), Some("p1"));
    assert!(c.take_settings_dirty());
}

#[test]
fn saving_needs_a_name_a_part_and_connected_headphones() {
    let (mut c, _) = core();
    assert_eq!(save(&mut c, "  ", parts(true, false, false)).unwrap_err().code(), "invalid");
    assert_eq!(save(&mut c, "Empty", PartChoice::default()).unwrap_err().code(), "invalid");
    let headphones = PartChoice { headphones: true, ..Default::default() };
    assert_eq!(save(&mut c, "Quiet", headphones).unwrap_err().code(), "unavailable");
    let part = HeadsetPart { transparency: Some(3), ..Default::default() };
    c.apply(Command::SaveProfile { name: "Quiet".into(), parts: headphones, headset: Some(part.clone()) }).unwrap();
    assert_eq!(c.state().profiles[0].parts.headphones, Some(part));
}

#[test]
fn applying_restores_running_apps_and_remembered_volumes() {
    let (mut c, _) = core();
    c.apply(Command::SetAppVolume { key: "spotify.exe".into(), volume: 0.3 }).unwrap();
    c.refresh().unwrap();
    save(&mut c, "Music low", parts(true, false, false)).unwrap();
    c.apply(Command::SetAppVolume { key: "spotify.exe".into(), volume: 0.9 }).unwrap();
    c.apply(Command::ApplyProfile { id: "p1".into() }).unwrap();
    assert!((volume(&mut c, "s1") - 0.3).abs() < 1e-6);
    assert!((c.settings().apps["spotify.exe"].volume - 0.3).abs() < 1e-6);
}

#[test]
fn routes_apply_now_or_when_the_app_next_plays() {
    let (mut c, _) = core();
    c.apply(Command::SetAppOutput { key: "spotify.exe".into(), device_id: Some("out2".into()) }).unwrap();
    save(&mut c, "Headphones", parts(true, false, false)).unwrap();
    c.apply(Command::SetAppOutput { key: "spotify.exe".into(), device_id: None }).unwrap();
    c.backend_mut().sessions.retain(|s| s.pid != 10);
    c.refresh().unwrap();
    c.apply(Command::ApplyProfile { id: "p1".into() }).unwrap();
    assert!(!c.backend_mut().calls.iter().any(|call| call == "output 11 Some(\"out2\")"));
    c.backend_mut().sessions.push(FakeBackend::session("s9", 11, Some("spotify.exe")));
    c.refresh().unwrap();
    assert!(c.backend_mut().calls.iter().any(|call| call == "output 11 Some(\"out2\")"), "{:?}", c.backend_mut().calls);
}

#[test]
fn applying_restores_processing_and_call_sends() {
    let (mut c, engine) = core();
    let mut p = c.settings().processing.clone();
    p.enabled = true;
    c.apply(Command::SetProcessing { processing: p.clone() }).unwrap();
    c.apply(Command::SetAppSend { key: "spotify.exe".into(), level: Some(0.5) }).unwrap();
    save(&mut c, "Call", parts(false, true, false)).unwrap();
    c.apply(Command::SetAppSend { key: "spotify.exe".into(), level: None }).unwrap();
    p.enabled = false;
    c.apply(Command::SetProcessing { processing: p }).unwrap();
    assert!(engine.log().running.is_none());
    c.apply(Command::ApplyProfile { id: "p1".into() }).unwrap();
    assert!(c.settings().processing.enabled);
    assert_eq!(c.settings().call_sends.get("spotify.exe"), Some(&0.5));
    assert!(engine.log().running.is_some());
    assert_eq!(engine.log().sources.len(), 1);
}

#[test]
fn applying_sets_the_default_output_or_explains_why_not() {
    let (mut c, _) = core();
    save(&mut c, "Speakers", parts(false, false, true)).unwrap();
    c.apply(Command::SetDefaultDevice { id: "out2".into(), role: Role::Console }).unwrap();
    c.apply(Command::ApplyProfile { id: "p1".into() }).unwrap();
    assert_eq!(default_output(&mut c), "out1");
    c.apply(Command::SetDefaultDevice { id: "out2".into(), role: Role::Console }).unwrap();
    c.backend_mut().devices.iter_mut().find(|d| d.id == "out1").unwrap().state = DeviceState::Unplugged;
    c.refresh().unwrap();
    c.take_notices();
    c.apply(Command::ApplyProfile { id: "p1".into() }).unwrap();
    assert_eq!(default_output(&mut c), "out2");
    let notices = c.take_notices();
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].to_string(), "Speakers isn't connected, so the default output wasn't changed");
}

#[test]
fn parts_a_profile_doesnt_have_are_left_alone() {
    let (mut c, _) = core();
    save(&mut c, "Output only", parts(false, false, true)).unwrap();
    c.apply(Command::SetAppVolume { key: "spotify.exe".into(), volume: 0.4 }).unwrap();
    c.apply(Command::ApplyProfile { id: "p1".into() }).unwrap();
    assert!((volume(&mut c, "s1") - 0.4).abs() < 1e-6);
}

#[test]
fn update_recaptures_existing_parts_and_keeps_headphones_when_disconnected() {
    let (mut c, _) = core();
    let part = HeadsetPart { manual_nc: Some(5), ..Default::default() };
    let choice = PartChoice { apps: true, headphones: true, ..Default::default() };
    c.apply(Command::SaveProfile { name: "Gaming".into(), parts: choice, headset: Some(part.clone()) }).unwrap();
    c.apply(Command::SetAppVolume { key: "chrome.exe".into(), volume: 0.2 }).unwrap();
    c.refresh().unwrap();
    c.apply(Command::UpdateProfile { id: "p1".into(), headset: None }).unwrap();
    let p = &c.state().profiles[0];
    assert!((p.parts.apps.as_ref().unwrap()["chrome.exe"].volume - 0.2).abs() < 1e-6);
    assert_eq!(p.parts.headphones, Some(part));
    assert!(p.parts.processing.is_none() && p.parts.default_output.is_none());
}

#[test]
fn rename_and_delete() {
    let (mut c, _) = core();
    save(&mut c, "Gaming", parts(true, false, false)).unwrap();
    save(&mut c, "Work", parts(true, false, false)).unwrap();
    c.apply(Command::RenameProfile { id: "p1".into(), name: "Games".into() }).unwrap();
    assert_eq!(c.state().profiles[0].name, "Games");
    assert_eq!(c.apply(Command::RenameProfile { id: "p1".into(), name: " ".into() }).unwrap_err().code(), "invalid");
    assert_eq!(c.state().active_profile.as_deref(), Some("p2"));
    c.apply(Command::DeleteProfile { id: "p2".into() }).unwrap();
    assert_eq!(c.state().profiles.len(), 1);
    assert_eq!(c.state().active_profile, None);
    assert_eq!(c.apply(Command::ApplyProfile { id: "p2".into() }).unwrap_err().code(), "notFound");
    assert_eq!(c.apply(Command::DeleteProfile { id: "p2".into() }).unwrap_err().code(), "notFound");
}
