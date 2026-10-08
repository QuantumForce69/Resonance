# Resonance Phase 3a — Processed virtual microphone

**Date:** 2026-10-08
**Status:** Design approved in brainstorming; awaiting spec review
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

New dependencies: `nnnoiseless` 0.5.2 (pure-Rust RNNoise, BSD-3), `rubato` 5.0.1 (resampling, MIT).

## 5. Architecture

```
src-tauri/src/engine/
├─ mod.rs          EngineHandle (start/stop/configure, status), EngineConfig, EngineStatus
├─ dsp/            pure, allocation-free processing on 48 kHz mono f32 blocks
│  ├─ denoise.rs   nnnoiseless wrapper, 480-sample frames, strength blend
│  ├─ gate.rs      envelope follower + hysteresis, attack/hold/release, range
│  ├─ compressor.rs feed-forward RMS compressor, soft knee, makeup
│  ├─ limiter.rs   peak limiter with 1.5 ms look-ahead, hard ceiling guarantee
│  └─ chain.rs     Chain { stages in fixed order }, per-stage bypass, before/after peak taps
├─ drift.rs        ring buffer + fill-level controller that nudges the resampler ratio (clock-drift compensation)
├─ io.rs           trait AudioIo (capture/render/monitor streams); WASAPI implementation + fake for tests
└─ thread.rs       real-time loop: capture → (resample to 48k) → chain → (resample) → render + monitor
```

- **Real-time rules:** the audio thread registers with MMCSS ("Pro Audio"), never allocates, never locks.
  Parameter changes arrive through a lock-free single-producer queue of `ParamChange` messages; meter peaks
  leave through atomics read by the existing 30 Hz meter loop.
- **Format:** the chain runs at 48 kHz mono f32. Capture is down-mixed to mono; render/monitor are up-mixed to
  the device's channel count. Devices not at 48 kHz go through `rubato`.
- **Clock drift:** source mic and VB-Cable run on independent clocks. A ring buffer between chain and render
  targets ~20 ms fill; a slow PI controller adjusts the output resampler ratio by at most ±0.5 % to hold it, so
  latency stays bounded and there are no clicks from over/underruns.
- **Latency target:** ≤ 30 ms mic-to-cable at 10 ms device periods (RNNoise adds 10 ms framing; limiter 1.5 ms).
- **Integration with Phase 1:** the engine is owned by a new `EngineHandle` managed by Tauri, separate from
  the WASAPI worker. Turning processing on/off goes through the existing `AudioBackend::set_default_device`
  (via the worker) to switch defaults, and remembers the prior defaults in `settings.json` so a crash can be
  undone on next launch. Because "CABLE Output" becomes the default mic, Phase 1's mic key, mute shortcut,
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
- Engine thread panic: caught at the thread boundary, status `Failed { message }`, defaults restored, a toast.
- Startup after a crash: if `settings.json` says processing was on with remembered prior defaults but the
  engine is not running yet, restore defaults first, then start the engine if processing is enabled.
- Default switching failures are reported as toasts; processing still runs (the user can pick CABLE Output
  manually).

## 9. Testing

- **DSP unit tests (offline, synthetic signals):** gate closes below threshold and opens above with the
  configured timing; compressor output level follows the ratio above threshold; limiter output never exceeds the
  ceiling on full-scale square waves and transients; noise suppression reduces white-noise energy by > 10 dB
  while a speech-like tone burst keeps > 80 % energy at strength 100 %, and is transparent at 0 %; each stage's
  bypass is bit-exact passthrough; no NaN/denormal output on silence.
- **Drift controller tests:** simulated source/sink rate mismatch of ±0.3 % for 10 minutes of audio keeps
  buffer fill within ±5 ms of target with no under/overrun.
- **Engine tests with a fake `AudioIo`:** start/stop, parameter changes applied, pause/resume on device loss,
  meters published.
- **Hardware tests (`#[ignore]`):** with VB-Cable installed, play a tone into the engine source path and record
  CABLE Output; verify level, measured round-trip latency ≤ 40 ms (the 30 ms engine target plus measurement overhead), no dropouts over 30 s; verify defaults switch and restore.
- **Frontend:** Processing section states (no cable, off, on, paused), stage controls send the right commands,
  Advanced drawers, monitoring.
