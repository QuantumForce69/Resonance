# Resonance Phase 2 — Soundcore Space One Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Control the Soundcore Space One (noise control, EQ, battery/status) from a new Headphones page and a tray-flyout strip.

**Architecture:** A `headset` Rust module runs as an async task on Tauri's tokio runtime, independent of the WASAPI audio worker. It talks to the headphones through `openscq30-lib` (pinned to v2.12.0) behind two small traits (`HeadsetConnector`, `HeadsetDevice`) so the state machine is tested with fakes; a pure `mapping` module converts the library's generic `Setting`/`Value` data to Resonance's typed `HeadsetState`/`HeadsetCommand`. The UI receives the full `HeadsetState` on a `headset` event, mirroring Phase 1's `state` event.

**Tech Stack:** Rust 1.99, `openscq30-lib` v2.12.0 (git), tokio 1, async-trait 0.1, macaddr 1; React 19 + TypeScript, Tailwind 4, Zustand, Vitest (all as in Phase 1).

## Global Constraints

- Spec: `docs/superpowers/specs/2026-10-07-phase2-space-one-design.md`; visual language: Phase 1 Design v2 (spec §12 of the parent spec, mockup https://claude.ai/artifact/EVtcBLh5RfMA4vaJePa8re).
- Dependency: `openscq30-lib = { git = "https://github.com/oppzippy/OpenSCQ30", tag = "v2.12.0" }`. Its license is GPL-3.0-or-later, so Resonance ships a GPL-3.0 `LICENSE`.
- Device model: `DeviceModel::SoundcoreA3035`. Session database: `%APPDATA%\Resonance\openscq30.sqlite`; a corrupt one is renamed `openscq30.bad-<unix>.sqlite` and recreated.
- Library facts verified against v2.12.0 with a demo device and the real headphones (name `soundcore Space One`):
  - `AmbientSoundMode`: `Select`, options `NoiseCanceling` | `Transparency` | `Normal`.
  - `NoiseCancelingMode`: `Select`, options `Custom` | `Adaptive` (UI calls `Custom` "Manual").
  - `AdaptiveNoiseCanceling`: `Information`, value `"<n>/5"` (read-only).
  - `ManualNoiseCanceling`, `ManualTransparency`: `I32Range` 1..=5 step 1.
  - `WindNoiseSuppression`: `Toggle`.
  - `BatteryLevel`: `Information`, value `"<level>/<max>"` (max 5). `IsCharging`: not provided by the A3035 (`None`) — charging is optional.
  - `FirmwareVersion`, `SerialNumber`: `Information`.
  - `PresetEqualizerProfile`: `PresetEqualizerProfileSelect` with 22 presets (ids like `SoundcoreSignature`, localized names like `Soundcore Signature`); value `None` = custom.
  - `VolumeAdjustments`: `Equalizer`, 8 bands `[100,200,400,800,1600,3200,6400,12800]` Hz, `fractionDigits` 1, `min` −120, `max` 134 (i.e. −12.0…+13.4 dB).
  - Enum-like values are set with `Value::String`; levels with `Value::I32`; toggles with `Value::Bool`; EQ with `Value::I16Vec`.
- Reconnect backoff: 5 s, doubling, capped at 30 s; `Retry` reconnects immediately and resets it.
- EQ fader edits are coalesced to at most one device write per 100 ms with the latest values.
- Demo mode for development: environment variable `RESONANCE_DEMO_HEADSET=1` uses OpenSCQ30's simulated Space One.
- Shell: PowerShell with `npm.cmd`; cargo via `--manifest-path M:\Resonance\src-tauri\Cargo.toml`.

---

## File Structure

```
src-tauri/
├─ Cargo.toml                        + openscq30-lib, async-trait, macaddr, tokio features
├─ src/headset/
│  ├─ mod.rs                         module exports
│  ├─ model.rs                       HeadsetState, HeadsetCommand, Connection, Battery, Eq, AmbientMode, NcMode
│  ├─ mapping.rs                     SettingSource trait; read_state(); command_values()  (pure)
│  ├─ device.rs                      HeadsetDevice + HeadsetConnector traits; LibDevice adapter
│  ├─ connector.rs                   LibConnector (OpenSCQ30Session: list → pair → connect; db recovery)
│  ├─ manager.rs                     Backoff, HeadsetHandle, HeadsetSink, run() state machine
│  └─ fixtures.rs                    #[cfg(test)] Space One settings parsed from verified JSON + fakes
├─ src/app/{mod.rs, commands.rs, sink.rs, wnd.rs}   wiring, commands, event, flyout height
├─ tests/headset_demo.rs             integration test against the simulated Space One
└─ tests/headset_hw.rs               #[ignore] test against the real headphones
LICENSE                              GPL-3.0 text
src/lib/{types.ts, api.ts, store.ts, bootstrap.ts}   headset types, calls, store slice, subscription
src/components/{LeverSwitch.tsx, ModeSwitch.tsx, Slider.tsx, ui.tsx, BatteryGauge.tsx, EqFaders.tsx, HeadsetStrip.tsx}
src/views/{HeadphonesView.tsx, Titlebar.tsx, MainWindow.tsx, Flyout.tsx}
src/test/fixtures.ts                 makeHeadset()
```

---

### Task 1: Dependency, license and headset model types

**Files:**
- Modify: `src-tauri/Cargo.toml`, `src-tauri/src/lib.rs`
- Create: `src-tauri/src/headset/mod.rs`, `src-tauri/src/headset/model.rs`, `LICENSE`

**Interfaces:**
- Produces (`resonance_lib::headset::model`): `AmbientMode {NoiseCanceling, Transparency, Normal}` with `as_lib(self) -> &'static str` / `from_lib(&str) -> Option<Self>`; `NcMode {Adaptive, Manual}` with the same pair (`Manual` ⇄ `"Custom"`); `Connection {Connecting, Connected, NotFound, Disconnected { retry_in_secs: u32 }}`; `Battery {level: u8, max: u8, charging: Option<bool>}`; `EqPreset {id, name}`; `Eq {presets, preset: Option<String>, band_hz: Vec<u16>, values: Vec<i16>, min: i16, max: i16, fraction_digits: i16}`; `HeadsetState` (all-optional fields + `connection`) with `HeadsetState::with_connection(Connection) -> Self`; `HeadsetCommand` (tagged camelCase).

- [ ] **Step 1: Add dependencies**

In `src-tauri/Cargo.toml`, change the `tokio` line under `[dependencies]` and add three lines below it:
```toml
tokio = { version = "1", features = ["sync", "rt-multi-thread", "macros", "time"] }
async-trait = "0.1"
macaddr = "1"
openscq30-lib = { git = "https://github.com/oppzippy/OpenSCQ30", tag = "v2.12.0" }
```
and under `[dev-dependencies]` add:
```toml
tokio = { version = "1", features = ["test-util"] }
```

Run: `cargo build --manifest-path M:\Resonance\src-tauri\Cargo.toml`
Expected: downloads and compiles `openscq30-lib` (several minutes the first time), ends with `Finished`.

- [ ] **Step 2: Add the GPL-3.0 license**

Run (PowerShell, from `M:\Resonance`):
```powershell
$lic = Get-ChildItem "$env:USERPROFILE\.cargo\git\checkouts" -Recurse -Filter LICENSE.txt | Where-Object { $_.FullName -match 'openscq30' } | Select-Object -First 1
Copy-Item $lic.FullName M:\Resonance\LICENSE
Select-String -Path M:\Resonance\LICENSE -Pattern 'GNU GENERAL PUBLIC LICENSE' | Select-Object -First 1
```
Expected: prints the `GNU GENERAL PUBLIC LICENSE` line.

- [ ] **Step 3: Write the failing model tests in a new `src-tauri/src/headset/model.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_deserialize_from_tagged_camel_case_json() {
        let c: HeadsetCommand = serde_json::from_str(r#"{"type":"setAmbientMode","mode":"transparency"}"#).unwrap();
        assert_eq!(c, HeadsetCommand::SetAmbientMode { mode: AmbientMode::Transparency });
        let c: HeadsetCommand = serde_json::from_str(r#"{"type":"setNcMode","mode":"manual"}"#).unwrap();
        assert_eq!(c, HeadsetCommand::SetNcMode { mode: NcMode::Manual });
        let c: HeadsetCommand = serde_json::from_str(r#"{"type":"setEqBands","values":[1,2,3]}"#).unwrap();
        assert_eq!(c, HeadsetCommand::SetEqBands { values: vec![1, 2, 3] });
        let c: HeadsetCommand = serde_json::from_str(r#"{"type":"retry"}"#).unwrap();
        assert_eq!(c, HeadsetCommand::Retry);
    }

    #[test]
    fn state_serializes_camel_case_with_tagged_connection() {
        let s = HeadsetState::with_connection(Connection::Disconnected { retry_in_secs: 10 });
        let v = serde_json::to_value(&s).unwrap();
        assert_eq!(v["connection"]["state"], "disconnected");
        assert_eq!(v["connection"]["retryInSecs"], 10);
        assert!(v["ambientMode"].is_null());
        assert!(v["manualNc"].is_null());
    }

    #[test]
    fn library_names_round_trip() {
        for m in [AmbientMode::NoiseCanceling, AmbientMode::Transparency, AmbientMode::Normal] {
            assert_eq!(AmbientMode::from_lib(m.as_lib()), Some(m));
        }
        assert_eq!(NcMode::Manual.as_lib(), "Custom");
        assert_eq!(NcMode::from_lib("Adaptive"), Some(NcMode::Adaptive));
        assert_eq!(NcMode::from_lib("Bogus"), None);
    }
}
```

`src-tauri/src/headset/mod.rs`:
```rust
pub mod model;
```

Add `pub mod headset;` to `src-tauri/src/lib.rs` (after `pub mod error;`).

- [ ] **Step 4: Run to verify failure**

Run: `cargo test --manifest-path M:\Resonance\src-tauri\Cargo.toml --lib headset::model`
Expected: FAIL to compile — `cannot find type HeadsetCommand`.

- [ ] **Step 5: Implement the model (top of `model.rs`)**

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AmbientMode {
    NoiseCanceling,
    Transparency,
    Normal,
}

impl AmbientMode {
    pub fn as_lib(self) -> &'static str {
        match self {
            AmbientMode::NoiseCanceling => "NoiseCanceling",
            AmbientMode::Transparency => "Transparency",
            AmbientMode::Normal => "Normal",
        }
    }

    pub fn from_lib(s: &str) -> Option<Self> {
        match s {
            "NoiseCanceling" => Some(AmbientMode::NoiseCanceling),
            "Transparency" => Some(AmbientMode::Transparency),
            "Normal" => Some(AmbientMode::Normal),
            _ => None,
        }
    }
}

/// Noise-cancelling strength source. The library calls `Manual` "Custom".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NcMode {
    Adaptive,
    Manual,
}

impl NcMode {
    pub fn as_lib(self) -> &'static str {
        match self {
            NcMode::Adaptive => "Adaptive",
            NcMode::Manual => "Custom",
        }
    }

    pub fn from_lib(s: &str) -> Option<Self> {
        match s {
            "Adaptive" => Some(NcMode::Adaptive),
            "Custom" => Some(NcMode::Manual),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Connection {
    Connecting,
    Connected,
    NotFound,
    Disconnected { retry_in_secs: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Battery {
    pub level: u8,
    pub max: u8,
    pub charging: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EqPreset {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Eq {
    pub presets: Vec<EqPreset>,
    /// `None` = custom curve.
    pub preset: Option<String>,
    pub band_hz: Vec<u16>,
    pub values: Vec<i16>,
    pub min: i16,
    pub max: i16,
    pub fraction_digits: i16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HeadsetState {
    pub connection: Connection,
    pub name: Option<String>,
    pub ambient_mode: Option<AmbientMode>,
    pub nc_mode: Option<NcMode>,
    pub manual_nc: Option<u8>,
    pub adaptive_nc: Option<u8>,
    pub transparency: Option<u8>,
    pub wind_noise: Option<bool>,
    pub battery: Option<Battery>,
    pub eq: Option<Eq>,
    pub firmware: Option<String>,
    pub serial: Option<String>,
}

impl HeadsetState {
    pub fn with_connection(connection: Connection) -> Self {
        Self {
            connection,
            name: None,
            ambient_mode: None,
            nc_mode: None,
            manual_nc: None,
            adaptive_nc: None,
            transparency: None,
            wind_noise: None,
            battery: None,
            eq: None,
            firmware: None,
            serial: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum HeadsetCommand {
    SetAmbientMode { mode: AmbientMode },
    SetNcMode { mode: NcMode },
    SetManualNc { level: u8 },
    SetTransparency { level: u8 },
    SetWindNoise { enabled: bool },
    SetEqPreset { preset: String },
    SetEqBands { values: Vec<i16> },
    Retry,
}
```

- [ ] **Step 6: Run to verify pass**

Run: `cargo test --manifest-path M:\Resonance\src-tauri\Cargo.toml --lib headset::model`
Expected: PASS (3 tests).

- [ ] **Step 7: Commit**

```powershell
git -C M:\Resonance add -A
git -C M:\Resonance commit -m "feat(headset): openscq30-lib dependency, GPL-3.0 license, headset model types"
```

---

### Task 2: Mapping between library settings and HeadsetState/commands

**Files:**
- Create: `src-tauri/src/headset/mapping.rs`, `src-tauri/src/headset/fixtures.rs`
- Modify: `src-tauri/src/headset/mod.rs`

**Interfaces:**
- Consumes: Task 1 model; `openscq30_lib::settings::{Setting, SettingId, Value}`; `crate::error::{AudioError, Result}`.
- Produces: `trait SettingSource { fn setting(&self, id: &SettingId) -> Option<Setting>; }`; `read_state(src: &dyn SettingSource, name: &str) -> HeadsetState` (connection = `Connected`); `command_values(cmd: &HeadsetCommand, state: &HeadsetState) -> Result<Vec<(SettingId, Value)>>`. `fixtures::space_one_settings() -> HashMap<SettingId, Setting>` (cfg(test)).

- [ ] **Step 1: Write the fixtures (verified library JSON) in `src-tauri/src/headset/fixtures.rs`**

```rust
//! Settings exactly as openscq30-lib v2.12.0 reports them for a Space One (captured from its demo device).
use std::collections::HashMap;

use openscq30_lib::settings::{Setting, SettingId};

const SETTINGS: &[(SettingId, &str)] = &[
    (SettingId::AmbientSoundMode, r#"{"type":"select","setting":{"options":["NoiseCanceling","Transparency","Normal"],"localizedOptions":["Noise Canceling","Transparency","Normal"]},"value":"NoiseCanceling"}"#),
    (SettingId::NoiseCancelingMode, r#"{"type":"select","setting":{"options":["Custom","Adaptive"],"localizedOptions":["Custom","Adaptive"]},"value":"Custom"}"#),
    (SettingId::AdaptiveNoiseCanceling, r#"{"type":"information","value":"3/5","translatedValue":"3/5"}"#),
    (SettingId::ManualNoiseCanceling, r#"{"type":"i32Range","setting":{"start":1,"end":5,"step":1},"value":4}"#),
    (SettingId::ManualTransparency, r#"{"type":"i32Range","setting":{"start":1,"end":5,"step":1},"value":2}"#),
    (SettingId::WindNoiseSuppression, r#"{"type":"toggle","value":true}"#),
    (SettingId::BatteryLevel, r#"{"type":"information","value":"4/5","translatedValue":"80%"}"#),
    (SettingId::FirmwareVersion, r#"{"type":"information","value":"06.87","translatedValue":"06.87"}"#),
    (SettingId::SerialNumber, r#"{"type":"information","value":"3035705028ABCDEF","translatedValue":"3035705028ABCDEF"}"#),
    (SettingId::PresetEqualizerProfile, r#"{"type":"presetEqualizerProfileSelect","equalizer":{"bandHz":[100,200,400,800,1600,3200,6400,12800],"fractionDigits":1,"min":-60,"max":60},"select":{"options":["SoundcoreSignature","Acoustic","Flat"],"localizedOptions":["Soundcore Signature","Acoustic","Flat"]},"presets":[[0,0,0,0,0,0,0,0],[40,10,20,20,40,40,40,20],[0,0,0,0,0,0,0,0]],"value":"SoundcoreSignature"}"#),
    (SettingId::VolumeAdjustments, r#"{"type":"equalizer","setting":{"bandHz":[100,200,400,800,1600,3200,6400,12800],"fractionDigits":1,"min":-120,"max":134},"readOnly":false,"value":[0,0,0,0,0,0,0,0]}"#),
];

pub fn space_one_settings() -> HashMap<SettingId, Setting> {
    SETTINGS.iter().map(|(id, json)| (*id, serde_json::from_str(json).expect("valid setting json"))).collect()
}
```

- [ ] **Step 2: Write the failing mapping tests in a new `src-tauri/src/headset/mapping.rs`**

```rust
#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::headset::fixtures::space_one_settings;

    struct Map(HashMap<SettingId, Setting>);
    impl SettingSource for Map {
        fn setting(&self, id: &SettingId) -> Option<Setting> {
            self.0.get(id).cloned()
        }
    }

    fn state() -> HeadsetState {
        read_state(&Map(space_one_settings()), "soundcore Space One")
    }

    #[test]
    fn reads_every_supported_setting() {
        let s = state();
        assert_eq!(s.connection, Connection::Connected);
        assert_eq!(s.name.as_deref(), Some("soundcore Space One"));
        assert_eq!(s.ambient_mode, Some(AmbientMode::NoiseCanceling));
        assert_eq!(s.nc_mode, Some(NcMode::Manual));
        assert_eq!(s.manual_nc, Some(4));
        assert_eq!(s.adaptive_nc, Some(3));
        assert_eq!(s.transparency, Some(2));
        assert_eq!(s.wind_noise, Some(true));
        assert_eq!(s.battery, Some(Battery { level: 4, max: 5, charging: None }));
        assert_eq!(s.firmware.as_deref(), Some("06.87"));
        assert_eq!(s.serial.as_deref(), Some("3035705028ABCDEF"));
        let eq = s.eq.expect("eq");
        assert_eq!(eq.band_hz, vec![100, 200, 400, 800, 1600, 3200, 6400, 12800]);
        assert_eq!((eq.min, eq.max, eq.fraction_digits), (-120, 134, 1));
        assert_eq!(eq.values, vec![0; 8]);
        assert_eq!(eq.preset.as_deref(), Some("SoundcoreSignature"));
        assert_eq!(eq.presets[0], EqPreset { id: "SoundcoreSignature".into(), name: "Soundcore Signature".into() });
    }

    #[test]
    fn missing_or_unexpected_settings_become_none() {
        let mut m = space_one_settings();
        m.remove(&SettingId::BatteryLevel);
        m.insert(SettingId::AmbientSoundMode, serde_json::from_str(r#"{"type":"toggle","value":true}"#).unwrap());
        m.insert(SettingId::AdaptiveNoiseCanceling, serde_json::from_str(r#"{"type":"information","value":"n/a","translatedValue":"n/a"}"#).unwrap());
        let s = read_state(&Map(m), "x");
        assert_eq!(s.battery, None);
        assert_eq!(s.ambient_mode, None);
        assert_eq!(s.adaptive_nc, None);
        assert_eq!(read_state(&Map(HashMap::new()), "x").eq, None);
    }

    #[test]
    fn charging_is_read_when_the_device_reports_it() {
        let mut m = space_one_settings();
        m.insert(SettingId::IsCharging, serde_json::from_str(r#"{"type":"information","value":"true","translatedValue":"Charging"}"#).unwrap());
        assert_eq!(read_state(&Map(m), "x").battery.unwrap().charging, Some(true));
    }

    #[test]
    fn simple_commands_map_to_library_values() {
        let s = state();
        assert_eq!(
            command_values(&HeadsetCommand::SetAmbientMode { mode: AmbientMode::Transparency }, &s).unwrap(),
            vec![(SettingId::AmbientSoundMode, Value::from("Transparency"))]
        );
        assert_eq!(
            command_values(&HeadsetCommand::SetNcMode { mode: NcMode::Manual }, &s).unwrap(),
            vec![(SettingId::NoiseCancelingMode, Value::from("Custom"))]
        );
        assert_eq!(
            command_values(&HeadsetCommand::SetManualNc { level: 9 }, &s).unwrap(),
            vec![(SettingId::ManualNoiseCanceling, Value::I32(5))]
        );
        assert_eq!(
            command_values(&HeadsetCommand::SetTransparency { level: 0 }, &s).unwrap(),
            vec![(SettingId::ManualTransparency, Value::I32(1))]
        );
        assert_eq!(
            command_values(&HeadsetCommand::SetWindNoise { enabled: false }, &s).unwrap(),
            vec![(SettingId::WindNoiseSuppression, Value::Bool(false))]
        );
        assert!(command_values(&HeadsetCommand::Retry, &s).unwrap().is_empty());
    }

    #[test]
    fn eq_preset_must_be_known() {
        let s = state();
        assert_eq!(
            command_values(&HeadsetCommand::SetEqPreset { preset: "Acoustic".into() }, &s).unwrap(),
            vec![(SettingId::PresetEqualizerProfile, Value::from("Acoustic"))]
        );
        let err = command_values(&HeadsetCommand::SetEqPreset { preset: "Nope".into() }, &s).unwrap_err();
        assert_eq!(err.code(), "invalid");
    }

    #[test]
    fn eq_bands_are_clamped_and_counted() {
        let s = state();
        assert_eq!(
            command_values(&HeadsetCommand::SetEqBands { values: vec![-500, 0, 0, 0, 0, 0, 0, 500] }, &s).unwrap(),
            vec![(SettingId::VolumeAdjustments, Value::I16Vec(vec![-120, 0, 0, 0, 0, 0, 0, 134]))]
        );
        let err = command_values(&HeadsetCommand::SetEqBands { values: vec![0; 3] }, &s).unwrap_err();
        assert_eq!(err.code(), "invalid");
        let none = HeadsetState::with_connection(Connection::Connected);
        assert_eq!(command_values(&HeadsetCommand::SetEqBands { values: vec![0; 8] }, &none).unwrap_err().code(), "unavailable");
    }
}
```

`src-tauri/src/headset/mod.rs`:
```rust
#[cfg(test)]
pub(crate) mod fixtures;
pub mod mapping;
pub mod model;
```

- [ ] **Step 3: Run to verify failure**

Run: `cargo test --manifest-path M:\Resonance\src-tauri\Cargo.toml --lib headset::mapping`
Expected: FAIL to compile — `cannot find trait SettingSource`.

- [ ] **Step 4: Implement `mapping.rs` (above the tests)**

```rust
use std::borrow::Cow;

use openscq30_lib::settings::{Setting, SettingId, Value};

use super::model::*;
use crate::error::{AudioError, Result};

/// Anything that can answer "what is this setting right now" — the real device or a test map.
pub trait SettingSource {
    fn setting(&self, id: &SettingId) -> Option<Setting>;
}

fn select(src: &dyn SettingSource, id: SettingId) -> Option<String> {
    match src.setting(&id)? {
        Setting::Select { value, .. } => Some(value.into_owned()),
        _ => None,
    }
}

fn information(src: &dyn SettingSource, id: SettingId) -> Option<String> {
    match src.setting(&id)? {
        Setting::Information { value, .. } => Some(value),
        _ => None,
    }
}

fn level(src: &dyn SettingSource, id: SettingId) -> Option<u8> {
    match src.setting(&id)? {
        Setting::I32Range { value, .. } => u8::try_from(value).ok(),
        _ => None,
    }
}

fn toggle(src: &dyn SettingSource, id: SettingId) -> Option<bool> {
    match src.setting(&id)? {
        Setting::Toggle { value } => Some(value),
        _ => None,
    }
}

/// Parses the library's `"<n>/<max>"` information strings.
fn fraction(text: &str) -> Option<(u8, u8)> {
    let (a, b) = text.split_once('/')?;
    Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
}

fn read_eq(src: &dyn SettingSource) -> Option<Eq> {
    let Setting::Equalizer { setting, value, .. } = src.setting(&SettingId::VolumeAdjustments)? else {
        return None;
    };
    let (presets, preset) = match src.setting(&SettingId::PresetEqualizerProfile) {
        Some(Setting::PresetEqualizerProfileSelect { select, value, .. }) => (
            select
                .options
                .iter()
                .zip(select.localized_options.iter())
                .map(|(id, name)| EqPreset { id: id.to_string(), name: name.clone() })
                .collect(),
            value.map(Cow::into_owned),
        ),
        _ => (Vec::new(), None),
    };
    Some(Eq {
        presets,
        preset,
        band_hz: setting.band_hz.to_vec(),
        values: value,
        min: setting.min,
        max: setting.max,
        fraction_digits: setting.fraction_digits,
    })
}

/// Builds the UI state for a connected headset from its current settings.
pub fn read_state(src: &dyn SettingSource, name: &str) -> HeadsetState {
    let mut s = HeadsetState::with_connection(Connection::Connected);
    s.name = Some(name.to_string());
    s.ambient_mode = select(src, SettingId::AmbientSoundMode).and_then(|v| AmbientMode::from_lib(&v));
    s.nc_mode = select(src, SettingId::NoiseCancelingMode).and_then(|v| NcMode::from_lib(&v));
    s.adaptive_nc = information(src, SettingId::AdaptiveNoiseCanceling).and_then(|v| fraction(&v)).map(|(n, _)| n);
    s.manual_nc = level(src, SettingId::ManualNoiseCanceling);
    s.transparency = level(src, SettingId::ManualTransparency);
    s.wind_noise = toggle(src, SettingId::WindNoiseSuppression);
    s.battery = information(src, SettingId::BatteryLevel).and_then(|v| fraction(&v)).map(|(level, max)| Battery {
        level,
        max,
        charging: information(src, SettingId::IsCharging).map(|v| v == "true"),
    });
    s.firmware = information(src, SettingId::FirmwareVersion);
    s.serial = information(src, SettingId::SerialNumber);
    s.eq = read_eq(src);
    s
}

fn eq_of(state: &HeadsetState) -> Result<&Eq> {
    state.eq.as_ref().ok_or_else(|| AudioError::Unavailable("The headphones' EQ isn't available".into()))
}

/// Translates a UI command into library setting writes, validating and clamping values.
pub fn command_values(cmd: &HeadsetCommand, state: &HeadsetState) -> Result<Vec<(SettingId, Value)>> {
    Ok(match cmd {
        HeadsetCommand::SetAmbientMode { mode } => vec![(SettingId::AmbientSoundMode, Value::from(mode.as_lib()))],
        HeadsetCommand::SetNcMode { mode } => vec![(SettingId::NoiseCancelingMode, Value::from(mode.as_lib()))],
        HeadsetCommand::SetManualNc { level } => {
            vec![(SettingId::ManualNoiseCanceling, Value::I32(i32::from((*level).clamp(1, 5))))]
        }
        HeadsetCommand::SetTransparency { level } => {
            vec![(SettingId::ManualTransparency, Value::I32(i32::from((*level).clamp(1, 5))))]
        }
        HeadsetCommand::SetWindNoise { enabled } => vec![(SettingId::WindNoiseSuppression, Value::Bool(*enabled))],
        HeadsetCommand::SetEqPreset { preset } => {
            if !eq_of(state)?.presets.iter().any(|p| &p.id == preset) {
                return Err(AudioError::Invalid(format!("Unknown EQ preset \"{preset}\"")));
            }
            vec![(SettingId::PresetEqualizerProfile, Value::String(Cow::Owned(preset.clone())))]
        }
        HeadsetCommand::SetEqBands { values } => {
            let eq = eq_of(state)?;
            if values.len() != eq.band_hz.len() {
                return Err(AudioError::Invalid(format!("Expected {} EQ bands, got {}", eq.band_hz.len(), values.len())));
            }
            let clamped = values.iter().map(|v| (*v).clamp(eq.min, eq.max)).collect();
            vec![(SettingId::VolumeAdjustments, Value::I16Vec(clamped))]
        }
        HeadsetCommand::Retry => Vec::new(),
    })
}
```

- [ ] **Step 5: Run to verify pass**

Run: `cargo test --manifest-path M:\Resonance\src-tauri\Cargo.toml --lib headset::`
Expected: PASS (model 3 + mapping 6). If `Value::from(&'static str)` is rejected for a non-static `&str`, the code above only passes `&'static str` from `as_lib`, so it compiles as written.

- [ ] **Step 6: Commit**

```powershell
git -C M:\Resonance add -A
git -C M:\Resonance commit -m "feat(headset): map openscq30 settings to HeadsetState and commands to setting writes"
```

---

### Task 3: Device traits, library adapters and the connection manager

**Files:**
- Create: `src-tauri/src/headset/device.rs`, `src-tauri/src/headset/connector.rs`, `src-tauri/src/headset/manager.rs`
- Modify: `src-tauri/src/headset/mod.rs`, `src-tauri/src/headset/fixtures.rs` (add fakes)

**Interfaces:**
- Consumes: Tasks 1–2.
- Produces:
  - `device.rs`: `#[async_trait] trait HeadsetDevice: SettingSource + Send + Sync { fn watch_for_changes(&self) -> watch::Receiver<()>; fn connection_status(&self) -> watch::Receiver<ConnectionStatus>; async fn set_setting_values(&self, values: Vec<(SettingId, Value)>) -> std::result::Result<(), String>; }`; `struct Found { pub name: String, pub device: Arc<dyn HeadsetDevice> }`; `#[async_trait] trait HeadsetConnector: Send + Sync { async fn connect(&self) -> std::result::Result<Option<Found>, String>; }`; `struct LibDevice(pub Arc<dyn OpenSCQ30Device + Send + Sync>)`.
  - `connector.rs`: `LibConnector::open(db_path: PathBuf, demo: bool) -> std::result::Result<LibConnector, String>` (async); implements `HeadsetConnector`.
  - `manager.rs`: `Backoff { new(), next_delay(&mut self) -> u32, reset(&mut self) }` with `START = 5`, `CAP = 30`; `trait HeadsetSink: Send + Sync + 'static { fn state(&self, s: &HeadsetState); }`; `HeadsetHandle::channel() -> (HeadsetHandle, Inbox)`; `HeadsetHandle::command(&self, HeadsetCommand) -> Result<()>` (async); `HeadsetHandle::state(&self) -> Result<HeadsetState>` (async); `async fn run(connector: Arc<dyn HeadsetConnector>, inbox: Inbox, sink: impl HeadsetSink)`; `EQ_COALESCE = 100 ms`.

- [ ] **Step 1: Write `device.rs`**

```rust
use std::sync::Arc;

use async_trait::async_trait;
use openscq30_lib::{
    connection::ConnectionStatus,
    device::OpenSCQ30Device,
    settings::{Setting, SettingId, Value},
};
use tokio::sync::watch;

use super::mapping::SettingSource;

/// The subset of a connected headset Resonance uses. Implemented by `LibDevice` and test fakes.
#[async_trait]
pub trait HeadsetDevice: SettingSource + Send + Sync {
    fn watch_for_changes(&self) -> watch::Receiver<()>;
    fn connection_status(&self) -> watch::Receiver<ConnectionStatus>;
    async fn set_setting_values(&self, values: Vec<(SettingId, Value)>) -> Result<(), String>;
}

pub struct Found {
    pub name: String,
    pub device: Arc<dyn HeadsetDevice>,
}

/// Finds and connects to the Space One. `Ok(None)` means Windows knows of no Space One.
#[async_trait]
pub trait HeadsetConnector: Send + Sync {
    async fn connect(&self) -> Result<Option<Found>, String>;
}

pub struct LibDevice(pub Arc<dyn OpenSCQ30Device + Send + Sync>);

impl SettingSource for LibDevice {
    fn setting(&self, id: &SettingId) -> Option<Setting> {
        self.0.setting(id)
    }
}

#[async_trait]
impl HeadsetDevice for LibDevice {
    fn watch_for_changes(&self) -> watch::Receiver<()> {
        self.0.watch_for_changes()
    }

    fn connection_status(&self) -> watch::Receiver<ConnectionStatus> {
        self.0.connection_status()
    }

    async fn set_setting_values(&self, values: Vec<(SettingId, Value)>) -> Result<(), String> {
        self.0.set_setting_values(values).await.map_err(|e| e.to_string())
    }
}
```

- [ ] **Step 2: Write `connector.rs`**

```rust
use std::{
    path::PathBuf,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use async_trait::async_trait;
use openscq30_lib::{storage::PairedDevice, DeviceModel, OpenSCQ30Session};

use super::device::{Found, HeadsetConnector, LibDevice};

const MODEL: DeviceModel = DeviceModel::SoundcoreA3035;

/// Connects through openscq30-lib: list Space Ones Windows knows about, register the first with the
/// library's own store if needed, and connect. `demo` uses the library's simulated device.
pub struct LibConnector {
    session: OpenSCQ30Session,
    demo: bool,
}

impl LibConnector {
    pub async fn open(db_path: PathBuf, demo: bool) -> Result<Self, String> {
        if let Some(dir) = db_path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let session = match OpenSCQ30Session::new(db_path.clone()).await {
            Ok(s) => s,
            Err(e) => {
                let ts = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
                let backup = db_path.with_file_name(format!("openscq30.bad-{ts}.sqlite"));
                tracing::warn!("headset database unusable ({e}); moved to {}", backup.display());
                let _ = std::fs::rename(&db_path, backup);
                OpenSCQ30Session::new(db_path).await.map_err(|e| e.to_string())?
            }
        };
        Ok(Self { session, demo })
    }
}

#[async_trait]
impl HeadsetConnector for LibConnector {
    async fn connect(&self) -> Result<Option<Found>, String> {
        let listed = if self.demo {
            self.session.list_demo_devices(MODEL).await
        } else {
            self.session.list_devices(MODEL).await
        }
        .map_err(|e| e.to_string())?;
        let Some(descriptor) = listed.into_iter().next() else { return Ok(None) };
        let paired = self.session.paired_devices().await.map_err(|e| e.to_string())?;
        if !paired.iter().any(|p| p.mac_address == descriptor.mac_address && p.model == MODEL) {
            self.session
                .pair(PairedDevice { mac_address: descriptor.mac_address, model: MODEL, is_demo: self.demo })
                .await
                .map_err(|e| e.to_string())?;
        }
        let device = self.session.connect(descriptor.mac_address).await.map_err(|e| e.to_string())?;
        Ok(Some(Found { name: descriptor.name, device: Arc::new(LibDevice(device)) }))
    }
}
```

- [ ] **Step 3: Add fakes to `fixtures.rs` (append)**

```rust
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use openscq30_lib::{connection::ConnectionStatus, settings::Value};
use tokio::sync::{mpsc, watch};

use super::{
    device::{Found, HeadsetConnector, HeadsetDevice},
    manager::HeadsetSink,
    mapping::SettingSource,
    model::HeadsetState,
};

pub struct FakeDevice {
    pub settings: Mutex<HashMap<SettingId, Setting>>,
    pub writes: Mutex<Vec<Vec<(SettingId, Value)>>>,
    pub changes: watch::Sender<()>,
    pub status: watch::Sender<ConnectionStatus>,
}

impl FakeDevice {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            settings: Mutex::new(space_one_settings()),
            writes: Mutex::new(Vec::new()),
            changes: watch::channel(()).0,
            status: watch::channel(ConnectionStatus::Connected).0,
        })
    }
}

impl SettingSource for FakeDevice {
    fn setting(&self, id: &SettingId) -> Option<Setting> {
        self.settings.lock().unwrap().get(id).cloned()
    }
}

#[async_trait]
impl HeadsetDevice for FakeDevice {
    fn watch_for_changes(&self) -> watch::Receiver<()> {
        self.changes.subscribe()
    }

    fn connection_status(&self) -> watch::Receiver<ConnectionStatus> {
        self.status.subscribe()
    }

    async fn set_setting_values(&self, values: Vec<(SettingId, Value)>) -> Result<(), String> {
        {
            let mut settings = self.settings.lock().unwrap();
            for (id, value) in &values {
                match (settings.get_mut(id), value) {
                    (Some(Setting::Select { value: v, .. }), Value::String(s)) => *v = s.clone(),
                    (Some(Setting::Equalizer { value: v, .. }), Value::I16Vec(new)) => *v = new.clone(),
                    _ => {}
                }
            }
        }
        self.writes.lock().unwrap().push(values);
        let _ = self.changes.send(());
        Ok(())
    }
}

pub struct FakeConnector {
    pub results: Mutex<VecDeque<Result<Option<Arc<FakeDevice>>, String>>>,
    pub calls: Mutex<usize>,
}

impl FakeConnector {
    pub fn new(results: Vec<Result<Option<Arc<FakeDevice>>, String>>) -> Arc<Self> {
        Arc::new(Self { results: Mutex::new(results.into()), calls: Mutex::new(0) })
    }
}

#[async_trait]
impl HeadsetConnector for FakeConnector {
    async fn connect(&self) -> Result<Option<Found>, String> {
        *self.calls.lock().unwrap() += 1;
        let next = self.results.lock().unwrap().pop_front().unwrap_or(Ok(None));
        next.map(|o| o.map(|d| Found { name: "soundcore Space One".into(), device: d }))
    }
}

pub struct ChanSink(pub mpsc::UnboundedSender<HeadsetState>);

impl HeadsetSink for ChanSink {
    fn state(&self, s: &HeadsetState) {
        let _ = self.0.send(s.clone());
    }
}
```

- [ ] **Step 4: Write the failing manager tests in a new `src-tauri/src/headset/manager.rs`**

```rust
#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::headset::{
        fixtures::{ChanSink, FakeConnector, FakeDevice},
        model::AmbientMode,
    };

    async fn next_matching(
        rx: &mut tokio::sync::mpsc::UnboundedReceiver<HeadsetState>,
        pred: impl Fn(&HeadsetState) -> bool,
    ) -> HeadsetState {
        loop {
            let s = tokio::time::timeout(Duration::from_secs(120), rx.recv()).await.expect("timed out").expect("closed");
            if pred(&s) {
                return s;
            }
        }
    }

    fn start(connector: Arc<FakeConnector>) -> (HeadsetHandle, tokio::sync::mpsc::UnboundedReceiver<HeadsetState>) {
        let (handle, inbox) = HeadsetHandle::channel();
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        tokio::spawn(run(connector, inbox, ChanSink(tx)));
        (handle, rx)
    }

    #[test]
    fn backoff_doubles_to_a_cap_and_resets() {
        let mut b = Backoff::new();
        let seq: Vec<u32> = (0..5).map(|_| b.next_delay()).collect();
        assert_eq!(seq, vec![5, 10, 20, 30, 30]);
        b.reset();
        assert_eq!(b.next_delay(), 5);
    }

    #[tokio::test(start_paused = true)]
    async fn connects_and_publishes_the_device_state() {
        let (handle, mut rx) = start(FakeConnector::new(vec![Ok(Some(FakeDevice::new()))]));
        let s = next_matching(&mut rx, |s| s.connection == Connection::Connected).await;
        assert_eq!(s.name.as_deref(), Some("soundcore Space One"));
        assert_eq!(s.ambient_mode, Some(AmbientMode::NoiseCanceling));
        assert_eq!(handle.state().await.unwrap().manual_nc, Some(4));
    }

    #[tokio::test(start_paused = true)]
    async fn not_found_retries_after_the_backoff() {
        let connector = FakeConnector::new(vec![Ok(None), Ok(None)]);
        let (_handle, mut rx) = start(connector.clone());
        next_matching(&mut rx, |s| s.connection == Connection::NotFound).await;
        next_matching(&mut rx, |s| s.connection == Connection::Connecting).await;
        next_matching(&mut rx, |s| s.connection == Connection::NotFound).await;
        assert!(*connector.calls.lock().unwrap() >= 2);
    }

    #[tokio::test(start_paused = true)]
    async fn commands_fail_while_disconnected_and_retry_reconnects_immediately() {
        let device = FakeDevice::new();
        let connector = FakeConnector::new(vec![Err("bluetooth off".into()), Ok(Some(device))]);
        let (handle, mut rx) = start(connector.clone());
        let s = next_matching(&mut rx, |s| matches!(s.connection, Connection::Disconnected { .. })).await;
        assert_eq!(s.connection, Connection::Disconnected { retry_in_secs: 5 });
        let err = handle.command(HeadsetCommand::SetWindNoise { enabled: true }).await.unwrap_err();
        assert_eq!(err.code(), "unavailable");
        handle.command(HeadsetCommand::Retry).await.unwrap();
        next_matching(&mut rx, |s| s.connection == Connection::Connected).await;
        assert_eq!(*connector.calls.lock().unwrap(), 2);
    }

    #[tokio::test(start_paused = true)]
    async fn commands_write_to_the_device_and_republish() {
        let device = FakeDevice::new();
        let (handle, mut rx) = start(FakeConnector::new(vec![Ok(Some(device.clone()))]));
        next_matching(&mut rx, |s| s.connection == Connection::Connected).await;
        handle.command(HeadsetCommand::SetAmbientMode { mode: AmbientMode::Transparency }).await.unwrap();
        next_matching(&mut rx, |s| s.ambient_mode == Some(AmbientMode::Transparency)).await;
        assert_eq!(
            device.writes.lock().unwrap().last().unwrap(),
            &vec![(SettingId::AmbientSoundMode, Value::from("Transparency"))]
        );
        let err = handle.command(HeadsetCommand::SetEqPreset { preset: "Nope".into() }).await.unwrap_err();
        assert_eq!(err.code(), "invalid");
    }

    #[tokio::test(start_paused = true)]
    async fn eq_band_edits_are_coalesced() {
        let device = FakeDevice::new();
        let (handle, mut rx) = start(FakeConnector::new(vec![Ok(Some(device.clone()))]));
        next_matching(&mut rx, |s| s.connection == Connection::Connected).await;
        for v in [10, 20, 30] {
            handle.command(HeadsetCommand::SetEqBands { values: vec![v; 8] }).await.unwrap();
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
        let writes = device.writes.lock().unwrap().clone();
        assert_eq!(writes, vec![vec![(SettingId::VolumeAdjustments, Value::I16Vec(vec![30; 8]))]]);
    }

    #[tokio::test(start_paused = true)]
    async fn a_dropped_connection_schedules_a_reconnect() {
        let device = FakeDevice::new();
        let (_handle, mut rx) = start(FakeConnector::new(vec![Ok(Some(device.clone()))]));
        next_matching(&mut rx, |s| s.connection == Connection::Connected).await;
        device.status.send(ConnectionStatus::Disconnected).unwrap();
        let s = next_matching(&mut rx, |s| matches!(s.connection, Connection::Disconnected { .. })).await;
        assert_eq!(s.connection, Connection::Disconnected { retry_in_secs: 5 });
        assert_eq!(s.ambient_mode, None, "stale settings are cleared");
    }
}
```

`src-tauri/src/headset/mod.rs`:
```rust
pub mod connector;
pub mod device;
#[cfg(test)]
pub(crate) mod fixtures;
pub mod manager;
pub mod mapping;
pub mod model;

pub use connector::LibConnector;
pub use manager::{run, HeadsetHandle, HeadsetSink};
```

- [ ] **Step 5: Run to verify failure**

Run: `cargo test --manifest-path M:\Resonance\src-tauri\Cargo.toml --lib headset::manager`
Expected: FAIL to compile — `cannot find struct Backoff`.

- [ ] **Step 6: Implement `manager.rs` (above the tests)**

```rust
use std::{sync::Arc, time::Duration};

use openscq30_lib::{
    connection::ConnectionStatus,
    settings::{SettingId, Value},
};
use tokio::{
    sync::{mpsc, oneshot},
    time::Instant,
};

use super::{
    device::{Found, HeadsetConnector},
    mapping::{command_values, read_state},
    model::{Connection, HeadsetCommand, HeadsetState},
};
use crate::error::{AudioError, Result};

pub const EQ_COALESCE: Duration = Duration::from_millis(100);

/// Reconnect delays in seconds: 5, 10, 20, then 30 forever until reset.
#[derive(Debug, Clone)]
pub struct Backoff {
    next: u32,
}

impl Backoff {
    pub const START: u32 = 5;
    pub const CAP: u32 = 30;

    pub fn new() -> Self {
        Self { next: Self::START }
    }

    pub fn next_delay(&mut self) -> u32 {
        let d = self.next;
        self.next = (self.next * 2).min(Self::CAP);
        d
    }

    pub fn reset(&mut self) {
        self.next = Self::START;
    }
}

impl Default for Backoff {
    fn default() -> Self {
        Self::new()
    }
}

/// Where the manager publishes state (a Tauri event in production, a channel in tests).
pub trait HeadsetSink: Send + Sync + 'static {
    fn state(&self, state: &HeadsetState);
}

enum Msg {
    Command(HeadsetCommand, oneshot::Sender<Result<()>>),
    Get(oneshot::Sender<HeadsetState>),
}

pub struct Inbox(mpsc::UnboundedReceiver<Msg>);

#[derive(Clone)]
pub struct HeadsetHandle {
    tx: mpsc::UnboundedSender<Msg>,
}

fn stopped() -> AudioError {
    AudioError::Unavailable("Headphone support isn't running".into())
}

fn not_connected() -> AudioError {
    AudioError::Unavailable("Space One isn't connected".into())
}

impl HeadsetHandle {
    pub fn channel() -> (Self, Inbox) {
        let (tx, rx) = mpsc::unbounded_channel();
        (Self { tx }, Inbox(rx))
    }

    pub async fn command(&self, cmd: HeadsetCommand) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.tx.send(Msg::Command(cmd, tx)).map_err(|_| stopped())?;
        rx.await.map_err(|_| stopped())?
    }

    pub async fn state(&self) -> Result<HeadsetState> {
        let (tx, rx) = oneshot::channel();
        self.tx.send(Msg::Get(tx)).map_err(|_| stopped())?;
        rx.await.map_err(|_| stopped())
    }
}

enum Exit {
    Shutdown,
    Disconnected,
    Retry,
}

/// Runs forever: connect, mirror the device while connected, back off and retry when not.
pub async fn run(connector: Arc<dyn HeadsetConnector>, mut inbox: Inbox, sink: impl HeadsetSink) {
    let mut backoff = Backoff::new();
    loop {
        let mut state = HeadsetState::with_connection(Connection::Connecting);
        sink.state(&state);
        let mut not_found = false;
        match connector.connect().await {
            Ok(Some(found)) => {
                backoff.reset();
                match connected(&found, &mut inbox, &sink, &mut state).await {
                    Exit::Shutdown => return,
                    Exit::Retry => continue,
                    Exit::Disconnected => tracing::info!("Space One disconnected"),
                }
            }
            Ok(None) => not_found = true,
            Err(e) => tracing::warn!("couldn't connect to the Space One: {e}"),
        }

        let delay = backoff.next_delay();
        state = HeadsetState::with_connection(if not_found {
            Connection::NotFound
        } else {
            Connection::Disconnected { retry_in_secs: delay }
        });
        sink.state(&state);

        let wait = tokio::time::sleep(Duration::from_secs(u64::from(delay)));
        tokio::pin!(wait);
        loop {
            tokio::select! {
                () = &mut wait => break,
                msg = inbox.0.recv() => match msg {
                    None => return,
                    Some(Msg::Get(reply)) => { let _ = reply.send(state.clone()); }
                    Some(Msg::Command(HeadsetCommand::Retry, reply)) => {
                        backoff.reset();
                        let _ = reply.send(Ok(()));
                        break;
                    }
                    Some(Msg::Command(_, reply)) => { let _ = reply.send(Err(not_connected())); }
                },
            }
        }
    }
}

async fn connected(found: &Found, inbox: &mut Inbox, sink: &impl HeadsetSink, state: &mut HeadsetState) -> Exit {
    let device = &found.device;
    let mut changes = device.watch_for_changes();
    let mut status = device.connection_status();
    if *status.borrow() == ConnectionStatus::Disconnected {
        return Exit::Disconnected;
    }
    let publish = |state: &mut HeadsetState| {
        *state = read_state(device.as_ref(), &found.name);
        sink.state(state);
    };
    publish(state);

    let mut pending_eq: Option<Vec<i16>> = None;
    let flush = tokio::time::sleep(Duration::ZERO);
    tokio::pin!(flush);

    loop {
        tokio::select! {
            changed = changes.changed() => {
                if changed.is_err() {
                    return Exit::Disconnected;
                }
                publish(state);
            }
            changed = status.changed() => {
                if changed.is_err() || *status.borrow() == ConnectionStatus::Disconnected {
                    return Exit::Disconnected;
                }
            }
            () = &mut flush, if pending_eq.is_some() => {
                let values = pending_eq.take().unwrap_or_default();
                if let Err(e) = device.set_setting_values(vec![(SettingId::VolumeAdjustments, Value::I16Vec(values))]).await {
                    tracing::warn!("couldn't apply EQ: {e}");
                }
                publish(state);
            }
            msg = inbox.0.recv() => match msg {
                None => return Exit::Shutdown,
                Some(Msg::Get(reply)) => { let _ = reply.send(state.clone()); }
                Some(Msg::Command(HeadsetCommand::Retry, reply)) => {
                    let _ = reply.send(Ok(()));
                    return Exit::Retry;
                }
                Some(Msg::Command(cmd, reply)) => match command_values(&cmd, state) {
                    Err(e) => { let _ = reply.send(Err(e)); }
                    Ok(values) => {
                        if let [(SettingId::VolumeAdjustments, Value::I16Vec(bands))] = values.as_slice() {
                            if pending_eq.is_none() {
                                flush.as_mut().reset(Instant::now() + EQ_COALESCE);
                            }
                            pending_eq = Some(bands.clone());
                            // Show the new curve immediately so faders don't jump back while coalescing.
                            if let Some(eq) = state.eq.as_mut() {
                                eq.values = bands.clone();
                                eq.preset = None;
                            }
                            sink.state(state);
                            let _ = reply.send(Ok(()));
                        } else {
                            let result = device
                                .set_setting_values(values)
                                .await
                                .map_err(|e| AudioError::Unavailable(format!("Couldn't change the headphones: {e}")));
                            let _ = reply.send(result);
                            publish(state);
                        }
                    }
                },
            },
        }
    }
}
```

- [ ] **Step 7: Run to verify pass**

Run: `cargo test --manifest-path M:\Resonance\src-tauri\Cargo.toml --lib headset::`
Expected: PASS (model 3, mapping 6, manager 7).

- [ ] **Step 8: Commit**

```powershell
git -C M:\Resonance add -A
git -C M:\Resonance commit -m "feat(headset): connector, device adapter and reconnecting manager with coalesced EQ"
```

---

### Task 4: Integration tests — simulated and real Space One

**Files:**
- Create: `src-tauri/tests/headset_demo.rs`, `src-tauri/tests/headset_hw.rs`

**Interfaces:**
- Consumes: `resonance_lib::headset::{LibConnector, device::HeadsetConnector, mapping::read_state, model::*}`.

- [ ] **Step 1: Write the demo-device test `src-tauri/tests/headset_demo.rs`**

```rust
use resonance_lib::headset::{
    device::{HeadsetConnector, HeadsetDevice},
    mapping::{command_values, read_state},
    model::{AmbientMode, Connection, HeadsetCommand},
    LibConnector,
};

#[tokio::test(flavor = "multi_thread")]
async fn simulated_space_one_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let connector = LibConnector::open(dir.path().join("openscq30.sqlite"), true).await.expect("session");
    let found = connector.connect().await.expect("connect").expect("a demo Space One");
    let state = read_state(found.device.as_ref(), &found.name);
    assert_eq!(state.connection, Connection::Connected);
    assert!(state.ambient_mode.is_some());
    let eq = state.eq.clone().expect("eq");
    assert_eq!(eq.band_hz.len(), 8);
    assert!(eq.presets.len() >= 20, "expected the full preset list, got {}", eq.presets.len());

    let values = command_values(&HeadsetCommand::SetAmbientMode { mode: AmbientMode::Transparency }, &state).unwrap();
    found.device.set_setting_values(values).await.expect("write");
    let after = read_state(found.device.as_ref(), &found.name);
    assert_eq!(after.ambient_mode, Some(AmbientMode::Transparency));

    // Reconnecting reuses the stored pairing.
    assert!(connector.connect().await.expect("reconnect").is_some());
}
```

- [ ] **Step 2: Write the hardware test `src-tauri/tests/headset_hw.rs`**

```rust
use resonance_lib::headset::{
    device::{HeadsetConnector, HeadsetDevice},
    mapping::{command_values, read_state},
    model::{AmbientMode, HeadsetCommand},
    LibConnector,
};

/// Needs the real Space One switched on and connected to this PC.
#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn real_space_one_reads_and_restores_ambient_mode() {
    let dir = tempfile::tempdir().unwrap();
    let connector = LibConnector::open(dir.path().join("openscq30.sqlite"), false).await.expect("session");
    let found = connector.connect().await.expect("connect").expect("Space One not found — is it on and paired?");
    let before = read_state(found.device.as_ref(), &found.name);
    println!("{before:#?}");
    assert!(before.battery.is_some(), "battery level should be reported");
    let original = before.ambient_mode.expect("ambient mode");
    let other = if original == AmbientMode::Normal { AmbientMode::Transparency } else { AmbientMode::Normal };

    found.device.set_setting_values(command_values(&HeadsetCommand::SetAmbientMode { mode: other }, &before).unwrap()).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    assert_eq!(read_state(found.device.as_ref(), &found.name).ambient_mode, Some(other));

    found.device.set_setting_values(command_values(&HeadsetCommand::SetAmbientMode { mode: original }, &before).unwrap()).await.unwrap();
}
```

- [ ] **Step 3: Run the demo test**

Run: `cargo test --manifest-path M:\Resonance\src-tauri\Cargo.toml --test headset_demo`
Expected: PASS. If the demo device does not apply writes, the assertion on `after` fails — then check the demo registry in the openscq30 checkout before changing anything (systematic debugging).

- [ ] **Step 4: Run the hardware test with the headphones on**

Run: `cargo test --manifest-path M:\Resonance\src-tauri\Cargo.toml --test headset_hw -- --ignored --nocapture`
Expected: PASS; the printed state shows the real battery, firmware and EQ. You will hear the mode change briefly and switch back.

- [ ] **Step 5: Commit**

```powershell
git -C M:\Resonance add -A
git -C M:\Resonance commit -m "test(headset): simulated and real Space One integration tests"
```

---

### Task 5: Tauri wiring — commands, event, background task, flyout height

**Files:**
- Modify: `src-tauri/src/app/mod.rs`, `src-tauri/src/app/commands.rs`, `src-tauri/src/app/sink.rs`, `src-tauri/src/app/wnd.rs`

**Interfaces:**
- Produces Tauri commands `headset_command({ cmd: HeadsetCommand })` and `get_headset() -> HeadsetState`; event `headset` (HeadsetState).

- [ ] **Step 1: Add the sink (append to `src-tauri/src/app/sink.rs`)**

```rust
pub struct HeadsetEvents {
    pub app: AppHandle,
}

impl crate::headset::HeadsetSink for HeadsetEvents {
    fn state(&self, state: &crate::headset::model::HeadsetState) {
        let _ = self.app.emit("headset", state);
    }
}
```

- [ ] **Step 2: Add the commands (append to `src-tauri/src/app/commands.rs`)**

```rust
#[tauri::command]
pub async fn headset_command(
    headset: State<'_, crate::headset::HeadsetHandle>,
    cmd: crate::headset::model::HeadsetCommand,
) -> Result<()> {
    headset.command(cmd).await
}

#[tauri::command]
pub async fn get_headset(headset: State<'_, crate::headset::HeadsetHandle>) -> Result<crate::headset::model::HeadsetState> {
    headset.state().await
}
```

- [ ] **Step 3: Start the manager in `src-tauri/src/app/mod.rs`**

Inside `.setup(move |app| { ... })`, directly after `app.manage(worker);`, insert:
```rust
            let (headset, inbox) = crate::headset::HeadsetHandle::channel();
            app.manage(headset);
            let headset_sink = sink::HeadsetEvents { app: handle.clone() };
            let demo = std::env::var_os("RESONANCE_DEMO_HEADSET").is_some();
            tauri::async_runtime::spawn(async move {
                match crate::headset::LibConnector::open(paths::data_dir().join("openscq30.sqlite"), demo).await {
                    Ok(connector) => crate::headset::run(std::sync::Arc::new(connector), inbox, headset_sink).await,
                    Err(e) => {
                        tracing::error!("headphone support unavailable: {e}");
                        use crate::headset::{model::{Connection, HeadsetState}, HeadsetSink};
                        headset_sink.state(&HeadsetState::with_connection(Connection::NotFound));
                    }
                }
            });
```
and add the two commands to `generate_handler!`:
```rust
            commands::headset_command,
            commands::get_headset,
```

- [ ] **Step 4: Make room for the headphone strip in the flyout**

In `src-tauri/src/app/wnd.rs` change:
```rust
const FLYOUT_H: f64 = 600.0;
```
to:
```rust
const FLYOUT_H: f64 = 680.0;
```

- [ ] **Step 5: Build and run all Rust tests**

Run:
```powershell
cargo build --manifest-path M:\Resonance\src-tauri\Cargo.toml
cargo test --manifest-path M:\Resonance\src-tauri\Cargo.toml --lib
cargo clippy --manifest-path M:\Resonance\src-tauri\Cargo.toml --all-targets -- -D warnings
```
Expected: build `Finished`; all tests PASS; clippy clean.

- [ ] **Step 6: Commit**

```powershell
git -C M:\Resonance add -A
git -C M:\Resonance commit -m "feat(app): run the headset manager and expose headset commands and events"
```

---

### Task 6: Frontend foundation — types, API, store slice, helpers, subscription

**Files:**
- Modify: `src/lib/types.ts` (append), `src/lib/api.ts` (append), `src/lib/store.ts` (replace), `src/lib/bootstrap.ts` (replace), `src/test/fixtures.ts` (append)
- Create: `src/lib/headset.ts`, `src/lib/headset.test.ts`
- Test: `src/lib/store.test.ts` (append)

**Interfaces:**
- Produces: TS types `AmbientMode, NcMode, HeadsetConnection, Battery, EqPreset, HeadsetEq, HeadsetState, HeadsetCommand`; `sendHeadset(cmd)`, `getHeadset()`, `onHeadset(cb)`; store fields `headset: HeadsetState | null`, `setHeadset(h)`; `runHeadset(cmd): Promise<boolean>`; `lib/headset.ts`: `isConnected(h)`, `connectionLabel(c)`, `AMBIENT_OPTIONS`, `LEVEL_OPTIONS`; fixture `makeHeadset(partial)`.

- [ ] **Step 1: Append the types to `src/lib/types.ts`**

```ts
export type AmbientMode = "noiseCanceling" | "transparency" | "normal";
export type NcMode = "adaptive" | "manual";

export type HeadsetConnection =
  | { state: "connecting" }
  | { state: "connected" }
  | { state: "notFound" }
  | { state: "disconnected"; retryInSecs: number };

export interface Battery {
  level: number;
  max: number;
  charging: boolean | null;
}

export interface EqPreset {
  id: string;
  name: string;
}

export interface HeadsetEq {
  presets: EqPreset[];
  preset: string | null;
  bandHz: number[];
  values: number[];
  min: number;
  max: number;
  fractionDigits: number;
}

export interface HeadsetState {
  connection: HeadsetConnection;
  name: string | null;
  ambientMode: AmbientMode | null;
  ncMode: NcMode | null;
  manualNc: number | null;
  adaptiveNc: number | null;
  transparency: number | null;
  windNoise: boolean | null;
  battery: Battery | null;
  eq: HeadsetEq | null;
  firmware: string | null;
  serial: string | null;
}

export type HeadsetCommand =
  | { type: "setAmbientMode"; mode: AmbientMode }
  | { type: "setNcMode"; mode: NcMode }
  | { type: "setManualNc"; level: number }
  | { type: "setTransparency"; level: number }
  | { type: "setWindNoise"; enabled: boolean }
  | { type: "setEqPreset"; preset: string }
  | { type: "setEqBands"; values: number[] }
  | { type: "retry" };
```

- [ ] **Step 2: Append to `src/lib/api.ts`**

```ts
import type { HeadsetCommand, HeadsetState } from "./types";

export const sendHeadset = (cmd: HeadsetCommand) => invoke<void>("headset_command", { cmd });
export const getHeadset = () => invoke<HeadsetState>("get_headset");
export const onHeadset = (cb: (h: HeadsetState) => void): Promise<UnlistenFn> =>
  listen<HeadsetState>("headset", (e) => cb(e.payload));
```

- [ ] **Step 3: Append the fixture to `src/test/fixtures.ts`**

```ts
import type { HeadsetState } from "../lib/types";

export function makeHeadset(p: Partial<HeadsetState> = {}): HeadsetState {
  return {
    connection: { state: "connected" },
    name: "soundcore Space One",
    ambientMode: "noiseCanceling",
    ncMode: "manual",
    manualNc: 3,
    adaptiveNc: 2,
    transparency: 4,
    windNoise: false,
    battery: { level: 4, max: 5, charging: null },
    eq: {
      presets: [
        { id: "SoundcoreSignature", name: "Soundcore Signature" },
        { id: "Acoustic", name: "Acoustic" },
      ],
      preset: "SoundcoreSignature",
      bandHz: [100, 200, 400, 800, 1600, 3200, 6400, 12800],
      values: [0, 0, 0, 0, 0, 0, 0, 0],
      min: -120,
      max: 134,
      fractionDigits: 1,
    },
    firmware: "06.87",
    serial: "3035705028ABCDEF",
    ...p,
  };
}
```

- [ ] **Step 4: Write the failing tests**

`src/lib/headset.test.ts`:
```ts
import { AMBIENT_OPTIONS, LEVEL_OPTIONS, connectionLabel, isConnected } from "./headset";
import { makeHeadset } from "../test/fixtures";

describe("headset helpers", () => {
  it("knows when the headphones are usable", () => {
    expect(isConnected(makeHeadset())).toBe(true);
    expect(isConnected(makeHeadset({ connection: { state: "connecting" } }))).toBe(false);
    expect(isConnected(null)).toBe(false);
  });

  it("describes every connection state in plain words", () => {
    expect(connectionLabel({ state: "connected" })).toBe("Connected");
    expect(connectionLabel({ state: "connecting" })).toBe("Connecting…");
    expect(connectionLabel({ state: "notFound" })).toBe("Not found. Switch it on and pair it with this PC");
    expect(connectionLabel({ state: "disconnected", retryInSecs: 10 })).toBe("Not connected, retrying in 10 s");
  });

  it("offers the three ambient modes and five levels", () => {
    expect(AMBIENT_OPTIONS.map((o) => o.label)).toEqual(["Noise cancelling", "Transparency", "Normal"]);
    expect(LEVEL_OPTIONS.map((o) => o.value)).toEqual([1, 2, 3, 4, 5]);
  });
});
```

Append to `src/lib/store.test.ts` (and add `sendHeadset: vi.fn()` to its `vi.mock("./api", ...)` factory):
```ts
import { runHeadset } from "./store";
import { makeHeadset } from "../test/fixtures";

describe("runHeadset", () => {
  beforeEach(() => useStore.setState({ toasts: [] }));

  it("toasts headset errors", async () => {
    vi.mocked(api.sendHeadset).mockRejectedValueOnce({ code: "unavailable", message: "Space One isn't connected" });
    expect(await runHeadset({ type: "retry" })).toBe(false);
    expect(useStore.getState().toasts.at(-1)?.message).toBe("Space One isn't connected");
  });

  it("stores the latest headset state", () => {
    useStore.getState().setHeadset(makeHeadset({ name: "x" }));
    expect(useStore.getState().headset?.name).toBe("x");
  });
});
```

- [ ] **Step 5: Run to verify failure**

Run: `npm.cmd test -- src/lib`
Expected: FAIL — `Failed to resolve import "./headset"`, `runHeadset` not exported.

- [ ] **Step 6: Implement**

`src/lib/headset.ts`:
```ts
import type { AmbientMode, HeadsetConnection, HeadsetState } from "./types";

export const AMBIENT_OPTIONS: { value: AmbientMode; label: string }[] = [
  { value: "noiseCanceling", label: "Noise cancelling" },
  { value: "transparency", label: "Transparency" },
  { value: "normal", label: "Normal" },
];

export const LEVEL_OPTIONS = [1, 2, 3, 4, 5].map((n) => ({ value: n, label: String(n) }));

export function isConnected(h: HeadsetState | null): h is HeadsetState {
  return h?.connection.state === "connected";
}

export function connectionLabel(c: HeadsetConnection): string {
  switch (c.state) {
    case "connected":
      return "Connected";
    case "connecting":
      return "Connecting…";
    case "notFound":
      return "Not found. Switch it on and pair it with this PC";
    case "disconnected":
      return `Not connected, retrying in ${c.retryInSecs} s`;
  }
}
```

Replace `src/lib/store.ts` with:
```ts
import { create } from "zustand";
import { send, sendHeadset, setMicHotkey } from "./api";
import type { Command, Device, HeadsetCommand, HeadsetState, MicConfig, UiState } from "./types";

export interface Toast {
  id: number;
  kind: "error" | "info";
  message: string;
}

interface Store {
  state: UiState | null;
  headset: HeadsetState | null;
  toasts: Toast[];
  setState: (s: UiState) => void;
  setHeadset: (h: HeadsetState) => void;
  pushToast: (kind: Toast["kind"], message: string) => void;
  dismissToast: (id: number) => void;
}

let nextToastId = 1;

export const useStore = create<Store>((set) => ({
  state: null,
  headset: null,
  toasts: [],
  setState: (state) => set({ state }),
  setHeadset: (headset) => set({ headset }),
  pushToast: (kind, message) =>
    set((s) => ({
      toasts: [...s.toasts.filter((t) => t.message !== message), { id: nextToastId++, kind, message }].slice(-4),
    })),
  dismissToast: (id) => set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) })),
}));

export function errorMessage(e: unknown): string {
  if (typeof e === "string") return e;
  if (e && typeof e === "object" && "message" in e && typeof (e as { message: unknown }).message === "string") {
    return (e as { message: string }).message;
  }
  return "Something went wrong";
}

async function attempt(action: () => Promise<unknown>): Promise<boolean> {
  try {
    await action();
    return true;
  } catch (e) {
    useStore.getState().pushToast("error", errorMessage(e));
    return false;
  }
}

/** Sends an audio command; failures become toasts. */
export const run = (cmd: Command) => attempt(() => send(cmd));

/** Sends a headphone command; failures become toasts. */
export const runHeadset = (cmd: HeadsetCommand) => attempt(() => sendHeadset(cmd));

/** Registers the hotkey (validated by the backend) and persists it. */
export const saveMicConfig = (config: MicConfig) => attempt(() => setMicHotkey(config));

export const outputs = (s: UiState): Device[] =>
  s.devices.filter((d) => d.flow === "render" && d.state === "active" && !d.hidden);
export const inputs = (s: UiState): Device[] =>
  s.devices.filter((d) => d.flow === "capture" && d.state === "active" && !d.hidden);
export const defaultOutput = (s: UiState): Device | undefined =>
  s.devices.find((d) => d.flow === "render" && d.defaultConsole && d.state === "active");
export const defaultMic = (s: UiState): Device | undefined =>
  s.devices.find((d) => d.flow === "capture" && d.defaultConsole && d.state === "active");
```

Replace `src/lib/bootstrap.ts` with:
```ts
import { useEffect } from "react";
import { getHeadset, getState, onHeadset, onMeters, onNotice, onState, takeNotices } from "./api";
import { ingestFrame } from "./meters";
import { errorMessage, useStore } from "./store";

/** Loads the initial state and subscribes to backend events for this window. */
export function useBootstrap() {
  useEffect(() => {
    let disposed = false;
    const { setState, setHeadset, pushToast } = useStore.getState();
    const unlisteners = [
      onState((s) => setState(s)),
      onHeadset((h) => setHeadset(h)),
      onMeters(ingestFrame),
      onNotice((e) => pushToast("error", e.message)),
    ];
    getState()
      .then((s) => !disposed && setState(s))
      .catch((e) => pushToast("error", errorMessage(e)));
    getHeadset()
      .then((h) => !disposed && setHeadset(h))
      .catch(() => {});
    takeNotices()
      .then((ns) => ns.forEach((n) => pushToast("error", n.message)))
      .catch(() => {});
    return () => {
      disposed = true;
      unlisteners.forEach((p) => p.then((u) => u()).catch(() => {}));
    };
  }, []);
}
```

- [ ] **Step 7: Run to verify pass**

Run: `npm.cmd test` then `npm.cmd run build`
Expected: all tests PASS (53 from Phase 1 + new); build clean.

- [ ] **Step 8: Commit**

```powershell
git -C M:\Resonance add -A
git -C M:\Resonance commit -m "feat(ui): headset types, API, store slice and subscription"
```

---

### Task 7: Components — lever switch, battery gauge, EQ faders, flyout strip

**Files:**
- Create: `src/components/LeverSwitch.tsx`, `src/components/BatteryGauge.tsx`, `src/components/EqFaders.tsx`, `src/components/HeadsetStrip.tsx`
- Modify: `src/components/ModeSwitch.tsx` (replace), `src/components/Slider.tsx`, `src/components/ui.tsx`
- Test: `src/components/BatteryGauge.test.tsx`, `src/components/EqFaders.test.tsx`, `src/components/HeadsetStrip.test.tsx`

**Interfaces:**
- Produces: `LeverSwitch<T extends string>({value, options, onChange, label, disabled?, className?})`; `BatteryGauge({battery, size?: "sm" | "md"})`; `EqFaders({eq, disabled?, onChange(values)})`, `formatHz(hz)`, `formatEqValue(value, fractionDigits)`; `HeadsetStrip({headset})`; `Slider` gains `valueText?: (v: number) => string`; `Segmented` and `Toggle` gain `disabled?: boolean`.

- [ ] **Step 1: Write the failing tests**

`src/components/BatteryGauge.test.tsx`:
```tsx
import { render, screen } from "@testing-library/react";
import { BatteryGauge } from "./BatteryGauge";

describe("BatteryGauge", () => {
  it("names the level for screen readers", () => {
    render(<BatteryGauge battery={{ level: 4, max: 5, charging: null }} />);
    expect(screen.getByRole("img", { name: "Battery 4 of 5" })).toBeInTheDocument();
  });

  it("mentions charging only when the device reports it", () => {
    render(<BatteryGauge battery={{ level: 1, max: 5, charging: true }} />);
    expect(screen.getByRole("img", { name: "Battery 1 of 5, charging" })).toBeInTheDocument();
  });
});
```

`src/components/EqFaders.test.tsx`:
```tsx
import { fireEvent, render, screen } from "@testing-library/react";
import { EqFaders, formatEqValue, formatHz } from "./EqFaders";
import { makeHeadset } from "../test/fixtures";

describe("EqFaders", () => {
  it("formats values and frequencies", () => {
    expect(formatEqValue(25, 1)).toBe("+2.5");
    expect(formatEqValue(-120, 1)).toBe("−12.0");
    expect(formatEqValue(0, 1)).toBe("0.0");
    expect(formatHz(800)).toBe("800");
    expect(formatHz(12800)).toBe("12.8k");
  });

  it("renders one fader per band and reports the changed curve", () => {
    const onChange = vi.fn();
    const eq = makeHeadset().eq!;
    render(<EqFaders eq={eq} onChange={onChange} />);
    expect(screen.getAllByRole("slider")).toHaveLength(8);
    fireEvent.change(screen.getByRole("slider", { name: "800 Hz" }), { target: { value: "50" } });
    expect(onChange).toHaveBeenCalledWith([0, 0, 0, 7, 0, 0, 0, 0]);
  });
});
```

`src/components/HeadsetStrip.test.tsx`:
```tsx
import { fireEvent, render, screen } from "@testing-library/react";
import { HeadsetStrip } from "./HeadsetStrip";
import { makeHeadset } from "../test/fixtures";
import * as api from "../lib/api";

vi.mock("../lib/api", () => ({ sendHeadset: vi.fn().mockResolvedValue(undefined) }));

describe("HeadsetStrip", () => {
  beforeEach(() => vi.mocked(api.sendHeadset).mockClear());

  it("shows battery and switches the ambient mode", () => {
    render(<HeadsetStrip headset={makeHeadset()} />);
    expect(screen.getByRole("img", { name: "Battery 4 of 5" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("radio", { name: "Normal" }));
    expect(api.sendHeadset).toHaveBeenCalledWith({ type: "setAmbientMode", mode: "normal" });
  });

  it("explains when the headphones aren't connected", () => {
    render(<HeadsetStrip headset={makeHeadset({ connection: { state: "disconnected", retryInSecs: 5 } })} />);
    expect(screen.getByText("Not connected, retrying in 5 s")).toBeInTheDocument();
    expect(screen.queryByRole("radio")).toBeNull();
  });
});
```

- [ ] **Step 2: Run to verify failure**

Run: `npm.cmd test -- src/components`
Expected: FAIL — unresolved `./BatteryGauge`, `./EqFaders`, `./HeadsetStrip`.

- [ ] **Step 3: Implement the shared controls**

`src/components/LeverSwitch.tsx`:
```tsx
interface Props<T extends string> {
  value: T;
  options: { value: T; label: string }[];
  onChange: (value: T) => void;
  label: string;
  disabled?: boolean;
  className?: string;
}

/** Multi-position lever switch (machined lever in a slot) with labelled positions. */
export function LeverSwitch<T extends string>({ value, options, onChange, label, disabled = false, className = "w-[300px]" }: Props<T>) {
  const n = options.length;
  const index = Math.max(0, options.findIndex((o) => o.value === value));
  return (
    <div className={`max-w-full ${className} ${disabled ? "opacity-45" : ""}`}>
      <div aria-hidden className="relative h-[30px] rounded-lg bg-[var(--slot)] shadow-[inset_0_0_0_1px_var(--groove),inset_0_2px_5px_rgb(0_0_0/0.6)]">
        <div
          className="absolute top-[3px] bottom-[3px] rounded-md border border-[var(--cap-edge)] bg-linear-to-b from-[var(--cap-hi)] to-[var(--cap-lo)] shadow-[0_3px_8px_var(--shadow)] transition-[left] duration-150"
          style={{ left: `calc(${(index * 100) / n}% + 3px)`, width: `calc(${100 / n}% - 6px)` }}
        >
          <div className="absolute top-[7px] bottom-[7px] left-1/2 -ml-px w-0.5 rounded-sm bg-accent shadow-[0_0_6px_var(--accent)]" />
        </div>
      </div>
      <div role="radiogroup" aria-label={label} className="mt-1.5 flex text-center">
        {options.map((o, i) => (
          <button
            key={o.value}
            type="button"
            role="radio"
            aria-checked={i === index}
            disabled={disabled}
            onClick={() => onChange(o.value)}
            className={`flex-1 border-0 bg-transparent py-1.5 text-[12px] disabled:cursor-default ${
              i === index ? "font-semibold text-paper" : "text-dim hover:text-fog"
            }`}
          >
            {o.label}
          </button>
        ))}
      </div>
    </div>
  );
}
```

Replace `src/components/ModeSwitch.tsx` with:
```tsx
import type { HotkeyMode } from "../lib/types";
import { LeverSwitch } from "./LeverSwitch";

const MODES: { value: HotkeyMode; label: string }[] = [
  { value: "toggle", label: "Toggle" },
  { value: "pushToTalk", label: "Push to talk" },
  { value: "pushToMute", label: "Push to mute" },
];

/** Three-position lever for the mute shortcut's behaviour. */
export function ModeSwitch({ value, onChange }: { value: HotkeyMode; onChange: (m: HotkeyMode) => void }) {
  return <LeverSwitch label="Shortcut behaviour" value={value} options={MODES} onChange={onChange} />;
}
```

In `src/components/Slider.tsx`: add `valueText?: (value: number) => string;` to `Props`, add `valueText` to the destructured parameters, and change the `aria-valuetext` line to:
```tsx
      aria-valuetext={(valueText ?? formatDb)(shown)}
```

In `src/components/ui.tsx` replace `Segmented` and `Toggle` with:
```tsx
export function Segmented<T extends string | number>({
  value,
  options,
  onChange,
  label,
  disabled = false,
}: {
  value: T;
  options: { value: T; label: string }[];
  onChange: (v: T) => void;
  label: string;
  disabled?: boolean;
}) {
  return (
    <div role="radiogroup" aria-label={label} className={`inline-flex rounded-[9px] border border-groove bg-night p-0.5 ${disabled ? "opacity-45" : ""}`}>
      {options.map((o) => (
        <button
          key={String(o.value)}
          type="button"
          role="radio"
          aria-checked={o.value === value}
          disabled={disabled}
          onClick={() => onChange(o.value)}
          className={`rounded-[7px] px-3 py-1 text-[13px] transition-colors disabled:cursor-default ${
            o.value === value ? "bg-accent text-white" : "text-fog hover:text-paper"
          }`}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

export function Toggle({
  checked,
  onChange,
  label,
  disabled = false,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
  label: string;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
      onClick={() => onChange(!checked)}
      className={`relative h-6 w-11 rounded-full border transition-colors disabled:opacity-45 ${checked ? "border-accent bg-accent" : "border-groove bg-night"}`}
    >
      <span
        className={`absolute top-[3px] left-0 h-4 w-4 rounded-full bg-white shadow transition-transform ${checked ? "translate-x-[22px]" : "translate-x-[3px]"}`}
      />
    </button>
  );
}
```

- [ ] **Step 4: Implement the headphone components**

`src/components/BatteryGauge.tsx`:
```tsx
import { Zap } from "lucide-react";
import type { Battery } from "../lib/types";

/** Segmented cell gauge; the Space One reports 5 steps, so no fake percentage. */
export function BatteryGauge({ battery, size = "md" }: { battery: Battery; size?: "sm" | "md" }) {
  const low = battery.level <= 1;
  const cell = size === "sm" ? "h-2.5 w-[7px]" : "h-4 w-3";
  const label = `Battery ${battery.level} of ${battery.max}${battery.charging ? ", charging" : ""}`;
  return (
    <div role="img" aria-label={label} title={label} className="flex items-center gap-1.5">
      <div className="flex items-center gap-[3px] rounded-[5px] border border-groove bg-[var(--slot)] p-[3px]">
        {Array.from({ length: battery.max }, (_, i) => (
          <span
            key={i}
            className={`${cell} rounded-[2px] ${
              i < battery.level
                ? low
                  ? "bg-coral shadow-[0_0_6px_var(--coral)]"
                  : "bg-lilac shadow-[0_0_6px_rgb(201_184_255/0.5)]"
                : "bg-[var(--ladder-off)]"
            }`}
          />
        ))}
      </div>
      {battery.charging && <Zap size={size === "sm" ? 12 : 14} className="text-lilac" aria-hidden />}
    </div>
  );
}
```

`src/components/EqFaders.tsx`:
```tsx
import type { HeadsetEq } from "../lib/types";
import { Slider } from "./Slider";

export function formatHz(hz: number): string {
  return hz >= 1000 ? `${hz / 1000}k` : `${hz}`;
}

/** Device units → signed dB text, e.g. 25 with 1 fraction digit → "+2.5". */
export function formatEqValue(value: number, fractionDigits: number): string {
  const db = value / 10 ** fractionDigits;
  const sign = db > 0 ? "+" : db < 0 ? "−" : "";
  return `${sign}${Math.abs(db).toFixed(fractionDigits)}`;
}

/** One vertical console fader per EQ band, scaled to the device's own range. */
export function EqFaders({ eq, disabled, onChange }: { eq: HeadsetEq; disabled?: boolean; onChange: (values: number[]) => void }) {
  const span = eq.max - eq.min;
  const toDevice = (x: number) => Math.round(eq.min + x * span);
  return (
    <div className="flex min-h-[260px] justify-between gap-2">
      {eq.bandHz.map((hz, i) => {
        const v = eq.values[i] ?? 0;
        return (
          <div key={hz} className="flex w-14 flex-col items-center gap-2">
            <div className="num text-[11px] text-fog">{formatEqValue(v, eq.fractionDigits)}</div>
            <div className="flex min-h-[190px] flex-1 justify-center">
              <Slider
                orientation="vertical"
                value={(v - eq.min) / span}
                label={`${formatHz(hz)} Hz`}
                disabled={disabled}
                valueText={(x) => `${formatEqValue(toDevice(x), eq.fractionDigits)} dB`}
                onChange={(x) => {
                  const next = [...eq.values];
                  next[i] = toDevice(x);
                  onChange(next);
                }}
              />
            </div>
            <div className="num text-[11px] text-dim">{formatHz(hz)}</div>
          </div>
        );
      })}
    </div>
  );
}
```

`src/components/HeadsetStrip.tsx`:
```tsx
import { AMBIENT_OPTIONS, connectionLabel, isConnected } from "../lib/headset";
import { runHeadset } from "../lib/store";
import type { HeadsetState } from "../lib/types";
import { BatteryGauge } from "./BatteryGauge";
import { LeverSwitch } from "./LeverSwitch";

/** Tray-flyout strip: headphone name, battery and the ambient-mode lever. */
export function HeadsetStrip({ headset }: { headset: HeadsetState }) {
  const connected = isConnected(headset);
  return (
    <section aria-label="Headphones" className="border-b border-groove px-[18px] py-3">
      <div className="mb-2 flex items-center justify-between gap-3">
        <span className="truncate text-[13px] font-medium">{headset.name ?? "Space One"}</span>
        {connected && headset.battery ? (
          <BatteryGauge battery={headset.battery} size="sm" />
        ) : (
          <span className="text-[12px] text-dim">{connectionLabel(headset.connection)}</span>
        )}
      </div>
      {connected && headset.ambientMode && (
        <LeverSwitch
          className="w-full"
          label="Noise control"
          value={headset.ambientMode}
          options={AMBIENT_OPTIONS}
          onChange={(mode) => runHeadset({ type: "setAmbientMode", mode })}
        />
      )}
    </section>
  );
}
```

- [ ] **Step 5: Run to verify pass**

Run: `npm.cmd test` then `npm.cmd run build`
Expected: all PASS (including Phase 1's MicView shortcut-behaviour test, now backed by `LeverSwitch`); build clean.

- [ ] **Step 6: Commit**

```powershell
git -C M:\Resonance add -A
git -C M:\Resonance commit -m "feat(ui): lever switch, battery gauge, EQ faders and headphone flyout strip"
```

---

### Task 8: Headphones page, title-bar tab and flyout integration

**Files:**
- Create: `src/views/HeadphonesView.tsx`, `src/views/HeadphonesView.test.tsx`
- Modify: `src/views/Titlebar.tsx`, `src/views/MainWindow.tsx`, `src/views/Flyout.tsx`

**Interfaces:**
- Consumes: Tasks 6–7.
- Produces: `HeadphonesView({ headset: HeadsetState | null })`; `View` gains `"headphones"`.

- [ ] **Step 1: Write the failing view tests `src/views/HeadphonesView.test.tsx`**

```tsx
import { fireEvent, render, screen } from "@testing-library/react";
import { HeadphonesView } from "./HeadphonesView";
import { makeHeadset } from "../test/fixtures";
import * as api from "../lib/api";

vi.mock("../lib/api", () => ({ sendHeadset: vi.fn().mockResolvedValue(undefined) }));

describe("HeadphonesView", () => {
  beforeEach(() => vi.mocked(api.sendHeadset).mockClear());

  it("waits for the first state", () => {
    render(<HeadphonesView headset={null} />);
    expect(screen.getByText(/connecting/i)).toBeInTheDocument();
  });

  it("switches ambient mode and manual noise-cancelling level", () => {
    render(<HeadphonesView headset={makeHeadset()} />);
    fireEvent.click(screen.getByRole("radio", { name: "4" }));
    expect(api.sendHeadset).toHaveBeenCalledWith({ type: "setManualNc", level: 4 });
    fireEvent.click(screen.getByRole("radio", { name: "Adaptive" }));
    expect(api.sendHeadset).toHaveBeenCalledWith({ type: "setNcMode", mode: "adaptive" });
    fireEvent.click(screen.getByRole("radio", { name: "Transparency" }));
    expect(api.sendHeadset).toHaveBeenCalledWith({ type: "setAmbientMode", mode: "transparency" });
  });

  it("shows the adaptive level read-only", () => {
    render(<HeadphonesView headset={makeHeadset({ ncMode: "adaptive" })} />);
    expect(screen.getByText("Adjusting automatically, currently 2 of 5")).toBeInTheDocument();
    expect(screen.queryByRole("radiogroup", { name: "Noise cancelling level" })).toBeNull();
  });

  it("offers transparency strength and wind-noise reduction in transparency mode", () => {
    render(<HeadphonesView headset={makeHeadset({ ambientMode: "transparency" })} />);
    fireEvent.click(screen.getByRole("radio", { name: "2" }));
    expect(api.sendHeadset).toHaveBeenCalledWith({ type: "setTransparency", level: 2 });
    fireEvent.click(screen.getByRole("switch", { name: "Reduce wind noise" }));
    expect(api.sendHeadset).toHaveBeenCalledWith({ type: "setWindNoise", enabled: true });
  });

  it("hides strength controls in normal mode", () => {
    render(<HeadphonesView headset={makeHeadset({ ambientMode: "normal" })} />);
    expect(screen.queryByRole("switch", { name: "Reduce wind noise" })).toBeNull();
    expect(screen.queryByRole("radio", { name: "1" })).toBeNull();
  });

  it("picks EQ presets and shows Custom for a custom curve", () => {
    const { rerender } = render(<HeadphonesView headset={makeHeadset()} />);
    fireEvent.change(screen.getByRole("combobox", { name: "EQ preset" }), { target: { value: "Acoustic" } });
    expect(api.sendHeadset).toHaveBeenCalledWith({ type: "setEqPreset", preset: "Acoustic" });
    const custom = makeHeadset();
    custom.eq = { ...custom.eq!, preset: null };
    rerender(<HeadphonesView headset={custom} />);
    expect(screen.getByRole("combobox", { name: "EQ preset" })).toHaveValue("");
    expect(screen.getByRole("option", { name: "Custom" })).toBeInTheDocument();
  });

  it("sends EQ band edits", () => {
    render(<HeadphonesView headset={makeHeadset()} />);
    fireEvent.change(screen.getByRole("slider", { name: "100 Hz" }), { target: { value: "50" } });
    expect(api.sendHeadset).toHaveBeenCalledWith({ type: "setEqBands", values: [7, 0, 0, 0, 0, 0, 0, 0] });
  });

  it("shows battery and device details", () => {
    render(<HeadphonesView headset={makeHeadset()} />);
    expect(screen.getByText("4 of 5")).toBeInTheDocument();
    expect(screen.getByText("06.87")).toBeInTheDocument();
    expect(screen.getByText("3035705028ABCDEF")).toBeInTheDocument();
  });

  it("disables controls and offers Retry when not connected", () => {
    render(<HeadphonesView headset={makeHeadset({ connection: { state: "disconnected", retryInSecs: 10 } })} />);
    expect(screen.getByText("Not connected, retrying in 10 s")).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: "Transparency" })).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(api.sendHeadset).toHaveBeenCalledWith({ type: "retry" });
  });
});
```

- [ ] **Step 2: Run to verify failure**

Run: `npm.cmd test -- src/views`
Expected: FAIL — `Failed to resolve import "./HeadphonesView"`.

- [ ] **Step 3: Implement `src/views/HeadphonesView.tsx`**

```tsx
import { AMBIENT_OPTIONS, LEVEL_OPTIONS, connectionLabel, isConnected } from "../lib/headset";
import { runHeadset } from "../lib/store";
import type { HeadsetState, NcMode } from "../lib/types";
import { BatteryGauge } from "../components/BatteryGauge";
import { EqFaders } from "../components/EqFaders";
import { LeverSwitch } from "../components/LeverSwitch";
import { Panel, Segmented, Toggle, ViewHeader } from "../components/ui";

const NC_MODES: { value: NcMode; label: string }[] = [
  { value: "adaptive", label: "Adaptive" },
  { value: "manual", label: "Manual" },
];

function NoiseCancelling({ headset, disabled }: { headset: HeadsetState; disabled: boolean }) {
  return (
    <div className="flex flex-wrap items-end gap-8">
      <div>
        <div className="mb-2 text-[13px] text-fog">Strength</div>
        <Segmented
          label="Noise cancelling strength"
          value={headset.ncMode ?? "manual"}
          options={NC_MODES}
          disabled={disabled || !headset.ncMode}
          onChange={(mode) => runHeadset({ type: "setNcMode", mode })}
        />
      </div>
      {headset.ncMode === "manual" && headset.manualNc !== null && (
        <div>
          <div className="mb-2 text-[13px] text-fog">Level</div>
          <Segmented
            label="Noise cancelling level"
            value={headset.manualNc}
            options={LEVEL_OPTIONS}
            disabled={disabled}
            onChange={(level) => runHeadset({ type: "setManualNc", level })}
          />
        </div>
      )}
      {headset.ncMode === "adaptive" && (
        <p className="m-0 pb-1.5 text-[13px] text-fog">
          {headset.adaptiveNc ? `Adjusting automatically, currently ${headset.adaptiveNc} of 5` : "Adjusting automatically"}
        </p>
      )}
    </div>
  );
}

export function HeadphonesView({ headset }: { headset: HeadsetState | null }) {
  if (!headset) {
    return <div className="grid h-full place-items-center text-[13px] text-dim">Connecting to the headphones…</div>;
  }
  const connected = isConnected(headset);
  const off = !connected;
  const name = headset.name ?? "Soundcore Space One";
  const eq = headset.eq;

  return (
    <div className="flex flex-col gap-5">
      <ViewHeader title="Headphones" subtitle={connected ? `${name} is connected` : name} />

      {!connected && (
        <div className="flex items-center justify-between gap-4 rounded-[10px] border border-groove bg-raised px-4 py-3">
          <span className="text-[13px] text-fog">{connectionLabel(headset.connection)}</span>
          <button type="button" className="key" onClick={() => runHeadset({ type: "retry" })}>
            Retry
          </button>
        </div>
      )}

      <div className="flex flex-wrap items-stretch gap-5">
        <section aria-label="Noise control" className="console flex min-w-0 flex-[3_1_520px] flex-col gap-6 px-7 py-6">
          <h2 className="m-0 text-[15px] font-medium text-fog">Noise control</h2>
          <LeverSwitch
            className="w-[420px]"
            label="Noise control"
            value={headset.ambientMode ?? "normal"}
            options={AMBIENT_OPTIONS}
            disabled={off || !headset.ambientMode}
            onChange={(mode) => runHeadset({ type: "setAmbientMode", mode })}
          />
          {headset.ambientMode === "noiseCanceling" && <NoiseCancelling headset={headset} disabled={off} />}
          {headset.ambientMode === "transparency" && headset.transparency !== null && (
            <div>
              <div className="mb-2 text-[13px] text-fog">Transparency strength</div>
              <Segmented
                label="Transparency strength"
                value={headset.transparency}
                options={LEVEL_OPTIONS}
                disabled={off}
                onChange={(level) => runHeadset({ type: "setTransparency", level })}
              />
            </div>
          )}
          {headset.ambientMode && headset.ambientMode !== "normal" && headset.windNoise !== null && (
            <div className="flex items-center justify-between gap-4 border-t border-groove pt-5">
              <div>
                <div className="font-medium">Reduce wind noise</div>
                <div className="mt-0.5 text-[12px] text-fog">Cuts rumble when you're outside</div>
              </div>
              <Toggle
                label="Reduce wind noise"
                checked={headset.windNoise}
                disabled={off}
                onChange={(enabled) => runHeadset({ type: "setWindNoise", enabled })}
              />
            </div>
          )}
        </section>

        <Panel className="flex min-w-0 flex-[2_1_300px] flex-col gap-5 px-6 py-6">
          <h2 className="m-0 text-[15px] font-medium text-fog">Battery</h2>
          {headset.battery ? (
            <div className="flex items-center gap-4">
              <BatteryGauge battery={headset.battery} />
              <span className="num text-[14px] text-fog">
                {headset.battery.level} of {headset.battery.max}
              </span>
            </div>
          ) : (
            <p className="m-0 text-[13px] text-dim">Unavailable</p>
          )}
          <dl className="m-0 mt-auto grid grid-cols-[auto_1fr] gap-x-6 gap-y-2 border-t border-groove pt-5 text-[13px]">
            <dt className="text-fog">Firmware</dt>
            <dd className="num m-0">{headset.firmware ?? "—"}</dd>
            <dt className="text-fog">Serial number</dt>
            <dd className="num m-0 truncate">{headset.serial ?? "—"}</dd>
          </dl>
        </Panel>
      </div>

      {eq && (
        <section aria-label="Equalizer" className="console flex flex-col gap-5 px-7 py-6">
          <div className="flex flex-wrap items-center justify-between gap-4">
            <h2 className="m-0 text-[15px] font-medium text-fog">Equalizer</h2>
            <select
              aria-label="EQ preset"
              className="field-select"
              value={eq.preset ?? ""}
              disabled={off}
              onChange={(e) => e.currentTarget.value && runHeadset({ type: "setEqPreset", preset: e.currentTarget.value })}
            >
              {eq.preset === null && <option value="">Custom</option>}
              {eq.presets.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.name}
                </option>
              ))}
            </select>
          </div>
          <EqFaders eq={eq} disabled={off} onChange={(values) => runHeadset({ type: "setEqBands", values })} />
        </section>
      )}
    </div>
  );
}
```

- [ ] **Step 4: Add the tab, route and flyout strip**

In `src/views/Titlebar.tsx` change the `View` type and `TABS`:
```tsx
export type View = "mixer" | "devices" | "mic" | "headphones" | "settings";

const TABS: { id: View; label: string }[] = [
  { id: "mixer", label: "Mixer" },
  { id: "devices", label: "Devices" },
  { id: "mic", label: "Microphone" },
  { id: "headphones", label: "Headphones" },
  { id: "settings", label: "Settings" },
];
```

In `src/views/MainWindow.tsx`: add `import { HeadphonesView } from "./HeadphonesView";`, read `const headset = useStore((s) => s.headset);`, and replace the `view === "mic"` branch with:
```tsx
        ) : view === "mic" ? (
          <MicView state={state} />
        ) : view === "headphones" ? (
          <HeadphonesView headset={headset} />
        ) : (
```

In `src/views/Flyout.tsx`: add `import { HeadsetStrip } from "../components/HeadsetStrip";`, read `const headset = useStore((s) => s.headset);` next to `state`, and insert directly after the closing `</section>` of the master-output section:
```tsx
      {headset && <HeadsetStrip headset={headset} />}
```

- [ ] **Step 5: Run all frontend tests and build**

Run:
```powershell
npm.cmd test
npm.cmd run build
```
Expected: all PASS; build clean.

- [ ] **Step 6: Visual check with the simulated headphones**

Run (PowerShell): `$env:RESONANCE_DEMO_HEADSET = "1"; npm.cmd run tauri dev`
Expected: the Headphones tab shows the demo Space One — lever, strength controls, battery gauge, 8 EQ faders on a console surface matching the Phase 1 look; the tray flyout shows the headphone strip with battery and lever. Change mode and EQ and watch them stick. Then quit, run `npm.cmd run tauri dev` without the variable and confirm the real Space One appears.

- [ ] **Step 7: Commit**

```powershell
git -C M:\Resonance add -A
git -C M:\Resonance commit -m "feat(ui): Headphones page, title-bar tab and flyout strip"
```

---

### Task 9: Verification, checklist and installer

**Files:**
- Modify: `docs/testing/phase1-manual-checklist.md` → append a Phase 2 section; `README.md`

- [ ] **Step 1: Append to `docs/testing/phase1-manual-checklist.md`**

```markdown

## Phase 2 — Space One
- [ ] Headphones tab shows the Space One as connected with battery, firmware and serial.
- [ ] Mode lever switches noise cancelling / transparency / normal, and you hear the change.
- [ ] Manual noise-cancelling levels 1–5 and transparency levels 1–5 apply; Adaptive shows its current level.
- [ ] Wind-noise reduction toggles.
- [ ] Choosing an EQ preset changes the sound; moving a fader switches the picker to Custom and the sound follows smoothly.
- [ ] Pressing the headphones' own ANC button updates Resonance within about a second.
- [ ] Turning the headphones off shows "Not connected, retrying…"; turning them on reconnects without clicking Retry.
- [ ] The tray flyout strip shows battery and switches mode.
```

- [ ] **Step 2: Add a licence line to `README.md`** (append)

```markdown

## Licence

GPL-3.0 (Resonance uses [OpenSCQ30](https://github.com/oppzippy/OpenSCQ30) for Soundcore headphone control). See `LICENSE`.
```

- [ ] **Step 3: Run the full automated suite**

Run:
```powershell
cargo test --manifest-path M:\Resonance\src-tauri\Cargo.toml --lib
cargo test --manifest-path M:\Resonance\src-tauri\Cargo.toml --test headset_demo
cargo test --manifest-path M:\Resonance\src-tauri\Cargo.toml --test wasapi -- --ignored --test-threads=1
cargo test --manifest-path M:\Resonance\src-tauri\Cargo.toml --test headset_hw -- --ignored --nocapture
cargo clippy --manifest-path M:\Resonance\src-tauri\Cargo.toml --all-targets -- -D warnings
npm.cmd test
npm.cmd run build
```
Expected: everything passes (stop any running dev copy first so the executable isn't locked).

- [ ] **Step 4: Build the installer**

Run: `npm.cmd run tauri build`
Expected: `src-tauri\target\release\bundle\nsis\Resonance_0.1.0_x64-setup.exe`.

- [ ] **Step 5: Walk the Phase 2 checklist with the real headphones and commit**

```powershell
git -C M:\Resonance add -A
git -C M:\Resonance commit -m "docs: Phase 2 checklist and licence note"
```
