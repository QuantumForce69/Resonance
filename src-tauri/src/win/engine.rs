use std::{
    panic::{catch_unwind, AssertUnwindSafe},
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        mpsc, Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use windows::core::{HSTRING, PCWSTR};
use windows::Win32::{
    Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0},
    Media::Audio::{
        IAudioCaptureClient, IAudioClient, IAudioRenderClient, IMMDevice, AUDCLNT_BUFFERFLAGS_SILENT,
        AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM, AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
        AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY, WAVEFORMATEX,
    },
    System::{
        Com::CLSCTX_ALL,
        Threading::{AvRevertMmThreadCharacteristics, AvSetMmThreadCharacteristicsW, CreateEventW, WaitForSingleObject},
    },
};

use super::{com::ComGuard, endpoints};
use crate::{
    engine::{
        dsp::{chain::Chain, FRAME},
        feed::{Feed, TARGET_QUEUE},
        fifo::Fifo,
        AudioEngine, EngineParams, Route, SourceSpec,
    },
    error::{AudioError, Result},
};

const WAVE_FORMAT_IEEE_FLOAT: u16 = 3;
const RATE: u32 = 48_000;
/// Shared-mode buffer durations, in 100 ns units.
const CAPTURE_BUFFER: i64 = 200_000; // 20 ms
const RENDER_BUFFER: i64 = 500_000; // 50 ms
/// How long to wait for captured audio before checking whether to stop.
const WAIT_MS: u32 = 500;
/// Consecutive empty waits (2 s) after which the source counts as lost.
const MAX_SILENT_WAITS: u32 = 4;
/// Let Windows convert between our 48 kHz float and each device's own format.
const AUTOCONVERT: u32 = AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY;
/// Above this many queued frames (60 ms) the oldest audio is dropped so latency can't build up.
const MAX_QUEUE: usize = TARGET_QUEUE * 3;
/// AUDCLNT_E_DEVICE_INVALIDATED, as it appears in an error message.
const DEVICE_GONE: &str = "0x88890004";

fn float_format(channels: u16) -> WAVEFORMATEX {
    WAVEFORMATEX {
        wFormatTag: WAVE_FORMAT_IEEE_FLOAT,
        nChannels: channels,
        nSamplesPerSec: RATE,
        nAvgBytesPerSec: RATE * 4 * channels as u32,
        nBlockAlign: 4 * channels,
        wBitsPerSample: 32,
        cbSize: 0,
    }
}

/// An auto-reset Win32 event, closed on drop.
struct Event(HANDLE);

impl Event {
    fn new() -> Result<Self> {
        unsafe { Ok(Event(CreateEventW(None, false, false, PCWSTR::null())?)) }
    }
}

impl Drop for Event {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

/// Puts this thread in the "Pro Audio" scheduling class for as long as it lives.
struct Mmcss(HANDLE);

impl Mmcss {
    fn join() -> Option<Self> {
        let mut index = 0u32;
        unsafe { AvSetMmThreadCharacteristicsW(&HSTRING::from("Pro Audio"), &mut index).ok().map(Mmcss) }
    }
}

impl Drop for Mmcss {
    fn drop(&mut self) {
        unsafe {
            let _ = AvRevertMmThreadCharacteristics(self.0);
        }
    }
}

/// Event-driven shared-mode capture, delivered as 48 kHz mono float.
struct Capture {
    client: IAudioClient,
    capture: IAudioCaptureClient,
    event: Event,
}

impl Capture {
    fn open(device: &IMMDevice) -> Result<Self> {
        unsafe {
            let client: IAudioClient = device.Activate(CLSCTX_ALL, None)?;
            let format = float_format(1);
            client.Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                AUDCLNT_STREAMFLAGS_EVENTCALLBACK | AUTOCONVERT,
                CAPTURE_BUFFER,
                0,
                &format,
                None,
            )?;
            let event = Event::new()?;
            client.SetEventHandle(event.0)?;
            let capture: IAudioCaptureClient = client.GetService()?;
            client.Start()?;
            Ok(Self { client, capture, event })
        }
    }

    /// Waits for the next packet; false on timeout.
    fn wait(&self, ms: u32) -> bool {
        unsafe { WaitForSingleObject(self.event.0, ms) == WAIT_OBJECT_0 }
    }

    /// Appends everything captured so far to `fifo`.
    fn drain(&self, fifo: &mut Fifo) -> Result<()> {
        unsafe {
            loop {
                if self.capture.GetNextPacketSize()? == 0 {
                    return Ok(());
                }
                let mut data = std::ptr::null_mut();
                let mut frames = 0u32;
                let mut flags = 0u32;
                self.capture.GetBuffer(&mut data, &mut frames, &mut flags, None, None)?;
                if data.is_null() || flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0 {
                    fifo.push_zeros(frames as usize);
                } else {
                    fifo.push_slice(std::slice::from_raw_parts(data as *const f32, frames as usize));
                }
                self.capture.ReleaseBuffer(frames)?;
            }
        }
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        unsafe {
            let _ = self.client.Stop();
        }
    }
}

/// Push-mode shared-mode render, fed 48 kHz stereo float.
struct Render {
    client: IAudioClient,
    render: IAudioRenderClient,
    size: u32,
}

impl Render {
    fn open(device: &IMMDevice) -> Result<Self> {
        unsafe {
            let client: IAudioClient = device.Activate(CLSCTX_ALL, None)?;
            let format = float_format(2);
            client.Initialize(AUDCLNT_SHAREMODE_SHARED, AUTOCONVERT, RENDER_BUFFER, 0, &format, None)?;
            let size = client.GetBufferSize()?;
            let render: IAudioRenderClient = client.GetService()?;
            let prefill = (TARGET_QUEUE as u32).min(size);
            render.GetBuffer(prefill)?;
            render.ReleaseBuffer(prefill, AUDCLNT_BUFFERFLAGS_SILENT.0 as u32)?;
            Ok(Self { client, render, size })
        }
    }

    /// Starts playback. Called once the first captured audio is queued, so a slow first packet from
    /// the microphone can't drain the prefilled silence.
    fn start(&self) -> Result<()> {
        unsafe { Ok(self.client.Start()?) }
    }

    fn silence(&self, frames: usize) -> Result<()> {
        unsafe {
            self.render.GetBuffer(frames as u32)?;
            Ok(self.render.ReleaseBuffer(frames as u32, AUDCLNT_BUFFERFLAGS_SILENT.0 as u32)?)
        }
    }

    /// Moves queued audio from `feed` into the device buffer, keeps the total queue under `MAX_QUEUE`,
    /// then steers the feed's drift control. Returns true if the device had run dry since the last write.
    fn write(&self, feed: &mut Feed, gain: f32, scratch: &mut [f32]) -> Result<bool> {
        unsafe {
            let padding = self.client.GetCurrentPadding()? as usize;
            let dry = padding == 0;
            let mut free = self.size as usize - padding;
            let mut queued = padding;
            if dry {
                // The device ran out (capture jitter or a device hiccup). Keep a bigger cushion from now
                // on and rebuild it at once with silence ahead of the next audio, rather than waiting
                // for the slow drift control.
                feed.grow_target();
                let gap = feed.target().saturating_sub(feed.queued()).min(free);
                if gap > 0 {
                    self.silence(gap)?;
                    queued += gap;
                    free -= gap;
                }
            }
            let want = free.min(feed.queued()).min(scratch.len());
            let got = feed.pull(&mut scratch[..want]);
            if got > 0 {
                let out = std::slice::from_raw_parts_mut(self.render.GetBuffer(got as u32)? as *mut f32, got * 2);
                for (pair, s) in out.as_chunks_mut::<2>().0.iter_mut().zip(&scratch[..got]) {
                    let v = s * gain;
                    *pair = [v, v];
                }
                self.render.ReleaseBuffer(got as u32, 0)?;
            }
            queued += got;
            feed.discard((queued + feed.queued()).saturating_sub(MAX_QUEUE));
            feed.steer(queued);
            Ok(dry)
        }
    }
}

impl Drop for Render {
    fn drop(&mut self) {
        unsafe {
            let _ = self.client.Stop();
        }
    }
}

#[derive(Default)]
struct Shared {
    peak_in: AtomicU32,
    peak_out: AtomicU32,
    underruns: AtomicU32,
    failure: Mutex<Option<String>>,
}

/// Non-negative floats order like their bit patterns, so `fetch_max` keeps the highest peak.
fn store_peak(slot: &AtomicU32, value: f32) {
    slot.fetch_max(value.max(0.0).to_bits(), Ordering::Relaxed);
}

fn take_peak(slot: &AtomicU32) -> f32 {
    f32::from_bits(slot.swap(0, Ordering::Relaxed))
}

/// Plain words for the common "device went away" driver error.
fn describe(e: &AudioError) -> String {
    let text = e.to_string();
    if text.contains(DEVICE_GONE) {
        "A microphone or output used for processing was disconnected".into()
    } else {
        text
    }
}

struct Running {
    stop: Arc<AtomicBool>,
    params: mpsc::Sender<EngineParams>,
    thread: JoinHandle<()>,
    shared: Arc<Shared>,
}

/// Runs the processing chain on its own real-time thread between WASAPI streams.
#[derive(Default)]
pub struct WasapiEngine {
    running: Option<Running>,
}

impl WasapiEngine {
    pub fn new() -> Self {
        Self::default()
    }

    /// Times an output ran dry since the engine started (each is an audible dropout).
    pub fn underruns(&self) -> u32 {
        self.running.as_ref().map_or(0, |r| r.shared.underruns.load(Ordering::Relaxed))
    }
}

impl AudioEngine for WasapiEngine {
    fn start(&mut self, route: &Route, params: &EngineParams) -> Result<()> {
        self.stop();
        let shared = Arc::new(Shared::default());
        let stop = Arc::new(AtomicBool::new(false));
        let (params_tx, params_rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let thread = {
            let (route, params, shared, stop) = (route.clone(), *params, shared.clone(), stop.clone());
            thread::Builder::new()
                .name("audio-engine".into())
                .spawn(move || {
                    let outcome =
                        catch_unwind(AssertUnwindSafe(|| run(&route, params, &params_rx, &stop, &shared, &ready_tx)));
                    let failure = match outcome {
                        Ok(Ok(())) => return,
                        Ok(Err(e)) => describe(&e),
                        Err(_) => "The audio engine stopped unexpectedly".to_string(),
                    };
                    if let Ok(mut f) = shared.failure.lock() {
                        *f = Some(failure.clone());
                    }
                    let _ = ready_tx.try_send(Err(AudioError::Os(failure)));
                })
                .map_err(|e| AudioError::Os(format!("Couldn't start the audio engine: {e}")))?
        };
        match ready_rx.recv_timeout(Duration::from_secs(3)) {
            Ok(Ok(())) => {
                self.running = Some(Running { stop, params: params_tx, thread, shared });
                Ok(())
            }
            Ok(Err(e)) => {
                let _ = thread.join();
                Err(e)
            }
            Err(_) => {
                stop.store(true, Ordering::Release);
                let _ = thread.join();
                Err(AudioError::Os("The audio devices didn't start in time".into()))
            }
        }
    }

    fn stop(&mut self) {
        if let Some(r) = self.running.take() {
            r.stop.store(true, Ordering::Release);
            let _ = r.thread.join();
        }
    }

    fn set_params(&mut self, params: &EngineParams) {
        if let Some(r) = &self.running {
            let _ = r.params.send(*params);
        }
    }

    fn failure(&self) -> Option<String> {
        self.running.as_ref()?.shared.failure.lock().ok()?.clone()
    }

    fn take_peaks(&self) -> (f32, f32) {
        self.running
            .as_ref()
            .map_or((0.0, 0.0), |r| (take_peak(&r.shared.peak_in), take_peak(&r.shared.peak_out)))
    }

    // Replaced in the next task by process-loopback capture.
    fn set_sources(&mut self, _: &[SourceSpec]) -> Vec<AudioError> {
        Vec::new()
    }

    fn supports_app_capture(&self) -> bool {
        false
    }
}

impl Drop for WasapiEngine {
    fn drop(&mut self) {
        self.stop();
    }
}

/// The engine thread: opens the streams, then processes every captured block until told to stop.
/// Nothing in the loop allocates or locks.
fn run(
    route: &Route,
    mut params: EngineParams,
    params_rx: &mpsc::Receiver<EngineParams>,
    stop: &AtomicBool,
    shared: &Shared,
    ready: &mpsc::SyncSender<Result<()>>,
) -> Result<()> {
    // Declared first so COM outlives every stream below.
    let _com = ComGuard::init_mta()?;
    let enumerator = endpoints::create_enumerator()?;
    let open_render = |id: &str| -> Result<Render> { Render::open(&endpoints::device_by_id(&enumerator, id)?) };
    let capture = Capture::open(&endpoints::device_by_id(&enumerator, &route.source_id)?)?;
    let cable = open_render(&route.cable_id)?;
    let monitor = route.monitor_id.as_deref().map(open_render).transpose()?;

    let mut chain = Chain::new(&params.dsp);
    let mut input = Fifo::new(RATE as usize);
    let mut cable_feed = Feed::new();
    let mut monitor_feed = Feed::new();
    let mut frame = [0.0f32; FRAME];
    let mut scratch = vec![0.0f32; RATE as usize / 10];
    let _mmcss = Mmcss::join();
    let _ = ready.try_send(Ok(()));

    let mut silent_waits = 0;
    let mut playing = false;
    while !stop.load(Ordering::Acquire) {
        if !capture.wait(WAIT_MS) {
            silent_waits += 1;
            if silent_waits >= MAX_SILENT_WAITS {
                return Err(AudioError::Unavailable("The microphone stopped sending audio".into()));
            }
            continue;
        }
        silent_waits = 0;
        while let Ok(p) = params_rx.try_recv() {
            if p.dsp != params.dsp {
                chain.set(&p.dsp);
            }
            params = p;
        }
        capture.drain(&mut input)?;
        while input.len() >= FRAME {
            input.pop_into(&mut frame);
            let (before, after) = chain.process(&mut frame);
            store_peak(&shared.peak_in, before);
            store_peak(&shared.peak_out, after);
            cable_feed.push(&frame);
            if monitor.is_some() {
                monitor_feed.push(&frame);
            }
        }
        if !playing {
            cable.start()?;
            if let Some(m) = &monitor {
                m.start()?;
            }
            playing = true;
        }
        let mut dry = cable.write(&mut cable_feed, 1.0, &mut scratch)?;
        if let Some(m) = &monitor {
            dry |= m.write(&mut monitor_feed, params.monitor_level, &mut scratch)?;
        }
        if dry {
            shared.underruns.fetch_add(1, Ordering::Relaxed);
        }
    }
    Ok(())
}
