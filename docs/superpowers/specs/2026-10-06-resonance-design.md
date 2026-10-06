# Resonance — Design Spec

**Date:** 2026-10-06
**Status:** Approved in brainstorming; Phase 1 to be planned next
**Audience:** Personal use (single user, Windows 11). GPL-3.0 dependencies are acceptable.

## 1. Goal

An all-round Windows audio tool with a modern UI that replaces day-to-day use of
Voicemeeter, the Windows volume mixer, and the Soundcore app:

- Per-app volume mixer with live meters and per-app output routing
- Microphone control (gain, mute, hotkeys, device selection, boost)
- Soundcore Space One (A3035) headphone control (ANC/transparency, EQ, battery)
- Voicemeeter-style audio engine: processed virtual mic and separate monitor/stream mixes
- Profiles, hotkeys, start with Windows

## 2. Non-goals

- Writing a kernel-mode virtual audio driver (we rely on VB-Cable).
- Supporting headphones other than the Space One (the `headset` module is trait-based so others could be added later, but none are in scope).
- macOS/Linux support.
- Commercial distribution, code signing, auto-update.

## 3. Phases

Each phase gets its own implementation plan and is usable on its own.

| Phase | Scope | Depends on |
|---|---|---|
| 1. Core mixer + mic | Tray app, flyout + full window, per-app volume/mute/meters, per-app output routing, device management, microphone panel, persistence | — |
| 2. Space One | Bluetooth connection, ANC/transparency/normal modes, EQ presets + custom EQ, battery, other settings OpenSCQ30 exposes | 1 |
| 3. Audio engine | VB-Cable-backed processed virtual mic (gate → EQ → compressor → limiter), mic monitoring, monitor mix vs. stream mix | 1 |
| 4. Profiles & polish | Named scenes snapshotting all state, global hotkeys for scenes, start with Windows, onboarding | 1–3 |

## 4. Technology

- **Shell:** Tauri 2 (Rust backend, WebView2 frontend).
- **Backend:** Rust (stable, MSVC toolchain). Windows APIs via the `windows` crate (WASAPI, `IMMDeviceEnumerator`, `IAudioSessionManager2`, `IAudioEndpointVolume`, `IAudioMeterInformation`, `IDeviceTopology`). Phase 2 uses the `openscq30` crates from github.com/oppzippy/OpenSCQ30 (GPL-3.0). Phase 3 uses WASAPI shared/exclusive streams directly.
- **Frontend:** React + TypeScript + Vite, Tailwind CSS, Zustand for state, Vitest + Testing Library.
- **Prerequisites to install:** Rust (rustup, MSVC target), Visual Studio 2022 Build Tools (C++ workload), WebView2 runtime (already on Win11). Phase 3: VB-Cable driver.

## 5. Architecture

```
┌──────────────────── React UI ─────────────────────┐
│ Flyout │ Mixer │ Devices │ Mic │ Headphones │ ... │
└──────────▲──────────────────────────────┬─────────┘
           │ events                        │ commands (tauri invoke)
┌──────────┴────────── Rust core ──────────▼─────────┐
│ app_state   single source of truth + persistence   │
│ ├─ mixer     audio sessions (per-app)              │
│ ├─ devices   endpoints, defaults, hot-plug         │
│ ├─ routing   per-app output device                 │
│ ├─ mic       capture endpoint control + hotkeys    │
│ ├─ meters    batched peak polling                  │
│ ├─ headset   Space One (Phase 2)                   │
│ ├─ engine    real-time DSP graph (Phase 3)         │
│ └─ profiles  scenes (Phase 4)                      │
└────────────────────────────────────────────────────┘
```

### Module contracts

Every platform-facing module is defined by a Rust trait with a WASAPI implementation
and a fake implementation for tests. Modules never emit UI events directly; they report
changes to `app_state`, which owns the event bridge.

- **`devices`** — `list() -> Vec<Device>`, `set_default(id, role)`, `set_volume(id, f32)`,
  `set_mute(id, bool)`, change notifications via `IMMNotificationClient`.
  `Device { id, name, flow: Render|Capture, state: Active|Unplugged|Disabled, is_default: {console, communications}, volume, muted }`.
- **`mixer`** — `list_sessions() -> Vec<Session>`, `set_volume(session, f32)`, `set_mute(session, bool)`,
  notifications via `IAudioSessionNotification` / `IAudioSessionEvents`.
  `Session { id, pid, exe_path, display_name, icon (PNG base64, cached), device_id, volume, muted, state }`.
  "System sounds" is a pseudo-session.
- **`routing`** — `get_output(pid) -> Option<DeviceId>`, `set_output(pid, Option<DeviceId>)` using the
  undocumented `IAudioPolicyConfigFactory` (Windows 10 1803+, same as EarTrumpet). `None` = follow default.
  If interface acquisition fails, the module reports `Unavailable` and the UI hides routing controls.
- **`mic`** — operates on capture endpoints: gain (`IAudioEndpointVolume`), mute, default + communications
  default selection, boost via `IDeviceTopology` `IAudioVolumeLevel` on the "Microphone Boost" part when present
  (else boost control hidden). Global hotkey modes: toggle mute, push-to-talk, push-to-mute
  (`RegisterHotKey` for toggle; low-level keyboard hook for hold modes).
- **`meters`** — one thread polls `IAudioMeterInformation` for all sessions + the active mic at 30 Hz and emits a
  single `meters` event `{ sessions: {id: peak}, mic: peak, master: peak }`.
- **`app_state`** — holds `State { devices, sessions, routing_available, mic, settings }`, applies commands,
  emits a `state_patch` event on change, and persists settings (debounced 500 ms) to
  `%APPDATA%\Resonance\settings.json`.

### Persistence (`settings.json`)

```json
{
  "version": 1,
  "apps": { "spotify.exe": { "volume": 0.6, "muted": false, "output": "{device-id}" } },
  "devices": { "{device-id}": { "alias": "Desk Speakers", "hidden": false } },
  "mic": { "hotkey": "Ctrl+Shift+M", "mode": "toggle" },
  "ui": { "theme": "system", "start_minimized": true }
}
```

Apps are keyed by lowercase exe file name. When a session for a known exe appears, its saved
volume/mute/output are re-applied.

## 6. Phase 1 UI

- **Tray icon:** left-click toggles the flyout; right-click menu (Open, Mute mic, Quit). Icon shows a red mic
  badge while the mic is muted.
- **Flyout** (≈360×520, anchored above tray): master output device + volume, mic strip (gain, meter, mute),
  list of app rows (icon, name, slider, meter, mute). Closes on focus loss.
- **Full window** (dark glass/Mica style, sidebar nav):
  - **Mixer:** vertical channel strips per app with icon, peak + peak-hold meter, fader, mute, and output-device dropdown.
  - **Devices:** output and input lists; set default / communications default, rename (alias), hide, per-device volume.
  - **Microphone:** device picker, gain, boost, live meter, mute, hotkey + mode configuration.
  - **Settings:** theme (system/light/dark), start minimized, reset per-app memory.
- Closing the window hides it to tray. Theme follows Windows and uses the system accent color.

## 7. Data flow

1. UI command → `invoke("set_session_volume", {id, volume})`. Slider drags throttled to 60 Hz.
2. Rust applies via WASAPI and updates `app_state`.
3. `app_state` emits `state_patch`; the UI store applies it. The UI renders only reported state.
4. External changes (other apps, hot-plug) arrive via WASAPI notifications → same path from step 2.
5. Meters bypass the store: the `meters` event writes into a ref-based map that meter components read
   in a `requestAnimationFrame` loop, avoiding React re-renders.

COM threading: WASAPI calls run on a dedicated MTA worker thread that owns all COM objects; commands are
sent to it over a channel. Notification callbacks forward to that thread.

## 8. Error handling

- Device removed → mark `Unplugged`, keep in state, grey out in UI; saved routing re-applied when it returns.
- Audio service restart / resume from sleep (`AUDCLNT_E_SERVICE_NOT_RUNNING`, `AUDCLNT_E_DEVICE_INVALIDATED`)
  → tear down and rebuild all COM objects, then re-sync state.
- Routing interface unavailable → `routing_available = false`, one-time notice, other features unaffected.
- All command errors return `{ code, message }` to the UI and show as toasts. Logs via `tracing` to
  `%APPDATA%\Resonance\logs\` (daily rotation, 7 days).
- Corrupt `settings.json` → back it up as `settings.bad-<timestamp>.json` and start with defaults.

## 9. Testing

- **Rust unit tests** with fake `devices`/`mixer`/`routing`/`mic` implementations: command application,
  per-app memory re-apply, debounce, unplug/replug handling, settings migration and corruption recovery.
- **Integration tests** against real WASAPI marked `#[ignore]` (run with `cargo test -- --ignored`):
  enumerate devices and sessions, set-and-restore a volume, toggle and restore mic mute.
- **Frontend:** Vitest + Testing Library for store patch logic and components (slider throttling, mute states,
  routing dropdown hidden when unavailable).
- **Manual checklist:** tray behaviour, flyout positioning on multiple monitors and DPI scales, themes, hotkeys in a
  fullscreen game.

## 10. Later phases (outline; detailed in their own specs)

- **Phase 2 — Space One:** `headset` module wrapping `openscq30` device connection over Windows Bluetooth (RFCOMM/BLE
  as OpenSCQ30 uses). UI page: connection status, battery, ambient sound mode (ANC / transparency / normal) and
  ANC strength where supported, EQ presets + 10-band custom EQ, other toggles exposed by the device's feature set.
  Reconnects automatically when the headset reconnects.
- **Phase 3 — Audio engine:** real-time thread (MMCSS "Pro Audio") capturing the physical mic, processing
  gate → parametric EQ → compressor → limiter, writing to VB-Cable's input so apps use "CABLE Output" as the mic.
  Monitor mix (to headphones) and stream mix (to a second virtual cable — VB-Cable A+B, selected in OBS) with
  per-source sends.
  Lock-free parameter updates; no allocation on the audio thread. Offline DSP tests with known signals.
- **Phase 4 — Profiles:** scene = snapshot of app volumes/routing, mic settings, headset mode/EQ, engine params;
  hotkeys per scene; start with Windows via registry Run key.
