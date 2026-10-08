# Resonance Phase 3a — Processed Virtual Mic Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Clean up the user's microphone in real time (AI noise suppression → gate → compressor → limiter), feed it to every app through VB-Cable as the Windows default mic, with optional monitoring and console-style controls on the Microphone page.

**Architecture:** Pure, allocation-free DSP (`engine::dsp`) runs on 480-sample blocks at 48 kHz. A WASAPI engine (`win::engine::WasapiEngine`) runs it on an MMCSS "Pro Audio" thread: event-driven capture from the source mic, push-mode render to "CABLE Input" (and optionally the default output), with a rubato resampler per output steered by a PI controller to follow clock drift. The audio worker's `AppCore` owns the engine behind an `AudioEngine` trait and reconciles it on every refresh with the settings, VB-Cable's presence and the device list; it also switches the Windows default mics to "CABLE Output" and restores them. The UI gets a `processing` block in `UiState`, sends one `setProcessing` command with the whole settings object, and shows before/after meters through the existing meter frame.

**Tech Stack:** Rust 1.99, windows 0.62 (WASAPI, MMCSS), `nnnoiseless` 0.5.2, `rubato` 5.0.1; React 19 + TypeScript, Tailwind 4, Zustand, Vitest (as in Phases 1–2).

## Global Constraints

- Spec: `docs/superpowers/specs/2026-10-08-phase3a-processed-mic-design.md`; visual language: Design v2 (parent spec §12, mockup https://claude.ai/artifact/EVtcBLh5RfMA4vaJePa8re).
- Dependencies: `nnnoiseless = { version = "0.5.2", default-features = false }`, `rubato = "5.0.1"`; add windows feature `Win32_Security` (needed by `CreateEventW`).
- Internal format: 48 kHz mono `f32`, blocks of `FRAME = 480` samples (10 ms, RNNoise's frame size). RNNoise expects 16-bit magnitudes (scale by 32768) and returns each frame exactly one frame late (measured: impulse lag 480 samples).
- Devices are opened shared-mode with `AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY`: capture as 48 kHz mono float (plus `EVENTCALLBACK`), renders as 48 kHz stereo float (push mode, no event).
- Output queue target `TARGET_QUEUE = 960` frames (20 ms); renders are prefilled with 960 frames of silence. Drift controller: `KP = 2e-6`, `KI = 2e-8`, smoothing `0.02`, correction clamp ±0.5 %. (Verified in simulation: ±0.3 % skew holds within ±4 ms, no underruns.)
- VB-Cable endpoints are recognised by Windows name prefix: render `"CABLE Input"`, capture `"CABLE Output"` (case-sensitive; Voicemeeter and "Hi-Fi Cable" don't match). Download page: `https://vb-audio.com/Cable/`.
- Defaults (spec §6): suppression 100 %; gate −45 dBFS, 2/150/120 ms, range −60 dB; compressor amount 50 % (amount a → threshold −10−25a dBFS, ratio 1.5+4.5a), 5/80 ms, makeup auto (= half the gain reduction at −18 dBFS); limiter −1 dBFS, 50 ms; monitoring off at 70 %.
- Ranges (clamped in the backend): strength 0–1; gate threshold −80…−10, attack 0.5–50, hold 0–1000, release 10–1000 ms, range −80…0 dB; compressor amount 0–1, threshold −60…0, ratio 1–20, attack 0.1–100, release 10–1000 ms, makeup −12…+24 dB; limiter ceiling −12…0 dB, release 5–500 ms; monitor level 0–1.
- After an engine failure, retry every `RETRY_REFRESHES = 8` refreshes (2 s).
- Shell: PowerShell with `npm.cmd`; cargo via `--manifest-path M:\Resonance\src-tauri\Cargo.toml`. Never run `cargo test --release` on the default target dir (it breaks the installed-style exe); hardware tests run in debug.
- The user's installed Resonance must be quit before `tauri dev` or the hardware tests (single instance; it may own the default mics).
- Commits end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

---

## File Structure

```
src-tauri/
├─ Cargo.toml                          + nnnoiseless, rubato, Win32_Security
├─ src/lib.rs                          + pub mod engine
├─ src/engine/
│  ├─ mod.rs                           AudioEngine trait, Route, EngineParams, NullEngine, re-exports
│  ├─ config.rs                        ProcessingSettings, DspParams + stage params, RestoreDefaults, EngineStatus, ProcessingView
│  ├─ dsp/mod.rs                       FRAME, SAMPLE_RATE, dB helpers, test signals
│  ├─ dsp/gate.rs                      Gate
│  ├─ dsp/compressor.rs                Compressor
│  ├─ dsp/limiter.rs                   Limiter
│  ├─ dsp/denoise.rs                   Denoiser (nnnoiseless)
│  ├─ dsp/chain.rs                     Chain
│  ├─ fifo.rs                          Fifo
│  ├─ drift.rs                         DriftController
│  └─ feed.rs                          Feed (resampler + fifo + drift)
├─ src/model.rs                        UiState.processing, MeterFrame.procIn/procOut, Command::SetProcessing
├─ src/settings.rs                     Settings.processing, Settings.processing_restore
├─ src/app_state/mod.rs                engine field, refresh → reconcile, SetProcessing, meters, shutdown, notices
├─ src/app_state/processing.rs         find_cable, source choice, default-mic switch/restore, reconcile
├─ src/app_state/processing_tests.rs   controller tests
├─ src/fake.rs                         FakeEngine
├─ src/worker.rs                       engine param, shutdown_and_wait, notices
├─ src/win/mod.rs                      + engine, shell modules
├─ src/win/endpoints.rs                + device_by_id
├─ src/win/engine.rs                   WasapiEngine
├─ src/win/shell.rs                    open_url
├─ src/app/mod.rs                      real engine, wait for shutdown on exit, open_vb_cable command
├─ src/app/commands.rs                 open_vb_cable
├─ src/app/tray.rs                     quit waits for shutdown
└─ tests/engine_hw.rs                  hardware tests (ignored; need VB-Cable)
src/
├─ lib/types.ts                        processing types, MeterFrame, Command
├─ lib/api.ts                          openVbCable
├─ lib/store.ts                        setProcessing
├─ lib/processing.ts (+ .test.ts)      amountCurve, compressorCurve, autoMakeup, processingSources, isProcessed
├─ lib/meters.ts (+ test)              procIn/procOut sources
├─ test/fixtures.ts                    makeProcessing, makeState.processing
├─ components/Knob.tsx (+ test)        rotary control
├─ components/StageModule.tsx          stage card with Advanced drawer
├─ components/MicKey.tsx               "Processed" tag
├─ views/ProcessingPanel.tsx (+ test)  Processing console section
├─ views/MicView.tsx (+ test)          hosts the panel; subtitle when processed
└─ views/Flyout.tsx (+ test)           passes processed to MicKey
docs/testing/phase1-manual-checklist.md  + Phase 3a section
```

---

### Task 1: Dependencies and processing settings types

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/src/lib.rs`
- Create: `src-tauri/src/engine/mod.rs`
- Create: `src-tauri/src/engine/config.rs`
- Modify: `src-tauri/src/settings.rs`

**Interfaces:**
- Produces: `engine::config::{DenoiseParams, GateParams, CompressorParams, LimiterParams, DspParams, ProcessingSettings, RestoreDefaults, EngineStatus, ProcessingView}` (re-exported from `engine`); `CompressorParams::{from_amount(f32) -> (f32, f32), curve(&self) -> (f32, f32), makeup(&self) -> f32}`; `DspParams::clamped(&self) -> DspParams`; `Settings.processing: ProcessingSettings`, `Settings.processing_restore: Option<RestoreDefaults>`.

- [ ] **Step 1: Add the dependencies**

In `src-tauri/Cargo.toml`, add after `base64 = "0.22"`:

```toml
nnnoiseless = { version = "0.5.2", default-features = false }
rubato = "5.0.1"
```

and add `"Win32_Security",` to the `windows` feature list after `"Win32_Graphics_Gdi",`.

- [ ] **Step 2: Write `engine/config.rs` with its tests**

```rust
use serde::{Deserialize, Serialize};

/// `v` clamped to `lo..=hi`, or `fallback` if it isn't a number.
fn fit(v: f32, lo: f32, hi: f32, fallback: f32) -> f32 {
    if v.is_finite() {
        v.clamp(lo, hi)
    } else {
        fallback
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DenoiseParams {
    pub enabled: bool,
    /// 0 = original signal, 1 = fully denoised.
    pub strength: f32,
}

impl Default for DenoiseParams {
    fn default() -> Self {
        Self { enabled: true, strength: 1.0 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GateParams {
    pub enabled: bool,
    pub threshold_db: f32,
    pub attack_ms: f32,
    pub hold_ms: f32,
    pub release_ms: f32,
    /// Attenuation while the gate is closed.
    pub range_db: f32,
}

impl Default for GateParams {
    fn default() -> Self {
        Self { enabled: true, threshold_db: -45.0, attack_ms: 2.0, hold_ms: 150.0, release_ms: 120.0, range_db: -60.0 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CompressorParams {
    pub enabled: bool,
    /// The main control (0..1); sets threshold and ratio unless `custom`.
    pub amount: f32,
    /// True once threshold or ratio were edited directly.
    pub custom: bool,
    pub threshold_db: f32,
    pub ratio: f32,
    pub attack_ms: f32,
    pub release_ms: f32,
    /// `None` = automatic makeup gain.
    pub makeup_db: Option<f32>,
}

impl Default for CompressorParams {
    fn default() -> Self {
        Self {
            enabled: true,
            amount: 0.5,
            custom: false,
            threshold_db: -22.5,
            ratio: 3.75,
            attack_ms: 5.0,
            release_ms: 80.0,
            makeup_db: None,
        }
    }
}

impl CompressorParams {
    /// Threshold (dBFS) and ratio for an Amount: 0 → −10 dBFS at 1.5:1, 1 → −35 dBFS at 6:1.
    pub fn from_amount(amount: f32) -> (f32, f32) {
        let a = amount.clamp(0.0, 1.0);
        (-10.0 - 25.0 * a, 1.5 + 4.5 * a)
    }

    /// The threshold and ratio in effect.
    pub fn curve(&self) -> (f32, f32) {
        if self.custom {
            (self.threshold_db, self.ratio)
        } else {
            Self::from_amount(self.amount)
        }
    }

    /// Makeup gain in dB: the explicit value, or half the gain reduction a −18 dBFS signal receives.
    pub fn makeup(&self) -> f32 {
        self.makeup_db.unwrap_or_else(|| {
            let (threshold, ratio) = self.curve();
            (-18.0 - threshold).max(0.0) * (1.0 - 1.0 / ratio) / 2.0
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LimiterParams {
    pub enabled: bool,
    pub ceiling_db: f32,
    pub release_ms: f32,
}

impl Default for LimiterParams {
    fn default() -> Self {
        Self { enabled: true, ceiling_db: -1.0, release_ms: 50.0 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DspParams {
    pub denoise: DenoiseParams,
    pub gate: GateParams,
    pub compressor: CompressorParams,
    pub limiter: LimiterParams,
}

impl DspParams {
    /// Every parameter forced into its allowed range (spec §6); non-numbers fall back to defaults.
    pub fn clamped(&self) -> Self {
        let d = DspParams::default();
        let mut p = *self;
        p.denoise.strength = fit(p.denoise.strength, 0.0, 1.0, d.denoise.strength);
        let g = &mut p.gate;
        g.threshold_db = fit(g.threshold_db, -80.0, -10.0, d.gate.threshold_db);
        g.attack_ms = fit(g.attack_ms, 0.5, 50.0, d.gate.attack_ms);
        g.hold_ms = fit(g.hold_ms, 0.0, 1000.0, d.gate.hold_ms);
        g.release_ms = fit(g.release_ms, 10.0, 1000.0, d.gate.release_ms);
        g.range_db = fit(g.range_db, -80.0, 0.0, d.gate.range_db);
        let c = &mut p.compressor;
        c.amount = fit(c.amount, 0.0, 1.0, d.compressor.amount);
        c.threshold_db = fit(c.threshold_db, -60.0, 0.0, d.compressor.threshold_db);
        c.ratio = fit(c.ratio, 1.0, 20.0, d.compressor.ratio);
        c.attack_ms = fit(c.attack_ms, 0.1, 100.0, d.compressor.attack_ms);
        c.release_ms = fit(c.release_ms, 10.0, 1000.0, d.compressor.release_ms);
        c.makeup_db = c.makeup_db.map(|m| fit(m, -12.0, 24.0, 0.0));
        let l = &mut p.limiter;
        l.ceiling_db = fit(l.ceiling_db, -12.0, 0.0, d.limiter.ceiling_db);
        l.release_ms = fit(l.release_ms, 5.0, 500.0, d.limiter.release_ms);
        p
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ProcessingSettings {
    pub enabled: bool,
    /// Capture device to process; `None` = the mic that was the Windows default before processing.
    pub source_id: Option<String>,
    pub dsp: DspParams,
    /// Play the processed mic on the default output.
    pub monitor: bool,
    pub monitor_level: f32,
}

impl Default for ProcessingSettings {
    fn default() -> Self {
        Self { enabled: false, source_id: None, dsp: DspParams::default(), monitor: false, monitor_level: 0.7 }
    }
}

/// The Windows default mics to put back when processing is turned off.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RestoreDefaults {
    pub console: Option<String>,
    pub comms: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize)]
#[serde(tag = "state", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum EngineStatus {
    #[default]
    Off,
    NoCable,
    Running,
    Paused { reason: String },
}

#[derive(Debug, Clone, PartialEq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessingView {
    pub settings: ProcessingSettings,
    pub status: EngineStatus,
    pub cable_installed: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_spec() {
        let d = DspParams::default();
        assert!(d.denoise.enabled && d.gate.enabled && d.compressor.enabled && d.limiter.enabled);
        assert_eq!(d.denoise.strength, 1.0);
        let g = d.gate;
        assert_eq!((g.threshold_db, g.attack_ms, g.hold_ms, g.release_ms, g.range_db), (-45.0, 2.0, 150.0, 120.0, -60.0));
        assert_eq!(d.compressor.curve(), (-22.5, 3.75));
        assert_eq!((d.limiter.ceiling_db, d.limiter.release_ms), (-1.0, 50.0));
        let s = ProcessingSettings::default();
        assert!(!s.enabled && !s.monitor);
        assert_eq!(s.monitor_level, 0.7);
    }

    #[test]
    fn amount_sets_threshold_and_ratio_until_custom() {
        assert_eq!(CompressorParams::from_amount(0.0), (-10.0, 1.5));
        assert_eq!(CompressorParams::from_amount(1.0), (-35.0, 6.0));
        let mut c = CompressorParams { custom: true, threshold_db: -30.0, ratio: 8.0, ..Default::default() };
        assert_eq!(c.curve(), (-30.0, 8.0));
        c.custom = false;
        assert_eq!(c.curve(), (-22.5, 3.75));
    }

    #[test]
    fn auto_makeup_is_half_the_reduction_at_minus_18() {
        // 12 dB over a −30 threshold at 4:1 reduces by 9 dB; half of that is 4.5.
        let c = CompressorParams { custom: true, threshold_db: -30.0, ratio: 4.0, ..Default::default() };
        assert!((c.makeup() - 4.5).abs() < 1e-5);
        let c = CompressorParams { custom: true, threshold_db: -10.0, ratio: 4.0, ..Default::default() };
        assert_eq!(c.makeup(), 0.0);
        let c = CompressorParams { makeup_db: Some(3.0), ..Default::default() };
        assert_eq!(c.makeup(), 3.0);
    }

    #[test]
    fn clamped_forces_ranges_and_replaces_non_numbers() {
        let mut p = DspParams::default();
        p.denoise.strength = -1.0;
        p.gate.threshold_db = -200.0;
        p.gate.hold_ms = f32::NAN;
        p.compressor.ratio = 50.0;
        p.compressor.makeup_db = Some(99.0);
        p.limiter.ceiling_db = 3.0;
        let c = p.clamped();
        assert_eq!(c.denoise.strength, 0.0);
        assert_eq!(c.gate.threshold_db, -80.0);
        assert_eq!(c.gate.hold_ms, 150.0);
        assert_eq!(c.compressor.ratio, 20.0);
        assert_eq!(c.compressor.makeup_db, Some(24.0));
        assert_eq!(c.limiter.ceiling_db, 0.0);
    }

    #[test]
    fn serializes_camel_case_with_a_tagged_status() {
        let view = ProcessingView {
            status: EngineStatus::Paused { reason: "gone".into() },
            cable_installed: true,
            ..Default::default()
        };
        let v = serde_json::to_value(&view).unwrap();
        assert_eq!(v["status"]["state"], "paused");
        assert_eq!(v["status"]["reason"], "gone");
        assert_eq!(v["cableInstalled"], true);
        assert!(v["settings"]["sourceId"].is_null());
        assert_eq!(v["settings"]["dsp"]["gate"]["thresholdDb"], -45.0);
        assert!(v["settings"]["dsp"]["compressor"]["makeupDb"].is_null());
        assert_eq!(serde_json::to_value(EngineStatus::NoCable).unwrap()["state"], "noCable");
    }

    #[test]
    fn partial_json_fills_in_defaults() {
        let p: ProcessingSettings =
            serde_json::from_str(r#"{"enabled":true,"dsp":{"gate":{"thresholdDb":-30}}}"#).unwrap();
        assert!(p.enabled);
        assert_eq!(p.dsp.gate.threshold_db, -30.0);
        assert_eq!(p.dsp.gate.attack_ms, 2.0);
        assert_eq!(p.dsp.denoise, DenoiseParams::default());
    }
}
```

- [ ] **Step 3: Write `engine/mod.rs` and register the module**

`src-tauri/src/engine/mod.rs`:

```rust
//! Phase 3a processed microphone: settings, DSP, clock-drift handling and the engine interface.
pub mod config;

pub use config::{
    CompressorParams, DenoiseParams, DspParams, EngineStatus, GateParams, LimiterParams, ProcessingSettings,
    ProcessingView, RestoreDefaults,
};
```

In `src-tauri/src/lib.rs`, add `pub mod engine;` after `pub mod backend;`.

- [ ] **Step 4: Persist the settings**

In `src-tauri/src/settings.rs`:
- change the import to `use crate::{engine::{ProcessingSettings, RestoreDefaults}, model::{MicConfig, UiSettings}};`
- add two fields to `Settings` after `pub ui: UiSettings,`:

```rust
    pub processing: ProcessingSettings,
    /// Default mics to restore; present while processing has them pointed at VB-Cable.
    pub processing_restore: Option<RestoreDefaults>,
```

- in `impl Default for Settings`, add `processing: ProcessingSettings::default(),` and `processing_restore: None,` after `ui: UiSettings::default(),`
- in the `round_trips` test, before `s.save(&path).unwrap();` add:

```rust
        s.processing.enabled = true;
        s.processing.dsp.gate.threshold_db = -50.0;
        s.processing_restore = Some(RestoreDefaults { console: Some("mic1".into()), comms: None });
```

and after `assert!(text.contains("\"startMinimized\""));` add `assert!(text.contains("\"processingRestore\""));`.

- [ ] **Step 5: Run the tests**

Run: `cargo test --manifest-path M:\Resonance\src-tauri\Cargo.toml --lib engine::config settings`
Expected: 6 config tests and 4 settings tests PASS (first build downloads nnnoiseless and rubato).

- [ ] **Step 6: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/lib.rs src-tauri/src/engine src-tauri/src/settings.rs
git commit -m "feat(engine): processing settings types and persistence"
```

---

### Task 2: Gate, compressor and limiter

**Files:**
- Create: `src-tauri/src/engine/dsp/mod.rs`
- Create: `src-tauri/src/engine/dsp/gate.rs`
- Create: `src-tauri/src/engine/dsp/compressor.rs`
- Create: `src-tauri/src/engine/dsp/limiter.rs`
- Modify: `src-tauri/src/engine/mod.rs`

**Interfaces:**
- Consumes: `GateParams`, `CompressorParams` (`curve()`, `makeup()`), `LimiterParams` from Task 1.
- Produces: `dsp::{SAMPLE_RATE: f32, FRAME: usize = 480, db_to_lin, lin_to_db, coeff, flush, peak}`; `Gate::{new(&GateParams), set(&GateParams), process(&mut [f32])}`; `Compressor::{new, set, process, reduction_for(f32) -> f32}`; `Limiter::{new, set, process}`, `limiter::LOOKAHEAD = 72`; test helpers `dsp::test_signals::{sine, rms, at, fan_noise}`.

- [ ] **Step 1: Write `dsp/mod.rs`**

```rust
//! Allocation-free processing of 48 kHz mono f32 audio.
pub mod compressor;
pub mod gate;
pub mod limiter;

pub const SAMPLE_RATE: f32 = 48_000.0;
/// Samples per processing block: 10 ms, RNNoise's frame size.
pub const FRAME: usize = 480;

pub fn db_to_lin(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

pub fn lin_to_db(x: f32) -> f32 {
    20.0 * x.max(1e-9).log10()
}

/// Per-sample coefficient of a one-pole smoother with time constant `ms` (0 = instant).
pub fn coeff(ms: f32) -> f32 {
    if ms <= 0.0 {
        0.0
    } else {
        (-1.0 / (ms * 0.001 * SAMPLE_RATE)).exp()
    }
}

/// Values this small become zero, so long silences can't leave slow denormal numbers behind.
pub fn flush(x: f32) -> f32 {
    if x.abs() < 1e-15 {
        0.0
    } else {
        x
    }
}

pub fn peak(buf: &[f32]) -> f32 {
    buf.iter().fold(0.0, |m, x| m.max(x.abs()))
}

#[cfg(test)]
pub mod test_signals {
    use super::{db_to_lin, SAMPLE_RATE};

    /// Sine at `freq` Hz with a peak of `db` dBFS.
    pub fn sine(freq: f32, db: f32, seconds: f32) -> Vec<f32> {
        let a = db_to_lin(db);
        (0..at(seconds)).map(|i| a * (std::f32::consts::TAU * freq * i as f32 / SAMPLE_RATE).sin()).collect()
    }

    pub fn rms(buf: &[f32]) -> f32 {
        (buf.iter().map(|x| x * x).sum::<f32>() / buf.len().max(1) as f32).sqrt()
    }

    /// Index of the sample `seconds` into a signal.
    pub fn at(seconds: f32) -> usize {
        (seconds * SAMPLE_RATE) as usize
    }

    /// Low-passed noise, like a fan or air conditioner, at `db` dBFS RMS.
    pub fn fan_noise(db: f32, seconds: f32) -> Vec<f32> {
        let mut seed = 0x1234_5678u32;
        let mut lp = 0.0f32;
        let raw: Vec<f32> = (0..at(seconds))
            .map(|_| {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let white = (seed >> 8) as f32 / 16_777_216.0 - 0.5;
                lp += 0.1 * (white - lp);
                lp
            })
            .collect();
        let gain = db_to_lin(db) / rms(&raw);
        raw.into_iter().map(|x| x * gain).collect()
    }
}
```

Add `pub mod dsp;` to `src-tauri/src/engine/mod.rs` after `pub mod config;`.

- [ ] **Step 2: Write `dsp/gate.rs` with its tests**

```rust
use super::{coeff, db_to_lin, flush, SAMPLE_RATE};
use crate::engine::config::GateParams;

/// The level detector falls this fast between peaks.
const DETECT_RELEASE_MS: f32 = 20.0;
/// The gate closes this far below the threshold, so levels hovering at it don't chatter.
const HYSTERESIS_DB: f32 = 6.0;

/// Noise gate: full level above the threshold; below it (after the hold time) attenuated by the range,
/// with smooth attack and release.
pub struct Gate {
    open_at: f32,
    close_at: f32,
    floor: f32,
    attack: f32,
    release: f32,
    hold: u32,
    detect_release: f32,
    detector: f32,
    gain: f32,
    hold_left: u32,
    open: bool,
}

impl Gate {
    pub fn new(p: &GateParams) -> Self {
        let mut g = Gate {
            open_at: 0.0,
            close_at: 0.0,
            floor: 0.0,
            attack: 0.0,
            release: 0.0,
            hold: 0,
            detect_release: coeff(DETECT_RELEASE_MS),
            detector: 0.0,
            gain: 0.0,
            hold_left: 0,
            open: false,
        };
        g.set(p);
        g.gain = g.floor;
        g
    }

    pub fn set(&mut self, p: &GateParams) {
        self.open_at = db_to_lin(p.threshold_db);
        self.close_at = db_to_lin(p.threshold_db - HYSTERESIS_DB);
        self.floor = db_to_lin(p.range_db);
        self.attack = coeff(p.attack_ms);
        self.release = coeff(p.release_ms);
        self.hold = (p.hold_ms * 0.001 * SAMPLE_RATE) as u32;
    }

    pub fn process(&mut self, buf: &mut [f32]) {
        for x in buf {
            let level = x.abs();
            self.detector = if level > self.detector { level } else { flush(self.detector * self.detect_release) };
            if self.detector >= self.open_at {
                self.open = true;
                self.hold_left = self.hold;
            } else if self.detector < self.close_at {
                if self.hold_left > 0 {
                    self.hold_left -= 1;
                } else {
                    self.open = false;
                }
            }
            let (target, c) = if self.open { (1.0, self.attack) } else { (self.floor, self.release) };
            self.gain = target + flush(c * (self.gain - target));
            *x *= self.gain;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::dsp::test_signals::{at, rms, sine};

    fn run(p: &GateParams, input: &[f32]) -> Vec<f32> {
        let mut g = Gate::new(p);
        let mut out = input.to_vec();
        for block in out.chunks_mut(480) {
            g.process(block);
        }
        out
    }

    fn ratio(input: &[f32], out: &[f32], from: f32, to: f32) -> f32 {
        let (a, b) = (at(from), at(to));
        rms(&out[a..b]) / rms(&input[a..b])
    }

    #[test]
    fn quiet_signal_is_attenuated_by_the_range() {
        let input = sine(220.0, -60.0, 1.0);
        let out = run(&GateParams::default(), &input);
        let r = ratio(&input, &out, 0.5, 1.0);
        let expected = db_to_lin(-60.0);
        assert!((r - expected).abs() < expected * 0.1, "ratio {r}");
    }

    #[test]
    fn range_sets_how_far_a_closed_gate_attenuates() {
        let input = sine(220.0, -60.0, 1.0);
        let out = run(&GateParams { range_db: -20.0, ..Default::default() }, &input);
        let r = ratio(&input, &out, 0.5, 1.0);
        assert!((r - 0.1).abs() < 0.005, "ratio {r}");
    }

    #[test]
    fn loud_signal_passes_unchanged() {
        let input = sine(220.0, -20.0, 1.0);
        let out = run(&GateParams::default(), &input);
        assert!((ratio(&input, &out, 0.1, 1.0) - 1.0).abs() < 0.01);
    }

    #[test]
    fn holds_open_then_releases() {
        let mut input = sine(220.0, -20.0, 0.5);
        input.extend(sine(220.0, -60.0, 1.0));
        let out = run(&GateParams::default(), &input);
        assert!(ratio(&input, &out, 0.58, 0.60) > 0.95, "should still be open during the hold");
        assert!(ratio(&input, &out, 1.45, 1.5) < 0.01, "should have closed");
    }

    #[test]
    fn silence_leaves_no_denormals() {
        let mut g = Gate::new(&GateParams::default());
        let mut loud = sine(220.0, -10.0, 0.1);
        g.process(&mut loud);
        let mut buf = vec![0.0f32; 480];
        for _ in 0..1000 {
            buf.fill(0.0);
            g.process(&mut buf);
        }
        assert!(buf.iter().all(|x| *x == 0.0));
        assert!(g.detector == 0.0 || g.detector.is_normal());
        assert!(g.gain.is_normal());
    }
}
```

- [ ] **Step 3: Write `dsp/compressor.rs` with its tests**

```rust
use super::{coeff, db_to_lin, flush};
use crate::engine::config::CompressorParams;

/// RMS detector time constant.
const RMS_MS: f32 = 10.0;
/// Soft-knee width around the threshold.
const KNEE_DB: f32 = 6.0;

/// Feed-forward RMS compressor with a soft knee and makeup gain.
pub struct Compressor {
    threshold: f32,
    slope: f32,
    makeup: f32,
    attack: f32,
    release: f32,
    rms: f32,
    mean_square: f32,
    reduction: f32,
}

impl Compressor {
    pub fn new(p: &CompressorParams) -> Self {
        let mut c = Compressor {
            threshold: 0.0,
            slope: 0.0,
            makeup: 0.0,
            attack: 0.0,
            release: 0.0,
            rms: coeff(RMS_MS),
            mean_square: 0.0,
            reduction: 0.0,
        };
        c.set(p);
        c
    }

    pub fn set(&mut self, p: &CompressorParams) {
        let (threshold, ratio) = p.curve();
        self.threshold = threshold;
        self.slope = 1.0 - 1.0 / ratio.max(1.0);
        self.makeup = p.makeup();
        self.attack = coeff(p.attack_ms);
        self.release = coeff(p.release_ms);
    }

    /// Static gain reduction (dB) for a detector level (dBFS).
    pub fn reduction_for(&self, level_db: f32) -> f32 {
        let over = level_db - self.threshold;
        if over <= -KNEE_DB / 2.0 {
            0.0
        } else if over < KNEE_DB / 2.0 {
            self.slope * (over + KNEE_DB / 2.0).powi(2) / (2.0 * KNEE_DB)
        } else {
            self.slope * over
        }
    }

    pub fn process(&mut self, buf: &mut [f32]) {
        for x in buf {
            let sq = *x * *x;
            self.mean_square = sq + flush(self.rms * (self.mean_square - sq));
            let level_db = 10.0 * self.mean_square.max(1e-12).log10();
            let target = self.reduction_for(level_db);
            let c = if target > self.reduction { self.attack } else { self.release };
            self.reduction = target + flush(c * (self.reduction - target));
            *x *= db_to_lin(self.makeup - self.reduction);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::dsp::test_signals::{at, rms, sine};

    fn custom(threshold_db: f32, ratio: f32) -> CompressorParams {
        CompressorParams { custom: true, threshold_db, ratio, makeup_db: Some(0.0), ..Default::default() }
    }

    fn run(p: &CompressorParams, input: &[f32]) -> Vec<f32> {
        let mut c = Compressor::new(p);
        let mut out = input.to_vec();
        for block in out.chunks_mut(480) {
            c.process(block);
        }
        out
    }

    fn rms_db(x: &[f32]) -> f32 {
        20.0 * rms(x).log10()
    }

    #[test]
    fn reduces_level_above_threshold_by_the_ratio() {
        // Peak −7 dBFS = RMS −10 dBFS: 20 dB over a −30 threshold, 4:1 → 15 dB of reduction.
        let input = sine(220.0, -7.0, 1.0);
        let out = run(&custom(-30.0, 4.0), &input);
        let (a, b) = (at(0.5), at(1.0));
        let level_in = rms_db(&input[a..b]);
        let expected = level_in - 0.75 * (level_in + 30.0);
        let got = rms_db(&out[a..b]);
        assert!((got - expected).abs() < 0.5, "expected {expected:.2}, got {got:.2}");
    }

    #[test]
    fn leaves_quiet_signal_alone() {
        let input = sine(220.0, -40.0, 1.0);
        let out = run(&custom(-30.0, 4.0), &input);
        let (a, b) = (at(0.5), at(1.0));
        assert!((rms_db(&out[a..b]) - rms_db(&input[a..b])).abs() < 0.05);
    }

    #[test]
    fn applies_makeup_gain() {
        let input = sine(220.0, -40.0, 1.0);
        let out = run(&CompressorParams { makeup_db: Some(6.0), ..custom(-30.0, 4.0) }, &input);
        let (a, b) = (at(0.5), at(1.0));
        assert!((rms_db(&out[a..b]) - rms_db(&input[a..b]) - 6.0).abs() < 0.05);
    }

    #[test]
    fn soft_knee_is_continuous() {
        let c = Compressor::new(&custom(-30.0, 4.0));
        for edge in [-33.0f32, -27.0] {
            assert!((c.reduction_for(edge - 1e-3) - c.reduction_for(edge + 1e-3)).abs() < 1e-2);
        }
        assert_eq!(c.reduction_for(-40.0), 0.0);
        assert!((c.reduction_for(-10.0) - 15.0).abs() < 1e-4);
    }

    #[test]
    fn silence_leaves_no_denormals() {
        let mut c = Compressor::new(&CompressorParams::default());
        let mut loud = sine(220.0, -3.0, 0.1);
        c.process(&mut loud);
        let mut buf = vec![0.0f32; 480];
        for _ in 0..1000 {
            buf.fill(0.0);
            c.process(&mut buf);
        }
        assert!(buf.iter().all(|x| *x == 0.0));
        assert!(c.mean_square == 0.0 || c.mean_square.is_normal());
        assert!(c.reduction == 0.0 || c.reduction.is_normal());
    }
}
```

- [ ] **Step 4: Write `dsp/limiter.rs` with its tests**

```rust
use super::{coeff, db_to_lin};
use crate::engine::config::LimiterParams;

/// Look-ahead: 1.5 ms at 48 kHz.
pub const LOOKAHEAD: usize = 72;
/// Gain falls with a time constant of a quarter of the look-ahead (0.375 ms).
const ATTACK_MS: f32 = 0.375;

/// Look-ahead peak limiter. Output never exceeds the ceiling: gain is lowered ahead of each peak, and
/// anything a fast transient still pushes over is clipped at the ceiling as a last resort.
pub struct Limiter {
    ceiling: f32,
    attack: f32,
    release: f32,
    delay: [f32; LOOKAHEAD],
    needed: [f32; LOOKAHEAD],
    pos: usize,
    gain: f32,
}

impl Limiter {
    pub fn new(p: &LimiterParams) -> Self {
        let mut l = Limiter {
            ceiling: 1.0,
            attack: coeff(ATTACK_MS),
            release: 0.0,
            delay: [0.0; LOOKAHEAD],
            needed: [1.0; LOOKAHEAD],
            pos: 0,
            gain: 1.0,
        };
        l.set(p);
        l
    }

    pub fn set(&mut self, p: &LimiterParams) {
        self.ceiling = db_to_lin(p.ceiling_db);
        self.release = coeff(p.release_ms);
    }

    pub fn process(&mut self, buf: &mut [f32]) {
        for x in buf {
            let input = *x;
            let level = input.abs();
            let need = if level > self.ceiling { self.ceiling / level } else { 1.0 };
            let delayed = self.delay[self.pos];
            let delayed_need = self.needed[self.pos];
            self.delay[self.pos] = input;
            self.needed[self.pos] = need;
            self.pos = (self.pos + 1) % LOOKAHEAD;
            let target = self.needed.iter().fold(delayed_need, |m, &v| m.min(v));
            let c = if target < self.gain { self.attack } else { self.release };
            self.gain = target + c * (self.gain - target);
            *x = (delayed * self.gain).clamp(-self.ceiling, self.ceiling);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::dsp::{peak, test_signals::sine};

    fn run(p: &LimiterParams, input: &[f32]) -> Vec<f32> {
        let mut l = Limiter::new(p);
        let mut out = input.to_vec();
        for block in out.chunks_mut(480) {
            l.process(block);
        }
        out
    }

    #[test]
    fn full_scale_square_never_exceeds_the_ceiling() {
        let ceiling = db_to_lin(-1.0);
        let input: Vec<f32> = (0..48_000).map(|i| if (i / 100) % 2 == 0 { 1.0 } else { -1.0 }).collect();
        let out = run(&LimiterParams::default(), &input);
        assert!(peak(&out) <= ceiling + 1e-6);
        assert!(out[1000..].iter().all(|x| x.abs() > ceiling * 0.98), "settled output sits at the ceiling");
    }

    #[test]
    fn a_single_spike_is_caught() {
        let mut input = vec![0.0f32; 4800];
        input[1000] = 1.0;
        input[1001] = -0.95;
        let out = run(&LimiterParams::default(), &input);
        assert!(peak(&out) <= db_to_lin(-1.0) + 1e-6);
    }

    #[test]
    fn audio_under_the_ceiling_passes_through_delayed() {
        let input = sine(440.0, -6.0, 0.2);
        let out = run(&LimiterParams::default(), &input);
        for i in 0..input.len() - LOOKAHEAD {
            assert!((out[i + LOOKAHEAD] - input[i]).abs() < 1e-6, "sample {i}");
        }
    }

    #[test]
    fn ceiling_follows_the_settings() {
        let input = sine(440.0, 0.0, 0.5);
        let out = run(&LimiterParams { ceiling_db: -6.0, ..Default::default() }, &input);
        assert!(peak(&out) <= db_to_lin(-6.0) + 1e-6);
    }
}
```

- [ ] **Step 5: Run the tests**

Run: `cargo test --manifest-path M:\Resonance\src-tauri\Cargo.toml --lib engine::dsp`
Expected: 14 tests PASS (gate 5, compressor 5, limiter 4).

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/engine
git commit -m "feat(engine): noise gate, compressor and look-ahead limiter"
```

---

### Task 3: Noise suppression and the processing chain

**Files:**
- Create: `src-tauri/src/engine/dsp/denoise.rs`
- Create: `src-tauri/src/engine/dsp/chain.rs`
- Modify: `src-tauri/src/engine/dsp/mod.rs`

**Interfaces:**
- Consumes: Task 2's stages and helpers; `DspParams` from Task 1.
- Produces: `Denoiser::{new(strength: f32), set_strength(f32), process(&mut [f32; FRAME])}`, `denoise::DENOISE_DELAY = FRAME`; `Chain::{new(&DspParams), set(&DspParams), process(&mut [f32; FRAME]) -> (f32, f32)}` returning (peak before, peak after).

- [ ] **Step 1: Register the modules**

In `dsp/mod.rs`, make the module list:

```rust
pub mod chain;
pub mod compressor;
pub mod denoise;
pub mod gate;
pub mod limiter;
```

- [ ] **Step 2: Write `dsp/denoise.rs` with its tests**

```rust
use nnnoiseless::DenoiseState;

use super::FRAME;

/// RNNoise returns each frame one frame late; the dry path is delayed to match.
pub const DENOISE_DELAY: usize = FRAME;
/// RNNoise works on 16-bit sample magnitudes.
const SCALE: f32 = 32_768.0;

/// AI noise suppression (RNNoise via nnnoiseless) with a wet/dry strength control.
pub struct Denoiser {
    state: Box<DenoiseState<'static>>,
    scaled: [f32; FRAME],
    wet: [f32; FRAME],
    dry: [f32; FRAME],
    strength: f32,
}

impl Denoiser {
    pub fn new(strength: f32) -> Self {
        Self { state: DenoiseState::new(), scaled: [0.0; FRAME], wet: [0.0; FRAME], dry: [0.0; FRAME], strength }
    }

    pub fn set_strength(&mut self, strength: f32) {
        self.strength = strength;
    }

    pub fn process(&mut self, frame: &mut [f32; FRAME]) {
        for (s, x) in self.scaled.iter_mut().zip(frame.iter()) {
            *s = x * SCALE;
        }
        self.state.process_frame(&mut self.wet, &self.scaled);
        for ((x, wet), dry) in frame.iter_mut().zip(self.wet.iter()).zip(self.dry.iter_mut()) {
            let wet = wet / SCALE;
            let delayed = std::mem::replace(dry, *x);
            *x = delayed + self.strength * (wet - delayed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::dsp::test_signals::{at, fan_noise, rms};
    use std::f32::consts::TAU;

    fn run(strength: f32, input: &[f32]) -> Vec<f32> {
        let mut d = Denoiser::new(strength);
        let mut out = input.to_vec();
        for f in out.chunks_exact_mut(FRAME) {
            d.process(f.try_into().unwrap());
        }
        out
    }

    #[test]
    fn removes_fan_noise() {
        let input = fan_noise(-40.0, 5.0);
        let out = run(1.0, &input);
        let (a, b) = (at(2.0), at(5.0));
        let reduction = 20.0 * (rms(&input[a..b]) / rms(&out[a..b]).max(1e-9)).log10();
        assert!(reduction > 15.0, "only {reduction:.1} dB");
    }

    #[test]
    fn keeps_a_voice_like_tone() {
        // Harmonic tone switched loud/quiet every 100 ms, like syllables.
        let input: Vec<f32> = (0..at(5.0))
            .map(|i| {
                let t = i as f32 / 48_000.0;
                let env = if (i / 4800) % 2 == 0 { 1.0 } else { 0.2 };
                env * 0.25 * ((TAU * 220.0 * t).sin() + 0.5 * (TAU * 440.0 * t).sin() + 0.3 * (TAU * 1250.0 * t).sin())
            })
            .collect();
        let out = run(1.0, &input);
        let (a, b) = (at(1.0), at(5.0));
        let kept = (rms(&out[a..b]) / rms(&input[a..b])).powi(2);
        assert!(kept > 0.8, "kept {kept:.2} of the energy");
    }

    #[test]
    fn zero_strength_is_the_input_one_frame_late() {
        let input = fan_noise(-30.0, 0.5);
        let out = run(0.0, &input);
        for i in 0..input.len() - DENOISE_DELAY {
            assert_eq!(out[i + DENOISE_DELAY], input[i], "sample {i}");
        }
    }

    #[test]
    fn wet_and_dry_paths_are_aligned() {
        let mut input = vec![0.0f32; FRAME * 20];
        for k in (4800..input.len()).step_by(960) {
            input[k] = 0.6;
        }
        let out = run(1.0, &input);
        let lag = out[4800..6000].iter().enumerate().max_by(|a, b| a.1.abs().total_cmp(&b.1.abs())).unwrap().0;
        assert_eq!(lag, DENOISE_DELAY);
    }
}
```

- [ ] **Step 3: Write `dsp/chain.rs` with its tests**

```rust
use super::{compressor::Compressor, denoise::Denoiser, gate::Gate, limiter::Limiter, peak, FRAME};
use crate::engine::config::DspParams;

/// The processing chain, in its fixed order: noise suppression → gate → compressor → limiter.
pub struct Chain {
    p: DspParams,
    denoise: Denoiser,
    gate: Gate,
    compressor: Compressor,
    limiter: Limiter,
}

impl Chain {
    pub fn new(p: &DspParams) -> Self {
        Self {
            p: *p,
            denoise: Denoiser::new(p.denoise.strength),
            gate: Gate::new(&p.gate),
            compressor: Compressor::new(&p.compressor),
            limiter: Limiter::new(&p.limiter),
        }
    }

    /// Applies new settings without resetting any stage (no clicks, no allocation).
    pub fn set(&mut self, p: &DspParams) {
        self.denoise.set_strength(p.denoise.strength);
        self.gate.set(&p.gate);
        self.compressor.set(&p.compressor);
        self.limiter.set(&p.limiter);
        self.p = *p;
    }

    /// Processes one block in place; returns its peak before and after.
    pub fn process(&mut self, frame: &mut [f32; FRAME]) -> (f32, f32) {
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
        if self.p.limiter.enabled {
            self.limiter.process(frame);
        }
        (before, peak(frame))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::dsp::{
        db_to_lin,
        test_signals::{fan_noise, sine},
    };

    fn all_off() -> DspParams {
        let mut p = DspParams::default();
        p.denoise.enabled = false;
        p.gate.enabled = false;
        p.compressor.enabled = false;
        p.limiter.enabled = false;
        p
    }

    fn run(chain: &mut Chain, input: &[f32]) -> Vec<f32> {
        let mut out = input.to_vec();
        for f in out.chunks_exact_mut(FRAME) {
            chain.process(f.try_into().unwrap());
        }
        out
    }

    #[test]
    fn bypassed_chain_is_bit_exact() {
        let input = fan_noise(-20.0, 0.5);
        assert_eq!(run(&mut Chain::new(&all_off()), &input), input);
    }

    #[test]
    fn each_stage_works_on_its_own() {
        let input = sine(220.0, -20.0, 0.5);
        for stage in 0..4 {
            let mut p = all_off();
            match stage {
                0 => p.denoise.enabled = true,
                1 => p.gate.enabled = true,
                2 => p.compressor.enabled = true,
                _ => p.limiter.enabled = true,
            }
            assert_ne!(run(&mut Chain::new(&p), &input), input, "stage {stage} had no effect");
        }
    }

    #[test]
    fn reports_peaks_before_and_after() {
        let mut p = all_off();
        p.limiter.enabled = true;
        p.limiter.ceiling_db = -6.0;
        let mut chain = Chain::new(&p);
        let mut last = (0.0, 0.0);
        for _ in 0..10 {
            let mut frame = [0.0f32; FRAME];
            for (i, x) in frame.iter_mut().enumerate() {
                *x = if i % 2 == 0 { 0.9 } else { -0.9 };
            }
            last = chain.process(&mut frame);
        }
        assert!((last.0 - 0.9).abs() < 1e-6);
        assert!(last.1 <= db_to_lin(-6.0) + 1e-6);
    }

    #[test]
    fn settings_change_live() {
        let mut p = all_off();
        p.gate.enabled = true;
        p.gate.threshold_db = -10.0;
        let mut chain = Chain::new(&p);
        let input = sine(220.0, -30.0, 0.5);
        let closed = run(&mut chain, &input);
        assert!(crate::engine::dsp::peak(&closed[12_000..]) < 0.001);
        p.gate.threshold_db = -45.0;
        chain.set(&p);
        let open = run(&mut chain, &input);
        assert!((crate::engine::dsp::peak(&open[12_000..]) - db_to_lin(-30.0)).abs() < 0.002);
    }

    #[test]
    fn full_chain_stays_under_the_ceiling() {
        let loud: Vec<f32> = sine(180.0, 0.0, 1.0).iter().zip(fan_noise(-30.0, 1.0)).map(|(s, n)| s + n).collect();
        let out = run(&mut Chain::new(&DspParams::default()), &loud);
        assert!(crate::engine::dsp::peak(&out) <= db_to_lin(-1.0) + 1e-6);
    }
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test --manifest-path M:\Resonance\src-tauri\Cargo.toml --lib engine::dsp`
Expected: 23 tests PASS (Task 2's 14, denoise 4, chain 5).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/engine/dsp
git commit -m "feat(engine): RNNoise suppression with strength blend and the processing chain"
```

---
### Task 4: Sample queue, drift controller and output feed

**Files:**
- Create: `src-tauri/src/engine/fifo.rs`
- Create: `src-tauri/src/engine/drift.rs`
- Create: `src-tauri/src/engine/feed.rs`
- Modify: `src-tauri/src/engine/mod.rs`

**Interfaces:**
- Consumes: `dsp::FRAME`.
- Produces: `Fifo::{new(capacity), len, is_empty, push_slice(&[f32]), push_zeros(usize), pop_into(&mut [f32]) -> usize, clear}`; `DriftController::{new(target_frames), update(queued: usize) -> f64}`, `drift::MAX_CORRECTION`; `Feed::{new(), push(&[f32; FRAME]), queued() -> usize, pull(&mut [f32]) -> usize, steer(device_queued: usize)}`, `feed::TARGET_QUEUE = 960`.

- [ ] **Step 1: Register the modules**

In `engine/mod.rs`, make the module list:

```rust
pub mod config;
pub mod drift;
pub mod dsp;
pub mod feed;
pub mod fifo;
```

- [ ] **Step 2: Write `fifo.rs` with its tests**

```rust
/// Fixed-capacity sample queue that never allocates after creation. When full, the oldest samples are
/// dropped to make room.
pub struct Fifo {
    buf: Vec<f32>,
    head: usize,
    len: usize,
}

impl Fifo {
    pub fn new(capacity: usize) -> Self {
        Self { buf: vec![0.0; capacity.max(1)], head: 0, len: 0 }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn clear(&mut self) {
        self.head = 0;
        self.len = 0;
    }

    fn push(&mut self, v: f32) {
        let cap = self.buf.len();
        if self.len == cap {
            self.head = (self.head + 1) % cap;
            self.len -= 1;
        }
        self.buf[(self.head + self.len) % cap] = v;
        self.len += 1;
    }

    pub fn push_slice(&mut self, data: &[f32]) {
        for &v in data {
            self.push(v);
        }
    }

    pub fn push_zeros(&mut self, n: usize) {
        for _ in 0..n {
            self.push(0.0);
        }
    }

    /// Moves up to `out.len()` of the oldest samples into `out`; returns how many.
    pub fn pop_into(&mut self, out: &mut [f32]) -> usize {
        let n = out.len().min(self.len);
        let cap = self.buf.len();
        for o in out.iter_mut().take(n) {
            *o = self.buf[self.head];
            self.head = (self.head + 1) % cap;
        }
        self.len -= n;
        n
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_order_across_wraparound() {
        let mut f = Fifo::new(4);
        let mut out = [0.0; 4];
        f.push_slice(&[1.0, 2.0, 3.0]);
        assert_eq!(f.pop_into(&mut out[..2]), 2);
        assert_eq!(&out[..2], &[1.0, 2.0]);
        f.push_slice(&[4.0, 5.0, 6.0]);
        assert_eq!(f.len(), 4);
        assert_eq!(f.pop_into(&mut out), 4);
        assert_eq!(out, [3.0, 4.0, 5.0, 6.0]);
        assert!(f.is_empty());
    }

    #[test]
    fn overflow_drops_the_oldest() {
        let mut f = Fifo::new(3);
        f.push_slice(&[1.0, 2.0, 3.0, 4.0, 5.0]);
        let mut out = [0.0; 3];
        assert_eq!(f.pop_into(&mut out), 3);
        assert_eq!(out, [3.0, 4.0, 5.0]);
    }

    #[test]
    fn pops_only_what_it_has() {
        let mut f = Fifo::new(8);
        f.push_zeros(2);
        f.push_slice(&[7.0]);
        let mut out = [9.0; 5];
        assert_eq!(f.pop_into(&mut out), 3);
        assert_eq!(out, [0.0, 0.0, 7.0, 9.0, 9.0]);
        f.push_slice(&[1.0]);
        f.clear();
        assert_eq!(f.pop_into(&mut out), 0);
    }
}
```

- [ ] **Step 3: Write `drift.rs` with its tests**

```rust
/// Largest speed correction applied to an output (±0.5 %, far beyond real clock drift).
pub const MAX_CORRECTION: f64 = 0.005;
const KP: f64 = 2e-6;
const KI: f64 = 2e-8;
/// Smoothing of the measured queue level per update.
const SMOOTHING: f64 = 0.02;

/// Keeps an output queue near its target level when the input and output devices run on slightly
/// different clocks, by steering the resampling ratio (output frames per input frame).
pub struct DriftController {
    target: f64,
    level: Option<f64>,
    integral: f64,
}

impl DriftController {
    pub fn new(target_frames: usize) -> Self {
        Self { target: target_frames as f64, level: None, integral: 0.0 }
    }

    /// Takes the frames currently queued for the device; returns the ratio to use next.
    pub fn update(&mut self, queued: usize) -> f64 {
        let q = queued as f64;
        let level = match self.level {
            Some(l) => l + SMOOTHING * (q - l),
            None => q,
        };
        self.level = Some(level);
        let error = self.target - level;
        self.integral = (self.integral + KI * error).clamp(-MAX_CORRECTION, MAX_CORRECTION);
        1.0 + (KP * error + self.integral).clamp(-MAX_CORRECTION, MAX_CORRECTION)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ten minutes of 10 ms cycles against a device whose clock runs `skew` fast.
    /// Returns the queue's lowest and highest deviation from target after the first minute, and underruns.
    fn simulate(skew: f64) -> (f64, f64, usize) {
        let mut d = DriftController::new(960);
        let (mut queued, mut ratio, mut made, mut played) = (960.0f64, 1.0f64, 0.0f64, 0.0f64);
        let (mut lo, mut hi, mut underruns) = (f64::MAX, f64::MIN, 0);
        for cycle in 0..60_000 {
            played += 480.0 * (1.0 + skew);
            let pull = played.floor();
            played -= pull;
            if queued < pull {
                underruns += 1;
                queued = 0.0;
            } else {
                queued -= pull;
            }
            made += 480.0 * ratio;
            let push = made.floor();
            made -= push;
            queued += push;
            ratio = d.update(queued as usize);
            if cycle >= 6_000 {
                lo = lo.min(queued - 960.0);
                hi = hi.max(queued - 960.0);
            }
        }
        (lo, hi, underruns)
    }

    #[test]
    fn holds_the_queue_within_5_ms_for_clock_skew_up_to_0_3_percent() {
        for skew in [-0.003, -0.001, 0.0, 0.001, 0.003] {
            let (lo, hi, underruns) = simulate(skew);
            assert_eq!(underruns, 0, "skew {skew}");
            assert!(lo > -240.0 && hi < 240.0, "skew {skew}: {lo}..{hi}");
        }
    }

    #[test]
    fn correction_is_bounded() {
        let mut d = DriftController::new(960);
        for _ in 0..10_000 {
            assert!((d.update(0) - 1.0).abs() <= MAX_CORRECTION + 1e-12);
        }
        for _ in 0..10_000 {
            assert!((d.update(100_000) - 1.0).abs() <= MAX_CORRECTION + 1e-12);
        }
    }
}
```

- [ ] **Step 4: Write `feed.rs` with its tests**

```rust
use rubato::{audioadapter_buffers::direct::InterleavedSlice, Adjustable, Async, FixedAsync, PolynomialDegree, Resampler};

use super::{drift::DriftController, dsp::FRAME, fifo::Fifo};

/// Frames kept queued for an output (its queue plus the device buffer): 20 ms rides out scheduling jitter.
pub const TARGET_QUEUE: usize = 960;
/// How far the resampling ratio may ever move from 1 (the drift controller stays within ±0.5 %).
const MAX_RATIO_RELATIVE: f64 = 1.1;

/// One output destination: resamples processed frames to follow the device's clock and queues them.
pub struct Feed {
    resampler: Async<f32>,
    fifo: Fifo,
    drift: DriftController,
    scratch: Vec<f32>,
}

impl Feed {
    pub fn new() -> Self {
        let resampler =
            Async::<f32>::new_poly(1.0, MAX_RATIO_RELATIVE, PolynomialDegree::Cubic, FRAME, 1, FixedAsync::Input)
                .expect("valid resampler settings");
        let scratch = vec![0.0; resampler.output_frames_max()];
        Self { resampler, fifo: Fifo::new(24_000), drift: DriftController::new(TARGET_QUEUE), scratch }
    }

    /// Resamples one processed block into the queue.
    pub fn push(&mut self, frame: &[f32; FRAME]) {
        let written = {
            let input = InterleavedSlice::new(&frame[..], 1, FRAME).expect("frame size");
            let len = self.scratch.len();
            let mut output = InterleavedSlice::new_mut(&mut self.scratch[..], 1, len).expect("scratch size");
            self.resampler.process_into_buffer(&input, &mut output, None).map(|(_, n)| n).unwrap_or(0)
        };
        self.fifo.push_slice(&self.scratch[..written]);
    }

    pub fn queued(&self) -> usize {
        self.fifo.len()
    }

    /// Takes up to `out.len()` queued frames.
    pub fn pull(&mut self, out: &mut [f32]) -> usize {
        self.fifo.pop_into(out)
    }

    /// Called once per cycle, after writing to the device, with the frames the device still holds.
    pub fn steer(&mut self, device_queued: usize) {
        let ratio = self.drift.update(self.fifo.len() + device_queued);
        let _ = self.resampler.set_resample_ratio(ratio, true);
    }
}

impl Default for Feed {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two minutes of 10 ms cycles against a device (prefilled with 20 ms of silence) whose clock runs
    /// `skew` fast. Returns the lowest and highest total queue after 30 s, and underruns.
    fn simulate(skew: f64) -> (usize, usize, usize) {
        let mut feed = Feed::new();
        let frame = [0.1f32; FRAME];
        let mut out = vec![0.0f32; 4800];
        let (mut device, mut played) = (TARGET_QUEUE as f64, 0.0f64);
        let (mut lo, mut hi, mut underruns) = (usize::MAX, 0, 0);
        for cycle in 0..12_000 {
            played += 480.0 * (1.0 + skew);
            let pull = played.floor();
            played -= pull;
            if device < pull {
                underruns += 1;
                device = 0.0;
            } else {
                device -= pull;
            }
            feed.push(&frame);
            device += feed.pull(&mut out) as f64;
            feed.steer(device as usize);
            if cycle >= 3_000 {
                let total = device as usize + feed.queued();
                lo = lo.min(total);
                hi = hi.max(total);
            }
        }
        (lo, hi, underruns)
    }

    #[test]
    fn follows_a_drifting_device_clock_without_underruns() {
        for skew in [-0.003, 0.0, 0.003] {
            let (lo, hi, underruns) = simulate(skew);
            assert_eq!(underruns, 0, "skew {skew}");
            assert!(lo > TARGET_QUEUE - 240 && hi < TARGET_QUEUE + 240, "skew {skew}: {lo}..{hi}");
        }
    }

    #[test]
    fn passes_audio_through_at_unity_ratio() {
        let mut feed = Feed::new();
        let frame = [0.25f32; FRAME];
        for _ in 0..10 {
            feed.push(&frame);
        }
        let mut out = vec![0.0f32; 4800];
        let n = feed.pull(&mut out);
        assert!((4790..=4800).contains(&n), "{n} frames");
        assert!(out[1000..n].iter().all(|x| (x - 0.25).abs() < 1e-4));
    }
}
```

- [ ] **Step 5: Run the tests**

Run: `cargo test --manifest-path M:\Resonance\src-tauri\Cargo.toml --lib engine::fifo engine::drift engine::feed`
Expected: 7 tests PASS (fifo 3, drift 2, feed 2). The feed simulation takes a few seconds in debug.

If `passes_audio_through_at_unity_ratio` reports a frame count outside 4790..=4800, print `n` and widen the range to the resampler's actual start-up delay (the cubic resampler holds back a few frames); the value check on `out[1000..n]` must still pass.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/engine
git commit -m "feat(engine): fixed-capacity queue, clock-drift controller and resampling output feed"
```

---

### Task 5: The engine interface and the processing controller

**Files:**
- Modify: `src-tauri/src/engine/mod.rs`
- Modify: `src-tauri/src/model.rs`
- Modify: `src-tauri/src/fake.rs`
- Create: `src-tauri/src/app_state/processing.rs`
- Create: `src-tauri/src/app_state/processing_tests.rs`
- Modify: `src-tauri/src/app_state/mod.rs`
- Modify: `src-tauri/src/worker.rs`

**Interfaces:**
- Consumes: Task 1's settings types.
- Produces:
  - `engine::{AudioEngine (trait: start(&mut self, &Route, &EngineParams) -> Result<()>, stop(&mut self), set_params(&mut self, &EngineParams), failure(&self) -> Option<String>, take_peaks(&self) -> (f32, f32)), Route { source_id, cable_id, monitor_id: Option<String> }, EngineParams { dsp: DspParams, monitor_level: f32 }, NullEngine}`.
  - `model::UiState.processing: ProcessingView`, `MeterFrame.proc_in/proc_out` (JSON `procIn`/`procOut`), `Command::SetProcessing { processing: ProcessingSettings }`.
  - `AppCore::{set_engine(Box<dyn AudioEngine>), shutdown(), take_notices() -> Vec<AudioError>}`; `app_state::processing::{find_cable, Cable, RETRY_REFRESHES}`.
  - `worker::spawn(make_backend, engine: Box<dyn AudioEngine>, settings_path, sink)`; `WorkerHandle::shutdown_and_wait(Duration)`.
  - `fake::FakeEngine` (test only) with `log() -> MutexGuard<EngineLog>`.

- [ ] **Step 1: Add the engine interface**

Append to `engine/mod.rs`:

```rust
use crate::error::{AudioError, Result};

/// Where the engine reads, writes and (optionally) monitors.
#[derive(Debug, Clone, PartialEq)]
pub struct Route {
    pub source_id: String,
    /// VB-Cable's render endpoint ("CABLE Input").
    pub cable_id: String,
    pub monitor_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EngineParams {
    pub dsp: DspParams,
    pub monitor_level: f32,
}

/// The real-time processing engine (WASAPI in production, a fake in tests). Owned by the audio worker.
pub trait AudioEngine: Send {
    /// Opens the devices and starts processing; returns once audio flows or the devices failed to open.
    fn start(&mut self, route: &Route, params: &EngineParams) -> Result<()>;
    fn stop(&mut self);
    /// Applies new settings to the running engine without interrupting audio.
    fn set_params(&mut self, params: &EngineParams);
    /// Why a started engine stopped by itself (device unplugged, driver error), if it did.
    fn failure(&self) -> Option<String>;
    /// Highest linear peaks before and after processing since the last call.
    fn take_peaks(&self) -> (f32, f32);
}

/// Stands in until a real engine is installed; processing reports itself unavailable.
pub struct NullEngine;

impl AudioEngine for NullEngine {
    fn start(&mut self, _: &Route, _: &EngineParams) -> Result<()> {
        Err(AudioError::Unavailable("Audio processing isn't available".into()))
    }
    fn stop(&mut self) {}
    fn set_params(&mut self, _: &EngineParams) {}
    fn failure(&self) -> Option<String> {
        None
    }
    fn take_peaks(&self) -> (f32, f32) {
        (0.0, 0.0)
    }
}
```

- [ ] **Step 2: Extend the model**

In `src-tauri/src/model.rs`:
- add `use crate::engine::{ProcessingSettings, ProcessingView};` below the serde import.
- add to `UiState` after `pub ui: UiSettings,`: `pub processing: ProcessingView,`
- add to `MeterFrame` after `pub mic: f32,`:

```rust
    /// Processed-mic peaks before and after the chain.
    pub proc_in: f32,
    pub proc_out: f32,
```

- add to `Command` after `ResetAppMemory,`: `SetProcessing { processing: ProcessingSettings },`
- in the test `ui_state_serializes_camel_case`, add `assert_eq!(v["processing"]["status"]["state"], "off");`
- in the test `command_deserializes_from_tagged_camel_case_json`, add before its closing brace:

```rust
        let c: Command =
            serde_json::from_str(r#"{"type":"setProcessing","processing":{"enabled":true,"monitorLevel":0.5}}"#).unwrap();
        let Command::SetProcessing { processing } = c else { panic!("wrong command") };
        assert!(processing.enabled);
        assert_eq!(processing.monitor_level, 0.5);
```

- [ ] **Step 3: Add `FakeEngine` to `fake.rs`**

Change the imports at the top of `src-tauri/src/fake.rs` to:

```rust
use std::sync::{Arc, Mutex, MutexGuard};

use crate::{
    backend::AudioBackend,
    engine::{AudioEngine, EngineParams, Route},
    error::{AudioError, Result},
    model::*,
};
```

and append:

```rust
#[derive(Debug, Default)]
pub struct EngineLog {
    pub running: Option<Route>,
    pub params: Option<EngineParams>,
    pub starts: usize,
    pub stops: usize,
    pub fail_start: Option<String>,
    pub failure: Option<String>,
    pub peaks: (f32, f32),
}

/// Records what the app asks of the engine; tests keep a clone to inspect and steer it.
#[derive(Clone, Default)]
pub struct FakeEngine(pub Arc<Mutex<EngineLog>>);

impl FakeEngine {
    pub fn log(&self) -> MutexGuard<'_, EngineLog> {
        self.0.lock().unwrap()
    }
}

impl AudioEngine for FakeEngine {
    fn start(&mut self, route: &Route, params: &EngineParams) -> Result<()> {
        let mut log = self.log();
        if let Some(e) = log.fail_start.clone() {
            return Err(AudioError::Os(e));
        }
        log.starts += 1;
        log.running = Some(route.clone());
        log.params = Some(*params);
        Ok(())
    }

    fn stop(&mut self) {
        let mut log = self.log();
        if log.running.take().is_some() {
            log.stops += 1;
        }
        log.failure = None;
    }

    fn set_params(&mut self, params: &EngineParams) {
        self.log().params = Some(*params);
    }

    fn failure(&self) -> Option<String> {
        let log = self.log();
        log.running.as_ref().and(log.failure.clone())
    }

    fn take_peaks(&self) -> (f32, f32) {
        self.log().peaks
    }
}
```

- [ ] **Step 4: Write the failing controller tests**

Create `src-tauri/src/app_state/processing_tests.rs`:

```rust
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
```

Register it in `app_state/mod.rs` next to `mod tests;`:

```rust
mod processing;
#[cfg(test)]
mod processing_tests;
```

and add `pub use processing::{find_cable, Cable, RETRY_REFRESHES};` below the existing `pub use group::...` line.

- [ ] **Step 5: Run the tests to see them fail**

Run: `cargo test --manifest-path M:\Resonance\src-tauri\Cargo.toml --lib app_state::processing_tests`
Expected: compile errors (`processing` module, `set_engine`, `shutdown` and the `SetProcessing` match arm don't exist yet).

- [ ] **Step 6: Write `app_state/processing.rs`**

```rust
use super::AppCore;
use crate::{
    backend::AudioBackend,
    engine::{AudioEngine, EngineParams, EngineStatus, ProcessingView, RestoreDefaults, Route},
    error::AudioError,
    model::{Device, DeviceState, Flow, Role},
};

const CABLE_RENDER: &str = "CABLE Input";
const CABLE_CAPTURE: &str = "CABLE Output";
/// After the engine fails to start or stops by itself, wait this many refreshes (2 s) before retrying.
pub const RETRY_REFRESHES: u32 = 8;

#[derive(Debug, Clone, PartialEq)]
pub struct Cable {
    pub render_id: String,
    pub capture_id: String,
}

/// VB-Cable's endpoints, matched by their Windows names (aliases don't matter).
pub fn find_cable(devices: &[Device]) -> Option<Cable> {
    let find = |flow: Flow, prefix: &str| {
        devices
            .iter()
            .find(|d| d.flow == flow && d.state == DeviceState::Active && d.original_name.starts_with(prefix))
            .map(|d| d.id.clone())
    };
    Some(Cable { render_id: find(Flow::Render, CABLE_RENDER)?, capture_id: find(Flow::Capture, CABLE_CAPTURE)? })
}

/// Engine bookkeeping that isn't saved.
#[derive(Default)]
pub struct ProcState {
    pub status: EngineStatus,
    /// The route the engine is running, if it is.
    pub route: Option<Route>,
    /// True once this run has pointed the Windows default mics at CABLE Output.
    pub switched: bool,
    pub retry_in: u32,
    pub last_error: Option<String>,
}

fn is_active_capture(devices: &[Device], id: &str) -> bool {
    devices.iter().any(|d| d.id == id && d.flow == Flow::Capture && d.state == DeviceState::Active)
}

fn default_id(devices: &[Device], flow: Flow, role: Role) -> Option<String> {
    devices
        .iter()
        .find(|d| {
            d.flow == flow
                && d.state == DeviceState::Active
                && match role {
                    Role::Console => d.default_console,
                    Role::Communications => d.default_comms,
                }
        })
        .map(|d| d.id.clone())
}

impl<B: AudioBackend> AppCore<B> {
    pub fn set_engine(&mut self, engine: Box<dyn AudioEngine>) {
        self.stop_engine();
        self.engine = engine;
    }

    pub(super) fn processing_view(&self) -> ProcessingView {
        ProcessingView {
            settings: self.settings.processing.clone(),
            status: self.engine_state.status.clone(),
            cable_installed: find_cable(&self.state.devices).is_some(),
        }
    }

    pub(super) fn engine_params(&self) -> EngineParams {
        EngineParams { dsp: self.settings.processing.dsp, monitor_level: self.settings.processing.monitor_level }
    }

    /// The mic to process: the chosen one, else the default from before processing, else the current
    /// default, else any mic. Never CABLE Output itself.
    fn source(&self, cable: &Cable) -> Option<String> {
        let devices = &self.state.devices;
        let usable = |id: Option<String>| id.filter(|id| *id != cable.capture_id && is_active_capture(devices, id));
        let remembered = self.settings.processing_restore.as_ref().and_then(|r| r.console.clone());
        usable(self.settings.processing.source_id.clone())
            .or_else(|| usable(remembered))
            .or_else(|| usable(default_id(devices, Flow::Capture, Role::Console)))
            .or_else(|| {
                devices
                    .iter()
                    .find(|d| d.flow == Flow::Capture && d.state == DeviceState::Active && d.id != cable.capture_id)
                    .map(|d| d.id.clone())
            })
    }

    fn stop_engine(&mut self) {
        if self.engine_state.route.take().is_some() {
            self.engine.stop();
        }
    }

    /// Points both default-mic roles at CABLE Output, first remembering the current ones (unless a run
    /// that didn't shut down cleanly already did). Returns true if Windows defaults changed.
    fn switch_defaults(&mut self, cable: &Cable) -> bool {
        if self.settings.processing_restore.is_none() {
            let devices = &self.state.devices;
            let current = |role| default_id(devices, Flow::Capture, role).filter(|id| *id != cable.capture_id);
            self.settings.processing_restore =
                Some(RestoreDefaults { console: current(Role::Console), comms: current(Role::Communications) });
            self.settings_dirty = true;
        }
        let mut changed = false;
        for role in [Role::Console, Role::Communications] {
            if default_id(&self.state.devices, Flow::Capture, role).as_deref() == Some(cable.capture_id.as_str()) {
                continue;
            }
            match self.backend.set_default_device(&cable.capture_id, role) {
                Ok(()) => changed = true,
                Err(e) => self.notices.push(AudioError::Os(format!("Couldn't make CABLE Output the default microphone: {e}"))),
            }
        }
        changed
    }

    /// Puts back the remembered default mics, but only for roles still pointing at CABLE Output.
    /// Returns true if Windows defaults changed.
    fn restore_defaults(&mut self) -> bool {
        self.engine_state.switched = false;
        let Some(saved) = self.settings.processing_restore.take() else { return false };
        self.settings_dirty = true;
        let Some(cable) = find_cable(&self.state.devices) else { return false };
        let mut changed = false;
        for (role, id) in [(Role::Console, saved.console), (Role::Communications, saved.comms)] {
            let Some(id) = id else { continue };
            let on_cable =
                default_id(&self.state.devices, Flow::Capture, role).as_deref() == Some(cable.capture_id.as_str());
            if on_cable && is_active_capture(&self.state.devices, &id) {
                match self.backend.set_default_device(&id, role) {
                    Ok(()) => changed = true,
                    Err(e) => tracing::warn!("couldn't restore the default mic: {e}"),
                }
            }
        }
        changed
    }

    /// Brings the engine and the Windows default mics in line with the processing settings.
    /// Returns true if it changed Windows defaults (the state must be re-read).
    pub(super) fn reconcile_processing(&mut self) -> bool {
        if !self.settings.processing.enabled {
            self.stop_engine();
            self.engine_state.status = EngineStatus::Off;
            return self.restore_defaults();
        }
        let Some(cable) = find_cable(&self.state.devices) else {
            self.stop_engine();
            self.engine_state.status = EngineStatus::NoCable;
            return false;
        };
        let mut changed = false;
        if !self.engine_state.switched {
            changed = self.switch_defaults(&cable);
            self.engine_state.switched = true;
        }
        let Some(source_id) = self.source(&cable) else {
            self.stop_engine();
            self.engine_state.status = EngineStatus::Paused { reason: "There's no microphone to process".into() };
            return changed;
        };
        let monitor_id = if self.settings.processing.monitor {
            default_id(&self.state.devices, Flow::Render, Role::Console).filter(|id| *id != cable.render_id)
        } else {
            None
        };
        let route = Route { source_id, cable_id: cable.render_id, monitor_id };

        if let Some(reason) = self.engine.failure() {
            if self.engine_state.route.take().is_some() {
                self.engine.stop();
                tracing::warn!("audio engine stopped: {reason}");
                self.engine_state.last_error = Some(reason);
                self.engine_state.retry_in = RETRY_REFRESHES;
            }
        }
        if self.engine_state.route.as_ref() == Some(&route) {
            self.engine_state.status = EngineStatus::Running;
            return changed;
        }
        self.stop_engine();
        if self.engine_state.retry_in > 0 {
            self.engine_state.retry_in -= 1;
        } else {
            match self.engine.start(&route, &self.engine_params()) {
                Ok(()) => {
                    self.engine_state.route = Some(route);
                    self.engine_state.last_error = None;
                    self.engine_state.status = EngineStatus::Running;
                    return changed;
                }
                Err(e) => {
                    tracing::warn!("audio engine didn't start: {e}");
                    self.engine_state.last_error = Some(e.to_string());
                    self.engine_state.retry_in = RETRY_REFRESHES;
                }
            }
        }
        let reason = self.engine_state.last_error.clone().unwrap_or_else(|| "Waiting for the audio devices".into());
        self.engine_state.status = EngineStatus::Paused { reason };
        changed
    }

    /// Called when Resonance quits: stops the engine and gives Windows its previous default mics back.
    /// Processing stays enabled for next time.
    pub fn shutdown(&mut self) {
        self.stop_engine();
        self.restore_defaults();
    }
}
```

- [ ] **Step 7: Wire the controller into `AppCore`**

In `src-tauri/src/app_state/mod.rs`:
- add `engine::{AudioEngine, NullEngine},` to the `crate::{...}` import.
- add three fields to `AppCore` after `settings_dirty: bool,`:

```rust
    engine: Box<dyn AudioEngine>,
    engine_state: processing::ProcState,
    notices: Vec<AudioError>,
```

- in `AppCore::new`, replace `Self { backend, settings, state, seen_sessions: HashSet::new(), settings_dirty: false }` with:

```rust
        let mut core = Self {
            backend,
            settings,
            state,
            seen_sessions: HashSet::new(),
            settings_dirty: false,
            engine: Box::new(NullEngine),
            engine_state: Default::default(),
            notices: Vec::new(),
        };
        core.state.processing = core.processing_view();
        core
```

- add below `take_settings_dirty`:

```rust
    /// Problems to show the user that didn't come from a command (e.g. a failed default-mic switch).
    pub fn take_notices(&mut self) -> Vec<AudioError> {
        std::mem::take(&mut self.notices)
    }
```

- replace `refresh` with:

```rust
    /// Re-reads the OS, restores saved prefs onto new sessions, remembers current values, and keeps the
    /// processing engine and default mics in line with the settings.
    pub fn refresh(&mut self) -> Result<bool> {
        let next = self.read_state()?;
        let previous = std::mem::replace(&mut self.state, next);
        if self.reconcile_processing() {
            self.state = self.read_state()?;
        }
        self.state.processing = self.processing_view();
        Ok(self.state != previous)
    }

    fn read_state(&mut self) -> Result<UiState> {
        let mut snap = self.backend.snapshot()?;
        if self.restore_new_sessions(&snap.sessions) {
            snap = self.backend.snapshot()?;
        }
        let apps = group_sessions(&snap.sessions);
        self.remember(&apps);
        Ok(self.build_state(snap.devices, apps))
    }
```

- in `build_state`, add `processing: self.state.processing.clone(),` after `ui: self.settings.ui.clone(),` (refresh overwrites it with a fresh view).
- add a match arm in `apply` after `Command::ResetAppMemory => { ... }`:

```rust
            Command::SetProcessing { processing } => {
                let mut p = processing;
                p.dsp = p.dsp.clamped();
                p.monitor_level = clamp01(p.monitor_level)?;
                self.settings.processing = p;
                self.settings_dirty = true;
                let params = self.engine_params();
                self.engine.set_params(&params);
                self.engine_state.retry_in = 0;
                if self.reconcile_processing() {
                    self.refresh()?;
                }
                self.state.processing = self.processing_view();
            }
```

- in `meters`, replace the last line with:

```rust
        let (proc_in, proc_out) = self.engine.take_peaks();
        MeterFrame { apps, master: peaks.master, mic: peaks.mic, proc_in, proc_out }
```

- [ ] **Step 8: Give the worker the engine, notices and a waiting shutdown**

In `src-tauri/src/worker.rs`:
- add `engine::AudioEngine,` to the `crate::{...}` import.
- change `Shutdown,` in `enum Msg` to `Shutdown(Option<mpsc::Sender<()>>),`
- replace `pub fn shutdown(&self)` with:

```rust
    pub fn shutdown(&self) {
        let _ = self.tx.send(Msg::Shutdown(None));
    }

    /// Asks the worker to stop the engine, give Windows its default mics back and save settings,
    /// waiting up to `timeout` for it to finish. Returns at once if the worker has already stopped.
    pub fn shutdown_and_wait(&self, timeout: Duration) {
        let (tx, rx) = mpsc::channel();
        if self.tx.send(Msg::Shutdown(Some(tx))).is_ok() {
            let _ = rx.recv_timeout(timeout);
        }
    }
```

- change `spawn`'s signature and body:

```rust
pub fn spawn<F, B, S>(make_backend: F, engine: Box<dyn AudioEngine>, settings_path: PathBuf, sink: S) -> WorkerHandle
```

and replace `let mut core = AppCore::new(backend, Settings::load(&settings_path));` with:

```rust
            let mut core = AppCore::new(backend, Settings::load(&settings_path));
            core.set_engine(engine);
```

- add a helper below `fn save`:

```rust
/// Stops the engine, gives Windows its default mics back and saves anything unsaved.
fn stop<B: AudioBackend>(core: &mut AppCore<B>, path: &Path, unsaved: bool) {
    core.shutdown();
    if core.take_settings_dirty() || unsaved {
        save(core, path);
    }
    core.stop_metering();
}
```

- replace the `Ok(Msg::Shutdown) | Err(RecvTimeoutError::Disconnected) => { ... }` arm with:

```rust
            Ok(Msg::Shutdown(done)) => {
                stop(core, path, dirty_since.is_some());
                if let Some(done) = done {
                    let _ = done.send(());
                }
                return;
            }
            Err(RecvTimeoutError::Disconnected) => {
                stop(core, path, dirty_since.is_some());
                return;
            }
```

- after the `if changed { ... }` block, add:

```rust
        for notice in core.take_notices() {
            sink.notice(&notice);
        }
```

- in the tests module, add `use crate::engine::NullEngine;`, change every `spawn(|| Ok(fake()), dir...` call to `spawn(|| Ok(fake()), Box::new(NullEngine), dir...` (three tests) and the failing-backend call to:

```rust
        let h = spawn::<_, FakeBackend, _>(
            || Err(AudioError::Os("no audio".into())),
            Box::new(NullEngine),
            dir.path().join("s.json"),
            TestSink(tx),
        );
```

and add this test:

```rust
    #[test]
    fn shutdown_and_wait_saves_before_returning() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s.json");
        let (tx, rx) = mpsc::channel();
        let h = spawn(|| Ok(fake()), Box::new(NullEngine), path.clone(), TestSink(tx));
        wait_for(&rx, "state");
        h.command_blocking(Command::SetAppVolume { key: "spotify.exe".into(), volume: 0.3 }).unwrap();
        h.shutdown_and_wait(Duration::from_secs(3));
        assert_eq!(Settings::load(&path).apps["spotify.exe"].volume, 0.3);
        h.shutdown_and_wait(Duration::from_secs(3));
    }
```

- [ ] **Step 9: Update the app's worker call so the crate builds**

In `src-tauri/src/app/mod.rs`, change the `worker::spawn(` call to pass `Box::new(crate::engine::NullEngine),` as the second argument (Task 7 swaps in the real engine).

- [ ] **Step 10: Run all Rust tests**

Run: `cargo test --manifest-path M:\Resonance\src-tauri\Cargo.toml --lib`
Expected: everything PASSES, including the 13 new `processing_tests`, the extended model tests and `shutdown_and_wait_saves_before_returning`.

- [ ] **Step 11: Commit**

```bash
git add src-tauri/src
git commit -m "feat(engine): processing controller - default-mic switching, engine lifecycle, retries, clean quit"
```

---
### Task 6: The WASAPI engine

**Files:**
- Modify: `src-tauri/src/win/endpoints.rs`
- Create: `src-tauri/src/win/engine.rs`
- Modify: `src-tauri/src/win/mod.rs`
- Create: `src-tauri/tests/engine_hw.rs`

**Interfaces:**
- Consumes: `Chain`, `Feed`, `TARGET_QUEUE`, `Fifo`, `FRAME` (Tasks 3–4); `AudioEngine`, `Route`, `EngineParams` (Task 5); `ComGuard`, `endpoints::create_enumerator` (Phase 1).
- Produces: `endpoints::device_by_id(&IMMDeviceEnumerator, &str) -> Result<IMMDevice>`; `win::WasapiEngine::{new(), underruns() -> u32}` implementing `AudioEngine`.

- [ ] **Step 1: Add `device_by_id`**

In `src-tauri/src/win/endpoints.rs`, add `use windows::core::HSTRING;` to the imports, change `use crate::error::Result;` to `use crate::error::{AudioError, Result};`, and append:

```rust
pub fn device_by_id(e: &IMMDeviceEnumerator, id: &str) -> Result<IMMDevice> {
    unsafe { e.GetDevice(&HSTRING::from(id)).map_err(|_| AudioError::device_gone()) }
}
```

- [ ] **Step 2: Write `win/engine.rs`**

```rust
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
        AudioEngine, EngineParams, Route,
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
            client.Start()?;
            Ok(Self { client, render, size })
        }
    }

    /// Moves queued audio from `feed` into the device buffer, then steers the feed's drift control.
    /// Returns true if the device had run dry since the last write.
    fn write(&self, feed: &mut Feed, gain: f32, scratch: &mut [f32]) -> Result<bool> {
        unsafe {
            let padding = self.client.GetCurrentPadding()?;
            let want = ((self.size - padding) as usize).min(feed.queued()).min(scratch.len());
            let got = feed.pull(&mut scratch[..want]);
            if got > 0 {
                let out = std::slice::from_raw_parts_mut(self.render.GetBuffer(got as u32)? as *mut f32, got * 2);
                for (pair, s) in out.chunks_exact_mut(2).zip(&scratch[..got]) {
                    let v = s * gain;
                    pair[0] = v;
                    pair[1] = v;
                }
                self.render.ReleaseBuffer(got as u32, 0)?;
            }
            feed.steer(padding as usize + got);
            Ok(padding == 0)
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
```

- [ ] **Step 3: Export it**

In `src-tauri/src/win/mod.rs`, add `mod engine;` after `mod endpoints;` and `pub use engine::WasapiEngine;` after `pub use backend::WasapiBackend;`.

- [ ] **Step 4: Build and lint**

Run: `cargo clippy --manifest-path M:\Resonance\src-tauri\Cargo.toml --all-targets -- -D warnings`
Expected: no warnings. (If a windows-rs signature differs from the code above, the compiler names it: e.g. `AvSetMmThreadCharacteristicsW` takes any `Param<PCWSTR>`, so `&HSTRING` and `w!("Pro Audio")` both work.)

- [ ] **Step 5: Write the hardware tests**

`src-tauri/tests/engine_hw.rs`:

```rust
#![cfg(windows)]
//! Needs VB-Cable installed, a working microphone, and the installed Resonance quit. Run with:
//! cargo test --manifest-path src-tauri/Cargo.toml --test engine_hw -- --ignored --test-threads=1
use std::{thread, time::Duration};

use resonance_lib::{
    app_state::AppCore,
    backend::AudioBackend,
    engine::{AudioEngine, EngineParams, EngineStatus, ProcessingSettings, Route},
    model::*,
    settings::Settings,
    win::{WasapiBackend, WasapiEngine},
};

fn active(devices: &[RawDevice], flow: Flow, prefix: &str) -> Option<String> {
    devices
        .iter()
        .find(|d| d.flow == flow && d.state == DeviceState::Active && d.name.starts_with(prefix))
        .map(|d| d.id.clone())
}

#[test]
#[ignore]
fn processes_the_default_mic_into_the_cable_without_dropouts() {
    let devices = WasapiBackend::new().unwrap().snapshot().unwrap().devices;
    let cable = active(&devices, Flow::Render, "CABLE Input").expect("VB-Cable isn't installed");
    let mics: Vec<&RawDevice> = devices
        .iter()
        .filter(|d| d.flow == Flow::Capture && d.state == DeviceState::Active && !d.name.starts_with("CABLE Output"))
        .collect();
    let mic = mics.iter().find(|d| d.default_console).or(mics.first()).expect("no microphone").id.clone();

    let mut engine = WasapiEngine::new();
    let params = EngineParams { dsp: Default::default(), monitor_level: 0.0 };
    engine.start(&Route { source_id: mic, cable_id: cable, monitor_id: None }, &params).unwrap();
    let mut peak_in = 0.0f32;
    for _ in 0..10 {
        thread::sleep(Duration::from_secs(1));
        peak_in = peak_in.max(engine.take_peaks().0);
    }
    let mut quieter = params;
    quieter.dsp.limiter.ceiling_db = -12.0;
    engine.set_params(&quieter);
    thread::sleep(Duration::from_millis(500));
    assert_eq!(engine.failure(), None);
    assert!(peak_in > 0.0, "the microphone delivered pure digital silence (is it muted?)");
    assert!(engine.underruns() <= 2, "{} dropouts", engine.underruns());
    engine.stop();
}

#[test]
#[ignore]
fn switches_the_default_mics_to_the_cable_and_back() {
    let mut core = AppCore::new(WasapiBackend::new().unwrap(), Settings::default());
    core.set_engine(Box::new(WasapiEngine::new()));
    core.refresh().unwrap();
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
    let cable_out = core
        .state()
        .devices
        .iter()
        .find(|d| d.flow == Flow::Capture && d.original_name.starts_with("CABLE Output"))
        .expect("VB-Cable isn't installed")
        .id
        .clone();
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
```

- [ ] **Step 6: Make sure the hardware tests compile and are skipped by default**

Run: `cargo test --manifest-path M:\Resonance\src-tauri\Cargo.toml --test engine_hw`
Expected: `2 ignored`. If VB-Cable is installed, also run `cargo test --manifest-path M:\Resonance\src-tauri\Cargo.toml --test engine_hw -- --ignored --test-threads=1` (installed Resonance quit) and expect both to PASS; otherwise note them as pending in Task 11.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/win src-tauri/tests/engine_hw.rs
git commit -m "feat(win): WASAPI processing engine on an MMCSS thread, with hardware tests"
```

---

### Task 7: App wiring — real engine, clean quit, VB-Cable link

**Files:**
- Create: `src-tauri/src/win/shell.rs`
- Modify: `src-tauri/src/win/mod.rs`
- Modify: `src-tauri/src/app/commands.rs`
- Modify: `src-tauri/src/app/mod.rs`
- Modify: `src-tauri/src/app/tray.rs`

**Interfaces:**
- Consumes: `WasapiEngine` (Task 6), `WorkerHandle::shutdown_and_wait` (Task 5).
- Produces: Tauri command `open_vb_cable` (no arguments); `win::open_url(&str)`.

- [ ] **Step 1: Add `win/shell.rs`**

```rust
use windows::core::{w, HSTRING, PCWSTR};
use windows::Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL};

/// Opens a web page in the default browser.
pub fn open_url(url: &str) {
    unsafe {
        let _ = ShellExecuteW(None, w!("open"), &HSTRING::from(url), PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL);
    }
}
```

In `win/mod.rs`, add `mod shell;` and `pub use shell::open_url;`.

- [ ] **Step 2: Add the command**

Append to `src-tauri/src/app/commands.rs`:

```rust
/// VB-Cable's download page (the only URL this command opens).
const VB_CABLE_URL: &str = "https://vb-audio.com/Cable/";

#[tauri::command]
pub fn open_vb_cable() {
    crate::win::open_url(VB_CABLE_URL);
}
```

- [ ] **Step 3: Use the real engine and wait for a clean shutdown on exit**

In `src-tauri/src/app/mod.rs`:
- in the `worker::spawn(` call, replace `Box::new(crate::engine::NullEngine),` with `Box::new(crate::win::WasapiEngine::new()),`
- add `commands::open_vb_cable,` to `generate_handler![...]` after `commands::get_headset,`
- replace the final

```rust
        .run(tauri::generate_context!())
        .expect("error while running Resonance");
```

with

```rust
        .build(tauri::generate_context!())
        .expect("error while building Resonance")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                // Stop the engine and give Windows its default mics back before the process ends.
                if let Some(worker) = app.try_state::<worker::WorkerHandle>() {
                    worker.shutdown_and_wait(std::time::Duration::from_secs(3));
                }
            }
        });
```

- [ ] **Step 4: Make the tray's Quit wait too**

In `src-tauri/src/app/tray.rs`, replace `app.state::<WorkerHandle>().shutdown();` with `app.state::<WorkerHandle>().shutdown_and_wait(std::time::Duration::from_secs(3));`.

- [ ] **Step 5: Build, lint and test**

Run: `cargo clippy --manifest-path M:\Resonance\src-tauri\Cargo.toml --all-targets -- -D warnings`
Then: `cargo test --manifest-path M:\Resonance\src-tauri\Cargo.toml`
Expected: no warnings; all tests PASS (hardware tests ignored).

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src
git commit -m "feat(app): run the WASAPI engine, restore mics on quit, open the VB-Cable page"
```

---
### Task 8: Frontend foundation — types, API, store, helpers, meters, fixtures

**Files:**
- Modify: `src/lib/types.ts`
- Modify: `src/lib/api.ts`
- Modify: `src/lib/store.ts`
- Create: `src/lib/processing.ts`
- Create: `src/lib/processing.test.ts`
- Modify: `src/lib/meters.ts`
- Modify: `src/lib/meters.test.ts`
- Modify: `src/components/Meter.tsx`
- Modify: `src/test/fixtures.ts`

**Interfaces:**
- Consumes: the JSON shapes from Tasks 1 and 5 (`UiState.processing`, `MeterFrame.procIn/procOut`, `setProcessing`) and the `open_vb_cable` command (Task 7).
- Produces: TS types `DenoiseParams, GateParams, CompressorParams, LimiterParams, DspParams, ProcessingSettings, EngineStatus, ProcessingView`; `openVbCable()`; `setProcessing(ProcessingSettings)`; `amountCurve, compressorCurve, autoMakeup, processingSources, isProcessed`; meter sources `"procIn"`/`"procOut"`; fixtures `DEFAULT_DSP`, `makeProcessing(partial)`.

- [ ] **Step 1: Add the types**

In `src/lib/types.ts`, insert before `export interface UiState`:

```ts
export interface DenoiseParams {
  enabled: boolean;
  strength: number;
}

export interface GateParams {
  enabled: boolean;
  thresholdDb: number;
  attackMs: number;
  holdMs: number;
  releaseMs: number;
  rangeDb: number;
}

export interface CompressorParams {
  enabled: boolean;
  amount: number;
  custom: boolean;
  thresholdDb: number;
  ratio: number;
  attackMs: number;
  releaseMs: number;
  makeupDb: number | null;
}

export interface LimiterParams {
  enabled: boolean;
  ceilingDb: number;
  releaseMs: number;
}

export interface DspParams {
  denoise: DenoiseParams;
  gate: GateParams;
  compressor: CompressorParams;
  limiter: LimiterParams;
}

export interface ProcessingSettings {
  enabled: boolean;
  sourceId: string | null;
  dsp: DspParams;
  monitor: boolean;
  monitorLevel: number;
}

export type EngineStatus =
  | { state: "off" }
  | { state: "noCable" }
  | { state: "running" }
  | { state: "paused"; reason: string };

export interface ProcessingView {
  settings: ProcessingSettings;
  status: EngineStatus;
  cableInstalled: boolean;
}
```

Then add `processing: ProcessingView;` to `UiState` after `ui: UiSettings;`, add `procIn: number;` and `procOut: number;` to `MeterFrame` after `mic: number;`, and add `| { type: "setProcessing"; processing: ProcessingSettings }` to `Command` after the `resetAppMemory` member (moving the `;` to the new last line).

- [ ] **Step 2: API and store**

In `src/lib/api.ts`, add after `export const showMain = ...`:

```ts
export const openVbCable = () => invoke<void>("open_vb_cable");
```

In `src/lib/store.ts`, add `ProcessingSettings` to the type import and add after `toggleMicMute`:

```ts
export const setProcessing = (processing: ProcessingSettings) => run({ type: "setProcessing", processing });
```

- [ ] **Step 3: Fixtures**

In `src/test/fixtures.ts`, change the first import to `import type { App, Device, DspParams, ProcessingView, UiState } from "../lib/types";`, add `processing: makeProcessing(),` in `makeState`'s object before `...p,`, and add below `makeApp`:

```ts
export const DEFAULT_DSP: DspParams = {
  denoise: { enabled: true, strength: 1 },
  gate: { enabled: true, thresholdDb: -45, attackMs: 2, holdMs: 150, releaseMs: 120, rangeDb: -60 },
  compressor: { enabled: true, amount: 0.5, custom: false, thresholdDb: -22.5, ratio: 3.75, attackMs: 5, releaseMs: 80, makeupDb: null },
  limiter: { enabled: true, ceilingDb: -1, releaseMs: 50 },
};

export function makeProcessing(p: Partial<ProcessingView> = {}): ProcessingView {
  return {
    settings: { enabled: false, sourceId: null, dsp: structuredClone(DEFAULT_DSP), monitor: false, monitorLevel: 0.7 },
    status: { state: "off" },
    cableInstalled: true,
    ...p,
  };
}
```

- [ ] **Step 4: Write the failing helper tests**

`src/lib/processing.test.ts`:

```ts
import { amountCurve, autoMakeup, compressorCurve, isProcessed, processingSources } from "./processing";
import { DEFAULT_DSP, makeDevice, makeProcessing, makeState } from "../test/fixtures";

describe("processing helpers", () => {
  it("maps the compressor amount like the backend", () => {
    expect(amountCurve(0)).toEqual({ thresholdDb: -10, ratio: 1.5 });
    expect(amountCurve(1)).toEqual({ thresholdDb: -35, ratio: 6 });
    expect(compressorCurve(DEFAULT_DSP.compressor)).toEqual({ thresholdDb: -22.5, ratio: 3.75 });
  });

  it("uses a custom threshold and ratio, and derives auto makeup from them", () => {
    const c = { ...DEFAULT_DSP.compressor, custom: true, thresholdDb: -30, ratio: 4 };
    expect(compressorCurve(c)).toEqual({ thresholdDb: -30, ratio: 4 });
    expect(autoMakeup(c)).toBeCloseTo(4.5);
  });

  it("never offers VB-Cable as a source", () => {
    const s = makeState();
    const name = "CABLE Output (VB-Audio Virtual Cable)";
    s.devices.push(makeDevice({ id: "cout", name, originalName: name, flow: "capture" }));
    expect(processingSources(s).map((d) => d.id)).toEqual(["mic1"]);
  });

  it("knows when processing is running", () => {
    expect(isProcessed(makeState())).toBe(false);
    expect(isProcessed(makeState({ processing: makeProcessing({ status: { state: "running" } }) }))).toBe(true);
  });
});
```

Run: `npm.cmd test -- src/lib/processing.test.ts`
Expected: FAIL (`./processing` doesn't exist).

- [ ] **Step 5: Write `src/lib/processing.ts`**

```ts
import type { CompressorParams, Device, UiState } from "./types";

const CABLE_CAPTURE = "CABLE Output";

/** Threshold and ratio for the compressor's Amount control (mirrors the backend). */
export function amountCurve(amount: number) {
  const a = Math.min(1, Math.max(0, amount));
  return { thresholdDb: -10 - 25 * a, ratio: 1.5 + 4.5 * a };
}

export function compressorCurve(c: CompressorParams) {
  return c.custom ? { thresholdDb: c.thresholdDb, ratio: c.ratio } : amountCurve(c.amount);
}

/** Automatic makeup: half the gain reduction a −18 dBFS signal receives. */
export function autoMakeup(c: CompressorParams) {
  const { thresholdDb, ratio } = compressorCurve(c);
  return (Math.max(0, -18 - thresholdDb) * (1 - 1 / ratio)) / 2;
}

/** Mics that can be processed: every active input except VB-Cable's own output. */
export const processingSources = (s: UiState): Device[] =>
  s.devices.filter((d) => d.flow === "capture" && d.state === "active" && !d.originalName.startsWith(CABLE_CAPTURE));

export const isProcessed = (s: UiState) => s.processing.status.state === "running";
```

- [ ] **Step 6: Meter sources**

In `src/lib/meters.ts`, change the comment above `ingestFrame` to `/** Sources: "master", "mic", "procIn", "procOut", "app:<key>". */` and add after `latest.set("mic", f.mic);`:

```ts
  latest.set("procIn", f.procIn);
  latest.set("procOut", f.procOut);
```

In `src/components/Meter.tsx`, change the `source` comment to `/** "master", "mic", "procIn", "procOut" or "app:<key>" */`.

In `src/lib/meters.test.ts`, add `procIn: 0, procOut: 0` to each of the three `ingestFrame({...})` objects (e.g. `ingestFrame({ apps: { "a.exe": 0.5, "b.exe": 0.25 }, master: 0.7, mic: 0.1, procIn: 0, procOut: 0 });`), and add inside `describe("ingestFrame", ...)`:

```ts
  it("stores the processing peaks", () => {
    ingestFrame({ apps: {}, master: 0, mic: 0, procIn: 0.4, procOut: 0.2 });
    expect(readLevel("procIn")).toBe(0.4);
    expect(readLevel("procOut")).toBe(0.2);
  });
```

- [ ] **Step 7: Run the frontend tests and type check**

Run: `npm.cmd test`
Then: `npx.cmd tsc --noEmit`
Expected: all tests PASS (4 new helper tests, 1 new meter test); no type errors.

- [ ] **Step 8: Commit**

```bash
git add src/lib src/components/Meter.tsx src/test/fixtures.ts
git commit -m "feat(ui): processing types, API, store action, helpers and meter sources"
```

---

### Task 9: Knob and stage module components

**Files:**
- Create: `src/components/Knob.tsx`
- Create: `src/components/Knob.test.tsx`
- Create: `src/components/StageModule.tsx`

**Interfaces:**
- Consumes: `throttle` (Phase 1), `Toggle` from `components/ui`.
- Produces: `Knob({ label, value, min, max, step, onChange, format, size?: "main" | "small", disabled? })` rendering `role="slider"` with `aria-valuetext`; `StageModule({ title, enabled, onToggle, children, advanced? })` rendering a `region` named `title`, a `switch` named `title`, and an "Advanced" disclosure button.

- [ ] **Step 1: Write the failing knob tests**

`src/components/Knob.test.tsx`:

```tsx
import { fireEvent, render, screen } from "@testing-library/react";
import { Knob } from "./Knob";

const db = (v: number) => `${v} dB`;
const pct = (v: number) => `${Math.round(v * 100)}%`;

describe("Knob", () => {
  it("describes its value", () => {
    render(<Knob label="Threshold" value={-45} min={-80} max={-10} step={1} format={db} onChange={() => {}} />);
    const knob = screen.getByRole("slider", { name: "Threshold" });
    expect(knob).toHaveAttribute("aria-valuenow", "-45");
    expect(knob).toHaveAttribute("aria-valuetext", "-45 dB");
    expect(screen.getByText("-45 dB")).toBeInTheDocument();
  });

  it("steps with the keyboard and stays in range", () => {
    const onChange = vi.fn();
    render(<Knob label="Threshold" value={-11} min={-80} max={-10} step={1} format={db} onChange={onChange} />);
    const knob = screen.getByRole("slider", { name: "Threshold" });
    fireEvent.keyDown(knob, { key: "ArrowUp" });
    expect(onChange).toHaveBeenLastCalledWith(-10);
    fireEvent.keyDown(knob, { key: "PageUp" });
    expect(onChange).toHaveBeenLastCalledWith(-10);
    fireEvent.keyDown(knob, { key: "ArrowDown" });
    expect(onChange).toHaveBeenLastCalledWith(-12);
    fireEvent.keyDown(knob, { key: "Home" });
    expect(onChange).toHaveBeenLastCalledWith(-80);
  });

  it("drags upward to increase", () => {
    const onChange = vi.fn();
    render(<Knob label="Amount" value={0} min={0} max={1} step={0.05} format={pct} onChange={onChange} />);
    const knob = screen.getByRole("slider", { name: "Amount" });
    fireEvent.pointerDown(knob, { clientY: 300, pointerId: 1 });
    fireEvent.pointerMove(knob, { clientY: 200, pointerId: 1 });
    expect(knob).toHaveAttribute("aria-valuetext", "50%");
    fireEvent.pointerUp(knob, { clientY: 200, pointerId: 1 });
    expect(onChange).toHaveBeenLastCalledWith(0.5);
  });

  it("ignores input when disabled", () => {
    const onChange = vi.fn();
    render(<Knob label="Amount" value={0} min={0} max={1} step={0.05} format={pct} onChange={onChange} disabled />);
    const knob = screen.getByRole("slider", { name: "Amount" });
    fireEvent.keyDown(knob, { key: "ArrowUp" });
    fireEvent.pointerDown(knob, { clientY: 300, pointerId: 1 });
    fireEvent.pointerMove(knob, { clientY: 100, pointerId: 1 });
    fireEvent.pointerUp(knob, { clientY: 100, pointerId: 1 });
    expect(onChange).not.toHaveBeenCalled();
  });
});
```

Run: `npm.cmd test -- src/components/Knob.test.tsx`
Expected: FAIL (`./Knob` doesn't exist).

- [ ] **Step 2: Write `src/components/Knob.tsx`**

```tsx
import { useMemo, useRef, useState, type KeyboardEvent, type PointerEvent } from "react";
import { throttle } from "../lib/throttle";

interface Props {
  label: string;
  value: number;
  min: number;
  max: number;
  step: number;
  onChange: (value: number) => void;
  format: (value: number) => string;
  size?: "main" | "small";
  disabled?: boolean;
}

/** Pixels of vertical drag that sweep the whole range. */
const DRAG_PX = 200;
/** The pointer sweeps ±135° from straight up. */
const SWEEP = 135;

const snap = (v: number, min: number, max: number, step: number) => {
  const stepped = Math.round((v - min) / step) * step + min;
  return Math.min(max, Math.max(min, Math.round(stepped * 1000) / 1000));
};

function point(c: number, r: number, deg: number) {
  const rad = (deg * Math.PI) / 180;
  return { x: c + r * Math.sin(rad), y: c - r * Math.cos(rad) };
}

function arc(c: number, r: number, from: number, to: number) {
  const a = point(c, r, from);
  const b = point(c, r, to);
  return `M ${a.x} ${a.y} A ${r} ${r} 0 ${to - from > 180 ? 1 : 0} 1 ${b.x} ${b.y}`;
}

/** Rotary control: drag up/down or use the arrow keys. Holds its local value while dragging. */
export function Knob({ label, value, min, max, step, onChange, format, size = "main", disabled = false }: Props) {
  const [local, setLocal] = useState<number | null>(null);
  const drag = useRef<{ y: number; start: number } | null>(null);
  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;
  const emit = useMemo(() => throttle((v: number) => onChangeRef.current(v), 33), []);

  const shown = local ?? value;
  const px = size === "main" ? 76 : 48;
  const stroke = size === "main" ? 4 : 3;
  const c = px / 2;
  const r = c - stroke - 2;
  const cap = r - (size === "main" ? 9 : 6);
  const angle = -SWEEP + ((shown - min) / (max - min)) * 2 * SWEEP;
  const inner = point(c, cap * 0.3, angle);
  const outer = point(c, cap * 0.85, angle);

  const commit = (v: number) => {
    const s = snap(v, min, max, step);
    if (s !== value) onChange(s);
  };

  const onKeyDown = (e: KeyboardEvent) => {
    const keys: Record<string, number> = {
      ArrowUp: value + step,
      ArrowRight: value + step,
      ArrowDown: value - step,
      ArrowLeft: value - step,
      PageUp: value + step * 10,
      PageDown: value - step * 10,
      Home: min,
      End: max,
    };
    const next = keys[e.key];
    if (next === undefined) return;
    e.preventDefault();
    commit(next);
  };

  const onPointerDown = (e: PointerEvent<HTMLDivElement>) => {
    if (disabled) return;
    e.currentTarget.setPointerCapture?.(e.pointerId);
    drag.current = { y: e.clientY, start: value };
    setLocal(value);
  };

  const onPointerMove = (e: PointerEvent<HTMLDivElement>) => {
    if (!drag.current) return;
    const v = snap(drag.current.start + ((drag.current.y - e.clientY) / DRAG_PX) * (max - min), min, max, step);
    setLocal(v);
    emit(v);
  };

  const onPointerUp = () => {
    if (!drag.current) return;
    drag.current = null;
    if (local !== null && local !== value) onChangeRef.current(local);
    setLocal(null);
  };

  return (
    <div className={`flex flex-col items-center gap-1 ${disabled ? "opacity-45" : ""}`}>
      <div
        role="slider"
        tabIndex={disabled ? -1 : 0}
        aria-label={label}
        aria-valuemin={min}
        aria-valuemax={max}
        aria-valuenow={shown}
        aria-valuetext={format(shown)}
        aria-disabled={disabled || undefined}
        onKeyDown={disabled ? undefined : onKeyDown}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerCancel={onPointerUp}
        className={`touch-none rounded-full outline-offset-2 ${disabled ? "cursor-default" : "cursor-ns-resize"}`}
        style={{ width: px, height: px }}
      >
        <svg width={px} height={px} viewBox={`0 0 ${px} ${px}`} aria-hidden>
          <path d={arc(c, r, -SWEEP, SWEEP)} fill="none" stroke="var(--groove)" strokeWidth={stroke} strokeLinecap="round" />
          {angle > -SWEEP + 0.5 && (
            <path d={arc(c, r, -SWEEP, angle)} fill="none" stroke="var(--accent)" strokeWidth={stroke} strokeLinecap="round" />
          )}
          <circle cx={c} cy={c} r={cap} fill="var(--raised)" stroke="var(--cap-edge)" strokeWidth="1" />
          <line x1={inner.x} y1={inner.y} x2={outer.x} y2={outer.y} stroke="var(--paper)" strokeWidth="2.5" strokeLinecap="round" />
        </svg>
      </div>
      <div className={`num text-paper ${size === "main" ? "text-[14px]" : "text-[12px]"}`}>{format(shown)}</div>
      <div className="text-[11px] text-dim">{label}</div>
    </div>
  );
}
```

- [ ] **Step 3: Write `src/components/StageModule.tsx`**

```tsx
import { useId, useState, type ReactNode } from "react";
import { ChevronDown } from "lucide-react";
import { Toggle } from "./ui";

interface Props {
  title: string;
  enabled: boolean;
  onToggle: (enabled: boolean) => void;
  /** The stage's main control. */
  children: ReactNode;
  /** Extra controls shown in the Advanced drawer. */
  advanced?: ReactNode;
}

/** One processing stage: on/off switch, its main control, and an optional Advanced drawer. */
export function StageModule({ title, enabled, onToggle, children, advanced }: Props) {
  const [open, setOpen] = useState(false);
  const drawer = useId();
  return (
    <section aria-label={title} className="flex min-w-0 flex-col rounded-xl border border-groove bg-night/40 p-4">
      <div className="flex items-center justify-between gap-3">
        <h3 className={`m-0 text-[14px] font-medium ${enabled ? "text-paper" : "text-dim"}`}>{title}</h3>
        <Toggle checked={enabled} onChange={onToggle} label={title} />
      </div>
      <div className={`flex flex-1 justify-center py-4 ${enabled ? "" : "opacity-50"}`}>{children}</div>
      {advanced && (
        <>
          <button
            type="button"
            aria-expanded={open}
            aria-controls={drawer}
            onClick={() => setOpen(!open)}
            className="flex items-center justify-center gap-1 border-0 bg-transparent text-[12px] text-fog hover:text-paper"
          >
            Advanced
            <ChevronDown size={14} aria-hidden className={`transition-transform ${open ? "rotate-180" : ""}`} />
          </button>
          {open && (
            <div id={drawer} className={`mt-3 flex flex-wrap justify-center gap-3 border-t border-groove pt-3 ${enabled ? "" : "opacity-50"}`}>
              {advanced}
            </div>
          )}
        </>
      )}
    </section>
  );
}
```

- [ ] **Step 4: Run the tests**

Run: `npm.cmd test -- src/components/Knob.test.tsx`
Expected: 4 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add src/components/Knob.tsx src/components/Knob.test.tsx src/components/StageModule.tsx
git commit -m "feat(ui): rotary knob and processing stage module"
```

---

### Task 10: Processing section, Microphone page and flyout tag

**Files:**
- Create: `src/views/ProcessingPanel.tsx`
- Create: `src/views/ProcessingPanel.test.tsx`
- Modify: `src/views/MicView.tsx`
- Modify: `src/views/MicView.test.tsx`
- Modify: `src/components/MicKey.tsx`
- Modify: `src/views/Flyout.tsx`
- Modify: `src/views/Flyout.test.tsx`

**Interfaces:**
- Consumes: Task 8's types/helpers/`setProcessing`/`openVbCable`; Task 9's `Knob`, `StageModule`; Phase 1's `Meter`, `DbScale`, `Slider`, `Banner`, `Badge`, `Toggle`.
- Produces: `ProcessingPanel({ state })`; `MicKey` prop `processed?: boolean`.

- [ ] **Step 1: Write the failing panel tests**

`src/views/ProcessingPanel.test.tsx`:

```tsx
import { fireEvent, render, screen, within } from "@testing-library/react";
import { ProcessingPanel } from "./ProcessingPanel";
import { makeDevice, makeProcessing, makeState } from "../test/fixtures";
import * as api from "../lib/api";

vi.mock("../lib/api", () => ({
  send: vi.fn().mockResolvedValue(undefined),
  openVbCable: vi.fn().mockResolvedValue(undefined),
}));

/** The settings object sent by the last setProcessing command. */
function lastSent() {
  const cmd = vi.mocked(api.send).mock.calls.at(-1)![0];
  if (cmd.type !== "setProcessing") throw new Error(`unexpected ${cmd.type}`);
  return cmd.processing;
}

const stage = (name: string) => screen.getByRole("region", { name });

describe("ProcessingPanel", () => {
  beforeEach(() => {
    vi.mocked(api.send).mockClear();
    vi.mocked(api.openVbCable).mockClear();
  });

  it("offers VB-Cable when it isn't installed", () => {
    render(<ProcessingPanel state={makeState({ processing: makeProcessing({ cableInstalled: false }) })} />);
    expect(screen.queryByRole("slider")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Get VB-Cable" }));
    expect(api.openVbCable).toHaveBeenCalled();
  });

  it("turns processing on", () => {
    const s = makeState();
    render(<ProcessingPanel state={s} />);
    fireEvent.click(screen.getByRole("button", { name: "Turn processing on" }));
    expect(lastSent()).toEqual({ ...s.processing.settings, enabled: true });
  });

  it("says what apps hear, and why processing is paused", () => {
    const { rerender } = render(<ProcessingPanel state={makeState({ processing: makeProcessing({ status: { state: "running" } }) })} />);
    expect(screen.getByRole("status")).toHaveTextContent(/apps hear your processed mic/i);
    rerender(<ProcessingPanel state={makeState({ processing: makeProcessing({ status: { state: "paused", reason: "Desk Mic was unplugged" } }) })} />);
    expect(screen.getByRole("status")).toHaveTextContent("Desk Mic was unplugged");
  });

  it("adjusts the gate threshold and turns a stage off", () => {
    render(<ProcessingPanel state={makeState()} />);
    fireEvent.keyDown(within(stage("Noise gate")).getByRole("slider", { name: "Threshold" }), { key: "ArrowUp" });
    expect(lastSent().dsp.gate.thresholdDb).toBe(-44);
    fireEvent.click(within(stage("Noise suppression")).getByRole("switch", { name: "Noise suppression" }));
    expect(lastSent().dsp.denoise.enabled).toBe(false);
  });

  it("sets compressor threshold and ratio from the amount", () => {
    render(<ProcessingPanel state={makeState()} />);
    fireEvent.keyDown(within(stage("Compressor")).getByRole("slider", { name: "Amount" }), { key: "ArrowUp" });
    const c = lastSent().dsp.compressor;
    expect(c.amount).toBeCloseTo(0.55);
    expect(c.custom).toBe(false);
    expect(c.thresholdDb).toBeCloseTo(-23.75);
    expect(c.ratio).toBeCloseTo(3.975);
  });

  it("reveals advanced controls; editing the threshold makes the compressor custom", () => {
    render(<ProcessingPanel state={makeState()} />);
    const comp = stage("Compressor");
    expect(within(comp).queryByRole("slider", { name: "Ratio" })).toBeNull();
    fireEvent.click(within(comp).getByRole("button", { name: "Advanced" }));
    expect(within(comp).getByRole("slider", { name: "Ratio" })).toHaveAttribute("aria-valuetext", "3.8:1");
    fireEvent.keyDown(within(comp).getByRole("slider", { name: "Threshold" }), { key: "ArrowUp" });
    expect(lastSent().dsp.compressor).toMatchObject({ custom: true, thresholdDb: -22, ratio: 3.75 });
  });

  it("switches makeup between automatic and manual", () => {
    render(<ProcessingPanel state={makeState()} />);
    const comp = stage("Compressor");
    fireEvent.click(within(comp).getByRole("button", { name: "Advanced" }));
    const auto = within(comp).getByRole("button", { name: "Auto" });
    expect(auto).toHaveAttribute("aria-pressed", "true");
    fireEvent.click(auto);
    expect(lastSent().dsp.compressor.makeupDb).toBe(1.5);
  });

  it("turns on monitoring", () => {
    render(<ProcessingPanel state={makeState()} />);
    fireEvent.click(screen.getByRole("switch", { name: "Hear yourself" }));
    expect(lastSent().monitor).toBe(true);
  });

  it("picks a source but never offers CABLE Output", () => {
    const s = makeState();
    const name = "CABLE Output (VB-Audio Virtual Cable)";
    s.devices.push(makeDevice({ id: "cout", name, originalName: name, flow: "capture" }));
    render(<ProcessingPanel state={s} />);
    const source = screen.getByRole("combobox", { name: "Source" });
    expect(within(source).queryByRole("option", { name })).toBeNull();
    fireEvent.change(source, { target: { value: "mic1" } });
    expect(lastSent().sourceId).toBe("mic1");
  });
});
```

Run: `npm.cmd test -- src/views/ProcessingPanel.test.tsx`
Expected: FAIL (`./ProcessingPanel` doesn't exist).

- [ ] **Step 2: Write `src/views/ProcessingPanel.tsx`**

```tsx
import { Power } from "lucide-react";
import { openVbCable } from "../lib/api";
import { amountCurve, autoMakeup, compressorCurve, processingSources } from "../lib/processing";
import { setProcessing } from "../lib/store";
import type { CompressorParams, DspParams, EngineStatus, GateParams, LimiterParams, ProcessingSettings, UiState } from "../lib/types";
import { DbScale } from "../components/DbScale";
import { Knob } from "../components/Knob";
import { Meter } from "../components/Meter";
import { Slider } from "../components/Slider";
import { StageModule } from "../components/StageModule";
import { Banner, Toggle } from "../components/ui";

const round1 = (v: number) => Math.round(v * 10) / 10;
const pct = (v: number) => `${Math.round(v * 100)}%`;
const db = (v: number) => `${v > 0 ? "+" : ""}${round1(v)} dB`;
const ms = (v: number) => `${round1(v)} ms`;
const ratio = (v: number) => `${round1(v)}:1`;

function statusText(status: EngineStatus): string {
  switch (status.state) {
    case "running":
      return "On. Apps hear your processed mic through CABLE Output";
    case "paused":
      return `Paused: ${status.reason}`;
    case "noCable":
      return "VB-Cable isn't available";
    case "off":
      return "Off. Apps hear your mic directly";
  }
}

/** The processed-mic console: source, master key, four stages, before/after meters and monitoring. */
export function ProcessingPanel({ state }: { state: UiState }) {
  const { settings: p, status, cableInstalled } = state.processing;
  const save = (patch: Partial<ProcessingSettings>) => setProcessing({ ...p, ...patch });
  const dsp = (patch: Partial<DspParams>) => save({ dsp: { ...p.dsp, ...patch } });
  const gate = (patch: Partial<GateParams>) => dsp({ gate: { ...p.dsp.gate, ...patch } });
  const comp = (patch: Partial<CompressorParams>) => dsp({ compressor: { ...p.dsp.compressor, ...patch } });
  const limiter = (patch: Partial<LimiterParams>) => dsp({ limiter: { ...p.dsp.limiter, ...patch } });

  if (!cableInstalled) {
    return (
      <section aria-label="Processing" className="console flex flex-col items-start gap-4 px-7 py-6">
        <h2 className="m-0 text-[18px] font-medium">Processing</h2>
        <Banner>
          Processing sends your cleaned-up mic to apps through VB-Cable, a free virtual audio cable. Install it, restart your PC, and
          Resonance will find it automatically.
        </Banner>
        <button type="button" className="key key-lit h-9 px-4 text-[13px]" onClick={() => void openVbCable()}>
          Get VB-Cable
        </button>
      </section>
    );
  }

  const c = p.dsp.compressor;
  const curve = compressorCurve(c);
  const g = p.dsp.gate;
  const l = p.dsp.limiter;

  return (
    <section aria-label="Processing" className="console flex flex-col gap-6 px-7 py-6">
      <div className="flex flex-wrap items-center gap-4">
        <button
          type="button"
          aria-pressed={p.enabled}
          aria-label={p.enabled ? "Turn processing off" : "Turn processing on"}
          onClick={() => save({ enabled: !p.enabled })}
          className={`grid h-12 w-12 shrink-0 place-items-center rounded-xl border ${p.enabled ? "mic-live text-lilac" : "border-groove bg-raised text-fog"}`}
        >
          <Power size={20} strokeWidth={1.8} />
        </button>
        <div className="min-w-0 flex-1">
          <h2 className="m-0 text-[18px] font-medium">Processing</h2>
          <p role="status" className={`m-0 text-[13px] ${status.state === "paused" ? "text-coral" : "text-fog"}`}>
            {statusText(status)}
          </p>
        </div>
        <label htmlFor="proc-source" className="text-[13px] text-fog">
          Source
        </label>
        <select
          id="proc-source"
          className="field-select max-w-[280px]"
          value={p.sourceId ?? ""}
          onChange={(e) => save({ sourceId: e.currentTarget.value || null })}
        >
          <option value="">Automatic (your usual mic)</option>
          {processingSources(state).map((d) => (
            <option key={d.id} value={d.id}>
              {d.name}
            </option>
          ))}
        </select>
      </div>

      <div className="grid grid-cols-[repeat(auto-fit,minmax(210px,1fr))] gap-4">
        <StageModule
          title="Noise suppression"
          enabled={p.dsp.denoise.enabled}
          onToggle={(enabled) => dsp({ denoise: { ...p.dsp.denoise, enabled } })}
        >
          <Knob
            label="Strength"
            value={p.dsp.denoise.strength}
            min={0}
            max={1}
            step={0.05}
            format={pct}
            onChange={(strength) => dsp({ denoise: { ...p.dsp.denoise, strength } })}
          />
        </StageModule>

        <StageModule
          title="Noise gate"
          enabled={g.enabled}
          onToggle={(enabled) => gate({ enabled })}
          advanced={
            <>
              <Knob size="small" label="Attack" value={g.attackMs} min={0.5} max={50} step={0.5} format={ms} onChange={(attackMs) => gate({ attackMs })} />
              <Knob size="small" label="Hold" value={g.holdMs} min={0} max={1000} step={10} format={ms} onChange={(holdMs) => gate({ holdMs })} />
              <Knob size="small" label="Release" value={g.releaseMs} min={10} max={1000} step={10} format={ms} onChange={(releaseMs) => gate({ releaseMs })} />
              <Knob size="small" label="Range" value={g.rangeDb} min={-80} max={0} step={1} format={db} onChange={(rangeDb) => gate({ rangeDb })} />
            </>
          }
        >
          <Knob label="Threshold" value={g.thresholdDb} min={-80} max={-10} step={1} format={db} onChange={(thresholdDb) => gate({ thresholdDb })} />
        </StageModule>

        <StageModule
          title="Compressor"
          enabled={c.enabled}
          onToggle={(enabled) => comp({ enabled })}
          advanced={
            <>
              <Knob
                size="small"
                label="Threshold"
                value={curve.thresholdDb}
                min={-60}
                max={0}
                step={0.5}
                format={db}
                onChange={(thresholdDb) => comp({ custom: true, thresholdDb, ratio: curve.ratio })}
              />
              <Knob
                size="small"
                label="Ratio"
                value={curve.ratio}
                min={1}
                max={20}
                step={0.5}
                format={ratio}
                onChange={(r) => comp({ custom: true, thresholdDb: curve.thresholdDb, ratio: r })}
              />
              <Knob size="small" label="Attack" value={c.attackMs} min={0.1} max={100} step={0.1} format={ms} onChange={(attackMs) => comp({ attackMs })} />
              <Knob size="small" label="Release" value={c.releaseMs} min={10} max={1000} step={10} format={ms} onChange={(releaseMs) => comp({ releaseMs })} />
              <div className="flex flex-col items-center gap-1.5">
                <Knob size="small" label="Makeup" value={c.makeupDb ?? round1(autoMakeup(c))} min={-12} max={24} step={0.5} format={db} onChange={(makeupDb) => comp({ makeupDb })} />
                <button
                  type="button"
                  aria-pressed={c.makeupDb === null}
                  onClick={() => comp({ makeupDb: c.makeupDb === null ? Math.round(autoMakeup(c) * 2) / 2 : null })}
                  className={`key h-6 px-2 text-[11px] ${c.makeupDb === null ? "key-lit" : ""}`}
                >
                  Auto
                </button>
              </div>
            </>
          }
        >
          <Knob
            label="Amount"
            value={c.amount}
            min={0}
            max={1}
            step={0.05}
            format={(v) => (c.custom ? "Custom" : pct(v))}
            onChange={(amount) => comp({ amount, custom: false, ...amountCurve(amount) })}
          />
        </StageModule>

        <StageModule
          title="Limiter"
          enabled={l.enabled}
          onToggle={(enabled) => limiter({ enabled })}
          advanced={<Knob size="small" label="Release" value={l.releaseMs} min={5} max={500} step={5} format={ms} onChange={(releaseMs) => limiter({ releaseMs })} />}
        >
          <Knob label="Ceiling" value={l.ceilingDb} min={-12} max={0} step={0.5} format={db} onChange={(ceilingDb) => limiter({ ceilingDb })} />
        </StageModule>
      </div>

      <div className="grid gap-6 border-t border-groove pt-6 md:grid-cols-2">
        <div className="flex flex-col gap-2">
          <div className="flex items-center gap-3">
            <span className="w-12 text-[12px] text-fog">Before</span>
            <Meter source="procIn" className="h-2.5!" />
          </div>
          <div className="flex items-center gap-3">
            <span className="w-12 text-[12px] text-fog">After</span>
            <Meter source="procOut" className="h-2.5!" />
          </div>
          <div className="pl-15">
            <DbScale kind="meter" orientation="horizontal" />
          </div>
        </div>
        <div className="flex items-center gap-4">
          <Toggle checked={p.monitor} onChange={(monitor) => save({ monitor })} label="Hear yourself" />
          <div className="min-w-0 flex-1">
            <div className="mb-1.5 flex items-baseline justify-between">
              <span className="text-[13px] text-fog">Hear yourself</span>
              <span className="num text-[14px]">{pct(p.monitorLevel)}</span>
            </div>
            <Slider value={p.monitorLevel} label="Monitoring level" disabled={!p.monitor} onChange={(monitorLevel) => save({ monitorLevel })} />
          </div>
        </div>
      </div>
    </section>
  );
}
```

- [ ] **Step 3: Run the panel tests**

Run: `npm.cmd test -- src/views/ProcessingPanel.test.tsx`
Expected: 9 tests PASS.

- [ ] **Step 4: Host the panel on the Microphone page**

In `src/views/MicView.tsx`:
- add imports `import { isProcessed } from "../lib/processing";` and `import { ProcessingPanel } from "./ProcessingPanel";`
- replace

```tsx
      <ViewHeader title="Microphone" subtitle={`${mic.name} is the mic every app hears by default`} />
```

with

```tsx
      <ViewHeader
        title="Microphone"
        subtitle={isProcessed(state) ? "Apps hear your mic through Resonance's processing" : `${mic.name} is the mic every app hears by default`}
      />
```

- replace the end of the component

```tsx
        </Panel>
      </div>
    </div>
  );
}
```

with

```tsx
        </Panel>
      </div>
      <ProcessingPanel state={state} />
    </div>
  );
}
```

In `src/views/MicView.test.tsx`, add `openVbCable: vi.fn().mockResolvedValue(undefined),` to the `vi.mock("../lib/api", ...)` object, add `makeProcessing` to the fixtures import, and add:

```tsx
  it("shows the processing section and says when apps hear the processed mic", () => {
    render(<MicView state={makeState({ processing: makeProcessing({ status: { state: "running" } }) })} />);
    expect(screen.getByRole("region", { name: "Processing" })).toBeInTheDocument();
    expect(screen.getByText(/apps hear your mic through resonance's processing/i)).toBeInTheDocument();
  });
```

- [ ] **Step 5: Tag the flyout mic key**

In `src/components/MicKey.tsx`:
- change the import `import { Keycaps } from "./ui";` to `import { Badge, Keycaps } from "./ui";`
- change the signature to:

```tsx
export function MicKey({
  mic,
  hotkey,
  size = "compact",
  processed = false,
}: {
  mic: Device | undefined;
  hotkey: string | null;
  size?: "compact" | "wide";
  /** Show the "Processed" tag (apps hear the processed mic). */
  processed?: boolean;
}) {
```

- replace `<span className="text-[13px] font-medium text-paper">{live ? "Mic live" : "Mic muted"}</span>` with:

```tsx
        <span className="flex items-center gap-1.5 text-[13px] font-medium text-paper">
          {live ? "Mic live" : "Mic muted"}
          {processed && <Badge tone="accent">Processed</Badge>}
        </span>
```

In `src/views/Flyout.tsx`, add `import { isProcessed } from "../lib/processing";` and change `<MicKey mic={mic} hotkey={state.mic.hotkey} size="wide" />` to `<MicKey mic={mic} hotkey={state.mic.hotkey} size="wide" processed={isProcessed(state)} />`.

In `src/views/Flyout.test.tsx`, add `makeProcessing` to the fixtures import and add:

```tsx
  it("tags the mic key while processing is running", () => {
    useStore.setState({ state: makeState() });
    const { unmount } = render(<Flyout />);
    expect(screen.queryByText("Processed")).toBeNull();
    unmount();
    useStore.setState({ state: makeState({ processing: makeProcessing({ status: { state: "running" } }) }) });
    render(<Flyout />);
    expect(screen.getByText("Processed")).toBeInTheDocument();
  });
```

- [ ] **Step 6: Run everything on the frontend**

Run: `npm.cmd test`
Then: `npm.cmd run build`
Expected: all tests PASS; type check and Vite build succeed.

- [ ] **Step 7: Look at it**

With the installed Resonance quit, run `npm.cmd run tauri dev`, open the Microphone tab and check against Design v2: the Processing console sits under the input channel; four stage cards in a row (wrapping on narrow windows); knobs turn by drag and keys; Advanced drawers open; Before/After meters move when processing runs; without VB-Cable the section shows the install message and the "Get VB-Cable" key opens the download page. Fix any layout problems before committing.

- [ ] **Step 8: Commit**

```bash
git add src
git commit -m "feat(ui): Processing console on the Microphone page and a Processed tag on the flyout mic key"
```

---

### Task 11: Verification, checklist and installer

**Files:**
- Modify: `docs/testing/phase1-manual-checklist.md`
- Modify: `README.md`

- [ ] **Step 1: Full automated run**

Run, in order:
- `cargo clippy --manifest-path M:\Resonance\src-tauri\Cargo.toml --all-targets -- -D warnings`
- `cargo test --manifest-path M:\Resonance\src-tauri\Cargo.toml`
- `npm.cmd test`
- `npm.cmd run build`

Expected: no warnings, every test PASSES (hardware tests ignored).

- [ ] **Step 2: Hardware tests (when VB-Cable is installed)**

With VB-Cable installed and the installed Resonance quit:
`cargo test --manifest-path M:\Resonance\src-tauri\Cargo.toml --test engine_hw -- --ignored --test-threads=1`
Expected: 2 PASS. If VB-Cable isn't installed yet, record "pending VB-Cable" in the checklist and in the hand-off.

- [ ] **Step 3: Add the Phase 3a manual checklist**

Append to `docs/testing/phase1-manual-checklist.md`:

```markdown
## Phase 3a — Processed mic (needs VB-Cable from https://vb-audio.com/Cable/)

- [ ] Without VB-Cable: Microphone page shows the install message; "Get VB-Cable" opens the download page; everything else works.
- [ ] Turn Processing on: Windows Sound settings show "CABLE Output" as default and communications mic; the flyout mic key shows "Processed".
- [ ] Discord/Teams voice test with "Default" input: voice is clear, fan/keyboard noise suppressed, no echo, no crackle over 10 minutes.
- [ ] Gate: silence between words is quiet; raising the threshold cuts more; Advanced Hold/Release behave.
- [ ] Compressor: loud and soft speech end up closer in level; Amount 0 vs 100 is clearly different; Auto makeup toggles.
- [ ] Limiter: shouting never clips in a recording (Audacity on CABLE Output).
- [ ] Monitoring: "Hear yourself" plays the processed mic in the headphones; level slider works; off by default.
- [ ] Mute key/shortcut mutes what apps hear while processing is on.
- [ ] Unplug the source mic: status shows "Paused" with a reason; plug it back in: processing resumes within a few seconds.
- [ ] Turn Processing off: the previous default and communications mics come back.
- [ ] Quit from the tray while processing: previous mics come back; relaunch: processing turns itself back on.
- [ ] Kill Resonance in Task Manager while processing, relaunch, turn Processing off: the original mics come back.
- [ ] Change the default mic in Windows while processing is on, then turn Processing off: your manual choice is kept.
```

- [ ] **Step 4: README**

In `README.md`, add a "Processed microphone" bullet to the feature list: AI noise suppression, gate, compressor and limiter on your mic, delivered to every app through the free VB-Cable driver (install it from https://vb-audio.com/Cable/); optional monitoring.

- [ ] **Step 5: Build the installer**

Run: `npm.cmd run tauri build`
Expected: `src-tauri\target\release\bundle\nsis\Resonance_0.1.0_x64-setup.exe` is rebuilt.

- [ ] **Step 6: Commit**

```bash
git add docs/testing/phase1-manual-checklist.md README.md
git commit -m "docs: Phase 3a checklist and README"
```

- [ ] **Step 7: Hand-off**

Tell the user: install the new installer and VB-Cable (restart after), then walk the Phase 3a checklist; mention any hardware tests still pending. Offer to merge to `main`.

---

## Execution notes (2026-10-08)

Deviations found while building, all committed:

- **VB-Cable names:** Pack45 installs "CABLE Input", "CABLE In 16ch" and briefly showed "Speakers (VB-Audio Virtual Cable)". `find_cable` accepts any playback endpoint ending "(VB-Audio Virtual Cable)", preferring "CABLE Input", then a non-16-channel one (test `recognises_newer_vb_cable_names`).
- **Installing VB-Cable makes it the Windows default playback and recording device.** The user's mic was put back by hand; speakers are the user's choice.
- **Output cushion is adaptive:** measured on real hardware, the mic's packets and the audio engine's pulls jitter by ~20 ms, so a fixed 20 ms queue ran dry. Playback now starts after the first captured audio; each real dry-out grows that output's target by 5 ms (max 50 ms) and rebuilds it at once with silence ahead of the next audio; queues over 60 ms are trimmed (`Feed::{grow_target, discard}`, `Fifo::discard`). 60 s run: 2 start-up gaps, 2 more while settling, none after.
- **Hardware test** also checks audio reaches CABLE Output and counts dropouts only after a 1 s warm-up.
- **Shutdown** refreshes once more before saving, so last-moment changes are kept.
- Clippy 1.99 wants `as_chunks_mut` instead of `chunks_exact_mut` with constant sizes.
- UI polish after a live look: knob focus ring only for keyboard focus; stage cards keep their own heights.
