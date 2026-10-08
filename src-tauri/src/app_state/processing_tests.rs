use super::{processing::RETRY_REFRESHES, *};
use crate::{
    engine::{EngineStatus, ProcessingSettings, RestoreDefaults, Route},
    fake::{FakeBackend, FakeEngine},
};

fn named(id: &str, flow: Flow, name: &str) -> RawDevice {
    let mut d = FakeBackend::device(id, flow);
    d.name = name.into();
    d
}

/// Speakers, a desk mic (default for both roles), a headset mic and, optionally, VB-Cable.
fn backend(with_cable: bool) -> FakeBackend {
    let mut out1 = FakeBackend::device("out1", Flow::Render);
    out1.default_console = true;
    out1.default_comms = true;
    let mut mic1 = named("mic1", Flow::Capture, "Desk Mic");
    mic1.default_console = true;
    mic1.default_comms = true;
    let mut devices = vec![out1, mic1, named("mic2", Flow::Capture, "Headset Mic")];
    if with_cable {
        devices.push(named("cin", Flow::Render, "CABLE Input (VB-Audio Virtual Cable)"));
        devices.push(named("cout", Flow::Capture, "CABLE Output (VB-Audio Virtual Cable)"));
    }
    FakeBackend { devices, ..Default::default() }
}

fn start(b: FakeBackend, settings: Settings) -> (AppCore<FakeBackend>, FakeEngine) {
    let engine = FakeEngine::default();
    let mut c = AppCore::new(b, settings);
    c.set_engine(Box::new(engine.clone()));
    c.refresh().unwrap();
    (c, engine)
}

fn core(with_cable: bool) -> (AppCore<FakeBackend>, FakeEngine) {
    start(backend(with_cable), Settings::default())
}

fn set(c: &mut AppCore<FakeBackend>, f: impl FnOnce(&mut ProcessingSettings)) {
    let mut p = c.settings().processing.clone();
    f(&mut p);
    c.apply(Command::SetProcessing { processing: p }).unwrap();
}

fn enable(c: &mut AppCore<FakeBackend>) {
    set(c, |p| p.enabled = true);
}

fn disable(c: &mut AppCore<FakeBackend>) {
    set(c, |p| p.enabled = false);
}

/// The Windows default mic for `role`, as the (fake) OS sees it.
fn os_default(c: &mut AppCore<FakeBackend>, role: Role) -> String {
    let d = c.backend_mut().devices.iter().find(|d| {
        d.flow == Flow::Capture
            && match role {
                Role::Console => d.default_console,
                Role::Communications => d.default_comms,
            }
    });
    d.unwrap().id.clone()
}

fn route(source: &str, monitor: Option<&str>) -> Route {
    Route { source_id: source.into(), cable_id: "cin".into(), monitor_id: monitor.map(String::from) }
}

#[test]
fn enabling_routes_the_mic_through_the_cable_and_makes_it_the_default() {
    let (mut c, engine) = core(true);
    assert_eq!(c.state().processing.status, EngineStatus::Off);
    assert!(c.state().processing.cable_installed);
    enable(&mut c);
    assert_eq!(engine.log().running, Some(route("mic1", None)));
    assert_eq!(os_default(&mut c, Role::Console), "cout");
    assert_eq!(os_default(&mut c, Role::Communications), "cout");
    assert_eq!(c.state().processing.status, EngineStatus::Running);
    assert!(c.state().devices.iter().any(|d| d.id == "cout" && d.default_console), "state was re-read");
    assert_eq!(
        c.settings().processing_restore,
        Some(RestoreDefaults { console: Some("mic1".into()), comms: Some("mic1".into()) })
    );
}

#[test]
fn disabling_stops_the_engine_and_restores_the_previous_mics() {
    let (mut c, engine) = core(true);
    enable(&mut c);
    disable(&mut c);
    assert_eq!(engine.log().running, None);
    assert_eq!(os_default(&mut c, Role::Console), "mic1");
    assert_eq!(os_default(&mut c, Role::Communications), "mic1");
    assert_eq!(c.settings().processing_restore, None);
    assert_eq!(c.state().processing.status, EngineStatus::Off);
}

#[test]
fn restoring_leaves_a_role_the_user_changed_alone() {
    let (mut c, _) = core(true);
    enable(&mut c);
    c.apply(Command::SetDefaultDevice { id: "mic2".into(), role: Role::Communications }).unwrap();
    disable(&mut c);
    assert_eq!(os_default(&mut c, Role::Console), "mic1");
    assert_eq!(os_default(&mut c, Role::Communications), "mic2");
}

#[test]
fn without_vb_cable_nothing_changes_until_it_appears() {
    let (mut c, engine) = core(false);
    assert!(!c.state().processing.cable_installed);
    enable(&mut c);
    assert_eq!(c.state().processing.status, EngineStatus::NoCable);
    assert_eq!(engine.log().starts, 0);
    assert_eq!(os_default(&mut c, Role::Console), "mic1");
    c.backend_mut().devices = backend(true).devices;
    c.refresh().unwrap();
    assert_eq!(c.state().processing.status, EngineStatus::Running);
    assert_eq!(os_default(&mut c, Role::Console), "cout");
}

#[test]
fn a_failed_engine_pauses_and_retries() {
    let (mut c, engine) = core(true);
    enable(&mut c);
    engine.log().failure = Some("Desk Mic was unplugged".into());
    c.refresh().unwrap();
    assert_eq!(c.state().processing.status, EngineStatus::Paused { reason: "Desk Mic was unplugged".into() });
    assert_eq!(engine.log().running, None);
    for _ in 0..RETRY_REFRESHES - 1 {
        c.refresh().unwrap();
    }
    assert_eq!(engine.log().starts, 1);
    c.refresh().unwrap();
    assert_eq!(engine.log().starts, 2);
    assert_eq!(c.state().processing.status, EngineStatus::Running);
}

#[test]
fn a_start_failure_is_shown_as_paused() {
    let (mut c, engine) = core(true);
    engine.log().fail_start = Some("CABLE Input is in use".into());
    enable(&mut c);
    assert_eq!(c.state().processing.status, EngineStatus::Paused { reason: "CABLE Input is in use".into() });
}

#[test]
fn dsp_changes_reach_the_running_engine_without_a_restart() {
    let (mut c, engine) = core(true);
    enable(&mut c);
    set(&mut c, |p| p.dsp.gate.threshold_db = -30.0);
    let log = engine.log();
    assert_eq!(log.starts, 1);
    assert_eq!(log.params.unwrap().dsp.gate.threshold_db, -30.0);
}

#[test]
fn settings_are_clamped_and_saved() {
    let (mut c, _) = core(true);
    c.take_settings_dirty();
    set(&mut c, |p| p.dsp.limiter.ceiling_db = 6.0);
    assert_eq!(c.settings().processing.dsp.limiter.ceiling_db, 0.0);
    assert!(c.take_settings_dirty());
    let mut p = c.settings().processing.clone();
    p.monitor_level = f32::NAN;
    assert_eq!(c.apply(Command::SetProcessing { processing: p }).unwrap_err().code(), "invalid");
}

#[test]
fn monitoring_plays_on_the_default_output() {
    let (mut c, engine) = core(true);
    set(&mut c, |p| {
        p.enabled = true;
        p.monitor = true;
    });
    assert_eq!(engine.log().running, Some(route("mic1", Some("out1"))));
    set(&mut c, |p| p.monitor_level = 0.3);
    assert_eq!(engine.log().starts, 1);
    assert_eq!(engine.log().params.unwrap().monitor_level, 0.3);
}

#[test]
fn a_chosen_source_is_used_but_never_the_cable_itself() {
    let (mut c, engine) = core(true);
    set(&mut c, |p| {
        p.enabled = true;
        p.source_id = Some("mic2".into());
    });
    assert_eq!(engine.log().running, Some(route("mic2", None)));
    set(&mut c, |p| p.source_id = Some("cout".into()));
    assert_eq!(engine.log().running, Some(route("mic1", None)));
    assert_eq!(engine.log().starts, 2);
}

#[test]
fn quitting_restores_the_mics_but_keeps_processing_on_for_next_time() {
    let (mut c, engine) = core(true);
    enable(&mut c);
    c.shutdown();
    assert_eq!(engine.log().running, None);
    assert_eq!(os_default(&mut c, Role::Console), "mic1");
    assert!(c.settings().processing.enabled);
    assert_eq!(c.settings().processing_restore, None);
}

#[test]
fn after_a_crash_the_remembered_mics_are_reused() {
    // Resonance died while processing: Windows still points at the cable; settings still hold the old mics.
    let mut s = Settings::default();
    s.processing.enabled = true;
    s.processing_restore = Some(RestoreDefaults { console: Some("mic2".into()), comms: Some("mic1".into()) });
    let mut b = backend(true);
    for d in b.devices.iter_mut().filter(|d| d.flow == Flow::Capture) {
        d.default_console = d.id == "cout";
        d.default_comms = d.id == "cout";
    }
    let (mut c, engine) = start(b, s);
    assert_eq!(engine.log().running, Some(route("mic2", None)));
    disable(&mut c);
    assert_eq!(os_default(&mut c, Role::Console), "mic2");
    assert_eq!(os_default(&mut c, Role::Communications), "mic1");
}

#[test]
fn meters_include_processing_peaks() {
    let (mut c, engine) = core(true);
    engine.log().peaks = (0.5, 0.25);
    let m = c.meters();
    assert_eq!((m.proc_in, m.proc_out), (0.5, 0.25));
}

#[test]
fn recognises_newer_vb_cable_names() {
    let mut b = backend(false);
    b.devices.push(named("c16", Flow::Render, "CABLE In 16 Ch (VB-Audio Virtual Cable)"));
    b.devices.push(named("cspk", Flow::Render, "Speakers (VB-Audio Virtual Cable)"));
    b.devices.push(named("cout", Flow::Capture, "CABLE Output (VB-Audio Virtual Cable)"));
    let (mut c, engine) = start(b, Settings::default());
    assert!(c.state().processing.cable_installed);
    enable(&mut c);
    assert_eq!(engine.log().running.as_ref().unwrap().cable_id, "cspk");
}
