# Resonance Phase 3b — Call mix (apps into your mic)

**Date:** 2026-10-08
**Status:** Design approved in brainstorming; awaiting spec review
**Builds on:** Phase 3a processed mic (`2026-10-08-phase3a-processed-mic-design.md`). The parent spec's
"stream mix" is narrowed to the user's actual need: sending app audio into calls.

## 1. Goal

Let chosen apps (music, a game, a soundboard) be heard in calls together with the processed mic, each at its own
level, with automatic ducking while the user talks. The user keeps hearing those apps exactly as before.

## 2. Scope

**In scope**
- Per-app "send to call" with a send level, on each Mixer app strip.
- A Call mix section in Processing (Microphone page): master "Apps in call" level, ducking on/off and amount, the
  list of sent apps.
- Mixing app audio into what the processed-mic engine writes to VB-Cable ("CABLE Input" → apps hear "CABLE Output").
- Ducking keyed by the user's voice.
- Sends remembered per app (by app key), like volumes.

**Out of scope (YAGNI)**
A separate OBS/stream output or a second virtual cable; sending the mic anywhere else; per-app EQ or processing
of sent apps; per-send meters; Windows 10 versions older than 2004 (no process loopback there).

## 3. Approach

Per-app **process loopback capture** (`ActivateAudioInterfaceAsync` on `VAD\Process_Loopback` with
`AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK`, `PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE`), opened inside
the existing engine thread. This captures one process (and its children) without changing its routing, so the user
still hears it. Rejected: routing sent apps to a second virtual device (the user would stop hearing them and it
fights per-app output routing); capturing the whole output (can't isolate apps).

## 4. Signal path

```
CABLE Input = limiter( chain(mic) + duck × master × Σ send_i × app_i )
```

- `chain(mic)` is 3a's denoise → gate → compressor; app audio joins **after** them (music isn't noise-suppressed,
  gated or compressed) and **before** the limiter, which still guarantees the ceiling. The limiter stage is
  therefore always applied to the summed signal when any app is sent, even if the user turned the limiter stage off
  (then a fixed −1 dBFS safety limiter is used).
- Each app source is captured as 48 kHz float stereo and downmixed to mono, matching the engine.
- Monitoring ("Hear yourself") keeps playing the processed mic only; sent apps are already audible to the user.

## 5. Sources and clocks

- Each sent app is captured per process ID listed in its audio sessions; with "include process tree", a PID that is a
  descendant of another listed PID is skipped so audio isn't counted twice.
- A loopback capture runs on the playback device's clock, not the mic's. Each source therefore gets an input queue
  with drift control (the 3a `Fifo` + `DriftController` + rubato resampler, steered by the queue level so it follows
  the mic clock). Target queue 20 ms, adaptive like 3a outputs.
- A silent or paused app delivers no packets; its queue simply runs empty and contributes silence. It never blocks
  the engine; the engine stays driven by the mic's capture events.
- Sources can be added and removed while the engine runs (no restart, no glitch on the mic path). Opening a capture
  happens off the real-time thread; the engine receives ready sources over the existing parameter channel.

## 6. Ducking

- Voice detector on the processed mic: envelope follower, "talking" when above the gate threshold (or −45 dBFS when
  the gate is off), 10 ms attack, 300 ms hold, 300 ms release.
- Duck gain moves smoothly between 0 dB and the duck amount (default −12 dB, range −40…0 dB).
- Settings: ducking on/off (default on), amount.

## 7. Settings and defaults

| Setting | Default | Range |
|---|---|---|
| Per-app send on/off | off | — |
| Per-app send level | 70 % | 0–100 % |
| Master "Apps in call" | 100 % | 0–100 % |
| Ducking | on, −12 dB | −40…0 dB |

Sends are stored per app key in `settings.json` (an app that isn't playing keeps its send; it starts when the app
plays). Master and ducking live in the processing settings.

## 8. UI (Design v2)

- **Mixer app strips:** a small "Call" key under each strip's mute key, lit violet when sent, plus a small send-level
  knob that appears when sent. System sounds can't be sent.
- **Microphone page → Processing → Call mix module:** master level fader, ducking switch + amount knob, and chips
  for each sent app (icon + name; click to stop sending).
- **Without processing on:** Call keys are dimmed with a hint "Turn on Processing to send apps into calls".
- **Echo warning:** sending Discord, Teams, Zoom, Skype or Slack shows a toast: "Sending a call app into the call
  makes everyone hear themselves."

## 9. Error handling

- A source that fails to open (old Windows, app exited, access denied) is skipped with a notice; the mic path keeps
  running. On Windows without process loopback, the Call keys are hidden and the module explains why.
- An app that exits drops its source; when it plays again (new PID), the send reconnects on the next refresh.
- Engine restarts (device change, retry) reopen all sources.

## 10. Testing

- **Offline DSP:** mixing sums sends at their levels; ducking reaches the duck amount within ~10 ms of voice and
  recovers after hold + release; the limiter ceiling holds with loud apps; no app audio when nothing is sent.
- **Source queue:** a ±0.3 % clock mismatch stays within target ±5 ms; an empty source produces silence without
  underrun counting.
- **Controller (fake engine):** sends follow app sessions and PIDs (descendant PIDs skipped), persist across
  sessions, are dropped when Processing is off, reconnect when an app reappears; echo warning for call apps.
- **Hardware (`#[ignore]`):** a test process plays a tone; with that process sent, the tone appears on CABLE Output;
  with it unsent, it doesn't.
- **Frontend:** Call key and send knob on strips, Call mix module (master, ducking, chips), dimmed state when
  Processing is off.
