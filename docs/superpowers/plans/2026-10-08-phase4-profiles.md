# Resonance Phase 4 — Profiles, Start with Windows, First-run Setup Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** One-click profiles (saved with chosen parts: apps, mic & processing, headphones, default output) switched from the tray flyout or a Profiles tab, plus Start with Windows and a first-run setup.

**Architecture:** Profiles are partial snapshots in `settings.json`. `AppCore` captures and applies the audio parts (apps, processing + call sends, default output) through its existing paths, keeping output routes for apps that aren't running until they play. The Tauri commands `save_profile`/`update_profile`/`apply_profile` add the headphone part via the headset manager (state → `HeadsetPart` → headset commands). Start with Windows uses `tauri-plugin-autostart` 2 with `--minimized`; the first-run setup is a modal shown until `ui.onboarded` is set.

**Tech Stack:** Rust 1.99, Tauri 2 + `tauri-plugin-autostart` 2 (2.7.0); React 19 + TypeScript, Tailwind 4, Zustand, Vitest.

## Global Constraints

- Spec: `docs/superpowers/specs/2026-10-08-phase4-profiles-design.md`; visual language Design v2.
- Parts: **Apps** (per app key: volume, mute, output — `unknown` when the app wasn't playing at save time, `default`, or a device id), **Mic & processing** (whole `ProcessingSettings` + `call_sends`), **Headphones** (ambient mode, NC mode, manual NC, transparency, wind noise, EQ preset or custom bands), **Default output** (render device for console + communications). The default mic is never a part.
- Saving needs a non-empty name and at least one part; Headphones only while the Space One is connected ("Connect the Space One to save its settings"). Update re-captures the profile's existing parts; Headphones is kept as saved if the Space One isn't connected.
- Applying attempts everything; each failure is one notice. Copy: default output unplugged → "<name> isn't connected, so the default output wasn't changed"; headphones off → "The Space One isn't connected, so its profile settings weren't applied"; app route to an unplugged device → "<app>'s saved output isn't connected".
- Headset commands order when applying: levels and settings first (manual NC, transparency, wind noise, EQ preset or bands), then NC mode, then ambient mode last.
- Profile ids `p1`, `p2`, … (next free number). Saving or applying makes the profile active; deleting the active profile clears it. Active is persisted but nothing is applied on start-up.
- Flyout: profile buttons in a row above the master output; active one lit (`mic-live text-lilac`); hidden when there are no profiles.
- Start with Windows: `tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, Some(vec!["--minimized"]))`; launched with `--minimized` → stay in the tray even if "Start in the tray" is off. The switch reads `autolaunch().is_enabled()`.
- First-run setup: modal over the main window while `ui.onboarded` is false; steps Microphone → VB-Cable → Finish (Turn on Processing only when VB-Cable is installed; Start with Windows); Skip or Done set `onboarded: true`.
- Shell: PowerShell with `npm.cmd`; cargo via `--manifest-path M:\Resonance\src-tauri\Cargo.toml`; files may be CRLF. First match of an edit anchor must be the intended one (several structs end in similar fields). Quit the installed Resonance before `tauri dev`/hardware tests.
- Commits end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

---

## File Structure

```
src-tauri/
├─ Cargo.toml                       + tauri-plugin-autostart = "2"
├─ src/lib.rs                       + pub mod profile
├─ src/profile.rs                   AppOutput, AppSnapshot, ProcessingPart, ProfileParts, PartChoice, Profile, next_id
├─ src/headset/part.rs              HeadsetPart::{from_state, commands}
├─ src/headset/mod.rs               + pub mod part
├─ src/settings.rs                  profiles, active_profile
├─ src/model.rs                     UiSettings.onboarded; UiState.{profiles, active_profile}; profile Commands
├─ src/app_state/profiles.rs        capture/save/update/rename/delete/apply, pending outputs
├─ src/app_state/profiles_tests.rs  controller tests
├─ src/app_state/mod.rs             pending_outputs field, restore hook, build_state, Command arms
├─ src/app/commands.rs              save_profile, update_profile, apply_profile, get_autostart, set_autostart
├─ src/app/mod.rs                   autostart plugin, --minimized, command registration
└─ tests/profiles_hw.rs             hardware test
src/
├─ lib/types.ts, api.ts, store.ts   profile types/commands/actions
├─ lib/profiles.ts (+test)          partLabels
├─ test/fixtures.ts                 profiles, activeProfile, onboarded, makeProfile
├─ components/StartWithWindows.tsx  autostart switch
├─ components/ProfileBar.tsx        flyout buttons
├─ views/ProfilesView.tsx (+test)   Profiles tab
├─ views/Onboarding.tsx (+test)     first-run setup
├─ views/{Titlebar,MainWindow,Flyout,SettingsView}.tsx (+tests)
```

---

### Task 1: Profile data, headset part and settings

**Files:**
- Create: `src-tauri/src/profile.rs`, `src-tauri/src/headset/part.rs`
- Modify: `src-tauri/src/lib.rs`, `src-tauri/src/headset/mod.rs`, `src-tauri/src/settings.rs`, `src-tauri/src/model.rs`, `src-tauri/src/app_state/mod.rs`

**Interfaces:**
- Produces: `profile::{AppOutput (Unknown | Default | Device(String); JSON {"kind":"unknown"|"default"|"device","id"?}), AppSnapshot { volume, muted, output }, ProcessingPart { settings: ProcessingSettings, call_sends: BTreeMap<String, f32> }, ProfileParts { apps: Option<BTreeMap<String, AppSnapshot>>, processing: Option<ProcessingPart>, headphones: Option<HeadsetPart>, default_output: Option<String> }, PartChoice { apps, processing, headphones, default_output: bool } (+ any()), ProfileParts::choice(), Profile { id, name, parts }, next_id(&[Profile]) -> String}`; `headset::part::HeadsetPart { ambient_mode, nc_mode, manual_nc, transparency, wind_noise, eq_preset, eq_bands }` with `from_state(&HeadsetState) -> Option<Self>` and `commands(&self) -> Vec<HeadsetCommand>`; `Settings.{profiles: Vec<Profile>, active_profile: Option<String>}`; `UiSettings.onboarded: bool`; `UiState.{profiles: Vec<Profile>, active_profile: Option<String>}`; `Command::{SaveProfile { name, parts: PartChoice, headset: Option<HeadsetPart> }, UpdateProfile { id, headset: Option<HeadsetPart> }, RenameProfile { id, name }, DeleteProfile { id }, ApplyProfile { id }}`.

- [ ] **Step 1: Write `src-tauri/src/headset/part.rs` with tests**

```rust
use serde::{Deserialize, Serialize};

use super::model::{AmbientMode, Connection, HeadsetCommand, HeadsetState, NcMode};

/// The Space One settings a profile restores.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HeadsetPart {
    pub ambient_mode: Option<AmbientMode>,
    pub nc_mode: Option<NcMode>,
    pub manual_nc: Option<u8>,
    pub transparency: Option<u8>,
    pub wind_noise: Option<bool>,
    /// An EQ preset id; `None` with `eq_bands` set means a custom curve.
    pub eq_preset: Option<String>,
    pub eq_bands: Option<Vec<i16>>,
}

impl HeadsetPart {
    /// The headphones' current settings, if they're connected.
    pub fn from_state(s: &HeadsetState) -> Option<Self> {
        if s.connection != Connection::Connected {
            return None;
        }
        Some(Self {
            ambient_mode: s.ambient_mode,
            nc_mode: s.nc_mode,
            manual_nc: s.manual_nc,
            transparency: s.transparency,
            wind_noise: s.wind_noise,
            eq_preset: s.eq.as_ref().and_then(|e| e.preset.clone()),
            eq_bands: s.eq.as_ref().filter(|e| e.preset.is_none()).map(|e| e.values.clone()),
        })
    }

    /// Commands that restore these settings: levels and settings first, then the noise-cancelling mode,
    /// then the ambient mode last, so a level change can't leave the headphones in the wrong mode.
    pub fn commands(&self) -> Vec<HeadsetCommand> {
        let mut out = Vec::new();
        if let Some(level) = self.manual_nc {
            out.push(HeadsetCommand::SetManualNc { level });
        }
        if let Some(level) = self.transparency {
            out.push(HeadsetCommand::SetTransparency { level });
        }
        if let Some(enabled) = self.wind_noise {
            out.push(HeadsetCommand::SetWindNoise { enabled });
        }
        match (&self.eq_preset, &self.eq_bands) {
            (Some(preset), _) => out.push(HeadsetCommand::SetEqPreset { preset: preset.clone() }),
            (None, Some(values)) => out.push(HeadsetCommand::SetEqBands { values: values.clone() }),
            (None, None) => {}
        }
        if let Some(mode) = self.nc_mode {
            out.push(HeadsetCommand::SetNcMode { mode });
        }
        if let Some(mode) = self.ambient_mode {
            out.push(HeadsetCommand::SetAmbientMode { mode });
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::headset::model::{Eq, EqPreset};

    fn connected() -> HeadsetState {
        let mut s = HeadsetState::with_connection(Connection::Connected);
        s.ambient_mode = Some(AmbientMode::NoiseCanceling);
        s.nc_mode = Some(NcMode::Manual);
        s.manual_nc = Some(4);
        s.transparency = Some(2);
        s.wind_noise = Some(true);
        s.eq = Some(Eq {
            presets: vec![EqPreset { id: "Acoustic".into(), name: "Acoustic".into() }],
            preset: Some("Acoustic".into()),
            band_hz: vec![100, 200],
            values: vec![10, -20],
            min: -120,
            max: 134,
            fraction_digits: 1,
        });
        s
    }

    #[test]
    fn captures_a_connected_headset_only() {
        let part = HeadsetPart::from_state(&connected()).unwrap();
        assert_eq!(part.ambient_mode, Some(AmbientMode::NoiseCanceling));
        assert_eq!((part.manual_nc, part.transparency, part.wind_noise), (Some(4), Some(2), Some(true)));
        assert_eq!((part.eq_preset.as_deref(), part.eq_bands.as_ref()), (Some("Acoustic"), None));
        assert!(HeadsetPart::from_state(&HeadsetState::with_connection(Connection::NotFound)).is_none());
    }

    #[test]
    fn a_custom_eq_is_saved_as_bands() {
        let mut s = connected();
        s.eq.as_mut().unwrap().preset = None;
        let part = HeadsetPart::from_state(&s).unwrap();
        assert_eq!((part.eq_preset, part.eq_bands), (None, Some(vec![10, -20])));
    }

    #[test]
    fn restores_levels_before_modes() {
        let part = HeadsetPart::from_state(&connected()).unwrap();
        assert_eq!(
            part.commands(),
            vec![
                HeadsetCommand::SetManualNc { level: 4 },
                HeadsetCommand::SetTransparency { level: 2 },
                HeadsetCommand::SetWindNoise { enabled: true },
                HeadsetCommand::SetEqPreset { preset: "Acoustic".into() },
                HeadsetCommand::SetNcMode { mode: NcMode::Manual },
                HeadsetCommand::SetAmbientMode { mode: AmbientMode::NoiseCanceling },
            ]
        );
        assert!(HeadsetPart::default().commands().is_empty());
    }
}
```

Add `pub mod part;` to `src-tauri/src/headset/mod.rs` after `pub mod model;`.

- [ ] **Step 2: Write `src-tauri/src/profile.rs` with tests**

```rust
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{engine::ProcessingSettings, headset::part::HeadsetPart};

/// Where an app's sound goes, as saved in a profile.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "camelCase")]
pub enum AppOutput {
    /// The app wasn't playing when the profile was saved, so its route isn't known: left alone.
    #[default]
    Unknown,
    /// Follows the Windows default output.
    Default,
    Device(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSnapshot {
    pub volume: f32,
    pub muted: bool,
    #[serde(default)]
    pub output: AppOutput,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessingPart {
    pub settings: ProcessingSettings,
    pub call_sends: BTreeMap<String, f32>,
}

/// The parts a profile controls; absent parts are left alone when it's applied.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ProfileParts {
    pub apps: Option<BTreeMap<String, AppSnapshot>>,
    pub processing: Option<ProcessingPart>,
    pub headphones: Option<HeadsetPart>,
    pub default_output: Option<String>,
}

/// Which parts to capture when saving.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PartChoice {
    pub apps: bool,
    pub processing: bool,
    pub headphones: bool,
    pub default_output: bool,
}

impl PartChoice {
    pub fn any(&self) -> bool {
        self.apps || self.processing || self.headphones || self.default_output
    }
}

impl ProfileParts {
    /// The parts this profile has, for re-capturing it.
    pub fn choice(&self) -> PartChoice {
        PartChoice {
            apps: self.apps.is_some(),
            processing: self.processing.is_some(),
            headphones: self.headphones.is_some(),
            default_output: self.default_output.is_some(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub parts: ProfileParts,
}

/// An id no existing profile uses: `p1`, `p2`, …
pub fn next_id(profiles: &[Profile]) -> String {
    let highest = profiles.iter().filter_map(|p| p.id.strip_prefix('p')?.parse::<u32>().ok()).max().unwrap_or(0);
    format!("p{}", highest + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(id: &str) -> Profile {
        Profile { id: id.into(), name: id.into(), parts: ProfileParts::default() }
    }

    #[test]
    fn ids_count_up_past_the_highest() {
        assert_eq!(next_id(&[]), "p1");
        assert_eq!(next_id(&[profile("p1"), profile("p7"), profile("odd")]), "p8");
    }

    #[test]
    fn choice_mirrors_the_parts() {
        let parts = ProfileParts { default_output: Some("out1".into()), ..Default::default() };
        assert_eq!(parts.choice(), PartChoice { default_output: true, ..Default::default() });
        assert!(parts.choice().any());
        assert!(!PartChoice::default().any());
    }

    #[test]
    fn serializes_camel_case_with_tagged_outputs() {
        let mut apps = BTreeMap::new();
        apps.insert("spotify.exe".to_string(), AppSnapshot { volume: 0.5, muted: false, output: AppOutput::Device("out2".into()) });
        apps.insert("game.exe".to_string(), AppSnapshot { volume: 1.0, muted: true, output: AppOutput::Unknown });
        let p = Profile { id: "p1".into(), name: "Gaming".into(), parts: ProfileParts { apps: Some(apps), default_output: Some("out1".into()), ..Default::default() } };
        let v = serde_json::to_value(&p).unwrap();
        assert_eq!(v["parts"]["apps"]["spotify.exe"]["output"]["kind"], "device");
        assert_eq!(v["parts"]["apps"]["spotify.exe"]["output"]["id"], "out2");
        assert_eq!(v["parts"]["apps"]["game.exe"]["output"]["kind"], "unknown");
        assert_eq!(v["parts"]["defaultOutput"], "out1");
        assert!(v["parts"]["headphones"].is_null());
        let back: Profile = serde_json::from_value(v).unwrap();
        assert_eq!(back, p);
    }
}
```

Add `pub mod profile;` to `src-tauri/src/lib.rs` after `pub mod paths;`.

- [ ] **Step 3: Settings, model and a placeholder command arm**

`settings.rs`: import `crate::profile::Profile` (add to the `use crate::{...}` list) and add to `Settings` after `call_sends`:

```rust
    pub profiles: Vec<Profile>,
    /// The profile applied (or saved) last; highlighted, not re-applied on start-up.
    pub active_profile: Option<String>,
```

with `profiles: Vec::new(), active_profile: None,` in `Default`.

`model.rs`:
- add `use crate::{headset::part::HeadsetPart, profile::{PartChoice, Profile}};` near the other `use crate::...` line.
- `UiSettings`: add after `pub start_minimized: bool,`:

```rust
    /// The first-run setup has been finished or skipped.
    pub onboarded: bool,
```

- `UiState`: add after `pub processing: ProcessingView,`:

```rust
    pub profiles: Vec<Profile>,
    pub active_profile: Option<String>,
```

- `Command`: add after `SetAppSend { .. },`:

```rust
    SaveProfile { name: String, parts: PartChoice, headset: Option<HeadsetPart> },
    UpdateProfile { id: String, headset: Option<HeadsetPart> },
    RenameProfile { id: String, name: String },
    DeleteProfile { id: String },
    ApplyProfile { id: String },
```

- in `ui_state_serializes_camel_case` add `assert_eq!(v["ui"]["onboarded"], false);` and `assert!(v["activeProfile"].is_null());`
- in `command_deserializes_from_tagged_camel_case_json` add:

```rust
        let c: Command = serde_json::from_str(
            r#"{"type":"saveProfile","name":"Gaming","parts":{"apps":true,"defaultOutput":true},"headset":null}"#,
        )
        .unwrap();
        assert_eq!(
            c,
            Command::SaveProfile {
                name: "Gaming".into(),
                parts: PartChoice { apps: true, default_output: true, ..Default::default() },
                headset: None
            }
        );
```

`app_state/mod.rs`:
- in `build_state`'s `UiState { ... }` add `profiles: self.settings.profiles.clone(), active_profile: self.settings.active_profile.clone(),` after `processing: ...,`
- add a temporary arm to `apply` (Task 2 replaces it):

```rust
            Command::SaveProfile { .. }
            | Command::UpdateProfile { .. }
            | Command::RenameProfile { .. }
            | Command::DeleteProfile { .. }
            | Command::ApplyProfile { .. } => {
                return Err(AudioError::Unavailable("Profiles aren't available yet".into()));
            }
```

`settings.rs` `round_trips` test: before `s.save(&path).unwrap();` add `s.profiles.push(crate::profile::Profile { id: "p1".into(), name: "Gaming".into(), parts: Default::default() }); s.active_profile = Some("p1".into());`.

- [ ] **Step 4: Run the tests**

Run: `cargo test --manifest-path M:\Resonance\src-tauri\Cargo.toml --lib -q`
Expected: all PASS (3 headset-part tests, 3 profile tests, extended model/settings tests).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src
git commit -m "feat(profiles): profile data, headset part mapping and settings"
```

---

### Task 2: Saving, updating and applying profiles

**Files:**
- Create: `src-tauri/src/app_state/profiles.rs`, `src-tauri/src/app_state/profiles_tests.rs`
- Modify: `src-tauri/src/app_state/mod.rs`

**Interfaces:**
- Consumes: Task 1 types; `AppCore` internals (`settings`, `state`, `backend`, `notices`, `apply`, `refresh`).
- Produces: `AppCore::{save_profile(&str, PartChoice, Option<HeadsetPart>), update_profile(&str, Option<HeadsetPart>), rename_profile(&str, &str), delete_profile(&str), apply_profile(&str)}` (all `-> Result<()>`, `pub(super)`), field `pending_outputs: HashMap<String, AppOutput>` applied in `restore_new_sessions`.

- [ ] **Step 1: Write the failing tests**

`src-tauri/src/app_state/profiles_tests.rs`:

```rust
use super::*;
use crate::{
    fake::{FakeBackend, FakeEngine},
    headset::part::HeadsetPart,
    profile::{AppOutput, PartChoice},
};

fn named(id: &str, flow: Flow, name: &str) -> RawDevice {
    let mut d = FakeBackend::device(id, flow);
    d.name = name.into();
    d
}

/// Speakers (default) and headphones, a mic, VB-Cable; Spotify (pid 10) and Chrome (pid 20).
fn backend() -> FakeBackend {
    let mut out1 = named("out1", Flow::Render, "Speakers");
    out1.default_console = true;
    out1.default_comms = true;
    let mut mic1 = named("mic1", Flow::Capture, "Desk Mic");
    mic1.default_console = true;
    mic1.default_comms = true;
    FakeBackend {
        devices: vec![
            out1,
            named("out2", Flow::Render, "Headphones"),
            mic1,
            named("cin", Flow::Render, "CABLE Input (VB-Audio Virtual Cable)"),
            named("cout", Flow::Capture, "CABLE Output (VB-Audio Virtual Cable)"),
        ],
        sessions: vec![
            FakeBackend::session("s1", 10, Some("spotify.exe")),
            FakeBackend::session("c1", 20, Some("chrome.exe")),
        ],
        routing: true,
        ..Default::default()
    }
}

fn core() -> (AppCore<FakeBackend>, FakeEngine) {
    let engine = FakeEngine::default();
    let mut c = AppCore::new(backend(), Settings::default());
    c.set_engine(Box::new(engine.clone()));
    c.refresh().unwrap();
    (c, engine)
}

fn parts(apps: bool, processing: bool, default_output: bool) -> PartChoice {
    PartChoice { apps, processing, headphones: false, default_output }
}

fn save(c: &mut AppCore<FakeBackend>, name: &str, choice: PartChoice) -> Result<()> {
    c.apply(Command::SaveProfile { name: name.into(), parts: choice, headset: None })
}

fn volume(c: &mut AppCore<FakeBackend>, session: &str) -> f32 {
    c.backend_mut().sessions.iter().find(|s| s.id == session).unwrap().volume
}

fn default_output(c: &mut AppCore<FakeBackend>) -> String {
    c.backend_mut().devices.iter().find(|d| d.flow == Flow::Render && d.default_console).unwrap().id.clone()
}

#[test]
fn saving_captures_only_the_chosen_parts() {
    let (mut c, _) = core();
    save(&mut c, " Work ", parts(true, false, true)).unwrap();
    let p = &c.state().profiles[0];
    assert_eq!((p.id.as_str(), p.name.as_str()), ("p1", "Work"));
    let apps = p.parts.apps.as_ref().unwrap();
    assert_eq!(apps["spotify.exe"].volume, 1.0);
    assert_eq!(apps["spotify.exe"].output, AppOutput::Default);
    assert_eq!(p.parts.default_output.as_deref(), Some("out1"));
    assert!(p.parts.processing.is_none() && p.parts.headphones.is_none());
    assert_eq!(c.state().active_profile.as_deref(), Some("p1"));
    assert!(c.take_settings_dirty());
}

#[test]
fn saving_needs_a_name_a_part_and_connected_headphones() {
    let (mut c, _) = core();
    assert_eq!(save(&mut c, "  ", parts(true, false, false)).unwrap_err().code(), "invalid");
    assert_eq!(save(&mut c, "Empty", PartChoice::default()).unwrap_err().code(), "invalid");
    let headphones = PartChoice { headphones: true, ..Default::default() };
    assert_eq!(save(&mut c, "Quiet", headphones).unwrap_err().code(), "unavailable");
    let part = HeadsetPart { transparency: Some(3), ..Default::default() };
    c.apply(Command::SaveProfile { name: "Quiet".into(), parts: headphones, headset: Some(part.clone()) }).unwrap();
    assert_eq!(c.state().profiles[0].parts.headphones, Some(part));
}

#[test]
fn applying_restores_running_apps_and_remembered_volumes() {
    let (mut c, _) = core();
    c.apply(Command::SetAppVolume { key: "spotify.exe".into(), volume: 0.3 }).unwrap();
    c.refresh().unwrap();
    save(&mut c, "Music low", parts(true, false, false)).unwrap();
    c.apply(Command::SetAppVolume { key: "spotify.exe".into(), volume: 0.9 }).unwrap();
    c.apply(Command::ApplyProfile { id: "p1".into() }).unwrap();
    assert!((volume(&mut c, "s1") - 0.3).abs() < 1e-6);
    assert!((c.settings().apps["spotify.exe"].volume - 0.3).abs() < 1e-6);
}

#[test]
fn routes_apply_now_or_when_the_app_next_plays() {
    let (mut c, _) = core();
    c.apply(Command::SetAppOutput { key: "spotify.exe".into(), device_id: Some("out2".into()) }).unwrap();
    save(&mut c, "Headphones", parts(true, false, false)).unwrap();
    c.apply(Command::SetAppOutput { key: "spotify.exe".into(), device_id: None }).unwrap();
    c.backend_mut().sessions.retain(|s| s.pid != 10);
    c.refresh().unwrap();
    c.apply(Command::ApplyProfile { id: "p1".into() }).unwrap();
    assert!(!c.backend_mut().calls.iter().any(|call| call == "output 11 Some(\"out2\")"));
    c.backend_mut().sessions.push(FakeBackend::session("s9", 11, Some("spotify.exe")));
    c.refresh().unwrap();
    assert!(c.backend_mut().calls.iter().any(|call| call == "output 11 Some(\"out2\")"), "{:?}", c.backend_mut().calls);
}

#[test]
fn applying_restores_processing_and_call_sends() {
    let (mut c, engine) = core();
    let mut p = c.settings().processing.clone();
    p.enabled = true;
    c.apply(Command::SetProcessing { processing: p.clone() }).unwrap();
    c.apply(Command::SetAppSend { key: "spotify.exe".into(), level: Some(0.5) }).unwrap();
    save(&mut c, "Call", parts(false, true, false)).unwrap();
    c.apply(Command::SetAppSend { key: "spotify.exe".into(), level: None }).unwrap();
    p.enabled = false;
    c.apply(Command::SetProcessing { processing: p }).unwrap();
    assert!(engine.log().running.is_none());
    c.apply(Command::ApplyProfile { id: "p1".into() }).unwrap();
    assert!(c.settings().processing.enabled);
    assert_eq!(c.settings().call_sends.get("spotify.exe"), Some(&0.5));
    assert!(engine.log().running.is_some());
    assert_eq!(engine.log().sources.len(), 1);
}

#[test]
fn applying_sets_the_default_output_or_explains_why_not() {
    let (mut c, _) = core();
    save(&mut c, "Speakers", parts(false, false, true)).unwrap();
    c.apply(Command::SetDefaultDevice { id: "out2".into(), role: Role::Console }).unwrap();
    c.apply(Command::ApplyProfile { id: "p1".into() }).unwrap();
    assert_eq!(default_output(&mut c), "out1");
    c.apply(Command::SetDefaultDevice { id: "out2".into(), role: Role::Console }).unwrap();
    c.backend_mut().devices.iter_mut().find(|d| d.id == "out1").unwrap().state = DeviceState::Unplugged;
    c.refresh().unwrap();
    c.take_notices();
    c.apply(Command::ApplyProfile { id: "p1".into() }).unwrap();
    assert_eq!(default_output(&mut c), "out2");
    let notices = c.take_notices();
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].to_string(), "Speakers isn't connected, so the default output wasn't changed");
}

#[test]
fn parts_a_profile_doesnt_have_are_left_alone() {
    let (mut c, _) = core();
    save(&mut c, "Output only", parts(false, false, true)).unwrap();
    c.apply(Command::SetAppVolume { key: "spotify.exe".into(), volume: 0.4 }).unwrap();
    c.apply(Command::ApplyProfile { id: "p1".into() }).unwrap();
    assert!((volume(&mut c, "s1") - 0.4).abs() < 1e-6);
}

#[test]
fn update_recaptures_existing_parts_and_keeps_headphones_when_disconnected() {
    let (mut c, _) = core();
    let part = HeadsetPart { manual_nc: Some(5), ..Default::default() };
    let choice = PartChoice { apps: true, headphones: true, ..Default::default() };
    c.apply(Command::SaveProfile { name: "Gaming".into(), parts: choice, headset: Some(part.clone()) }).unwrap();
    c.apply(Command::SetAppVolume { key: "chrome.exe".into(), volume: 0.2 }).unwrap();
    c.refresh().unwrap();
    c.apply(Command::UpdateProfile { id: "p1".into(), headset: None }).unwrap();
    let p = &c.state().profiles[0];
    assert!((p.parts.apps.as_ref().unwrap()["chrome.exe"].volume - 0.2).abs() < 1e-6);
    assert_eq!(p.parts.headphones, Some(part));
    assert!(p.parts.processing.is_none() && p.parts.default_output.is_none());
}

#[test]
fn rename_and_delete() {
    let (mut c, _) = core();
    save(&mut c, "Gaming", parts(true, false, false)).unwrap();
    save(&mut c, "Work", parts(true, false, false)).unwrap();
    c.apply(Command::RenameProfile { id: "p1".into(), name: "Games".into() }).unwrap();
    assert_eq!(c.state().profiles[0].name, "Games");
    assert_eq!(c.apply(Command::RenameProfile { id: "p1".into(), name: " ".into() }).unwrap_err().code(), "invalid");
    assert_eq!(c.state().active_profile.as_deref(), Some("p2"));
    c.apply(Command::DeleteProfile { id: "p2".into() }).unwrap();
    assert_eq!(c.state().profiles.len(), 1);
    assert_eq!(c.state().active_profile, None);
    assert_eq!(c.apply(Command::ApplyProfile { id: "p2".into() }).unwrap_err().code(), "notFound");
    assert_eq!(c.apply(Command::DeleteProfile { id: "p2".into() }).unwrap_err().code(), "notFound");
}
```

Register in `app_state/mod.rs`:

```rust
mod profiles;
#[cfg(test)]
mod profiles_tests;
```

Run: `cargo test --manifest-path M:\Resonance\src-tauri\Cargo.toml --lib -q app_state::profiles_tests`
Expected: compile errors (module missing).

- [ ] **Step 2: Write `src-tauri/src/app_state/profiles.rs`**

```rust
use std::collections::BTreeMap;

use super::AppCore;
use crate::{
    backend::AudioBackend,
    error::{AudioError, Result},
    headset::part::HeadsetPart,
    model::{Command, DeviceState, Flow, Role},
    profile::{next_id, AppOutput, AppSnapshot, PartChoice, ProcessingPart, Profile, ProfileParts},
    settings::AppPrefs,
};

fn not_found() -> AudioError {
    AudioError::NotFound("That profile no longer exists".into())
}

fn clean_name(name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AudioError::Invalid("Give the profile a name".into()));
    }
    Ok(name.to_string())
}

impl<B: AudioBackend> AppCore<B> {
    /// The current setup, for the chosen parts.
    fn capture(&self, choice: PartChoice, headset: Option<HeadsetPart>) -> ProfileParts {
        let apps = choice.apps.then(|| {
            let mut apps: BTreeMap<String, AppSnapshot> = self
                .settings
                .apps
                .iter()
                .map(|(key, p)| (key.clone(), AppSnapshot { volume: p.volume, muted: p.muted, output: AppOutput::Unknown }))
                .collect();
            for a in self.state.apps.iter().filter(|a| !a.key.starts_with("pid:")) {
                let output = match (&a.output, a.is_system) {
                    (_, true) => AppOutput::Unknown,
                    (Some(id), false) => AppOutput::Device(id.clone()),
                    (None, false) => AppOutput::Default,
                };
                apps.insert(a.key.clone(), AppSnapshot { volume: a.volume, muted: a.muted, output });
            }
            apps
        });
        let processing = choice.processing.then(|| ProcessingPart {
            settings: self.settings.processing.clone(),
            call_sends: self.settings.call_sends.clone(),
        });
        let default_output = if choice.default_output {
            self.state
                .devices
                .iter()
                .find(|d| d.flow == Flow::Render && d.default_console && d.state == DeviceState::Active)
                .map(|d| d.id.clone())
        } else {
            None
        };
        ProfileParts { apps, processing, headphones: if choice.headphones { headset } else { None }, default_output }
    }

    fn publish_profiles(&mut self) {
        self.settings_dirty = true;
        self.state.profiles = self.settings.profiles.clone();
        self.state.active_profile = self.settings.active_profile.clone();
    }

    fn profile_index(&self, id: &str) -> Result<usize> {
        self.settings.profiles.iter().position(|p| p.id == id).ok_or_else(not_found)
    }

    pub(super) fn save_profile(&mut self, name: &str, choice: PartChoice, headset: Option<HeadsetPart>) -> Result<()> {
        let name = clean_name(name)?;
        if !choice.any() {
            return Err(AudioError::Invalid("Choose at least one part for the profile".into()));
        }
        if choice.headphones && headset.is_none() {
            return Err(AudioError::Unavailable("Connect the Space One to save its settings".into()));
        }
        let parts = self.capture(choice, headset);
        let id = next_id(&self.settings.profiles);
        self.settings.profiles.push(Profile { id: id.clone(), name, parts });
        self.settings.active_profile = Some(id);
        self.publish_profiles();
        Ok(())
    }

    /// Re-captures the profile's parts; its saved headphone settings stay if the Space One isn't connected.
    pub(super) fn update_profile(&mut self, id: &str, headset: Option<HeadsetPart>) -> Result<()> {
        let i = self.profile_index(id)?;
        let old = self.settings.profiles[i].parts.clone();
        let mut parts = self.capture(old.choice(), headset);
        if old.headphones.is_some() && parts.headphones.is_none() {
            parts.headphones = old.headphones;
        }
        self.settings.profiles[i].parts = parts;
        self.publish_profiles();
        Ok(())
    }

    pub(super) fn rename_profile(&mut self, id: &str, name: &str) -> Result<()> {
        let name = clean_name(name)?;
        let i = self.profile_index(id)?;
        self.settings.profiles[i].name = name;
        self.publish_profiles();
        Ok(())
    }

    pub(super) fn delete_profile(&mut self, id: &str) -> Result<()> {
        let i = self.profile_index(id)?;
        self.settings.profiles.remove(i);
        if self.settings.active_profile.as_deref() == Some(id) {
            self.settings.active_profile = None;
        }
        self.publish_profiles();
        Ok(())
    }

    /// Applies every part the profile has; problems become notices and never stop the other parts.
    pub(super) fn apply_profile(&mut self, id: &str) -> Result<()> {
        let parts = self.settings.profiles[self.profile_index(id)?].parts.clone();
        if let Some(apps) = &parts.apps {
            self.apply_apps(apps);
        }
        if let Some(processing) = parts.processing {
            self.settings.call_sends = processing.call_sends;
            self.settings_dirty = true;
            if let Err(e) = self.apply(Command::SetProcessing { processing: processing.settings }) {
                self.notices.push(e);
            }
        }
        if let Some(output) = &parts.default_output {
            self.apply_default_output(output);
        }
        self.settings.active_profile = Some(id.to_string());
        self.settings_dirty = true;
        self.refresh()?;
        Ok(())
    }

    fn apply_apps(&mut self, apps: &BTreeMap<String, AppSnapshot>) {
        for (key, snap) in apps {
            self.settings.apps.insert(key.clone(), AppPrefs { volume: snap.volume, muted: snap.muted });
            let running = self.state.apps.iter().find(|a| &a.key == key).map(|a| (a.name.clone(), a.session_ids.clone(), a.pids.clone()));
            match running {
                Some((name, sessions, pids)) => {
                    for sid in &sessions {
                        let _ = self.backend.set_session_volume(sid, snap.volume);
                        let _ = self.backend.set_session_mute(sid, snap.muted);
                    }
                    self.route_app(&name, &pids, &snap.output);
                }
                None if snap.output != AppOutput::Unknown => {
                    self.pending_outputs.insert(key.clone(), snap.output.clone());
                }
                None => {}
            }
        }
        self.settings_dirty = true;
    }

    /// Sends an app's processes to its saved output; a saved device that's unplugged becomes a notice.
    pub(super) fn route_app(&mut self, name: &str, pids: &[u32], output: &AppOutput) {
        if !self.backend.routing_available() {
            return;
        }
        let target = match output {
            AppOutput::Unknown => return,
            AppOutput::Default => None,
            AppOutput::Device(id) => {
                if !self.state.devices.iter().any(|d| &d.id == id && d.state == DeviceState::Active) {
                    self.notices.push(AudioError::NotFound(format!("{name}'s saved output isn't connected")));
                    return;
                }
                Some(id.as_str())
            }
        };
        for &pid in pids {
            let _ = self.backend.set_app_output(pid, target);
        }
    }

    fn apply_default_output(&mut self, id: &str) {
        let found = self.state.devices.iter().find(|d| d.id == id && d.flow == Flow::Render).map(|d| (d.state == DeviceState::Active, d.name.clone()));
        match found {
            Some((true, _)) => {
                for role in [Role::Console, Role::Communications] {
                    if let Err(e) = self.backend.set_default_device(id, role) {
                        self.notices.push(e);
                        break;
                    }
                }
            }
            Some((false, name)) => {
                self.notices.push(AudioError::NotFound(format!("{name} isn't connected, so the default output wasn't changed")));
            }
            None => self.notices.push(AudioError::NotFound(
                "The profile's output device isn't connected, so the default output wasn't changed".into(),
            )),
        }
    }
}
```

- [ ] **Step 3: Hook into `AppCore`**

`app_state/mod.rs`:
- add `use crate::profile::AppOutput;` and add `HashMap` to the `std::collections` import if not present.
- add field `pending_outputs: HashMap<String, AppOutput>,` to `AppCore` (after `notices`) and `pending_outputs: HashMap::new(),` in `new`.
- in `restore_new_sessions`, before `self.seen_sessions = ...`, route new sessions whose app has a pending output:

```rust
        let fresh: Vec<(String, u32)> =
            sessions.iter().filter(|s| !self.seen_sessions.contains(&s.id)).map(|s| (app_key(s), s.pid)).collect();
        let mut routed = Vec::new();
        for (key, pid) in fresh {
            if let Some(output) = self.pending_outputs.get(&key).cloned() {
                self.route_app(&key, &[pid], &output);
                routed.push(key);
                applied = true;
            }
        }
        for key in routed {
            self.pending_outputs.remove(&key);
        }
```

- replace the temporary profile arm in `apply` with:

```rust
            Command::SaveProfile { name, parts, headset } => self.save_profile(&name, parts, headset)?,
            Command::UpdateProfile { id, headset } => self.update_profile(&id, headset)?,
            Command::RenameProfile { id, name } => self.rename_profile(&id, &name)?,
            Command::DeleteProfile { id } => self.delete_profile(&id)?,
            Command::ApplyProfile { id } => self.apply_profile(&id)?,
```

- [ ] **Step 4: Run all tests and lint**

Run: `cargo test --manifest-path M:\Resonance\src-tauri\Cargo.toml --lib -q`
Then: `cargo clippy --manifest-path M:\Resonance\src-tauri\Cargo.toml --all-targets -- -D warnings`
Expected: all PASS (9 new profile tests); no warnings.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src
git commit -m "feat(profiles): save, update, rename, delete and apply profiles"
```

---

### Task 3: App wiring — profile commands, headphones, Start with Windows

**Files:**
- Modify: `src-tauri/Cargo.toml`, `src-tauri/src/app/commands.rs`, `src-tauri/src/app/mod.rs`
- Create: `src-tauri/tests/profiles_hw.rs`

**Interfaces:**
- Consumes: Tasks 1–2; `HeadsetHandle::{state, command}`; `sink::push_notice`.
- Produces: Tauri commands `save_profile(name: String, parts: PartChoice)`, `update_profile(id: String)`, `apply_profile(id: String)`, `get_autostart() -> bool`, `set_autostart(enabled: bool)`.

- [ ] **Step 1: Add the plugin**

`Cargo.toml`: add `tauri-plugin-autostart = "2"` after `tauri-plugin-single-instance = "2"`.

- [ ] **Step 2: Commands**

Append to `src-tauri/src/app/commands.rs` (add imports `use tauri_plugin_autostart::ManagerExt;`, `use super::sink::push_notice;`, and `crate::{headset::{model::Connection, part::HeadsetPart, HeadsetHandle}, profile::PartChoice}` to the existing `use crate::{...}`):

```rust
async fn headset_part(headset: &HeadsetHandle) -> Option<HeadsetPart> {
    headset.state().await.ok().and_then(|s| HeadsetPart::from_state(&s))
}

#[tauri::command]
pub async fn save_profile(
    worker: State<'_, WorkerHandle>,
    headset: State<'_, HeadsetHandle>,
    name: String,
    parts: PartChoice,
) -> Result<()> {
    let snapshot = if parts.headphones { headset_part(&headset).await } else { None };
    worker.command(Command::SaveProfile { name, parts, headset: snapshot }).await
}

#[tauri::command]
pub async fn update_profile(worker: State<'_, WorkerHandle>, headset: State<'_, HeadsetHandle>, id: String) -> Result<()> {
    let snapshot = headset_part(&headset).await;
    worker.command(Command::UpdateProfile { id, headset: snapshot }).await
}

/// Applies the audio parts on the audio worker, then the headphone part through the headset manager.
#[tauri::command]
pub async fn apply_profile(
    app: AppHandle,
    worker: State<'_, WorkerHandle>,
    headset: State<'_, HeadsetHandle>,
    id: String,
) -> Result<()> {
    let part = worker.state().await?.profiles.into_iter().find(|p| p.id == id).and_then(|p| p.parts.headphones);
    worker.command(Command::ApplyProfile { id }).await?;
    if let Some(part) = part {
        let connected = headset.state().await.is_ok_and(|s| s.connection == Connection::Connected);
        if !connected {
            push_notice(&app, AudioError::Unavailable("The Space One isn't connected, so its profile settings weren't applied".into()));
            return Ok(());
        }
        for cmd in part.commands() {
            if let Err(e) = headset.command(cmd).await {
                push_notice(&app, e);
                break;
            }
        }
    }
    Ok(())
}

#[tauri::command]
pub fn get_autostart(app: AppHandle) -> Result<bool> {
    app.autolaunch().is_enabled().map_err(|e| AudioError::Os(e.to_string()))
}

#[tauri::command]
pub fn set_autostart(app: AppHandle, enabled: bool) -> Result<()> {
    let launcher = app.autolaunch();
    let result = if enabled { launcher.enable() } else { launcher.disable() };
    result.map_err(|e| AudioError::Os(e.to_string()))
}
```

- [ ] **Step 3: Register the plugin, the commands and `--minimized`**

`src-tauri/src/app/mod.rs`:
- after `let initial = Settings::load(&settings_path);` add:

```rust
    // Set by the Start with Windows entry: stay in the tray whatever "Start in the tray" says.
    let launched_minimized = std::env::args().any(|a| a == "--minimized");
```

- after `.plugin(tauri_plugin_global_shortcut::Builder::new().build())` add:

```rust
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, Some(vec!["--minimized"])))
```

- change `if !initial.ui.start_minimized {` to `if !initial.ui.start_minimized && !launched_minimized {`
- add to `generate_handler![...]` after `commands::open_vb_cable,`:

```rust
            commands::save_profile,
            commands::update_profile,
            commands::apply_profile,
            commands::get_autostart,
            commands::set_autostart,
```

- [ ] **Step 4: Hardware test**

`src-tauri/tests/profiles_hw.rs`:

```rust
#![cfg(windows)]
//! Changes the default output briefly and puts it back. Quit the installed Resonance first. Run with:
//! cargo test --manifest-path src-tauri/Cargo.toml --test profiles_hw -- --ignored --nocapture
use resonance_lib::{app_state::AppCore, model::*, profile::PartChoice, settings::Settings, win::WasapiBackend};

fn default_output(core: &AppCore<WasapiBackend>) -> String {
    core.state().devices.iter().find(|d| d.flow == Flow::Render && d.default_console).unwrap().id.clone()
}

#[test]
#[ignore]
fn a_profile_restores_the_default_output_and_app_volumes() {
    let mut core = AppCore::new(WasapiBackend::new().unwrap(), Settings::default());
    core.refresh().unwrap();
    let original = default_output(&core);
    let other = core
        .state()
        .devices
        .iter()
        .find(|d| d.flow == Flow::Render && d.state == DeviceState::Active && d.id != original)
        .expect("needs a second output device")
        .id
        .clone();
    let app = core.state().apps.iter().find(|a| !a.is_system).map(|a| (a.key.clone(), a.volume));
    let parts = PartChoice { apps: true, default_output: true, ..Default::default() };
    core.apply(Command::SaveProfile { name: "Test".into(), parts, headset: None }).unwrap();

    core.apply(Command::SetDefaultDevice { id: other.clone(), role: Role::Console }).unwrap();
    if let Some((key, _)) = &app {
        core.apply(Command::SetAppVolume { key: key.clone(), volume: 0.31 }).unwrap();
    }
    core.apply(Command::ApplyProfile { id: "p1".into() }).unwrap();

    assert_eq!(default_output(&core), original);
    if let Some((key, volume)) = app {
        let now = core.state().apps.iter().find(|a| a.key == key).unwrap().volume;
        assert!((now - volume).abs() < 0.01, "{key}: {now} vs {volume}");
    }
    assert!(core.take_notices().is_empty());
}
```

- [ ] **Step 5: Build, lint, test**

Run: `cargo clippy --manifest-path M:\Resonance\src-tauri\Cargo.toml --all-targets -- -D warnings`
Then: `cargo test --manifest-path M:\Resonance\src-tauri\Cargo.toml -q`
Then (installed Resonance quit): `cargo test --manifest-path M:\Resonance\src-tauri\Cargo.toml --test profiles_hw -- --ignored --nocapture`
Expected: clean; all PASS; the hardware test PASSES and the default output is back to the original afterwards.

- [ ] **Step 6: Commit**

```bash
git add src-tauri
git commit -m "feat(app): profile commands with headphone settings, Start with Windows plugin, --minimized"
```

---
### Task 4: Frontend foundation — types, API, store actions, fixtures

**Files:**
- Modify: `src/lib/types.ts`, `src/lib/api.ts`, `src/lib/store.ts`, `src/test/fixtures.ts`
- Create: `src/lib/profiles.ts`, `src/lib/profiles.test.ts`

**Interfaces:**
- Consumes: JSON from Tasks 1–3.
- Produces: TS types `AppOutput`, `AppSnapshot`, `HeadsetPart`, `ProcessingPart`, `ProfileParts`, `PartChoice`, `Profile`; `UiState.{profiles, activeProfile}`; `UiSettings.onboarded`; API `saveProfile(name, parts)`, `updateProfile(id)`, `applyProfile(id)`, `getAutostart()`, `setAutostart(enabled)`; store actions `saveProfileAs(name, parts)`, `recaptureProfile(id)`, `switchProfile(id)`, `renameProfile(id, name)`, `deleteProfile(id)`, `finishOnboarding(ui)`; helper `partLabels(profile): string[]`; fixtures `makeProfile(partial)`.

- [ ] **Step 1: Types**

In `src/lib/types.ts`:
- `UiSettings`: add `onboarded: boolean;` after `startMinimized: boolean;`
- add before `export interface UiState`:

```ts
export type AppOutput = { kind: "unknown" } | { kind: "default" } | { kind: "device"; id: string };

export interface AppSnapshot {
  volume: number;
  muted: boolean;
  output: AppOutput;
}

export interface HeadsetPart {
  ambientMode: AmbientMode | null;
  ncMode: NcMode | null;
  manualNc: number | null;
  transparency: number | null;
  windNoise: boolean | null;
  eqPreset: string | null;
  eqBands: number[] | null;
}

export interface ProcessingPart {
  settings: ProcessingSettings;
  callSends: Record<string, number>;
}

export interface ProfileParts {
  apps: Record<string, AppSnapshot> | null;
  processing: ProcessingPart | null;
  headphones: HeadsetPart | null;
  defaultOutput: string | null;
}

export interface PartChoice {
  apps: boolean;
  processing: boolean;
  headphones: boolean;
  defaultOutput: boolean;
}

export interface Profile {
  id: string;
  name: string;
  parts: ProfileParts;
}
```

- `UiState`: add `profiles: Profile[];` and `activeProfile: string | null;` after `processing: ProcessingView;`
- `Command`: add `| { type: "renameProfile"; id: string; name: string }` and `| { type: "deleteProfile"; id: string }` after the `setAppSend` member (moving the `;`).

- [ ] **Step 2: API and store**

`src/lib/api.ts` — add `PartChoice` to the type import and after `openVbCable`:

```ts
export const saveProfile = (name: string, parts: PartChoice) => invoke<void>("save_profile", { name, parts });
export const updateProfile = (id: string) => invoke<void>("update_profile", { id });
export const applyProfile = (id: string) => invoke<void>("apply_profile", { id });
export const getAutostart = () => invoke<boolean>("get_autostart");
export const setAutostart = (enabled: boolean) => invoke<void>("set_autostart", { enabled });
```

`src/lib/store.ts` — change the api import to `import { applyProfile, saveProfile, send, sendHeadset, setMicHotkey, updateProfile } from "./api";`, add `PartChoice` and `UiSettings` to the type import, and add after `toggleAppSend`:

```ts
export const saveProfileAs = (name: string, parts: PartChoice) => attempt(() => saveProfile(name, parts));
export const recaptureProfile = (id: string) => attempt(() => updateProfile(id));
export const switchProfile = (id: string) => attempt(() => applyProfile(id));
export const renameProfile = (id: string, name: string) => run({ type: "renameProfile", id, name });
export const deleteProfile = (id: string) => run({ type: "deleteProfile", id });
export const finishOnboarding = (ui: UiSettings) => run({ type: "setUiSettings", ui: { ...ui, onboarded: true } });
```

- [ ] **Step 3: Fixtures**

`src/test/fixtures.ts`: change the type import to also bring in `Profile`; in `makeState` change `ui: { theme: "system", startMinimized: false },` to `ui: { theme: "system", startMinimized: false, onboarded: true },` and add `profiles: [], activeProfile: null,` before `...p,`; add:

```ts
export function makeProfile(p: Partial<Profile> & { id: string }): Profile {
  return {
    name: `Profile ${p.id}`,
    parts: { apps: {}, processing: null, headphones: null, defaultOutput: null },
    ...p,
  };
}
```

- [ ] **Step 4: Write the failing helper test**

`src/lib/profiles.test.ts`:

```ts
import { partLabels } from "./profiles";
import { DEFAULT_DSP, makeProfile } from "../test/fixtures";

describe("partLabels", () => {
  it("names the parts a profile controls, in a fixed order", () => {
    expect(partLabels(makeProfile({ id: "p1" }))).toEqual(["Apps"]);
    const all = makeProfile({
      id: "p2",
      parts: {
        apps: null,
        processing: {
          settings: { enabled: true, sourceId: null, dsp: DEFAULT_DSP, monitor: false, monitorLevel: 0.7, call: { master: 1, duck: true, duckDb: -12 } },
          callSends: {},
        },
        headphones: { ambientMode: "transparency", ncMode: null, manualNc: null, transparency: 3, windNoise: null, eqPreset: null, eqBands: null },
        defaultOutput: "out1",
      },
    });
    expect(partLabels(all)).toEqual(["Mic & processing", "Headphones", "Default output"]);
  });
});
```

Run: `npm.cmd test -- src/lib/profiles.test.ts` → FAIL (`./profiles` missing).

- [ ] **Step 5: Write `src/lib/profiles.ts`**

```ts
import type { PartChoice, Profile } from "./types";

export const PARTS: { key: keyof PartChoice; label: string }[] = [
  { key: "apps", label: "Apps" },
  { key: "processing", label: "Mic & processing" },
  { key: "headphones", label: "Headphones" },
  { key: "defaultOutput", label: "Default output" },
];

/** Labels of the parts a profile controls. */
export function partLabels(p: Profile): string[] {
  return PARTS.filter((part) => p.parts[part.key] !== null).map((part) => part.label);
}
```

- [ ] **Step 6: Run tests and type check**

Run: `npm.cmd test` then `npx.cmd tsc --noEmit`
Expected: all PASS; no type errors.

- [ ] **Step 7: Commit**

```bash
git add src/lib src/test/fixtures.ts
git commit -m "feat(ui): profile types, API and store actions"
```

---

### Task 5: The Profiles tab

**Files:**
- Create: `src/views/ProfilesView.tsx`, `src/views/ProfilesView.test.tsx`
- Modify: `src/views/Titlebar.tsx`, `src/views/MainWindow.tsx`

**Interfaces:**
- Consumes: Task 4 actions/helpers; `useStore` headset state; `Panel`, `Banner`, `Badge`, `ViewHeader` from `components/ui`.
- Produces: `ProfilesView({ state })`; `View` gains `"profiles"`.

- [ ] **Step 1: Write the failing tests**

`src/views/ProfilesView.test.tsx`:

```tsx
import { fireEvent, render, screen, within } from "@testing-library/react";
import { ProfilesView } from "./ProfilesView";
import { makeHeadset, makeProfile, makeState } from "../test/fixtures";
import { useStore } from "../lib/store";
import * as api from "../lib/api";

vi.mock("../lib/api", () => ({
  send: vi.fn().mockResolvedValue(undefined),
  saveProfile: vi.fn().mockResolvedValue(undefined),
  updateProfile: vi.fn().mockResolvedValue(undefined),
  applyProfile: vi.fn().mockResolvedValue(undefined),
}));

const withProfiles = () =>
  makeState({
    profiles: [makeProfile({ id: "p1", name: "Gaming" }), makeProfile({ id: "p2", name: "Work", parts: { apps: null, processing: null, headphones: null, defaultOutput: "out1" } })],
    activeProfile: "p1",
  });

describe("ProfilesView", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useStore.setState({ headset: makeHeadset() });
  });

  it("explains profiles when there are none", () => {
    render(<ProfilesView state={makeState()} />);
    expect(screen.getByText(/no profiles yet/i)).toBeInTheDocument();
  });

  it("saves the current setup with the chosen parts", () => {
    render(<ProfilesView state={makeState()} />);
    const save = screen.getByRole("button", { name: "Save profile" });
    expect(save).toBeDisabled();
    fireEvent.change(screen.getByRole("textbox", { name: "Profile name" }), { target: { value: "Gaming" } });
    fireEvent.click(screen.getByRole("checkbox", { name: "Default output" }));
    fireEvent.click(save);
    expect(api.saveProfile).toHaveBeenCalledWith("Gaming", { apps: true, processing: true, headphones: false, defaultOutput: true });
  });

  it("only offers headphones while they're connected", () => {
    useStore.setState({ headset: makeHeadset({ connection: { state: "notFound" } }) });
    render(<ProfilesView state={makeState()} />);
    expect(screen.getByRole("checkbox", { name: "Headphones" })).toBeDisabled();
  });

  it("shows each profile's parts and marks the active one", () => {
    render(<ProfilesView state={withProfiles()} />);
    const gaming = screen.getByRole("region", { name: "Gaming" });
    expect(within(gaming).getByText("Active")).toBeInTheDocument();
    expect(within(gaming).getByText("Apps")).toBeInTheDocument();
    const work = screen.getByRole("region", { name: "Work" });
    expect(within(work).getByText("Default output")).toBeInTheDocument();
    expect(within(work).queryByText("Active")).toBeNull();
  });

  it("applies and updates a profile", () => {
    render(<ProfilesView state={withProfiles()} />);
    const work = screen.getByRole("region", { name: "Work" });
    fireEvent.click(within(work).getByRole("button", { name: "Apply" }));
    expect(api.applyProfile).toHaveBeenCalledWith("p2");
    fireEvent.click(within(work).getByRole("button", { name: "Update" }));
    expect(api.updateProfile).toHaveBeenCalledWith("p2");
  });

  it("renames a profile", () => {
    render(<ProfilesView state={withProfiles()} />);
    const gaming = screen.getByRole("region", { name: "Gaming" });
    fireEvent.click(within(gaming).getByRole("button", { name: "Rename" }));
    fireEvent.change(within(gaming).getByRole("textbox", { name: "New name" }), { target: { value: "Games" } });
    fireEvent.click(within(gaming).getByRole("button", { name: "Save" }));
    expect(api.send).toHaveBeenCalledWith({ type: "renameProfile", id: "p1", name: "Games" });
  });

  it("asks before deleting", () => {
    render(<ProfilesView state={withProfiles()} />);
    const work = screen.getByRole("region", { name: "Work" });
    fireEvent.click(within(work).getByRole("button", { name: "Delete" }));
    expect(api.send).not.toHaveBeenCalled();
    fireEvent.click(within(work).getByRole("button", { name: "Delete Work" }));
    expect(api.send).toHaveBeenCalledWith({ type: "deleteProfile", id: "p2" });
  });
});
```

Run: `npm.cmd test -- src/views/ProfilesView.test.tsx` → FAIL (`./ProfilesView` missing).

- [ ] **Step 2: Write `src/views/ProfilesView.tsx`**

```tsx
import { useState } from "react";
import { PARTS, partLabels } from "../lib/profiles";
import { deleteProfile, recaptureProfile, renameProfile, saveProfileAs, switchProfile, useStore } from "../lib/store";
import type { PartChoice, Profile, UiState } from "../lib/types";
import { Badge, Banner, Panel, ViewHeader } from "../components/ui";

const PRIMARY = "key mic-live text-lilac h-8 px-3.5 text-[13px]";
const SECONDARY = "key h-8 px-3 text-[13px]";

function SaveForm() {
  const headsetOn = useStore((s) => s.headset?.connection.state === "connected");
  const [name, setName] = useState("");
  const [parts, setParts] = useState<PartChoice>({ apps: true, processing: true, headphones: false, defaultOutput: false });
  const chosen = { ...parts, headphones: parts.headphones && headsetOn };
  const canSave = name.trim() !== "" && Object.values(chosen).some(Boolean);

  const save = async () => {
    if (await saveProfileAs(name.trim(), chosen)) setName("");
  };

  return (
    <Panel className="flex flex-col gap-4 px-6 py-5">
      <div>
        <h2 className="m-0 text-[16px] font-medium">Save current setup</h2>
        <p className="m-0 mt-0.5 text-[12px] text-fog">Choose what the profile controls. Anything you leave out stays as it is when you switch.</p>
      </div>
      <div className="flex flex-wrap items-end gap-4">
        <label className="flex min-w-[220px] flex-1 flex-col gap-1.5 text-[13px] text-fog">
          Profile name
          <input
            type="text"
            aria-label="Profile name"
            value={name}
            placeholder="e.g. Gaming"
            onChange={(e) => setName(e.currentTarget.value)}
            className="field-select"
          />
        </label>
        <button type="button" className={PRIMARY} disabled={!canSave} onClick={() => void save()}>
          Save profile
        </button>
      </div>
      <div className="flex flex-wrap gap-x-6 gap-y-2">
        {PARTS.map(({ key, label }) => {
          const disabled = key === "headphones" && !headsetOn;
          return (
            <label key={key} className={`flex items-center gap-2 text-[13px] ${disabled ? "text-dim" : "text-paper"}`} title={disabled ? "Connect the Space One to include it" : undefined}>
              <input
                type="checkbox"
                aria-label={label}
                checked={chosen[key]}
                disabled={disabled}
                onChange={(e) => setParts({ ...parts, [key]: e.currentTarget.checked })}
                className="h-4 w-4 accent-[var(--accent)]"
              />
              {label}
            </label>
          );
        })}
      </div>
    </Panel>
  );
}

function ProfileCard({ profile, active }: { profile: Profile; active: boolean }) {
  const [renaming, setRenaming] = useState<string | null>(null);
  const [confirming, setConfirming] = useState(false);

  return (
    <section aria-label={profile.name} className={`flex flex-col gap-4 rounded-[14px] border bg-console px-5 py-4 ${active ? "border-accent/60" : "border-groove"}`}>
      <div className="flex items-start justify-between gap-3">
        {renaming === null ? (
          <h3 className="m-0 truncate text-[16px] font-medium">{profile.name}</h3>
        ) : (
          <div className="flex flex-1 gap-2">
            <input
              type="text"
              aria-label="New name"
              value={renaming}
              onChange={(e) => setRenaming(e.currentTarget.value)}
              className="field-select min-w-0 flex-1"
            />
            <button
              type="button"
              className={PRIMARY}
              disabled={renaming.trim() === ""}
              onClick={() => {
                void renameProfile(profile.id, renaming.trim());
                setRenaming(null);
              }}
            >
              Save
            </button>
            <button type="button" className={SECONDARY} onClick={() => setRenaming(null)}>
              Cancel
            </button>
          </div>
        )}
        {active && renaming === null && <Badge tone="accent">Active</Badge>}
      </div>
      <div className="flex flex-wrap gap-1.5">
        {partLabels(profile).map((label) => (
          <Badge key={label}>{label}</Badge>
        ))}
      </div>
      {confirming ? (
        <div className="flex flex-wrap items-center gap-2">
          <span className="text-[13px] text-fog">Delete this profile?</span>
          <button type="button" className="key h-8 border-coral/60 px-3 text-[13px] text-coral" aria-label={`Delete ${profile.name}`} onClick={() => void deleteProfile(profile.id)}>
            Delete
          </button>
          <button type="button" className={SECONDARY} onClick={() => setConfirming(false)}>
            Cancel
          </button>
        </div>
      ) : (
        <div className="flex flex-wrap gap-2">
          <button type="button" className={PRIMARY} onClick={() => void switchProfile(profile.id)}>
            Apply
          </button>
          <button type="button" className={SECONDARY} title="Save the current setup into this profile" onClick={() => void recaptureProfile(profile.id)}>
            Update
          </button>
          <button type="button" className={SECONDARY} onClick={() => setRenaming(profile.name)}>
            Rename
          </button>
          <button type="button" className={SECONDARY} onClick={() => setConfirming(true)}>
            Delete
          </button>
        </div>
      )}
    </section>
  );
}

export function ProfilesView({ state }: { state: UiState }) {
  return (
    <div className="flex max-w-4xl flex-col gap-5">
      <ViewHeader title="Profiles" subtitle="Switch your whole setup in one click, here or from the tray" />
      <SaveForm />
      {state.profiles.length === 0 ? (
        <Banner>No profiles yet. Set Resonance up the way you like, then save it as a profile.</Banner>
      ) : (
        <div className="grid gap-4 md:grid-cols-2">
          {state.profiles.map((p) => (
            <ProfileCard key={p.id} profile={p} active={p.id === state.activeProfile} />
          ))}
        </div>
      )}
    </div>
  );
}
```

- [ ] **Step 3: Add the tab**

`src/views/Titlebar.tsx`: `export type View = "mixer" | "devices" | "mic" | "headphones" | "profiles" | "settings";` and add `{ id: "profiles", label: "Profiles" },` after the Headphones tab.

`src/views/MainWindow.tsx`: `import { ProfilesView } from "./ProfilesView";` and before the final `: (` settings branch add:

```tsx
        ) : view === "profiles" ? (
          <ProfilesView state={state} />
```

- [ ] **Step 4: Run tests**

Run: `npm.cmd test` → all PASS (7 new).

- [ ] **Step 5: Commit**

```bash
git add src/views
git commit -m "feat(ui): Profiles tab - save, apply, update, rename, delete"
```

---

### Task 6: Flyout profile buttons and the Start with Windows switch

**Files:**
- Create: `src/components/ProfileBar.tsx`, `src/components/ProfileBar.test.tsx`, `src/components/StartWithWindows.tsx`, `src/views/SettingsView.test.tsx`
- Modify: `src/views/Flyout.tsx`, `src/views/SettingsView.tsx`

**Interfaces:**
- Consumes: `switchProfile`, `getAutostart`/`setAutostart`, `Toggle`.
- Produces: `ProfileBar({ state })` (`navigation` "Profiles"); `StartWithWindows()` (switch "Start with Windows").

- [ ] **Step 1: Write the failing tests**

`src/components/ProfileBar.test.tsx`:

```tsx
import { fireEvent, render, screen } from "@testing-library/react";
import { ProfileBar } from "./ProfileBar";
import { makeProfile, makeState } from "../test/fixtures";
import * as api from "../lib/api";

vi.mock("../lib/api", () => ({ applyProfile: vi.fn().mockResolvedValue(undefined) }));

describe("ProfileBar", () => {
  it("is hidden without profiles", () => {
    render(<ProfileBar state={makeState()} />);
    expect(screen.queryByRole("navigation", { name: "Profiles" })).toBeNull();
  });

  it("lights the active profile and switches on click", () => {
    const state = makeState({ profiles: [makeProfile({ id: "p1", name: "Gaming" }), makeProfile({ id: "p2", name: "Work" })], activeProfile: "p1" });
    render(<ProfileBar state={state} />);
    expect(screen.getByRole("button", { name: "Gaming" })).toHaveAttribute("aria-pressed", "true");
    fireEvent.click(screen.getByRole("button", { name: "Work" }));
    expect(api.applyProfile).toHaveBeenCalledWith("p2");
  });
});
```

`src/views/SettingsView.test.tsx`:

```tsx
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { SettingsView } from "./SettingsView";
import { makeState } from "../test/fixtures";
import * as api from "../lib/api";

vi.mock("../lib/api", () => ({
  send: vi.fn().mockResolvedValue(undefined),
  getAutostart: vi.fn().mockResolvedValue(false),
  setAutostart: vi.fn().mockResolvedValue(undefined),
}));

describe("SettingsView", () => {
  it("shows and changes Start with Windows", async () => {
    render(<SettingsView state={makeState()} />);
    const toggle = screen.getByRole("switch", { name: "Start with Windows" });
    await waitFor(() => expect(toggle).toBeEnabled());
    expect(toggle).toHaveAttribute("aria-checked", "false");
    fireEvent.click(toggle);
    expect(api.setAutostart).toHaveBeenCalledWith(true);
    expect(toggle).toHaveAttribute("aria-checked", "true");
  });
});
```

Run: `npm.cmd test -- src/components/ProfileBar.test.tsx src/views/SettingsView.test.tsx` → FAIL.

- [ ] **Step 2: Write `src/components/ProfileBar.tsx`**

```tsx
import { switchProfile } from "../lib/store";
import type { UiState } from "../lib/types";

/** The flyout's profile buttons; the last applied profile is lit. */
export function ProfileBar({ state }: { state: UiState }) {
  if (state.profiles.length === 0) return null;
  return (
    <nav aria-label="Profiles" className="flex gap-1.5 overflow-x-auto border-b border-groove px-3.5 py-2.5">
      {state.profiles.map((p) => {
        const active = p.id === state.activeProfile;
        return (
          <button
            key={p.id}
            type="button"
            aria-pressed={active}
            onClick={() => void switchProfile(p.id)}
            className={`key h-7 shrink-0 px-3 text-[12px] ${active ? "mic-live text-lilac" : ""}`}
          >
            {p.name}
          </button>
        );
      })}
    </nav>
  );
}
```

In `src/views/Flyout.tsx`: `import { ProfileBar } from "../components/ProfileBar";` and render `<ProfileBar state={state} />` immediately before `<section aria-label="Master output"`.

- [ ] **Step 3: Write `src/components/StartWithWindows.tsx`**

```tsx
import { useEffect, useState } from "react";
import { getAutostart, setAutostart } from "../lib/api";
import { errorMessage, useStore } from "../lib/store";
import { Toggle } from "./ui";

/** Switch for the Windows start-up entry; reads the real state, reverts if Windows refuses. */
export function StartWithWindows() {
  const [on, setOn] = useState<boolean | null>(null);

  useEffect(() => {
    let live = true;
    getAutostart()
      .then((v) => live && setOn(v))
      .catch(() => live && setOn(null));
    return () => {
      live = false;
    };
  }, []);

  const change = (v: boolean) => {
    setOn(v);
    setAutostart(v).catch((e) => {
      setOn(!v);
      useStore.getState().pushToast("error", errorMessage(e));
    });
  };

  return <Toggle label="Start with Windows" checked={on ?? false} disabled={on === null} onChange={change} />;
}
```

In `src/views/SettingsView.tsx`: `import { StartWithWindows } from "../components/StartWithWindows";` and add before the "Start in the tray" field:

```tsx
        <Field label="Start with Windows" hint="Resonance starts in the tray when you sign in">
          <StartWithWindows />
        </Field>
```

- [ ] **Step 4: Run tests**

Run: `npm.cmd test` → all PASS. (Add `applyProfile: vi.fn()` to `Flyout.test.tsx`'s api mock if it complains.)

- [ ] **Step 5: Commit**

```bash
git add src
git commit -m "feat(ui): profile buttons in the flyout and the Start with Windows switch"
```

---

### Task 7: First-run setup

**Files:**
- Create: `src/views/Onboarding.tsx`, `src/views/Onboarding.test.tsx`
- Modify: `src/views/MainWindow.tsx`

**Interfaces:**
- Consumes: `finishOnboarding`, `run`, `setProcessing`, `inputs`, `defaultMic` (store); `openVbCable` (api); `StartWithWindows`; `Banner`, `Field`, `Logo`, `Toggle`.
- Produces: `Onboarding({ state })` rendering a `dialog` named "Welcome to Resonance".

- [ ] **Step 1: Write the failing tests**

`src/views/Onboarding.test.tsx`:

```tsx
import { fireEvent, render, screen } from "@testing-library/react";
import { Onboarding } from "./Onboarding";
import { makeDevice, makeProcessing, makeState } from "../test/fixtures";
import * as api from "../lib/api";

vi.mock("../lib/api", () => ({
  send: vi.fn().mockResolvedValue(undefined),
  openVbCable: vi.fn().mockResolvedValue(undefined),
  getAutostart: vi.fn().mockResolvedValue(false),
  setAutostart: vi.fn().mockResolvedValue(undefined),
}));

const fresh = (cable = true) =>
  makeState({ ui: { theme: "system", startMinimized: false, onboarded: false }, processing: makeProcessing({ cableInstalled: cable }) });

const next = () => fireEvent.click(screen.getByRole("button", { name: "Next" }));

describe("Onboarding", () => {
  beforeEach(() => vi.clearAllMocks());

  it("sets the chosen mic for apps and calls", () => {
    const s = fresh();
    s.devices.push(makeDevice({ id: "mic2", name: "Headset Mic", flow: "capture" }));
    render(<Onboarding state={s} />);
    expect(screen.getByRole("dialog", { name: "Welcome to Resonance" })).toBeInTheDocument();
    fireEvent.change(screen.getByRole("combobox", { name: "Your microphone" }), { target: { value: "mic2" } });
    expect(api.send).toHaveBeenCalledWith({ type: "setDefaultDevice", id: "mic2", role: "console" });
    expect(api.send).toHaveBeenCalledWith({ type: "setDefaultDevice", id: "mic2", role: "communications" });
  });

  it("offers VB-Cable when it's missing and hides the processing switch", () => {
    render(<Onboarding state={fresh(false)} />);
    next();
    fireEvent.click(screen.getByRole("button", { name: "Get VB-Cable" }));
    expect(api.openVbCable).toHaveBeenCalled();
    next();
    expect(screen.queryByRole("switch", { name: "Turn on Processing" })).toBeNull();
    expect(screen.getByRole("switch", { name: "Start with Windows" })).toBeInTheDocument();
  });

  it("turns on processing and finishes", () => {
    const s = fresh();
    render(<Onboarding state={s} />);
    next();
    expect(screen.getByText(/vb-cable is installed/i)).toBeInTheDocument();
    next();
    fireEvent.click(screen.getByRole("switch", { name: "Turn on Processing" }));
    expect(api.send).toHaveBeenCalledWith({ type: "setProcessing", processing: { ...s.processing.settings, enabled: true } });
    fireEvent.click(screen.getByRole("button", { name: "Done" }));
    expect(api.send).toHaveBeenLastCalledWith({ type: "setUiSettings", ui: { ...s.ui, onboarded: true } });
  });

  it("can be skipped", () => {
    const s = fresh();
    render(<Onboarding state={s} />);
    fireEvent.click(screen.getByRole("button", { name: "Skip" }));
    expect(api.send).toHaveBeenCalledWith({ type: "setUiSettings", ui: { ...s.ui, onboarded: true } });
  });
});
```

Run: `npm.cmd test -- src/views/Onboarding.test.tsx` → FAIL.

- [ ] **Step 2: Write `src/views/Onboarding.tsx`**

```tsx
import { useState } from "react";
import { openVbCable } from "../lib/api";
import { defaultMic, finishOnboarding, inputs, run, setProcessing } from "../lib/store";
import type { UiState } from "../lib/types";
import { StartWithWindows } from "../components/StartWithWindows";
import { Banner, Field, Logo, Toggle } from "../components/ui";

const PRIMARY = "key mic-live text-lilac h-9 px-4 text-[13px]";
const STEPS = 3;

/** First-run setup: microphone, VB-Cable, then optional processing and Start with Windows. */
export function Onboarding({ state }: { state: UiState }) {
  const [step, setStep] = useState(0);
  const mic = defaultMic(state);
  const cable = state.processing.cableInstalled;
  const done = () => void finishOnboarding(state.ui);

  const chooseMic = (id: string) => {
    void run({ type: "setDefaultDevice", id, role: "console" });
    void run({ type: "setDefaultDevice", id, role: "communications" });
  };

  return (
    <div role="dialog" aria-modal="true" aria-labelledby="welcome-title" className="fixed inset-0 z-50 grid place-items-center bg-black/60 p-6">
      <div className="console flex w-full max-w-lg flex-col gap-5 px-7 py-6">
        <div className="flex items-center gap-3">
          <Logo size={24} />
          <h2 id="welcome-title" className="m-0 text-[20px] font-medium">
            Welcome to Resonance
          </h2>
        </div>
        <p className="m-0 text-[12px] text-dim">
          Step {step + 1} of {STEPS}
        </p>

        {step === 0 && (
          <Field label="Your microphone" hint="The mic apps and calls hear" htmlFor="welcome-mic">
            <select id="welcome-mic" className="field-select" value={mic?.id ?? ""} onChange={(e) => e.currentTarget.value && chooseMic(e.currentTarget.value)}>
              {!mic && <option value="">Choose a microphone</option>}
              {inputs(state).map((d) => (
                <option key={d.id} value={d.id}>
                  {d.name}
                </option>
              ))}
            </select>
          </Field>
        )}

        {step === 1 &&
          (cable ? (
            <Banner>VB-Cable is installed, so Resonance can clean up your mic for every app and mix apps into calls.</Banner>
          ) : (
            <div className="flex flex-col items-start gap-3">
              <Banner>
                To clean up your mic for every app, Resonance uses VB-Cable, a free virtual audio cable. Install it now or any time
                later, then restart your PC.
              </Banner>
              <button type="button" className="key h-9 px-4 text-[13px]" onClick={() => void openVbCable()}>
                Get VB-Cable
              </button>
            </div>
          ))}

        {step === 2 && (
          <div className="flex flex-col gap-4">
            {cable && (
              <div className="flex items-center justify-between gap-4">
                <span className="text-[13px]">Turn on Processing (noise suppression for your mic)</span>
                <Toggle
                  label="Turn on Processing"
                  checked={state.processing.settings.enabled}
                  onChange={(enabled) => void setProcessing({ ...state.processing.settings, enabled })}
                />
              </div>
            )}
            <div className="flex items-center justify-between gap-4">
              <span className="text-[13px]">Start Resonance with Windows</span>
              <StartWithWindows />
            </div>
          </div>
        )}

        <div className="flex items-center justify-between">
          <button type="button" className="border-0 bg-transparent text-[13px] text-fog hover:text-paper" onClick={done}>
            Skip
          </button>
          {step < STEPS - 1 ? (
            <button type="button" className={PRIMARY} onClick={() => setStep(step + 1)}>
              Next
            </button>
          ) : (
            <button type="button" className={PRIMARY} onClick={done}>
              Done
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
```

`src/views/MainWindow.tsx`: `import { Onboarding } from "./Onboarding";` and render `{state && !state.ui.onboarded && <Onboarding state={state} />}` just before `<Toasts />`.

- [ ] **Step 3: Run tests and build**

Run: `npm.cmd test` then `npm.cmd run build` → all PASS; build succeeds.

- [ ] **Step 4: Look at it**

Quit the installed Resonance, `npm.cmd run tauri dev`. The welcome appears (the dev profile's settings predate it): step through it, check the layout, choose Done. Then: Profiles tab — save "Test" (Apps + Default output), see the card; open the tray flyout — the Test button is lit; Settings — Start with Windows switch reflects Windows (toggle it on and off again). Delete the Test profile. Fix any layout problems.

- [ ] **Step 5: Commit**

```bash
git add src
git commit -m "feat(ui): first-run setup"
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

Expected: no warnings; everything PASSES.

- [ ] **Step 2: Hardware test** (installed Resonance quit)

`cargo test --manifest-path M:\Resonance\src-tauri\Cargo.toml --test profiles_hw -- --ignored --nocapture` → PASS; default output unchanged afterwards.

- [ ] **Step 3: Checklist**

Append to `docs/testing/phase1-manual-checklist.md`:

```markdown
## Phase 4 — Profiles, Start with Windows, first-run setup

- [ ] First launch after installing shows the welcome; Skip or Done closes it for good.
- [ ] Save "Gaming" (Apps + Headphones) with the game loud and ANC on; save "Work" (Apps + Headphones) with music low and transparency on.
- [ ] Switching from the tray flyout restores the volumes and the headphone mode; the active button is lit.
- [ ] Save "Call" (Mic & processing): processing on, Spotify sent; switching to it turns processing on and sends Spotify.
- [ ] A profile with only Default output leaves app volumes alone.
- [ ] With the Space One off, switching to Gaming applies the apps and says the headphones were skipped.
- [ ] An app that wasn't running gets its saved volume and output when it starts.
- [ ] Update, Rename and Delete (with confirmation) work; deleting the active profile clears the highlight.
- [ ] Start with Windows on: sign out and in → Resonance is in the tray, no window. Off: it doesn't start.
```

- [ ] **Step 4: README**

Add a bullet to the feature list in `README.md`: "profiles: save your setup (app volumes and routing, mic processing and call sends, headphone mode and EQ, default output) and switch from the tray; start with Windows; a short first-run setup".

- [ ] **Step 5: Installer**

Run: `npm.cmd run tauri build` → installer rebuilt.

- [ ] **Step 6: Commit and hand off**

```bash
git add docs/testing/phase1-manual-checklist.md README.md
git commit -m "docs: Phase 4 checklist and README"
```

Tell the user what was built and verified, what to try, and offer to merge to `main`.

---

## Execution notes (2026-10-08)

- Task 1 also needed `onboarded` added to an existing `UiSettings` literal in `app_state/tests.rs`.
- **Start-up entry follows the installed copy:** switching Start with Windows on from a development build registered
  `targetdebugesonance.exe`. Release builds now call `autolaunch().enable()` on launch when the entry is enabled,
  re-pointing it at themselves; debug builds never touch it.
- Live check: welcome completed in the dev app, Profiles tab save/delete with confirmation, Settings switch shows the
  real state.
- **Startup folder instead of the Run key (after release):** on the user's PC Windows started every enabled Run entry
  at sign-in except Resonance's (valid value, approved, nothing blocked, nothing logged). Start with Windows is now a
  `Resonance.lnk` shortcut in `shell:startup` (`win/startup.rs`, IShellLinkW); `tauri-plugin-autostart` was
  removed. Release builds call `startup::refresh()` on launch, which moves the old Run entry to the shortcut and keeps
  the shortcut pointing at the running executable.
