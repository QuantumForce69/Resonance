# Resonance Phase 3a — Processed virtual microphone

**Date:** 2026-10-08
**Status:** Approved; implementation plan `docs/superpowers/plans/2026-10-08-phase3a-processed-mic.md`
**Parent spec:** `2026-10-06-resonance-design.md` (Phase 3). Phase 3 is split: **3a** (this spec) is the
processed virtual mic with monitoring; **3b** (later, own spec) is the per-app stream mix.

## 1. Goal

Clean up the user's microphone in real time and offer it to every app as a virtual microphone:
noise suppression → noise gate → compressor → limiter, with optional monitoring in the headphones.

## 2. Scope

**In scope**
- Source: any active capture device (default: the user's Behringer, i.e. the default mic at the time processing
  is first enabled).
- Stages, each with an on/off key, one main control, and an Advanced drawer:

  | Stage | Main control | Advanced |
  |---|---|---|
  | Noise suppression (RNNoise via `nnnoiseless`) | Strength 0–100 % (wet/dry blend of denoised vs. original) | — |
  | Noise gate | Threshold (dBFS) | Attack, hold, release, range (attenuation when closed) |
  | Compressor | Amount 0–100 % (maps to threshold + ratio; see §6) | Threshold, ratio, attack, release, makeup gain |
  | Limiter | Ceiling (dBFS) | Release |

- Output to VB-Cable ("CABLE Input" render endpoint); apps record "CABLE Output".
- When processing is **on**, Resonance makes "CABLE Output" the Windows default and communications mic; when
  it is turned **off** (or Resonance exits), it restores the previously remembered default and communications mics —
  but only for roles still pointing at CABLE Output, so a manual change the user made meanwhile is respected.
- Monitoring: hear the processed signal on the default output device, on/off + level.
- Before/after level meters.
- Settings persisted in `settings.json`.

**Out of scope (YAGNI)**
EQ (user declined), stream mix and per-app capture (Phase 3b), presets/profiles (Phase 4), VST plugins,
multiple simultaneous processed mics, bundling or installing VB-Cable.

## 3. Prerequisite

VB-Cable (free, vb-audio.com) installed by the user; it creates render endpoint "CABLE Input" and capture
endpoint "CABLE Output". Resonance finds them by friendly name containing `CABLE Input` / `CABLE Output`
(excluding "VB-Audio Voicemeeter" endpoints). If missing, the Processing section explains how to install it and
everything else in Resonance works as before.

## 4. Approach

An in-process Rust audio engine on its own real-time thread, using WASAPI shared-mode, event-driven capture
and render clients. Rejected: a driver-level APO (fragile, registry surgery, breaks on driver updates) and
chaining third-party tools (not integrated).

New dependencies: `nnnoiseless` 0.5.2 (pure-Rust RNNoise, BSD-3; default features off) and `rubato` 5.0.1 (MIT;
its `Async` polynomial resampler is used only to follow clock drift). Windows' shared-mode format conversion
(`AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM`) presents every device as 48 kHz float, so Resonance does no fixed-rate
resampling itself.

## 5. Architecture

```
src-tauri/src/
├─ engine/
│  ├─ mod.rs        AudioEngine trait, Route, EngineParams, NullEngine
│  ├─ config.rs     ProcessingSettings, DspParams (+ per-stage params), EngineStatus, ProcessingView
│  ├─ dsp/          pure, allocation-free processing of 48 kHz mono f32 in 480-sample blocks
│  │  ├─ denoise.rs  nnnoiseless wrapper with strength blend (dry path delayed one frame to match RNNoise)
│  │  ├─ gate.rs     peak detector + hysteresis, attack/hold/release, range
│  │  ├─ compressor.rs feed-forward RMS compressor, 6 dB soft knee, makeup
│  │  ├─ limiter.rs  1.5 ms look-ahead peak limiter with a hard ceiling guarantee
│  │  └─ chain.rs    fixed-order chain, per-stage bypass, before/after peaks
│  ├─ fifo.rs       fixed-capacity sample queue (never allocates after creation)
│  ├─ drift.rs      PI controller that steers the resampling ratio from the output queue level
│  └─ feed.rs       one output destination: rubato resampler + queue + drift controller
├─ win/engine.rs    WasapiEngine: event-driven capture, push-mode renders, MMCSS "Pro Audio" thread
└─ app_state/processing.rs  reconciles settings, VB-Cable presence, Windows default mics and the engine
```

- **Real-time rules:** the engine thread registers with MMCSS ("Pro Audio"), never allocates and never locks while
  running. Parameter changes arrive over a channel whose receive side is lock-free (`std::sync::mpsc`); peaks leave
  through atomics read by the existing 30 Hz meter loop.
- **Format:** capture is opened as 48 kHz mono float, renders as 48 kHz stereo float; Windows converts to and from the
  devices' real formats.
- **Clock drift:** source mic and VB-Cable run on independent clocks. Each output keeps ~20 ms queued (its own queue
  plus the device buffer); a PI controller adjusts that output's resampling ratio by at most ±0.5 % to hold the level,
  so latency stays bounded with no over/underrun clicks. Simulated against ±0.3 % clock skew it holds the queue within
  ±4 ms with no underruns.
- **Latency:** about 40 ms mic-to-cable (10 ms capture period, 10 ms RNNoise framing delay, 1.5 ms limiter
  look-ahead, ~20 ms output queue).
- **Integration with Phase 1:** the audio worker's `AppCore` owns the engine (behind the `AudioEngine` trait) and
  reconciles it on every 250 ms refresh: settings + VB-Cable presence + devices decide whether it runs and on which
  route. Default mics are switched with the existing `AudioBackend::set_default_device`, and the prior defaults are
  remembered in `settings.json` so a crash can be undone on next launch. Quitting waits (up to 3 s) for the worker
  to stop the engine and restore the defaults. Because "CABLE Output" becomes the default mic, Phase 1's mic key, mute shortcut,
  level slider and mic meter automatically act on what apps hear.

## 6. Parameters (defaults)

| Parameter | Default | Range |
|---|---|---|
| Suppression strength | 100 % | 0–100 % |
| Gate threshold | −45 dBFS | −80 to −10 |
| Gate attack / hold / release | 2 ms / 150 ms / 120 ms | 0.5–50 / 0–1000 / 10–1000 ms |
| Gate range | −60 dB | −80 to 0 |
| Compressor amount | 50 % | 0–100 % → threshold −10…−35 dBFS, ratio 1.5…6 |
| Compressor attack / release | 5 ms / 80 ms | 0.1–100 / 10–1000 ms |
| Compressor makeup | auto (half the gain reduction at −18 dBFS input) | −12 to +24 dB |
| Limiter ceiling | −1.0 dBFS | −12 to 0 |
| Limiter release | 50 ms | 5–500 ms |
| Monitoring | off, level 70 % | — |

Moving the compressor's main "Amount" control recomputes threshold and ratio; editing them in Advanced
detaches from Amount (Amount shows "Custom").

## 7. UI (Design v2 console language)

On the Microphone page, a new **Processing** console section below the existing input channel:
- Header row: source picker (active capture devices, CABLE Output excluded) and the master **Processing** key
  (lit violet when on).
- Four stage modules in a row, each with its on/off key, main knob (rotary, like the boost knob), value
  readout, and an **Advanced** disclosure revealing its extra controls as small knobs/fields.
- **Before / After** ladder meters side by side so noise removal and gating are visible.
- **Monitoring** switch with a level fader.
- Missing VB-Cable: the section shows an explanation and a link button to vb-audio.com/Cable; controls hidden.
- Tray flyout: the mic key shows a small "Processed" tag while processing is on.

## 8. Error handling

- Source or cable device removed / format change / audio-service restart: the engine stops cleanly, status
  becomes `Paused { reason }` shown on the page, and it restarts automatically when the device returns
  (driven by the Phase 1 device notifications).
- Engine thread panic or driver error: caught at the thread boundary and treated like a lost device: status
  `Paused { reason }`, retried every 2 s.
- Startup after a crash: if `settings.json` says processing was on with remembered prior defaults but the
  engine is not running yet, restore defaults first, then start the engine if processing is enabled.
- Default switching failures are reported as notices (toasts); processing still runs (the user can pick CABLE
  Output manually).

## 9. Testing

- **DSP unit tests (offline, synthetic signals):** gate closes below threshold and opens above with the
  configured timing; compressor output level follows the ratio above threshold; limiter output never exceeds the
  ceiling on full-scale square waves and transients; noise suppression reduces fan-like (low-passed) noise by > 15 dB
  while a speech-like tone burst keeps > 80 % energy at strength 100 %, and at 0 % is the input delayed one frame; each stage's
  bypass is bit-exact passthrough; no NaN/denormal output on silence.
- **Drift tests:** a simulated ±0.3 % clock mismatch for 10 minutes keeps the queue within ±5 ms of target with no
  underruns, both for the controller alone and for a full feed with the real resampler (2 minutes).
- **Controller tests with a fake engine and fake backend:** enable/disable switch and restore default mics (only
  roles still on the cable), no-cable state, pause and retry on failure, live parameter changes without restarts,
  monitoring route, source choice, quit and crash recovery, meters.
- **Hardware tests (`#[ignore]`, need VB-Cable):** the engine processes the real default mic into CABLE Input for
  10 s with no failure and at most 2 output dropouts; enabling processing makes CABLE Output the default and
  communications mic and disabling restores the originals.
- **Frontend:** Processing section states (no cable, off, on, paused), stage controls send the right commands,
  Advanced drawers, monitoring.
