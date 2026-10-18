#![cfg(windows)]
//! Needs VB-Cable installed, a working microphone, and the installed Resonance quit. Run with:
//! cargo test --manifest-path src-tauri/Cargo.toml --test engine_hw -- --ignored --test-threads=1
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

use resonance_lib::{
    app_state::{find_cable, AppCore},
    backend::AudioBackend,
    engine::{AudioEngine, CallSettings, DspParams, EngineParams, EngineStatus, ProcessingSettings, Route, SourceSpec},
    model::*,
    settings::Settings,
    win::{app_capture_supported, WasapiBackend, WasapiEngine},
};
use windows::core::HSTRING;
use windows::Win32::{Media::Audio::*, System::Com::*};

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

#[test]
#[ignore]
fn restart_audio_brings_the_processed_mic_back() {
    let (mut core, _, _, cable_out) = setup();
    core.set_engine(Box::new(WasapiEngine::new()));
    let console_mic = |c: &AppCore<WasapiBackend>| {
        c.state().devices.iter().find(|d| d.flow == Flow::Capture && d.default_console).map(|d| d.id.clone())
    };
    let before = console_mic(&core);
    assert_ne!(before.as_deref(), Some(cable_out.as_str()), "quit Resonance (it's processing) before this test");

    core.apply(Command::SetProcessing { processing: ProcessingSettings { enabled: true, ..Default::default() } }).unwrap();
    assert_eq!(core.state().processing.status, EngineStatus::Running);
    thread::sleep(Duration::from_secs(1));

    core.rebuild(WasapiBackend::new().unwrap(), Box::new(WasapiEngine::new())).unwrap();
    for _ in 0..20 {
        if core.state().processing.status == EngineStatus::Running {
            break;
        }
        thread::sleep(Duration::from_millis(250));
        core.refresh().unwrap();
    }
    assert_eq!(core.state().processing.status, EngineStatus::Running, "processing came back after the restart");
    assert_eq!(console_mic(&core).as_deref(), Some(cable_out.as_str()), "default mic stayed on CABLE Output");

    // Turning processing off after a restart still gives the original mic back.
    core.apply(Command::SetProcessing { processing: ProcessingSettings { enabled: false, ..Default::default() } }).unwrap();
    assert_eq!(console_mic(&core), before, "turning processing off still gives the original mic back");
}

/// Plays a 440 Hz tone on `device_id` from this process until dropped.
struct Tone {
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}

impl Tone {
    fn play(device_id: String, amplitude: f32) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let thread = thread::spawn(move || unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED).ok().unwrap();
            let e: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).unwrap();
            let device = e.GetDevice(&HSTRING::from(device_id)).unwrap();
            let client: IAudioClient = device.Activate(CLSCTX_ALL, None).unwrap();
            let format = WAVEFORMATEX {
                wFormatTag: 3,
                nChannels: 2,
                nSamplesPerSec: 48_000,
                nAvgBytesPerSec: 384_000,
                nBlockAlign: 8,
                wBitsPerSample: 32,
                cbSize: 0,
            };
            client
                .Initialize(
                    AUDCLNT_SHAREMODE_SHARED,
                    AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
                    500_000,
                    0,
                    &format,
                    None,
                )
                .unwrap();
            let render: IAudioRenderClient = client.GetService().unwrap();
            let size = client.GetBufferSize().unwrap();
            client.Start().unwrap();
            let mut phase = 0.0f32;
            while !flag.load(Ordering::Relaxed) {
                let free = size - client.GetCurrentPadding().unwrap();
                if free > 0 {
                    let buf = std::slice::from_raw_parts_mut(render.GetBuffer(free).unwrap() as *mut f32, free as usize * 2);
                    for pair in buf.as_chunks_mut::<2>().0 {
                        let v = amplitude * phase.sin();
                        phase = (phase + std::f32::consts::TAU * 440.0 / 48_000.0) % std::f32::consts::TAU;
                        *pair = [v, v];
                    }
                    render.ReleaseBuffer(free, 0).unwrap();
                }
                thread::sleep(Duration::from_millis(5));
            }
            let _ = client.Stop();
        });
        Self { stop, thread: Some(thread) }
    }
}

impl Drop for Tone {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

#[test]
#[ignore]
fn app_capture_is_supported() {
    let _com = WasapiBackend::new().unwrap();
    assert!(app_capture_supported());
}

/// Plays a tone from a separate helper process (this test binary re-run as `tone_helper`), so capturing
/// that process hears only the tone, not this process's own engine output.
struct ToneProcess(std::process::Child);

impl ToneProcess {
    fn play(device_id: &str, amplitude: f32) -> Self {
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["tone_helper", "--exact", "--ignored", "--nocapture"])
            .env("RESONANCE_TONE_DEVICE", device_id)
            .env("RESONANCE_TONE_AMPLITUDE", amplitude.to_string())
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        thread::sleep(Duration::from_secs(1));
        Self(child)
    }

    fn pid(&self) -> u32 {
        self.0.id()
    }
}

impl Drop for ToneProcess {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Not a test of its own: the body of the helper process `ToneProcess` starts. Plays until killed.
#[test]
#[ignore]
fn tone_helper() {
    let Ok(device) = std::env::var("RESONANCE_TONE_DEVICE") else { return };
    let amplitude = std::env::var("RESONANCE_TONE_AMPLITUDE").ok().and_then(|a| a.parse().ok()).unwrap_or(0.2);
    let _tone = Tone::play(device, amplitude);
    thread::sleep(Duration::from_secs(60));
}

/// Plays a short quiet tone on your speakers.
#[test]
#[ignore]
fn sends_an_app_into_the_call() {
    let (core, mic, cable_in, cable_out) = setup();
    // Play on a real output so only the send can bring the tone into the cable.
    let speaker = core
        .state()
        .devices
        .iter()
        .find(|d| d.flow == Flow::Render && d.state == DeviceState::Active && !d.original_name.contains("VB-Audio"))
        .expect("no output besides VB-Cable")
        .id
        .clone();
    // A shut gate silences the mic; apps join after the gate, so only the tone can reach the cable.
    let mut dsp = DspParams::default();
    dsp.denoise.enabled = false;
    dsp.compressor.enabled = false;
    dsp.gate.threshold_db = -10.0;
    dsp.gate.range_db = -80.0;
    let params = EngineParams { dsp, monitor_level: 0.0, call: CallSettings { duck: false, ..Default::default() } };

    let mut engine = WasapiEngine::new();
    engine.start(&Route { source_id: mic, cable_id: cable_in, monitor_id: None }, &params).unwrap();
    let tone = ToneProcess::play(&speaker, 0.2);
    let mut listener = WasapiBackend::new().unwrap();
    listener.snapshot().unwrap();
    // Meter readings every 250 ms. Windows' peak meter falls gradually, so a stopped sound shows as a
    // steady decline rather than an instant zero.
    let mut readings = |seconds: u64| -> Vec<f32> {
        (0..seconds * 4)
            .map(|_| {
                thread::sleep(Duration::from_millis(250));
                listener.peaks(Some(&cable_out)).mic
            })
            .collect()
    };
    let max = |r: &[f32]| r.iter().fold(0.0f32, |m, v| m.max(*v));
    let without = max(&readings(2));
    let problems = engine.set_sources(&[SourceSpec { key: "tone".into(), pid: tone.pid(), level: 1.0 }]);
    assert!(problems.is_empty(), "{problems:?}");
    let with = max(&readings(3));
    engine.set_sources(&[]);
    let after = readings(2);
    println!("CABLE Output peak: unsent {without:.4}, sent {with:.4}, unsent again {after:.3?}");
    assert!(without < 0.01, "audio reached the cable without a send (is something playing to CABLE Input, e.g. as the default output?)");
    let falling = after.windows(2).all(|w| w[1] <= w[0] + 0.005);
    assert!(falling && after[after.len() - 1] < with * 0.5, "the tone kept reaching the cable after unsending: {after:?}");
    assert!(with > 0.1, "the sent tone didn't reach the cable");
    assert_eq!(engine.failure(), None);
}
