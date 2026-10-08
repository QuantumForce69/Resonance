#![cfg(windows)]
//! Needs VB-Cable installed, a working microphone, and the installed Resonance quit. Run with:
//! cargo test --manifest-path src-tauri/Cargo.toml --test engine_hw -- --ignored --test-threads=1
use std::{thread, time::Duration};

use resonance_lib::{
    app_state::{find_cable, AppCore},
    backend::AudioBackend,
    engine::{AudioEngine, DspParams, EngineParams, EngineStatus, ProcessingSettings, Route},
    model::*,
    settings::Settings,
    win::{WasapiBackend, WasapiEngine},
};

/// A core over the real devices, plus VB-Cable's endpoints and a real (non-cable) microphone.
fn setup() -> (AppCore<WasapiBackend>, String, String, String) {
    let mut core = AppCore::new(WasapiBackend::new().unwrap(), Settings::default());
    core.refresh().unwrap();
    let cable = find_cable(&core.state().devices).expect("VB-Cable isn't installed");
    let mics: Vec<&Device> = core
        .state()
        .devices
        .iter()
        .filter(|d| d.flow == Flow::Capture && d.state == DeviceState::Active && d.id != cable.capture_id)
        .collect();
    let mic = mics.iter().find(|d| d.default_console).or(mics.first()).expect("no microphone").id.clone();
    (core, mic, cable.render_id, cable.capture_id)
}

#[test]
#[ignore]
fn processes_the_default_mic_into_the_cable_without_dropouts() {
    let (_, mic, cable_in, cable_out) = setup();
    // All stages off, so whatever the mic hears must come out of CABLE Output.
    let mut dsp = DspParams::default();
    dsp.denoise.enabled = false;
    dsp.gate.enabled = false;
    dsp.compressor.enabled = false;
    dsp.limiter.enabled = false;
    let params = EngineParams { dsp, monitor_level: 0.0, call: Default::default() };

    let mut engine = WasapiEngine::new();
    engine.start(&Route { source_id: mic, cable_id: cable_in, monitor_id: None }, &params).unwrap();
    let mut listener = WasapiBackend::new().unwrap();
    listener.snapshot().unwrap();
    let (mut peak_in, mut peak_cable) = (0.0f32, 0.0f32);
    let mut warm_up = 0;
    for i in 0..40 {
        thread::sleep(Duration::from_millis(250));
        if i == 3 {
            // The output cushion settles in the first second; only count dropouts after that.
            warm_up = engine.underruns();
        }
        peak_in = peak_in.max(engine.take_peaks().0);
        peak_cable = peak_cable.max(listener.peaks(Some(&cable_out)).mic);
    }
    listener.stop_metering();
    engine.set_params(&EngineParams { dsp: DspParams::default(), monitor_level: 0.0, call: Default::default() });
    thread::sleep(Duration::from_millis(500));
    let dropouts = engine.underruns() - warm_up;
    println!("mic peak {peak_in:.5}, CABLE Output peak {peak_cable:.5}, dropouts {dropouts} (+{warm_up} while starting)");
    assert_eq!(engine.failure(), None);
    assert!(peak_in > 0.0, "the microphone delivered pure digital silence (is it muted?)");
    assert!(peak_cable > 0.0, "nothing arrived on CABLE Output");
    assert!(dropouts <= 1, "{dropouts} dropouts after warm-up");
    engine.stop();
}

#[test]
#[ignore]
fn switches_the_default_mics_to_the_cable_and_back() {
    let (mut core, _, _, cable_out) = setup();
    core.set_engine(Box::new(WasapiEngine::new()));
    let default = |c: &AppCore<WasapiBackend>, role: Role| {
        c.state()
            .devices
            .iter()
            .find(|d| {
                d.flow == Flow::Capture
                    && match role {
                        Role::Console => d.default_console,
                        Role::Communications => d.default_comms,
                    }
            })
            .map(|d| d.id.clone())
    };
    let (console, comms) = (default(&core, Role::Console), default(&core, Role::Communications));
    assert_ne!(console.as_deref(), Some(cable_out.as_str()), "quit Resonance (it's processing) before this test");

    let mut p = ProcessingSettings { enabled: true, ..Default::default() };
    core.apply(Command::SetProcessing { processing: p.clone() }).unwrap();
    assert_eq!(core.state().processing.status, EngineStatus::Running);
    assert_eq!(default(&core, Role::Console).as_deref(), Some(cable_out.as_str()));
    assert_eq!(default(&core, Role::Communications).as_deref(), Some(cable_out.as_str()));
    thread::sleep(Duration::from_secs(2));

    p.enabled = false;
    core.apply(Command::SetProcessing { processing: p }).unwrap();
    assert_eq!(default(&core, Role::Console), console);
    assert_eq!(default(&core, Role::Communications), comms);
}
