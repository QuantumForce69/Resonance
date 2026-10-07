# Resonance Phase 2 — Soundcore Space One control

**Date:** 2026-10-07
**Status:** Design approved in brainstorming; awaiting spec review
**Parent spec:** `2026-10-06-resonance-design.md` (Phase 2 row of §3)

## 1. Goal

Control the user's Soundcore Space One (model A3035) from Resonance: noise control, EQ, and battery/status,
with a quick mode switch and battery gauge in the tray flyout and full controls on a new Headphones page.

## 2. Scope

**In scope**

| Area | Controls |
|---|---|
| Noise control | Ambient mode (Noise cancelling / Transparency / Normal); noise-cancelling sub-mode Adaptive or Manual; manual noise-cancelling strength 1–5; adaptive strength 1–5 shown read-only; transparency strength 1–5; wind-noise reduction on/off |
| EQ | Preset picker (all presets the library offers for the A3035); custom 8-band EQ |
| Battery & status | Battery level (0–5 steps, as reported), charging flag, connection status, firmware version, serial number |

**Out of scope (YAGNI; the library supports them, they can be added later)**
Button configuration, ambient-mode cycle, volume limiter, dual connections, LDAC toggle, auto power-off,
auto play/pause, voice prompts, low-battery prompt, saving named custom EQ profiles, other Soundcore models.

## 3. Approach

Depend on **openscq30-lib** (github.com/oppzippy/OpenSCQ30, crate `openscq30-lib`, GPL-3.0) as a git
dependency pinned to tag **v2.12.0**. It already implements the A3035 protocol over Windows' Bluetooth RFCOMM
stack and exposes a generic API:

- `OpenSCQ30Session::new(db_path)`, `paired_devices()`, `list_devices(DeviceModel::SoundcoreA3035)`,
  `pair(PairedDevice)` (OpenSCQ30's own model↔MAC association, not Bluetooth pairing), `connect(mac)`,
  and `list_demo_devices(model)` for a simulated device.
- `OpenSCQ30Device`: `connection_status()` (watch channel), `setting(&SettingId)`, `watch_for_changes()`,
  `set_setting_values(Vec<(SettingId, Value)>)`.

Settings used (`SettingId`): `AmbientSoundMode` (`NoiseCanceling` | `Transparency` | `Normal`),
`NoiseCancelingMode` (`Adaptive` | `Custom`), `AdaptiveNoiseCanceling` (1–5, read-only),
`ManualNoiseCanceling` (1–5), `ManualTransparency` (1–5), `WindNoiseSuppression` (bool),
`PresetEqualizerProfile` (optional preset name), `VolumeAdjustments` (8 band values), battery level/charging,
firmware version, serial number. Exact `SettingId` names for battery/firmware/serial, the EQ band
frequencies, value range and precision are read from the library's `Setting` metadata at runtime (and pinned
down in the implementation plan from the v2.12.0 source), never hard-coded from memory.

**Licensing consequence:** Resonance becomes GPL-3.0. A `LICENSE` file (GPL-3.0 text) is added. Personal use;
acceptable per the parent spec.

Rejected alternatives: shelling out to the OpenSCQ30 CLI (slow, no change notifications); reimplementing the
protocol (large effort, no benefit).

## 4. Architecture

```
React UI ── Headphones page, flyout strip
   ▲ "headset" event (HeadsetState)        │ invoke("headset_command", {cmd})
Rust (Tauri)
   └─ headset/  (async task on Tauri's tokio runtime; independent of the WASAPI audio worker)
        ├─ manager.rs   HeadsetManager: discover → connect → watch → reconnect; owns the session
        ├─ device.rs    trait HeadsetDevice (wraps OpenSCQ30Device; fake in tests)
        ├─ mapping.rs   SettingId/Value ⇄ HeadsetState / HeadsetCommand  (pure, heavily tested)
        └─ model.rs     HeadsetState, HeadsetCommand, ConnectionState, Eq types
```

- **HeadsetState** (sent to the UI whenever anything changes):
  `connection: Connecting | Connected | NotFound | Disconnected { retryInSecs }`, `name`,
  `ambientMode`, `ncMode`, `manualNc`, `adaptiveNc`, `transparency`, `windNoise`,
  `battery { level: 0..=5, charging }`, `eq { presets: [String], preset: Option<String>, bands: [EqBand] }`
  where `EqBand { hz, value, min, max, step }`, `firmware`, `serial`.
- **HeadsetCommand** (UI → backend): `SetAmbientMode`, `SetNcMode`, `SetManualNc(1..=5)`,
  `SetTransparency(1..=5)`, `SetWindNoise(bool)`, `SetEqPreset(String)`, `SetEqBands([i16; 8])`, `Retry`.
  Commands are validated/clamped in `mapping.rs` before reaching the device.
- **Session database:** `%APPDATA%\Resonance\openscq30.sqlite` (OpenSCQ30's model↔MAC store).

## 5. Behaviour

- **Discovery (on launch and on Retry):** if the session already has a paired A3035, use it; otherwise
  `list_devices(SoundcoreA3035)`, take the first result, `pair` it, then connect. None found → `NotFound`.
- **Watching:** after connecting, the manager awaits `watch_for_changes()` and `connection_status()`; any
  change rebuilds `HeadsetState` via `mapping.rs` and emits the `headset` event.
- **Reconnect:** on disconnect or failed connect, retry after 5 s, doubling to a 30 s cap; state shows
  `Disconnected { retryInSecs }`. `Retry` reconnects immediately and resets the backoff.
- **EQ edits:** fader drags are throttled in the UI (as in Phase 1) and the backend coalesces to at most one
  `set_setting_values` call per 100 ms with the latest band values. Choosing a preset sends
  `PresetEqualizerProfile`; moving any band switches the preset display to "Custom".
- **Commands while not connected:** rejected with a readable error ("Space One isn't connected"), shown as a
  toast; controls are disabled in that state anyway.

## 6. UI (Design v2 console language)

- **Title bar:** new "Headphones" tab between Microphone and Settings.
- **Headphones page:**
  - Header: device name + connection state; firmware and serial in a quiet footer.
  - **Battery:** 5-cell gauge (lilac cells, coral when ≤1), charging bolt when charging. No percentage.
  - **Mode:** large three-position lever (Noise cancelling / Transparency / Normal), same construction as
    the Phase 1 mode switch.
  - **Strength:** when Noise cancelling — Adaptive / Manual toggle; Manual shows a 1–5 detent control,
    Adaptive shows the headphones' current level read-only. When Transparency — 1–5 detent control.
    Wind-noise reduction switch shown in Noise cancelling and Transparency.
  - **EQ:** preset picker + 8 vertical faders on a console surface, each with its frequency label and value
    readout; scale and range come from the band metadata.
  - Not connected: controls disabled, a banner with the state and a **Retry** key.
- **Tray flyout:** a strip below the output section — device name, 5-cell battery, and a compact
  three-way mode switch. When not connected it reads "Space One not connected".

## 7. Error handling

- Library errors map to `AudioError` (`Unavailable` for connection/timeouts, `Invalid` for rejected values,
  `Os` otherwise) and are logged.
- Bluetooth off / headphones off / out of range all surface as `Disconnected` with automatic retry; nothing
  blocks the rest of Resonance.
- If `openscq30.sqlite` is corrupt, it is renamed to `openscq30.bad-<unix>.sqlite` and recreated (same rule as
  `settings.json`).

## 8. Testing

- **Unit (Rust):** `mapping.rs` both directions (every setting, unknown/missing values, clamping, Custom-EQ
  detection) and the manager's state machine (connect, disconnect, backoff, retry, coalesced EQ) against a fake
  `HeadsetDevice`.
- **Integration (Rust, no hardware):** connect to OpenSCQ30's demo A3035 via `list_demo_devices`, read the
  state, change mode/strength/EQ and read them back.
- **Hardware (`#[ignore]`):** connect to the real Space One, read battery/firmware, toggle ambient mode and
  restore it.
- **Frontend (Vitest):** Headphones page (each mode's controls, disabled state, Retry), battery gauge,
  EQ faders, flyout strip.
- **Manual checklist additions:** pressing the headphones' own ANC button updates Resonance within a second;
  turning the headphones off and on reconnects automatically.
