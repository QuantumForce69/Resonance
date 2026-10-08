# Resonance Phase 3b — Call Mix Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let chosen apps be heard in calls with the processed mic — per-app send keys and levels on the Mixer, a Call mix module (master level, ducking) on the Microphone page — while the user keeps hearing those apps normally.

**Architecture:** Each sent app is captured with Windows process loopback (`VAD\Process_Loopback`, include process tree) opened on the audio worker thread and handed to the 3a engine thread. There each source is downmixed to mono and resampled through a primed queue that follows the mic clock; a `CallBus` sums sources at their levels, applies the master level and voice-keyed ducking, and the chain adds the bus after the compressor and before the limiter. `AppCore` decides which processes to capture from the app list and saved sends, and keeps the engine's source set in sync on every refresh.

**Tech Stack:** Rust 1.99, windows 0.62 (process loopback, ToolHelp), rubato 5.0.1 (via 3a's `Feed`); React 19 + TypeScript, Tailwind 4, Zustand, Vitest.

## Global Constraints

- Spec: `docs/superpowers/specs/2026-10-08-phase3b-call-mix-design.md`; builds on 3a (`2026-10-08-phase3a-processed-mic-design.md`, plan `2026-10-08-phase3a-processed-mic.md` incl. its execution notes); visual language: Design v2.
- Signal: `CABLE Input = limiter( chain_voice(mic) + duck × master × Σ send_i × app_i )`. When any app is sent and the user's limiter stage is off, a fixed −1 dBFS / 50 ms safety limiter is applied. Monitoring ("Hear yourself") plays the voice only (after the compressor, before the limiter).
- Defaults/ranges: per-app send off, level 0.7 (0–1); master 1.0 (0–1); ducking on, −12 dB (−40…0). Voice detector: threshold = gate threshold if the gate is on, else −45 dBFS; 10 ms attack, 300 ms hold, 300 ms release (detector release 20 ms).
- Process loopback facts (verified by prototype): activate with `ActivateAudioInterfaceAsync(VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK, &IAudioClient::IID, Some(&propvariant), handler)`; the `PROPVARIANT` is `VT_BLOB` pointing at `AUDIOCLIENT_ACTIVATION_PARAMS` and **must be wrapped in `ManuallyDrop`** (dropping it frees the stack blob → heap corruption); the completion handler implements `IActivateAudioInterfaceCompletionHandler` + `IAgileObject` and sets an event; `Initialize(SHARED, LOOPBACK | AUTOCONVERTPCM | SRC_DEFAULT_QUALITY, 1_000_000, 0, 48 kHz float stereo)`. Captures its own process's tone at the played level. COM interfaces are not `Send`: wrap captures in a type with a documented `unsafe impl Send` (created and used only in MTA threads).
- Sources are polled (no event) once per mic cycle; loopback buffer 100 ms. A source queue starts output only when primed with `TARGET_QUEUE` (960) frames and re-primes after running dry; a silent app contributes silence and never stalls the engine.
- A PID whose ancestor is another PID of the same app is skipped (include-tree capture would double it). Parent PIDs are only looked up for apps with more than one PID.
- System sounds can't be sent. Sending a known call app (`discord.exe`, `teams.exe`, `ms-teams.exe`, `zoom.exe`, `skype.exe`, `slack.exe`) shows an info toast: "Sending a call app into the call makes everyone hear themselves."
- Without Processing running, Call keys are disabled with the hint "Turn on Processing to send apps into calls". Without process loopback support, Call keys are hidden and the module says "Sending apps into calls needs Windows 10 version 2004 or newer."
- Shell: PowerShell with `npm.cmd`; cargo via `--manifest-path M:\Resonance\src-tauri\Cargo.toml`; repo files may be CRLF (edit tools must preserve). Quit the installed Resonance before `tauri dev`/hardware tests. Never `cargo test --release` on the default target dir.
- Commits end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

---

## File Structure

```
src-tauri/
├─ Cargo.toml                         + Win32_System_Diagnostics_ToolHelp
├─ src/engine/config.rs               CallSettings; ProcessingSettings.call; ProcessingView.call_available
├─ src/engine/mod.rs                  EngineParams.call; SourceSpec; AudioEngine::{set_sources, supports_app_capture}
├─ src/engine/dsp/ducker.rs           Ducker
├─ src/engine/dsp/callbus.rs          CallBus
├─ src/engine/dsp/chain.rs            voice()/finish() split, safety limiter
├─ src/engine/source.rs               SourceQueue
├─ src/settings.rs                    Settings.call_sends
├─ src/model.rs                       App.send; Command::SetAppSend
├─ src/backend.rs                     AudioBackend::process_parents (default: empty)
├─ src/app_state/group.rs             App.send initialised
├─ src/app_state/call.rs              root_pids, desired sources, SetAppSend, sync
├─ src/app_state/call_tests.rs        controller tests
├─ src/app_state/{mod,processing}.rs  hooks: build_state sends, refresh/reconcile sync, view flag
├─ src/fake.rs                        FakeBackend.parents, FakeEngine sources/support
├─ src/win/procinfo.rs                parent_pids (ToolHelp)
├─ src/win/backend.rs                 process_parents
├─ src/win/loopback.rs                activation, LoopbackCapture, supported()
├─ src/win/engine.rs                  source messages, mixing in the run loop
└─ tests/engine_hw.rs                 + sends_an_app_into_the_call
src/
├─ lib/types.ts, store.ts             CallSettings, App.send, callAvailable, setAppSend
├─ lib/processing.ts (+test)          callReady, isCallApp, ECHO_WARNING
├─ test/fixtures.ts                   send: null, call defaults
├─ views/ChannelStrip.tsx (+test)     Call key + send knob
├─ views/MixerView.tsx                passes call state to strips
├─ views/CallMix.tsx (+test)          Call mix module
└─ views/ProcessingPanel.tsx          hosts CallMix
```

---

### Task 1: Call settings, sends and model fields

**Files:**
- Modify: `src-tauri/src/engine/config.rs`, `src-tauri/src/engine/mod.rs`, `src-tauri/src/settings.rs`, `src-tauri/src/model.rs`, `src-tauri/src/app_state/group.rs`, `src-tauri/src/app_state/processing.rs`

**Interfaces:**
- Produces: `config::CallSettings { master: f32, duck: bool, duck_db: f32 }` with `Default` and `clamped()`; `ProcessingSettings.call: CallSettings`; `ProcessingView.call_available: bool`; `EngineParams.call: CallSettings`; `Settings.call_sends: BTreeMap<String, f32>`; `App.send: Option<f32>`; `Command::SetAppSend { key: String, level: Option<f32> }` (JSON `{"type":"setAppSend","key":..,"level":..}`).

- [ ] **Step 1: Add `CallSettings` to `engine/config.rs`**

Insert before `#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]` of `ProcessingSettings`:

```rust
/// How sent apps are mixed into calls.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CallSettings {
    /// Level of all sent apps together (0..1).
    pub master: f32,
    /// Lower sent apps while the user talks.
    pub duck: bool,
    /// How far sent apps drop while the user talks.
    pub duck_db: f32,
}

impl Default for CallSettings {
    fn default() -> Self {
        Self { master: 1.0, duck: true, duck_db: -12.0 }
    }
}

impl CallSettings {
    /// Forced into the allowed ranges; non-numbers fall back to defaults.
    pub fn clamped(&self) -> Self {
        let d = CallSettings::default();
        CallSettings {
            master: fit(self.master, 0.0, 1.0, d.master),
            duck: self.duck,
            duck_db: fit(self.duck_db, -40.0, 0.0, d.duck_db),
        }
    }
}

```

In `ProcessingSettings` add after `pub monitor_level: f32,`:

```rust
    pub call: CallSettings,
```

and in its `Default` add `call: CallSettings::default(),` after `monitor_level: 0.7,` (keep the struct literal on one line or reformat; e.g. `Self { enabled: false, source_id: None, dsp: DspParams::default(), monitor: false, monitor_level: 0.7, call: CallSettings::default() }`).

In `ProcessingView` add after `pub cable_installed: bool,`:

```rust
    /// Windows can capture single apps (process loopback), so apps can be sent into calls.
    pub call_available: bool,
```

Add tests to the `tests` module:

```rust
    #[test]
    fn call_settings_default_and_clamp() {
        let c = CallSettings::default();
        assert_eq!((c.master, c.duck, c.duck_db), (1.0, true, -12.0));
        let wild = CallSettings { master: 3.0, duck: false, duck_db: -90.0 }.clamped();
        assert_eq!((wild.master, wild.duck, wild.duck_db), (1.0, false, -40.0));
        let nan = CallSettings { master: f32::NAN, duck: true, duck_db: f32::NAN }.clamped();
        assert_eq!((nan.master, nan.duck_db), (1.0, -12.0));
        let v = serde_json::to_value(ProcessingView::default()).unwrap();
        assert_eq!(v["settings"]["call"]["duckDb"], -12.0);
        assert_eq!(v["callAvailable"], false);
    }
```

In `engine/mod.rs`, re-export `CallSettings` (add it to the `pub use config::{...}` list) and add the field to `EngineParams`:

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EngineParams {
    pub dsp: DspParams,
    pub monitor_level: f32,
    pub call: CallSettings,
}
```

In `app_state/processing.rs`, make `engine_params` pass it:

```rust
    pub(super) fn engine_params(&self) -> EngineParams {
        let p = &self.settings.processing;
        EngineParams { dsp: p.dsp, monitor_level: p.monitor_level, call: p.call }
    }
```

and in `processing_view` add `call_available: self.engine.supports_app_capture(),` after `cable_installed: ...` — **this line is added in Task 4** together with the trait method; for now add `call_available: false,`.

- [ ] **Step 2: Sends in settings and the model**

`settings.rs`: add to `Settings` after `processing_restore`:

```rust
    /// Apps sent into calls: app key → send level (0..1).
    pub call_sends: BTreeMap<String, f32>,
```

with `call_sends: BTreeMap::new(),` in `Default`. In the `round_trips` test add before `s.save(&path).unwrap();`: `s.call_sends.insert("spotify.exe".into(), 0.5);` and after the text check: `assert!(text.contains("\"callSends\""));`.

`model.rs`: add to `App` after `pub output: Option<String>,`:

```rust
    /// Send level into calls, if this app is sent.
    pub send: Option<f32>,
```

add to `Command` after `SetProcessing { .. },`: `SetAppSend { key: String, level: Option<f32> },` and to `command_deserializes_from_tagged_camel_case_json`:

```rust
        let c: Command = serde_json::from_str(r#"{"type":"setAppSend","key":"spotify.exe","level":null}"#).unwrap();
        assert_eq!(c, Command::SetAppSend { key: "spotify.exe".into(), level: None });
```

`app_state/group.rs`: in the `App { ... }` literal add `send: None,` after `output: None,`.

`app_state/mod.rs` `build_state`: change the signature's `apps: Vec<App>` use so sends are filled in. Replace

```rust
    fn build_state(&self, raw: Vec<RawDevice>, apps: Vec<App>) -> UiState {
```

with

```rust
    fn build_state(&self, raw: Vec<RawDevice>, mut apps: Vec<App>) -> UiState {
        for a in &mut apps {
            a.send = self.settings.call_sends.get(&a.key).copied();
        }
```

In `apply`, add a temporary arm so the crate compiles (Task 4 replaces it):

```rust
            Command::SetAppSend { .. } => {
                return Err(AudioError::Unavailable("Sending apps into calls isn't available yet".into()));
            }
```

Every `EngineParams { dsp, monitor_level }` literal in tests must gain `call: Default::default()` — run `rg "EngineParams \{" src-tauri` and update each (the hardware tests in `tests/engine_hw.rs` too).

- [ ] **Step 3: Run the tests**

Run: `cargo test --manifest-path M:\Resonance\src-tauri\Cargo.toml --lib -q`
Then: `cargo test --manifest-path M:\Resonance\src-tauri\Cargo.toml --test engine_hw --no-run`
Expected: all PASS; hardware tests compile.

- [ ] **Step 4: Commit**

```bash
git add src-tauri
git commit -m "feat(engine): call mix settings, per-app sends and model fields"
```

---

### Task 2: Ducking, the call bus and the chain split

**Files:**
- Create: `src-tauri/src/engine/dsp/ducker.rs`, `src-tauri/src/engine/dsp/callbus.rs`
- Modify: `src-tauri/src/engine/dsp/mod.rs`, `src-tauri/src/engine/dsp/chain.rs`

**Interfaces:**
- Consumes: `CallSettings` (Task 1); `coeff`, `db_to_lin`, `flush`, `FRAME`, `SAMPLE_RATE`, `Limiter`, `LimiterParams` (3a).
- Produces: `Ducker::{new(), process(&mut self, voice: &[f32], threshold_db: f32, duck_db: Option<f32>, gains: &mut [f32])}`; `CallBus::{new(), clear(), add(&mut self, src: &[f32; FRAME], level: f32), finish(&mut self, voice: &[f32; FRAME], call: &CallSettings, threshold_db: f32) -> Option<&[f32; FRAME]>}`; `Chain::{voice(&mut self, &mut [f32; FRAME]) -> f32, finish(&mut self, &mut [f32; FRAME], apps: Option<&[f32; FRAME]>) -> f32, duck_threshold_db(&self) -> f32}` (existing `process` keeps working).

- [ ] **Step 1: Register the modules**

In `dsp/mod.rs` the module list becomes:

```rust
pub mod callbus;
pub mod chain;
pub mod compressor;
pub mod denoise;
pub mod ducker;
pub mod gate;
pub mod limiter;
```

- [ ] **Step 2: Write `dsp/ducker.rs` with tests**

```rust
use super::{coeff, db_to_lin, flush, SAMPLE_RATE};

const DETECT_RELEASE_MS: f32 = 20.0;
const ATTACK_MS: f32 = 10.0;
const HOLD_MS: f32 = 300.0;
const RELEASE_MS: f32 = 300.0;

/// Turns the user's processed voice into a gain for sent apps: down by the duck amount while they talk,
/// back up after a hold and a smooth release.
pub struct Ducker {
    detector: f32,
    detect_release: f32,
    attack: f32,
    release: f32,
    hold: u32,
    hold_left: u32,
    gain: f32,
}

impl Ducker {
    pub fn new() -> Self {
        Self {
            detector: 0.0,
            detect_release: coeff(DETECT_RELEASE_MS),
            attack: coeff(ATTACK_MS),
            release: coeff(RELEASE_MS),
            hold: (HOLD_MS * 0.001 * SAMPLE_RATE) as u32,
            hold_left: 0,
            gain: 1.0,
        }
    }

    /// Writes one gain per voice sample into `gains`. `duck_db: None` means ducking is off (gain 1).
    pub fn process(&mut self, voice: &[f32], threshold_db: f32, duck_db: Option<f32>, gains: &mut [f32]) {
        let threshold = db_to_lin(threshold_db);
        let ducked = duck_db.map_or(1.0, db_to_lin);
        for (x, g) in voice.iter().zip(gains.iter_mut()) {
            let level = x.abs();
            self.detector = if level > self.detector { level } else { flush(self.detector * self.detect_release) };
            if self.detector >= threshold {
                self.hold_left = self.hold;
            } else if self.hold_left > 0 {
                self.hold_left -= 1;
            }
            let talking = self.hold_left > 0;
            let (target, c) = if talking { (ducked, self.attack) } else { (1.0, self.release) };
            self.gain = target + flush(c * (self.gain - target));
            *g = self.gain;
        }
    }
}

impl Default for Ducker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::dsp::{
        lin_to_db,
        test_signals::{at, sine},
    };

    fn gains_for(voice: &[f32], duck_db: Option<f32>) -> Vec<f32> {
        let mut d = Ducker::new();
        let mut g = vec![0.0; voice.len()];
        for (v, out) in voice.chunks(480).zip(g.chunks_mut(480)) {
            d.process(v, -45.0, duck_db, out);
        }
        g
    }

    #[test]
    fn ducks_while_talking_and_recovers_after_hold_and_release() {
        let mut voice = vec![0.0f32; at(0.5)];
        voice.extend(sine(200.0, -20.0, 1.0));
        voice.extend(vec![0.0f32; at(2.0)]);
        let g = gains_for(&voice, Some(-12.0));
        assert!((g[at(0.49)] - 1.0).abs() < 1e-6, "no ducking before the voice");
        assert!((lin_to_db(g[at(0.55)]) + 12.0).abs() < 1.0, "ducked within 50 ms: {}", lin_to_db(g[at(0.55)]));
        assert!((lin_to_db(g[at(1.4)]) + 12.0).abs() < 0.1, "stays ducked while talking");
        assert!(lin_to_db(g[at(1.75)]) < -11.0, "held for 300 ms after the voice stops");
        assert!(lin_to_db(g[at(3.4)]) > -0.5, "recovered: {}", lin_to_db(g[at(3.4)]));
    }

    #[test]
    fn quiet_input_and_disabled_ducking_leave_gain_at_one() {
        let quiet = sine(200.0, -60.0, 1.0);
        assert!(gains_for(&quiet, Some(-12.0)).iter().all(|g| (g - 1.0).abs() < 1e-6));
        let loud = sine(200.0, -10.0, 1.0);
        assert!(gains_for(&loud, None).iter().all(|g| (g - 1.0).abs() < 1e-6));
    }
}
```

- [ ] **Step 3: Write `dsp/callbus.rs` with tests**

```rust
use super::{ducker::Ducker, FRAME};
use crate::engine::config::CallSettings;

/// Sums sent apps for one block, then applies the master level and ducking.
pub struct CallBus {
    bus: [f32; FRAME],
    gains: [f32; FRAME],
    ducker: Ducker,
    active: bool,
}

impl CallBus {
    pub fn new() -> Self {
        Self { bus: [0.0; FRAME], gains: [1.0; FRAME], ducker: Ducker::new(), active: false }
    }

    /// Starts a new block.
    pub fn clear(&mut self) {
        self.bus.fill(0.0);
        self.active = false;
    }

    /// Adds one source's block at `level`.
    pub fn add(&mut self, src: &[f32; FRAME], level: f32) {
        for (b, s) in self.bus.iter_mut().zip(src) {
            *b += s * level;
        }
        self.active = true;
    }

    /// Applies master level and ducking (driven by the processed `voice`). Returns the bus if any source
    /// was added this block. The ducker runs every block so its state stays continuous.
    pub fn finish(&mut self, voice: &[f32; FRAME], call: &CallSettings, threshold_db: f32) -> Option<&[f32; FRAME]> {
        self.ducker.process(voice, threshold_db, call.duck.then_some(call.duck_db), &mut self.gains);
        if !self.active {
            return None;
        }
        for (b, g) in self.bus.iter_mut().zip(&self.gains) {
            *b *= call.master * g;
        }
        Some(&self.bus)
    }
}

impl Default for CallBus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SILENT: [f32; FRAME] = [0.0; FRAME];

    #[test]
    fn nothing_sent_means_no_bus() {
        let mut bus = CallBus::new();
        bus.clear();
        assert!(bus.finish(&SILENT, &CallSettings::default(), -45.0).is_none());
    }

    #[test]
    fn sums_sources_at_their_levels_and_the_master() {
        let mut bus = CallBus::new();
        bus.clear();
        bus.add(&[0.5; FRAME], 0.5);
        bus.add(&[0.2; FRAME], 1.0);
        let call = CallSettings { master: 0.5, ..Default::default() };
        let out = bus.finish(&SILENT, &call, -45.0).unwrap();
        assert!(out.iter().all(|x| (x - 0.225).abs() < 1e-6), "{}", out[0]);
    }

    #[test]
    fn voice_ducks_the_bus() {
        let mut bus = CallBus::new();
        let voice = [0.5f32; FRAME];
        let call = CallSettings { duck_db: -20.0, ..Default::default() };
        let mut last = 0.0;
        for _ in 0..20 {
            bus.clear();
            bus.add(&[1.0; FRAME], 1.0);
            last = bus.finish(&voice, &call, -45.0).unwrap()[FRAME - 1];
        }
        assert!((last - 0.1).abs() < 0.005, "{last}");
    }
}
```

- [ ] **Step 4: Split the chain**

In `dsp/chain.rs`:
- change the imports to `use super::{compressor::Compressor, denoise::Denoiser, gate::Gate, limiter::Limiter, peak, FRAME};` plus `use crate::engine::config::{DspParams, LimiterParams};`
- add a field `safety: Limiter,` to `Chain` and `safety: Limiter::new(&LimiterParams::default()),` in `new`
- replace `process` with:

```rust
    /// The voice path: noise suppression, gate and compressor. Returns the block's peak before processing.
    pub fn voice(&mut self, frame: &mut [f32; FRAME]) -> f32 {
        let before = peak(frame);
        if self.p.denoise.enabled {
            self.denoise.process(frame);
        }
        if self.p.gate.enabled {
            self.gate.process(frame);
        }
        if self.p.compressor.enabled {
            self.compressor.process(frame);
        }
        before
    }

    /// Adds the call bus (if any) and limits. With apps mixed in and the limiter stage off, a fixed −1 dBFS
    /// safety limiter still protects the call. Returns the block's peak after.
    pub fn finish(&mut self, frame: &mut [f32; FRAME], apps: Option<&[f32; FRAME]>) -> f32 {
        if let Some(apps) = apps {
            for (x, a) in frame.iter_mut().zip(apps) {
                *x += a;
            }
        }
        if self.p.limiter.enabled {
            self.limiter.process(frame);
        } else if apps.is_some() {
            self.safety.process(frame);
        }
        peak(frame)
    }

    /// The voice level that counts as talking, for ducking: the gate threshold, or −45 dBFS without a gate.
    pub fn duck_threshold_db(&self) -> f32 {
        if self.p.gate.enabled {
            self.p.gate.threshold_db
        } else {
            -45.0
        }
    }

    /// Voice path and limiter with no apps; returns the peaks before and after.
    pub fn process(&mut self, frame: &mut [f32; FRAME]) -> (f32, f32) {
        let before = self.voice(frame);
        (before, self.finish(frame, None))
    }
```

Add tests to `chain.rs`'s module:

```rust
    #[test]
    fn apps_are_added_after_the_voice_path_and_limited() {
        let mut p = all_off();
        p.gate.enabled = true;
        p.gate.threshold_db = -10.0; // gate shuts the quiet "mic"
        let mut chain = Chain::new(&p);
        let apps = [0.9f32; FRAME];
        let mut out = 0.0;
        for _ in 0..20 {
            let mut frame = [0.001f32; FRAME];
            chain.voice(&mut frame);
            out = chain.finish(&mut frame, Some(&apps));
        }
        // The gate didn't touch the apps; the safety limiter held them at −1 dBFS.
        assert!((out - db_to_lin(-1.0)).abs() < 0.01, "{out}");
    }

    #[test]
    fn without_apps_and_limiter_nothing_is_limited() {
        let mut chain = Chain::new(&all_off());
        let mut frame = [0.99f32; FRAME];
        chain.voice(&mut frame);
        assert_eq!(chain.finish(&mut frame, None), 0.99);
    }
```

- [ ] **Step 5: Run the tests**

Run: `cargo test --manifest-path M:\Resonance\src-tauri\Cargo.toml --lib -q engine::dsp`
Expected: all PASS (3a's 23 plus ducker 2, callbus 3, chain 2).

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/engine/dsp
git commit -m "feat(engine): voice-keyed ducking, call bus and chain split with a safety limiter"
```

---

### Task 3: Source queue

**Files:**
- Create: `src-tauri/src/engine/source.rs`
- Modify: `src-tauri/src/engine/mod.rs`

**Interfaces:**
- Consumes: `Fifo`, `Feed`, `TARGET_QUEUE`, `FRAME` (3a).
- Produces: `SourceQueue::{new(), push(&mut self, mono: &[f32]), pull(&mut self, out: &mut [f32; FRAME])}`.

- [ ] **Step 1: Register** — add `pub mod source;` to `engine/mod.rs` after `pub mod fifo;`.

- [ ] **Step 2: Write `engine/source.rs` with tests**

```rust
use super::{
    dsp::FRAME,
    feed::{Feed, TARGET_QUEUE},
    fifo::Fifo,
};

/// One sent app on its way into the call: captured audio (on the app's playback clock) is resampled to
/// follow the mic clock. Output starts once `TARGET_QUEUE` frames are queued and pauses (silence) whenever
/// the queue runs dry, so jitter never chops the sound.
pub struct SourceQueue {
    raw: Fifo,
    block: [f32; FRAME],
    feed: Feed,
    primed: bool,
}

impl SourceQueue {
    pub fn new() -> Self {
        Self { raw: Fifo::new(48_000), block: [0.0; FRAME], feed: Feed::new(), primed: false }
    }

    /// Adds captured mono audio.
    pub fn push(&mut self, mono: &[f32]) {
        self.raw.push_slice(mono);
        while self.raw.len() >= FRAME {
            self.raw.pop_into(&mut self.block);
            self.feed.push(&self.block);
        }
    }

    /// Takes one engine block; anything missing is silence.
    pub fn pull(&mut self, out: &mut [f32; FRAME]) {
        if !self.primed {
            if self.feed.queued() < TARGET_QUEUE {
                out.fill(0.0);
                return;
            }
            self.primed = true;
        }
        let got = self.feed.pull(out);
        out[got..].fill(0.0);
        if got < FRAME {
            self.primed = false;
        } else {
            self.feed.steer(0);
        }
    }
}

impl Default for SourceQueue {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silent_until_primed_then_continuous() {
        let mut q = SourceQueue::new();
        let mut out = [1.0f32; FRAME];
        q.pull(&mut out);
        assert!(out.iter().all(|x| *x == 0.0));
        q.push(&[0.5; 3 * FRAME]);
        q.pull(&mut out);
        assert!(out[FRAME / 2..].iter().all(|x| (x - 0.5).abs() < 1e-3));
    }

    #[test]
    fn re_primes_after_running_dry() {
        let mut q = SourceQueue::new();
        let mut out = [0.0f32; FRAME];
        q.push(&[0.5; 3 * FRAME]);
        for _ in 0..10 {
            q.pull(&mut out);
        }
        assert!(out.iter().all(|x| *x == 0.0), "dry → silence");
        q.push(&[0.5; FRAME]);
        q.pull(&mut out);
        assert!(out.iter().all(|x| *x == 0.0), "not primed again yet");
    }

    /// Two minutes of 10 ms cycles: the app delivers 480 × (1 + skew) frames per cycle in uneven packets.
    fn simulate(skew: f64) -> (usize, usize, usize) {
        let mut q = SourceQueue::new();
        let mut out = [0.0f32; FRAME];
        let (mut made, mut partial, mut lo, mut hi) = (0.0f64, 0, usize::MAX, 0);
        let chunk = vec![0.25f32; 2000];
        for cycle in 0..12_000 {
            made += 480.0 * (1.0 + skew);
            let n = made.floor() as usize;
            made -= n as f64;
            let split = n / 3;
            q.push(&chunk[..split]);
            q.push(&chunk[..n - split]);
            q.pull(&mut out);
            if cycle >= 3_000 {
                if out[FRAME - 1] == 0.0 {
                    partial += 1;
                }
                let level = q.feed.queued();
                lo = lo.min(level);
                hi = hi.max(level);
            }
        }
        (lo, hi, partial)
    }

    #[test]
    fn follows_the_mic_clock_without_gaps() {
        for skew in [-0.003, 0.0, 0.003] {
            let (lo, hi, partial) = simulate(skew);
            assert_eq!(partial, 0, "skew {skew}");
            assert!(lo > TARGET_QUEUE - 240 - FRAME && hi < TARGET_QUEUE + 240 + FRAME, "skew {skew}: {lo}..{hi}");
        }
    }
}
```

- [ ] **Step 3: Run the tests**

Run: `cargo test --manifest-path M:\Resonance\src-tauri\Cargo.toml --lib -q engine::source`
Expected: 3 PASS. (The queue is measured right after a pull, so its range is the 3a target ±5 ms widened by one block.)

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/engine
git commit -m "feat(engine): primed, drift-following queue for sent app audio"
```

---
### Task 4: The controller — which processes the call hears

**Files:**
- Modify: `src-tauri/src/engine/mod.rs`, `src-tauri/src/backend.rs`, `src-tauri/src/fake.rs`, `src-tauri/src/app_state/mod.rs`, `src-tauri/src/app_state/processing.rs`
- Create: `src-tauri/src/app_state/call.rs`, `src-tauri/src/app_state/call_tests.rs`

**Interfaces:**
- Consumes: Task 1's `App.send`, `Settings.call_sends`, `Command::SetAppSend`, `ProcessingView.call_available`.
- Produces: `engine::SourceSpec { key: String, pid: u32, level: f32 }`; `AudioEngine::set_sources(&mut self, &[SourceSpec]) -> Vec<AudioError>` and `AudioEngine::supports_app_capture(&self) -> bool`; `AudioBackend::process_parents(&mut self) -> HashMap<u32, u32>` (default empty); `app_state::root_pids(&[u32], &HashMap<u32, u32>) -> Vec<u32>`; `ProcState.sources: Vec<SourceSpec>`; `FakeBackend.parents`; `EngineLog.{sources, source_updates, fail_pids, no_app_capture}`.

- [ ] **Step 1: Extend the engine interface**

In `engine/mod.rs` add after `EngineParams`:

```rust
/// One process captured into the call, at a send level.
#[derive(Debug, Clone, PartialEq)]
pub struct SourceSpec {
    /// The app it belongs to (for messages).
    pub key: String,
    pub pid: u32,
    pub level: f32,
}
```

add to the `AudioEngine` trait:

```rust
    /// Makes the running engine capture exactly these processes into the call. Returns problems to show
    /// (a capture that couldn't open); those processes are skipped.
    fn set_sources(&mut self, sources: &[SourceSpec]) -> Vec<AudioError>;
    /// Whether Windows can capture single apps (process loopback).
    fn supports_app_capture(&self) -> bool;
```

and to `impl AudioEngine for NullEngine`:

```rust
    fn set_sources(&mut self, _: &[SourceSpec]) -> Vec<AudioError> {
        Vec::new()
    }
    fn supports_app_capture(&self) -> bool {
        false
    }
```

In `backend.rs` add `use std::collections::HashMap;` and to the trait:

```rust
    /// Parent process ID of every running process (to skip child processes of a sent app).
    fn process_parents(&mut self) -> HashMap<u32, u32> {
        HashMap::new()
    }
```

- [ ] **Step 2: Extend the fakes**

In `fake.rs`:
- imports: add `use std::collections::HashMap;` and `SourceSpec` to the `engine::{...}` import.
- `FakeBackend`: add field `pub parents: HashMap<u32, u32>,` and to its `impl AudioBackend`:

```rust
    fn process_parents(&mut self) -> HashMap<u32, u32> {
        self.parents.clone()
    }
```

- `EngineLog`: add fields

```rust
    pub sources: Vec<SourceSpec>,
    pub source_updates: usize,
    /// Processes whose capture fails to open.
    pub fail_pids: Vec<u32>,
    pub no_app_capture: bool,
```

- `FakeEngine::stop`: also clear `log.sources` (add `log.sources.clear();` before `log.failure = None;`).
- add to `impl AudioEngine for FakeEngine`:

```rust
    fn set_sources(&mut self, sources: &[SourceSpec]) -> Vec<AudioError> {
        let mut log = self.log();
        log.source_updates += 1;
        let (failed, ok): (Vec<SourceSpec>, Vec<SourceSpec>) =
            sources.iter().cloned().partition(|s| log.fail_pids.contains(&s.pid));
        log.sources = ok;
        failed.into_iter().map(|s| AudioError::Unavailable(format!("Couldn't capture {}", s.key))).collect()
    }

    fn supports_app_capture(&self) -> bool {
        !self.log().no_app_capture
    }
```

- [ ] **Step 3: Write the failing tests**

`src-tauri/src/app_state/call_tests.rs`:

```rust
use std::collections::HashMap;

use super::*;
use crate::{
    engine::{EngineStatus, SourceSpec},
    fake::{FakeBackend, FakeEngine},
};

fn named(id: &str, flow: Flow, name: &str) -> RawDevice {
    let mut d = FakeBackend::device(id, flow);
    d.name = name.into();
    d
}

/// Speakers, a mic, VB-Cable; Spotify (pid 10), Chrome (pid 20 and its child 21) and system sounds.
fn backend() -> FakeBackend {
    let mut out1 = FakeBackend::device("out1", Flow::Render);
    out1.default_console = true;
    let mut mic1 = named("mic1", Flow::Capture, "Desk Mic");
    mic1.default_console = true;
    mic1.default_comms = true;
    let mut system = FakeBackend::session("sys", 0, None);
    system.is_system = true;
    FakeBackend {
        devices: vec![
            out1,
            mic1,
            named("cin", Flow::Render, "CABLE Input (VB-Audio Virtual Cable)"),
            named("cout", Flow::Capture, "CABLE Output (VB-Audio Virtual Cable)"),
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
```

Register in `app_state/mod.rs` next to the other test modules:

```rust
mod call;
#[cfg(test)]
mod call_tests;
```

and export `pub use call::root_pids;` next to the other `pub use` lines.

- [ ] **Step 4: Run them to see them fail**

Run: `cargo test --manifest-path M:\Resonance\src-tauri\Cargo.toml --lib -q app_state::call_tests`
Expected: compile errors (`call` module missing).

- [ ] **Step 5: Write `app_state/call.rs`**

```rust
use std::collections::HashMap;

use super::{clamp01, AppCore, SYSTEM_KEY};
use crate::{
    backend::AudioBackend,
    engine::SourceSpec,
    error::{AudioError, Result},
};

/// Process trees deeper than this are treated as unrelated (guards against PID-reuse loops).
const MAX_DEPTH: usize = 64;

/// Keeps only PIDs with no ancestor among `pids`: capturing a process includes its children, so a child
/// captured as well would be heard twice. If a parent loop removes every PID, the first one is kept.
pub fn root_pids(pids: &[u32], parents: &HashMap<u32, u32>) -> Vec<u32> {
    let has_listed_ancestor = |pid: u32| {
        let mut p = pid;
        for _ in 0..MAX_DEPTH {
            match parents.get(&p) {
                Some(&parent) if parent != 0 && parent != p => {
                    if pids.contains(&parent) {
                        return true;
                    }
                    p = parent;
                }
                _ => return false,
            }
        }
        false
    };
    let roots: Vec<u32> = pids.iter().copied().filter(|&pid| !has_listed_ancestor(pid)).collect();
    if roots.is_empty() {
        pids.first().copied().into_iter().collect()
    } else {
        roots
    }
}

impl<B: AudioBackend> AppCore<B> {
    /// The processes the call should hear now: every sent app that's playing, while the engine runs.
    fn desired_sources(&mut self) -> Vec<SourceSpec> {
        if self.engine_state.route.is_none() {
            return Vec::new();
        }
        let sent: Vec<(String, Vec<u32>, f32)> = self
            .state
            .apps
            .iter()
            .filter_map(|a| Some((a.key.clone(), a.pids.clone(), a.send?)))
            .filter(|(_, pids, _)| !pids.is_empty())
            .collect();
        let parents =
            if sent.iter().any(|(_, pids, _)| pids.len() > 1) { self.backend.process_parents() } else { HashMap::new() };
        sent.into_iter()
            .flat_map(|(key, pids, level)| {
                root_pids(&pids, &parents).into_iter().map(move |pid| SourceSpec { key: key.clone(), pid, level })
            })
            .collect()
    }

    /// Brings the engine's captured processes in line with the sends; capture problems become notices.
    pub(super) fn sync_sources(&mut self) {
        let want = self.desired_sources();
        if want != self.engine_state.sources {
            let problems = self.engine.set_sources(&want);
            self.notices.extend(problems);
            self.engine_state.sources = want;
        }
    }

    pub(super) fn set_app_send(&mut self, key: &str, level: Option<f32>) -> Result<()> {
        if key == SYSTEM_KEY {
            return Err(AudioError::Invalid("System sounds can't be sent into calls".into()));
        }
        match level {
            Some(v) => {
                let v = clamp01(v)?;
                self.settings.call_sends.insert(key.to_string(), v);
            }
            None => {
                self.settings.call_sends.remove(key);
            }
        }
        self.settings_dirty = true;
        let send = self.settings.call_sends.get(key).copied();
        self.patch_app(key, |a| a.send = send);
        self.sync_sources();
        Ok(())
    }
}
```

- [ ] **Step 6: Hook it in**

`app_state/processing.rs`:
- `use` line: add `SourceSpec` to the `engine::{...}` import.
- `ProcState`: add field

```rust
    /// The processes the engine was last told to capture into the call.
    pub sources: Vec<SourceSpec>,
```

- `stop_engine`: clear the sources whatever the route, so a restarted engine gets them again:

```rust
    fn stop_engine(&mut self) {
        self.engine_state.sources.clear();
        if self.engine_state.route.take().is_some() {
            self.engine.stop();
        }
    }
```

- in `reconcile_processing`'s failure branch, after `self.engine.stop();` add `self.engine_state.sources.clear();`
- `processing_view`: replace `call_available: false,` with `call_available: self.engine.supports_app_capture(),`

`app_state/mod.rs`:
- in `refresh`, call `self.sync_sources();` right before `self.state.processing = self.processing_view();`
- in the `SetProcessing` arm, call `self.sync_sources();` right before `self.state.processing = self.processing_view();`
- replace the temporary `SetAppSend` arm with:

```rust
            Command::SetAppSend { key, level } => self.set_app_send(&key, level)?,
```

- [ ] **Step 7: Run all tests**

Run: `cargo test --manifest-path M:\Resonance\src-tauri\Cargo.toml --lib -q`
Expected: all PASS, including the 11 new call tests and every 3a processing test.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src
git commit -m "feat(engine): call mix controller - sends follow apps, PIDs and processing"
```

---

### Task 5: Windows — process loopback, parent PIDs and mixing in the engine

**Files:**
- Modify: `src-tauri/Cargo.toml`, `src-tauri/src/win/procinfo.rs`, `src-tauri/src/win/backend.rs`, `src-tauri/src/win/mod.rs`, `src-tauri/src/win/engine.rs`, `src-tauri/tests/engine_hw.rs`
- Create: `src-tauri/src/win/loopback.rs`

**Interfaces:**
- Consumes: `SourceQueue` (Task 3), `CallBus`, `Chain::{voice, finish, duck_threshold_db}` (Task 2), `SourceSpec`/trait methods (Task 4).
- Produces: `procinfo::parent_pids() -> HashMap<u32, u32>`; `loopback::{LoopbackCapture::{open(pid) -> Result<Self>, drain(&self, &mut SourceQueue, &mut [f32]) -> Result<()>}, supported() -> bool}`; `win::app_capture_supported() -> bool`; `WasapiEngine` implements `set_sources`/`supports_app_capture`.

- [ ] **Step 1: Parent PIDs**

`Cargo.toml`: add `"Win32_System_Diagnostics_ToolHelp",` to the windows features.

`win/procinfo.rs`: add imports `use std::collections::HashMap;` and `System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS},` inside the `windows::Win32::{...}` import, then add:

```rust
/// Parent PID of every running process (from a ToolHelp snapshot); empty if the snapshot fails.
pub fn parent_pids() -> HashMap<u32, u32> {
    let mut map = HashMap::new();
    unsafe {
        let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return map };
        let mut entry = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        if Process32FirstW(snapshot, &mut entry).is_ok() {
            loop {
                map.insert(entry.th32ProcessID, entry.th32ParentProcessID);
                if Process32NextW(snapshot, &mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snapshot);
    }
    map
}
```

and a test in its `tests` module:

```rust
    #[test]
    fn knows_this_process_and_its_parent() {
        let parents = super::parent_pids();
        let parent = parents.get(&std::process::id()).copied().expect("this process is listed");
        assert_ne!(parent, 0);
    }
```

`win/backend.rs`, in `impl AudioBackend for WasapiBackend`:

```rust
    fn process_parents(&mut self) -> HashMap<u32, u32> {
        super::procinfo::parent_pids()
    }
```

- [ ] **Step 2: Share the event and format helpers**

In `win/engine.rs` make the helpers visible to sibling modules: `fn float_format` → `pub(super) fn float_format`, `struct Event(HANDLE);` → `pub(super) struct Event(pub(super) HANDLE);`, `fn new() -> Result<Self>` inside `impl Event` → `pub(super) fn new() -> Result<Self>`.

- [ ] **Step 3: Write `win/loopback.rs`**

```rust
use std::mem::ManuallyDrop;

use windows::core::{implement, IUnknown, Interface, Ref, HRESULT};
use windows::Win32::{
    Foundation::{HANDLE, WAIT_OBJECT_0},
    Media::Audio::{
        ActivateAudioInterfaceAsync, IActivateAudioInterfaceAsyncOperation, IActivateAudioInterfaceCompletionHandler,
        IActivateAudioInterfaceCompletionHandler_Impl, IAudioCaptureClient, IAudioClient, AUDCLNT_BUFFERFLAGS_SILENT,
        AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM, AUDCLNT_STREAMFLAGS_LOOPBACK,
        AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY, AUDIOCLIENT_ACTIVATION_PARAMS, AUDIOCLIENT_ACTIVATION_PARAMS_0,
        AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK, AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS,
        PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE, VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK,
    },
    System::{
        Com::{
            IAgileObject, IAgileObject_Impl,
            StructuredStorage::{PROPVARIANT, PROPVARIANT_0, PROPVARIANT_0_0, PROPVARIANT_0_0_0},
            BLOB,
        },
        Threading::{SetEvent, WaitForSingleObject},
        Variant::VT_BLOB,
    },
};

use super::engine::{float_format, Event};
use crate::{
    engine::source::SourceQueue,
    error::{AudioError, Result},
};

const ACTIVATE_TIMEOUT_MS: u32 = 2000;
/// Loopback buffer (100 ms): sources are polled once per mic cycle, which can be ~20 ms apart.
const BUFFER: i64 = 1_000_000;

/// Signals an event when Windows finishes activating the loopback client.
#[implement(IActivateAudioInterfaceCompletionHandler, IAgileObject)]
struct Activated(HANDLE);

impl IActivateAudioInterfaceCompletionHandler_Impl for Activated_Impl {
    fn ActivateCompleted(&self, _: Ref<IActivateAudioInterfaceAsyncOperation>) -> windows::core::Result<()> {
        unsafe { SetEvent(self.0) }
    }
}

impl IAgileObject_Impl for Activated_Impl {}

/// An audio client that hears process `pid` and its children, wherever they play.
fn activate(pid: u32) -> Result<IAudioClient> {
    unsafe {
        let mut params = AUDIOCLIENT_ACTIVATION_PARAMS {
            ActivationType: AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
            Anonymous: AUDIOCLIENT_ACTIVATION_PARAMS_0 {
                ProcessLoopbackParams: AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS {
                    TargetProcessId: pid,
                    ProcessLoopbackMode: PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE,
                },
            },
        };
        // Never dropped: PropVariantClear would try to free `params`, which lives on this stack.
        let prop = ManuallyDrop::new(PROPVARIANT {
            Anonymous: PROPVARIANT_0 {
                Anonymous: ManuallyDrop::new(PROPVARIANT_0_0 {
                    vt: VT_BLOB,
                    wReserved1: 0,
                    wReserved2: 0,
                    wReserved3: 0,
                    Anonymous: PROPVARIANT_0_0_0 {
                        blob: BLOB {
                            cbSize: std::mem::size_of::<AUDIOCLIENT_ACTIVATION_PARAMS>() as u32,
                            pBlobData: &mut params as *mut _ as *mut u8,
                        },
                    },
                }),
            },
        });
        let done = Event::new()?;
        let handler: IActivateAudioInterfaceCompletionHandler = Activated(done.0).into();
        let operation =
            ActivateAudioInterfaceAsync(VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK, &IAudioClient::IID, Some(&*prop), &handler)?;
        if WaitForSingleObject(done.0, ACTIVATE_TIMEOUT_MS) != WAIT_OBJECT_0 {
            return Err(AudioError::Os("Windows didn't open the app's audio in time".into()));
        }
        let mut result = HRESULT(0);
        let mut unknown: Option<IUnknown> = None;
        operation.GetActivateResult(&mut result, &mut unknown)?;
        result.ok()?;
        let unknown = unknown.ok_or_else(|| AudioError::Os("Windows returned no audio client".into()))?;
        Ok(unknown.cast()?)
    }
}

/// Captures one process tree's audio, downmixed to mono.
pub struct LoopbackCapture {
    client: IAudioClient,
    capture: IAudioCaptureClient,
}

// SAFETY: created on an MTA thread (the audio worker) and used and dropped only on MTA threads (the engine
// thread); WASAPI clients are free-threaded.
unsafe impl Send for LoopbackCapture {}

impl LoopbackCapture {
    pub fn open(pid: u32) -> Result<Self> {
        unsafe {
            let client = activate(pid)?;
            let format = float_format(2);
            client.Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                AUDCLNT_STREAMFLAGS_LOOPBACK | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
                BUFFER,
                0,
                &format,
                None,
            )?;
            let capture: IAudioCaptureClient = client.GetService()?;
            client.Start()?;
            Ok(Self { client, capture })
        }
    }

    /// Moves everything captured so far into `queue` as mono. `scratch` must hold one packet (100 ms).
    pub fn drain(&self, queue: &mut SourceQueue, scratch: &mut [f32]) -> Result<()> {
        unsafe {
            loop {
                if self.capture.GetNextPacketSize()? == 0 {
                    return Ok(());
                }
                let mut data = std::ptr::null_mut();
                let mut frames = 0u32;
                let mut flags = 0u32;
                self.capture.GetBuffer(&mut data, &mut frames, &mut flags, None, None)?;
                let n = (frames as usize).min(scratch.len());
                if data.is_null() || flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0 {
                    scratch[..n].fill(0.0);
                } else {
                    let stereo = std::slice::from_raw_parts(data as *const f32, n * 2);
                    for (m, pair) in scratch[..n].iter_mut().zip(stereo.as_chunks::<2>().0) {
                        *m = (pair[0] + pair[1]) * 0.5;
                    }
                }
                queue.push(&scratch[..n]);
                self.capture.ReleaseBuffer(frames)?;
            }
        }
    }
}

impl Drop for LoopbackCapture {
    fn drop(&mut self) {
        unsafe {
            let _ = self.client.Stop();
        }
    }
}

/// Whether this Windows supports capturing single apps (tries it on this process).
pub fn supported() -> bool {
    activate(std::process::id()).is_ok()
}
```

`win/mod.rs`: add `mod loopback;` and

```rust
/// Whether this Windows can capture single apps (process loopback). Needs COM on the calling thread.
pub fn app_capture_supported() -> bool {
    loopback::supported()
}
```

- [ ] **Step 4: Sources in the engine**

In `win/engine.rs`:
- imports: add `use std::{collections::HashMap, sync::OnceLock};` (merge into the existing `std` import), `use super::loopback::LoopbackCapture;`, and extend the `crate::engine` import with `dsp::callbus::CallBus`, `source::SourceQueue` and `SourceSpec`.
- add constants:

```rust
/// Most apps the call can hear at once (sources beyond this are ignored).
const MAX_SOURCES: usize = 32;
/// Largest loopback packet (100 ms) in frames.
const SOURCE_SCRATCH: usize = 4800;
```

- add the message and source types (above `struct Running`):

```rust
/// What the audio worker tells the engine thread.
enum Msg {
    Params(EngineParams),
    /// Start hearing a process (opened on the worker thread, so the engine never waits on Windows).
    Add(Box<LiveSource>),
    Remove(u32),
    Level(u32, f32),
}

/// A sent process inside the engine.
struct LiveSource {
    pid: u32,
    level: f32,
    capture: LoopbackCapture,
    queue: SourceQueue,
    dead: bool,
}
```

- `Running.params: mpsc::Sender<EngineParams>` → `Running.msgs: mpsc::Sender<Msg>`; in `start`, the channel is `mpsc::channel::<Msg>()` and `Running { stop, msgs: params_tx, thread, shared }`.
- `WasapiEngine` gets two more fields and a `Default` that sets them:

```rust
/// Runs the processing chain on its own real-time thread between WASAPI streams.
#[derive(Default)]
pub struct WasapiEngine {
    running: Option<Running>,
    /// Processes the engine thread currently hears, with their levels.
    open: HashMap<u32, f32>,
    app_capture: OnceLock<bool>,
}
```

- in `start`, after `self.stop();` add `self.open.clear();`; in `stop`, add `self.open.clear();` at the end.
- `set_params` sends `Msg::Params(*params)` on `r.msgs`.
- add to `impl AudioEngine for WasapiEngine`:

```rust
    fn set_sources(&mut self, sources: &[SourceSpec]) -> Vec<AudioError> {
        let Some(running) = &self.running else {
            self.open.clear();
            return Vec::new();
        };
        let wanted: HashMap<u32, f32> = sources.iter().map(|s| (s.pid, s.level)).collect();
        self.open.retain(|pid, _| {
            let keep = wanted.contains_key(pid);
            if !keep {
                let _ = running.msgs.send(Msg::Remove(*pid));
            }
            keep
        });
        let mut problems = Vec::new();
        for s in sources {
            match self.open.get(&s.pid) {
                Some(&level) if level == s.level => {}
                Some(_) => {
                    let _ = running.msgs.send(Msg::Level(s.pid, s.level));
                    self.open.insert(s.pid, s.level);
                }
                None => match LoopbackCapture::open(s.pid) {
                    Ok(capture) => {
                        let source = LiveSource { pid: s.pid, level: s.level, capture, queue: SourceQueue::new(), dead: false };
                        let _ = running.msgs.send(Msg::Add(Box::new(source)));
                        self.open.insert(s.pid, s.level);
                    }
                    Err(e) => problems.push(AudioError::Unavailable(format!("Couldn't hear {} for the call: {e}", s.key))),
                },
            }
        }
        problems
    }

    fn supports_app_capture(&self) -> bool {
        *self.app_capture.get_or_init(super::loopback::supported)
    }
```

- in `run`: the receiver parameter becomes `msgs: &mpsc::Receiver<Msg>` (update the call in `start`), and before the loop add:

```rust
    let mut sources: Vec<Box<LiveSource>> = Vec::with_capacity(MAX_SOURCES);
    let mut source_scratch = vec![0.0f32; SOURCE_SCRATCH];
    let mut app_block = [0.0f32; FRAME];
    let mut call_bus = CallBus::new();
```

- replace the parameter-draining loop

```rust
        while let Ok(p) = params_rx.try_recv() {
            if p.dsp != params.dsp {
                chain.set(&p.dsp);
            }
            params = p;
        }
```

with

```rust
        while let Ok(msg) = msgs.try_recv() {
            match msg {
                Msg::Params(p) => {
                    if p.dsp != params.dsp {
                        chain.set(&p.dsp);
                    }
                    params = p;
                }
                Msg::Add(source) => {
                    if sources.len() < MAX_SOURCES {
                        sources.push(source);
                    }
                }
                // Dropping a source releases its capture here; removals are rare and short.
                Msg::Remove(pid) => sources.retain(|s| s.pid != pid),
                Msg::Level(pid, level) => {
                    if let Some(s) = sources.iter_mut().find(|s| s.pid == pid) {
                        s.level = level;
                    }
                }
            }
        }
```

- replace the block from `capture.drain(&mut input)?;` through the end of the `while input.len() >= FRAME { ... }` loop with:

```rust
        capture.drain(&mut input)?;
        for s in sources.iter_mut() {
            // A process that went away just stops contributing; the mic keeps flowing.
            if s.capture.drain(&mut s.queue, &mut source_scratch).is_err() {
                s.dead = true;
            }
        }
        sources.retain(|s| !s.dead);
        while input.len() >= FRAME {
            input.pop_into(&mut frame);
            let before = chain.voice(&mut frame);
            if monitor.is_some() {
                monitor_feed.push(&frame);
            }
            call_bus.clear();
            for s in sources.iter_mut() {
                s.queue.pull(&mut app_block);
                call_bus.add(&app_block, s.level);
            }
            let apps = call_bus.finish(&frame, &params.call, chain.duck_threshold_db());
            let after = chain.finish(&mut frame, apps);
            store_peak(&shared.peak_in, before);
            store_peak(&shared.peak_out, after);
            cable_feed.push(&frame);
        }
```

- [ ] **Step 5: Build and lint**

Run: `cargo clippy --manifest-path M:\Resonance\src-tauri\Cargo.toml --all-targets -- -D warnings`
Then: `cargo test --manifest-path M:\Resonance\src-tauri\Cargo.toml --lib -q`
Expected: clean; all PASS (including `knows_this_process_and_its_parent`).

- [ ] **Step 6: Hardware tests**

Add to `src-tauri/tests/engine_hw.rs` (imports: `std::sync::{atomic::{AtomicBool, Ordering}, Arc}`, `resonance_lib::engine::{CallSettings, SourceSpec}`, `resonance_lib::win::app_capture_supported`, `windows::core::HSTRING`, `windows::Win32::{Media::Audio::*, System::Com::*}`):

```rust
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
    let _tone = Tone::play(speaker, 0.2);
    let mut listener = WasapiBackend::new().unwrap();
    listener.snapshot().unwrap();
    let mut peak = |seconds: u64| {
        let mut p = 0.0f32;
        for _ in 0..seconds * 4 {
            thread::sleep(Duration::from_millis(250));
            p = p.max(listener.peaks(Some(&cable_out)).mic);
        }
        p
    };
    let without = peak(2);
    let problems = engine.set_sources(&[SourceSpec { key: "test".into(), pid: std::process::id(), level: 1.0 }]);
    assert!(problems.is_empty(), "{problems:?}");
    let with = peak(3);
    engine.set_sources(&[]);
    let after = peak(2);
    println!("CABLE Output peak: unsent {without:.4}, sent {with:.4}, unsent again {after:.4}");
    assert!(without < 0.01 && after < 0.01, "the tone leaked into the cable without a send");
    assert!(with > 0.1, "the sent tone didn't reach the cable");
    assert_eq!(engine.failure(), None);
}
```

Run, with the installed Resonance quit: `cargo test --manifest-path M:\Resonance\src-tauri\Cargo.toml --test engine_hw -- --ignored --test-threads=1 --nocapture`
Expected: all 4 PASS; the call test prints unsent ≈ 0, sent ≈ 0.2.

- [ ] **Step 7: Commit**

```bash
git add src-tauri
git commit -m "feat(win): capture sent apps with process loopback and mix them into the call"
```

---
### Task 6: Frontend foundation — types, store, helpers, fixtures

**Files:**
- Modify: `src/lib/types.ts`, `src/lib/store.ts`, `src/lib/processing.ts`, `src/lib/processing.test.ts`, `src/test/fixtures.ts`

**Interfaces:**
- Consumes: JSON from Tasks 1 and 4 (`App.send`, `processing.settings.call`, `processing.callAvailable`, `setAppSend`).
- Produces: TS `CallSettings`; `App.send: number | null`; `ProcessingSettings.call`; `ProcessingView.callAvailable`; `setAppSend(app, level | null)`, `toggleAppSend(app)`; helpers `callReady(state)`, `isCallApp(app)`, `sentApps(state)`, `pct(v)`, `signedDb(v)`, constants `DEFAULT_SEND = 0.7`, `ECHO_WARNING`, `CALL_OFF_HINT`; fixtures with `send: null` and call defaults.

- [ ] **Step 1: Types**

In `src/lib/types.ts`:
- add `send: number | null;` to `App` after `output: string | null;`
- add before `export interface ProcessingSettings`:

```ts
export interface CallSettings {
  master: number;
  duck: boolean;
  duckDb: number;
}
```

- add `call: CallSettings;` to `ProcessingSettings` after `monitorLevel: number;`
- add `callAvailable: boolean;` to `ProcessingView` after `cableInstalled: boolean;`
- add `| { type: "setAppSend"; key: string; level: number | null }` to `Command` after the `setProcessing` member (moving the `;`).

- [ ] **Step 2: Fixtures**

In `src/test/fixtures.ts`: in `makeApp` add `send: null,` after `output: null,`; in `makeProcessing` change the settings object to include `call: { master: 1, duck: true, duckDb: -12 },` after `monitorLevel: 0.7,` and add `callAvailable: true,` after `cableInstalled: true,`.

- [ ] **Step 3: Write the failing helper tests**

Append to `src/lib/processing.test.ts` (and add `callReady, isCallApp, pct, sentApps, signedDb` to its import from `./processing`, `makeApp` to the fixtures import):

```ts
describe("call mix helpers", () => {
  it("is ready only when Windows supports it and processing runs", () => {
    expect(callReady(makeState())).toBe(false);
    const running = makeState({ processing: makeProcessing({ status: { state: "running" } }) });
    expect(callReady(running)).toBe(true);
    running.processing.callAvailable = false;
    expect(callReady(running)).toBe(false);
  });

  it("recognises call apps and lists sent apps", () => {
    expect(isCallApp(makeApp({ key: "discord.exe" }))).toBe(true);
    expect(isCallApp(makeApp({ key: "spotify.exe" }))).toBe(false);
    const s = makeState();
    s.apps[0] = { ...s.apps[0], send: 0.5 };
    expect(sentApps(s).map((a) => a.key)).toEqual(["spotify.exe"]);
  });

  it("formats levels", () => {
    expect(pct(0.7)).toBe("70%");
    expect(signedDb(-12)).toBe("-12 dB");
    expect(signedDb(3.25)).toBe("+3.3 dB");
  });
});
```

Run: `npm.cmd test -- src/lib/processing.test.ts`
Expected: FAIL (helpers missing).

- [ ] **Step 4: Helpers and store actions**

Append to `src/lib/processing.ts` (and change its import to `import type { App, CompressorParams, Device, UiState } from "./types";`):

```ts
const CALL_APPS = ["discord.exe", "teams.exe", "ms-teams.exe", "zoom.exe", "skype.exe", "slack.exe"];
export const DEFAULT_SEND = 0.7;
export const ECHO_WARNING = "Sending a call app into the call makes everyone hear themselves.";
export const CALL_OFF_HINT = "Turn on Processing to send apps into calls";

/** Apps can be sent right now: Windows can capture single apps and processing is running. */
export const callReady = (s: UiState) => s.processing.callAvailable && isProcessed(s);
export const isCallApp = (a: App) => CALL_APPS.includes(a.key);
export const sentApps = (s: UiState) => s.apps.filter((a) => a.send !== null);

const round1 = (v: number) => Math.round(v * 10) / 10;
export const pct = (v: number) => `${Math.round(v * 100)}%`;
export const signedDb = (v: number) => `${v > 0 ? "+" : ""}${round1(v)} dB`;
```

In `src/lib/store.ts`: add `import { DEFAULT_SEND, ECHO_WARNING, isCallApp } from "./processing";` and after `setProcessing`:

```ts
export const setAppSend = (app: App, level: number | null) => run({ type: "setAppSend", key: app.key, level });

/** Sends an app into calls (at the default level) or stops sending it; warns before sending a call app. */
export function toggleAppSend(app: App) {
  if (app.send === null && isCallApp(app)) useStore.getState().pushToast("info", ECHO_WARNING);
  return setAppSend(app, app.send === null ? DEFAULT_SEND : null);
}
```

- [ ] **Step 5: Run the tests and type check**

Run: `npm.cmd test`
Then: `npx.cmd tsc --noEmit`
Expected: all PASS; no type errors.

- [ ] **Step 6: Commit**

```bash
git add src/lib src/test/fixtures.ts
git commit -m "feat(ui): call mix types, store actions and helpers"
```

---

### Task 7: Call keys on the Mixer and the Call mix module

**Files:**
- Modify: `src/views/ChannelStrip.tsx`, `src/views/MixerView.tsx`, `src/views/ProcessingPanel.tsx`
- Create: `src/views/ChannelStrip.test.tsx`, `src/views/CallMix.tsx`, `src/views/CallMix.test.tsx`

**Interfaces:**
- Consumes: Task 6 helpers/actions; `Knob`, `Slider`, `Toggle`, `Banner`, `AppIcon` (earlier phases).
- Produces: `ChannelStrip({ app, call: "ready" | "off" | "unavailable" })`; `CallMix({ state })` rendering a `region` named "Call mix".

- [ ] **Step 1: Write the failing strip tests**

`src/views/ChannelStrip.test.tsx`:

```tsx
import { fireEvent, render, screen, within } from "@testing-library/react";
import { ChannelStrip } from "./ChannelStrip";
import { makeApp } from "../test/fixtures";
import { useStore } from "../lib/store";
import * as api from "../lib/api";

vi.mock("../lib/api", () => ({
  send: vi.fn().mockResolvedValue(undefined),
  getAppIcon: vi.fn().mockResolvedValue(null),
}));

const spotify = makeApp({ key: "spotify.exe", name: "Spotify" });

describe("ChannelStrip call send", () => {
  beforeEach(() => {
    vi.mocked(api.send).mockClear();
    useStore.setState({ toasts: [] });
  });

  it("sends an app into calls at the default level", () => {
    render(<ChannelStrip app={spotify} call="ready" />);
    fireEvent.click(screen.getByRole("button", { name: "Send Spotify into calls" }));
    expect(api.send).toHaveBeenCalledWith({ type: "setAppSend", key: "spotify.exe", level: 0.7 });
  });

  it("shows the send level of a sent app and adjusts it", () => {
    render(<ChannelStrip app={{ ...spotify, send: 0.5 }} call="ready" />);
    const strip = screen.getByRole("region", { name: "Spotify" });
    expect(within(strip).getByRole("button", { name: "Send Spotify into calls" })).toHaveAttribute("aria-pressed", "true");
    fireEvent.keyDown(within(strip).getByRole("slider", { name: "Send" }), { key: "ArrowUp" });
    expect(api.send).toHaveBeenLastCalledWith({ type: "setAppSend", key: "spotify.exe", level: 0.55 });
    fireEvent.click(within(strip).getByRole("button", { name: "Send Spotify into calls" }));
    expect(api.send).toHaveBeenLastCalledWith({ type: "setAppSend", key: "spotify.exe", level: null });
  });

  it("is disabled with a hint while processing is off", () => {
    render(<ChannelStrip app={{ ...spotify, send: 0.5 }} call="off" />);
    const key = screen.getByRole("button", { name: "Send Spotify into calls" });
    expect(key).toBeDisabled();
    expect(key).toHaveAttribute("title", "Turn on Processing to send apps into calls");
    expect(screen.queryByRole("slider", { name: "Send" })).toBeNull();
  });

  it("has no call key when Windows can't capture apps, or for system sounds", () => {
    const { rerender } = render(<ChannelStrip app={spotify} call="unavailable" />);
    expect(screen.queryByRole("button", { name: /into calls/ })).toBeNull();
    rerender(<ChannelStrip app={makeApp({ key: "#system", name: "System sounds", isSystem: true })} call="ready" />);
    expect(screen.queryByRole("button", { name: /into calls/ })).toBeNull();
  });

  it("warns before sending a call app", () => {
    render(<ChannelStrip app={makeApp({ key: "discord.exe", name: "Discord" })} call="ready" />);
    fireEvent.click(screen.getByRole("button", { name: "Send Discord into calls" }));
    expect(useStore.getState().toasts.map((t) => t.message)).toContain(
      "Sending a call app into the call makes everyone hear themselves.",
    );
  });
});
```

Run: `npm.cmd test -- src/views/ChannelStrip.test.tsx`
Expected: FAIL (no Call key yet).

- [ ] **Step 2: Add the Call key to the strip**

In `src/views/ChannelStrip.tsx`:
- imports: change the store import to `import { run, setAppSend, toggleAppMute, toggleAppSend } from "../lib/store";`, add `import { CALL_OFF_HINT, pct } from "../lib/processing";` and `import { Knob } from "../components/Knob";`
- add above `export function ChannelStrip`:

```tsx
export type CallState = "ready" | "off" | "unavailable";

/** The strip's "Call" key and, while sent, its send-level knob. */
function CallSend({ app, ready }: { app: App; ready: boolean }) {
  const sent = app.send !== null;
  return (
    <div className="flex flex-col items-center gap-1.5">
      <button
        type="button"
        aria-pressed={sent}
        aria-label={`Send ${app.name} into calls`}
        disabled={!ready}
        title={ready ? undefined : CALL_OFF_HINT}
        onClick={() => void toggleAppSend(app)}
        className={`key h-7 w-full text-[12px] disabled:cursor-default disabled:opacity-45 ${sent && ready ? "key-lit" : ""}`}
      >
        Call
      </button>
      {sent && ready && (
        <Knob size="small" label="Send" value={app.send ?? 0} min={0} max={1} step={0.05} format={pct} onChange={(v) => setAppSend(app, v)} />
      )}
    </div>
  );
}
```

- change the signature to `export function ChannelStrip({ app, call }: { app: App; call: CallState }) {`
- after the `<div className="flex justify-center">` that holds the `MuteButton`, add:

```tsx
      {call !== "unavailable" && !app.isSystem && <CallSend app={app} ready={call === "ready"} />}
```

In `src/views/MixerView.tsx`: add `import { callReady } from "../lib/processing";` and, inside the component before the `return`, `const call = !state.processing.callAvailable ? "unavailable" : callReady(state) ? "ready" : "off";` then change `<ChannelStrip key={a.key} app={a} />` to `<ChannelStrip key={a.key} app={a} call={call} />`.

Run: `npm.cmd test -- src/views/ChannelStrip.test.tsx src/views/MixerView.test.tsx`
Expected: PASS.

- [ ] **Step 3: Write the failing Call mix tests**

`src/views/CallMix.test.tsx`:

```tsx
import { fireEvent, render, screen } from "@testing-library/react";
import { CallMix } from "./CallMix";
import { makeProcessing, makeState } from "../test/fixtures";
import * as api from "../lib/api";

vi.mock("../lib/api", () => ({
  send: vi.fn().mockResolvedValue(undefined),
  getAppIcon: vi.fn().mockResolvedValue(null),
}));

function lastCall() {
  const cmd = vi.mocked(api.send).mock.calls.at(-1)![0];
  if (cmd.type !== "setProcessing") throw new Error(`unexpected ${cmd.type}`);
  return cmd.processing.call;
}

describe("CallMix", () => {
  beforeEach(() => vi.mocked(api.send).mockClear());

  it("sets the level of all sent apps", () => {
    render(<CallMix state={makeState()} />);
    fireEvent.change(screen.getByRole("slider", { name: "Apps in call" }), { target: { value: "50" } });
    expect(lastCall().master).toBe(0.5);
  });

  it("turns ducking off and sets how far apps duck", () => {
    render(<CallMix state={makeState()} />);
    fireEvent.click(screen.getByRole("switch", { name: "Duck apps while I talk" }));
    expect(lastCall().duck).toBe(false);
    fireEvent.keyDown(screen.getByRole("slider", { name: "Duck by" }), { key: "ArrowDown" });
    expect(lastCall().duckDb).toBe(-13);
  });

  it("lists sent apps and stops sending one", () => {
    const s = makeState();
    s.apps[0] = { ...s.apps[0], send: 0.5 };
    render(<CallMix state={s} />);
    fireEvent.click(screen.getByRole("button", { name: "Stop sending Spotify" }));
    expect(api.send).toHaveBeenCalledWith({ type: "setAppSend", key: "spotify.exe", level: null });
  });

  it("says when nothing is sent, and when Windows can't do it", () => {
    const { rerender } = render(<CallMix state={makeState()} />);
    expect(screen.getByText(/no apps sent yet/i)).toBeInTheDocument();
    rerender(<CallMix state={makeState({ processing: makeProcessing({ callAvailable: false }) })} />);
    expect(screen.getByText(/needs windows 10 version 2004 or newer/i)).toBeInTheDocument();
    expect(screen.queryByRole("slider")).toBeNull();
  });
});
```

Run: `npm.cmd test -- src/views/CallMix.test.tsx`
Expected: FAIL (`./CallMix` missing).

- [ ] **Step 4: Write `src/views/CallMix.tsx`**

```tsx
import { pct, sentApps, signedDb } from "../lib/processing";
import { setAppSend, setProcessing } from "../lib/store";
import type { CallSettings, UiState } from "../lib/types";
import { AppIcon } from "../components/AppIcon";
import { Knob } from "../components/Knob";
import { Slider } from "../components/Slider";
import { Banner, Toggle } from "../components/ui";

/** Processing's Call mix module: level of sent apps, ducking, and the apps currently sent. */
export function CallMix({ state }: { state: UiState }) {
  const p = state.processing.settings;
  const call = p.call;
  const save = (patch: Partial<CallSettings>) => setProcessing({ ...p, call: { ...call, ...patch } });

  if (!state.processing.callAvailable) {
    return (
      <section aria-label="Call mix" className="flex flex-col gap-3 rounded-xl border border-groove bg-night/40 p-4">
        <h3 className="m-0 text-[14px] font-medium">Call mix</h3>
        <Banner>Sending apps into calls needs Windows 10 version 2004 or newer.</Banner>
      </section>
    );
  }

  const sent = sentApps(state);
  return (
    <section aria-label="Call mix" className="flex flex-col gap-4 rounded-xl border border-groove bg-night/40 p-4">
      <div className="flex flex-wrap items-baseline justify-between gap-2">
        <h3 className="m-0 text-[14px] font-medium">Call mix</h3>
        <span className="text-[12px] text-fog">Apps you send with the Call key on the Mixer are heard in calls</span>
      </div>
      <div className="flex flex-wrap items-center gap-6">
        <div className="min-w-[220px] flex-1">
          <div className="mb-1.5 flex items-baseline justify-between">
            <span className="text-[13px] text-fog">Apps in call</span>
            <span className="num text-[14px]">{pct(call.master)}</span>
          </div>
          <Slider value={call.master} label="Apps in call" valueText={pct} onChange={(master) => save({ master })} />
        </div>
        <div className="flex items-center gap-3">
          <Toggle checked={call.duck} onChange={(duck) => save({ duck })} label="Duck apps while I talk" />
          <span className="text-[13px] text-fog">Duck while I talk</span>
        </div>
        <Knob
          size="small"
          label="Duck by"
          value={call.duckDb}
          min={-40}
          max={0}
          step={1}
          format={signedDb}
          disabled={!call.duck}
          onChange={(duckDb) => save({ duckDb })}
        />
      </div>
      <div className="flex flex-wrap gap-2">
        {sent.length === 0 ? (
          <p className="m-0 text-[12px] text-dim">No apps sent yet.</p>
        ) : (
          sent.map((a) => (
            <button
              key={a.key}
              type="button"
              aria-label={`Stop sending ${a.name}`}
              title={`Stop sending ${a.name}`}
              onClick={() => void setAppSend(a, null)}
              className="flex items-center gap-2 rounded-full border border-accent/50 px-2.5 py-1 text-[12px] text-lilac hover:border-coral hover:text-coral"
            >
              <AppIcon app={a} size={16} />
              {a.name}
              <span className="num text-fog">{pct(a.send ?? 0)}</span>
            </button>
          ))
        )}
      </div>
    </section>
  );
}
```

- [ ] **Step 5: Host it in Processing**

In `src/views/ProcessingPanel.tsx`:
- add `import { CallMix } from "./CallMix";` and add `pct, signedDb` to the `../lib/processing` import
- delete the local `const pct = ...` and `const db = ...` lines and add `const db = signedDb;` in their place (keep `round1`, `ms`, `ratio`)
- insert `<CallMix state={state} />` immediately before `<div className="grid gap-6 border-t border-groove pt-6 md:grid-cols-2">`.

In `src/views/ProcessingPanel.test.tsx` and `src/views/MicView.test.tsx`, add `getAppIcon: vi.fn().mockResolvedValue(null),` to the `vi.mock("../lib/api", ...)` objects.

- [ ] **Step 6: Run everything on the frontend**

Run: `npm.cmd test`
Then: `npm.cmd run build`
Expected: all PASS (5 strip tests, 4 Call mix tests, 3 helper tests new); build succeeds.

- [ ] **Step 7: Look at it**

With the installed Resonance quit, `npm.cmd run tauri dev`: turn Processing on; on the Mixer each app strip shows a Call key (Spotify: send it, the knob appears); on the Microphone page the Call mix module lists Spotify. Play music: the After meter moves and drops while you talk (ducking). Fix layout problems (strip height, knob fit in the 124 px strip) before committing. Put every setting back afterwards.

- [ ] **Step 8: Commit**

```bash
git add src
git commit -m "feat(ui): Call keys on mixer strips and the Call mix module"
```

---

### Task 8: Verification, checklist and installer

**Files:**
- Modify: `docs/testing/phase1-manual-checklist.md`, `README.md`

- [ ] **Step 1: Full automated run**

- `cargo clippy --manifest-path M:\Resonance\src-tauri\Cargo.toml --all-targets -- -D warnings`
- `cargo test --manifest-path M:\Resonance\src-tauri\Cargo.toml`
- `npm.cmd test`
- `npm.cmd run build`

Expected: no warnings; everything PASSES (hardware tests ignored).

- [ ] **Step 2: Hardware tests** (installed Resonance quit)

`cargo test --manifest-path M:\Resonance\src-tauri\Cargo.toml --test engine_hw -- --ignored --test-threads=1 --nocapture`
Expected: 4 PASS. Afterwards confirm the Windows default mic is still the Behringer.

- [ ] **Step 3: Checklist**

Append to `docs/testing/phase1-manual-checklist.md`:

```markdown
## Phase 3b — Call mix (needs Processing on)

- [ ] Mixer: every app (not System sounds) has a Call key; with Processing off it's dimmed and explains why.
- [ ] Send Spotify: the key lights, a Send knob appears; in a Discord voice test the other side hears the music and you.
- [ ] You still hear Spotify on your own speakers/headphones, unchanged.
- [ ] Send level and "Apps in call" both change how loud the music is in the call.
- [ ] Ducking: the music drops while you talk and comes back about half a second after you stop; "Duck by" changes how much; turning ducking off keeps it steady.
- [ ] Chrome with several tabs playing: heard once, not doubled.
- [ ] Close Spotify while sent, reopen it and play: it's heard in the call again without touching anything.
- [ ] Sending Discord shows the echo warning.
- [ ] Turn Processing off and on: sends come back. Quit and relaunch: sends are remembered.
- [ ] Loud music never clips in a recording of CABLE Output.
```

- [ ] **Step 4: README**

In `README.md`, extend the processed-microphone bullet with: "and a call mix: send any app into your calls with its own level, with automatic ducking while you talk".

- [ ] **Step 5: Installer**

Run: `npm.cmd run tauri build`
Expected: `src-tauri\target\release\bundle\nsis\Resonance_0.1.0_x64-setup.exe` rebuilt.

- [ ] **Step 6: Commit and hand off**

```bash
git add docs/testing/phase1-manual-checklist.md README.md
git commit -m "docs: Phase 3b checklist and README"
```

Tell the user what was built and verified, what to try (the checklist), and offer to merge to `main`.
