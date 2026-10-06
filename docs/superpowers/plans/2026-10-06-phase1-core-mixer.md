# Resonance Phase 1 — Core Mixer + Mic Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A tray-resident Windows app with a modern UI that controls per-app volume/mute/output device, all audio devices, and the microphone (gain, mute, boost, global hotkeys), remembering per-app settings.

**Architecture:** Tauri 2 app. A dedicated Rust "audio worker" thread owns all COM/WASAPI objects behind an `AudioBackend` trait (WASAPI impl + fake for tests). Pure logic lives in `AppCore` (state building, per-app memory, command application). The worker polls the OS every 250 ms, emits the full `UiState` to the React UI when it changes, emits meter frames at 30 Hz while a window is visible, and persists settings with a 500 ms debounce.

**Tech Stack:** Rust 1.99 (MSVC), Tauri 2, `windows` 0.62, tauri-plugin-global-shortcut 2, tauri-plugin-single-instance 2, React 19 + TypeScript + Vite, Tailwind CSS 4, Zustand 5, Vitest, lucide-react.

## Global Constraints

- Target OS: Windows 11 (Win10 21H2+ works); x86_64 MSVC toolchain.
- Personal use; GPL-3.0 dependencies allowed (not used in Phase 1).
- Data dir: `%APPDATA%\Resonance\`; settings file `settings.json`; logs `logs\resonance.*.log`, daily rotation, keep 7.
- Settings JSON is camelCase, `version: 1`.
- Per-app memory keyed by lowercase exe file name; system sounds key `#system`; unidentifiable processes `pid:<n>` (never persisted).
- OS poll interval 250 ms; meters 30 Hz (33 ms) only while the main window or flyout is visible; settings save debounce 500 ms; slider command throttle 16 ms.
- Per-app routing uses undocumented `IAudioPolicyConfigFactory` (21H2+ IID `ab3d4648-e242-459f-b02f-541c70306324`, class `Windows.Media.Internal.AudioPolicyConfig`); if activation fails, routing UI is hidden and everything else works.
- Default-device switching uses undocumented `IPolicyConfig` (CLSID `870af99c-171d-4f9e-af0d-e63df40c2bc9`, IID `f8679f50-850a-41cf-9c72-430f290290c8`).
- UI shows OS-reported state only (except while a slider is being dragged).
- Default mic hotkey `Ctrl+Shift+M`, mode `toggle`.
- Shell commands below are PowerShell; use `npm.cmd` (script execution policy blocks `npm.ps1`). Cargo commands run from `M:\Resonance\src-tauri`.

## Spec deviations (decided during planning; Task 1 updates the spec)

1. OS change detection is **polling at 4 Hz** instead of COM notification callbacks: one code path that also covers audio-service restarts and sleep/resume; cost ~1–3 ms per poll.
2. The worker emits the **full `UiState`** (`state` event) instead of patches — it is small (tens of items) and removes patch/merge bugs.
3. Platform access goes through **one `AudioBackend` trait** rather than one trait per module.
4. Per-app **output routing is persisted by Windows itself** (same store the Settings app uses); `settings.json` stores only volume/mute per app.
5. Hold-to-talk / hold-to-mute use **tauri-plugin-global-shortcut's press/release events** instead of a custom low-level keyboard hook.

---

## File Structure

```
M:\Resonance
├─ package.json, index.html, vite.config.ts, tsconfig.json
├─ scripts/make-icons.mjs              generates app + tray PNGs
├─ src/                                 React UI
│  ├─ main.tsx, App.tsx, styles.css
│  ├─ lib/types.ts                      mirrors Rust UiState/Command
│  ├─ lib/api.ts                        invoke/listen wrappers
│  ├─ lib/store.ts                      zustand store, selectors, run()
│  ├─ lib/throttle.ts, lib/levels.ts    pure helpers
│  ├─ lib/meters.ts                     rAF meter engine (no React re-renders)
│  ├─ lib/hotkey.ts                     KeyboardEvent → hotkey string
│  ├─ lib/bootstrap.ts, lib/theme.ts    hooks
│  ├─ components/                       Slider, Meter, MuteButton, AppIcon, DeviceSelect, AppRow, MicStrip, Toasts, HotkeyRecorder
│  ├─ views/                            Flyout, MainWindow, Titlebar, Sidebar, MixerView, ChannelStrip, DevicesView, MicView, SettingsView
│  └─ test/setup.ts, test/fixtures.ts
└─ src-tauri/
   ├─ Cargo.toml, build.rs, tauri.conf.json, capabilities/default.json, icons/
   ├─ src/main.rs, src/lib.rs
   ├─ src/model.rs        all shared data types + Command
   ├─ src/error.rs        AudioError
   ├─ src/settings.rs     Settings load/save
   ├─ src/paths.rs        data/settings/log paths
   ├─ src/backend.rs      AudioBackend trait
   ├─ src/fake.rs         FakeBackend (tests only)
   ├─ src/app_state/{mod.rs, group.rs, tests.rs}   AppCore logic
   ├─ src/worker.rs       audio worker thread + WorkerHandle
   ├─ src/win/            WASAPI implementation (mod, util, com, endpoints, procinfo, policy, routing, boost, meter, icon, accent, backend)
   ├─ src/app/            Tauri wiring (mod, commands, sink, tray, wnd, hotkeys, logging)
   └─ tests/wasapi.rs     #[ignore] integration tests on real hardware
```

---

### Task 1: Project scaffold (frontend + Tauri shell + icons) and spec update

**Files:**
- Create: `package.json`, `index.html`, `vite.config.ts`, `tsconfig.json`, `.gitignore`, `scripts/make-icons.mjs`, `src/main.tsx`, `src/App.tsx`, `src/styles.css`, `src/test/setup.ts`, `src/lib/levels.ts`, `src/lib/levels.test.ts`
- Create: `src-tauri/Cargo.toml`, `src-tauri/build.rs`, `src-tauri/tauri.conf.json`, `src-tauri/capabilities/default.json`, `src-tauri/src/main.rs`, `src-tauri/src/lib.rs`
- Modify: `docs/superpowers/specs/2026-10-06-resonance-design.md` (record the 4 deviations above)

**Interfaces:**
- Produces: `peakToPosition(peak: number): number`, `MIN_DB = -60` in `src/lib/levels.ts`; crate `resonance_lib` with `pub fn run()`.

- [ ] **Step 1: Write `package.json`**

```json
{
  "name": "resonance",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "tsc --noEmit && vite build",
    "test": "vitest run",
    "tauri": "tauri",
    "icons": "node scripts/make-icons.mjs && tauri icon src-tauri/icons/app-icon.png"
  }
}
```

- [ ] **Step 2: Install dependencies**

Run:
```powershell
npm.cmd install react react-dom zustand lucide-react @tauri-apps/api
npm.cmd install -D @tauri-apps/cli typescript vite @vitejs/plugin-react tailwindcss @tailwindcss/vite vitest jsdom @testing-library/react @testing-library/user-event @testing-library/jest-dom @types/react @types/react-dom @types/node
```
Expected: both finish with `added N packages`, no `ERR!`.

- [ ] **Step 3: Write config files**

`.gitignore`:
```
node_modules/
dist/
src-tauri/target/
src-tauri/gen/
*.log
```

`index.html`:
```html
<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>Resonance</title>
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="/src/main.tsx"></script>
  </body>
</html>
```

`vite.config.ts`:
```ts
/// <reference types="vitest/config" />
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  build: { target: "chrome120", outDir: "dist" },
  test: { environment: "jsdom", setupFiles: ["./src/test/setup.ts"], globals: true },
});
```

`tsconfig.json`:
```json
{
  "compilerOptions": {
    "target": "ES2022",
    "lib": ["ES2022", "DOM", "DOM.Iterable"],
    "module": "ESNext",
    "moduleResolution": "bundler",
    "jsx": "react-jsx",
    "strict": true,
    "noUnusedLocals": true,
    "noUnusedParameters": true,
    "skipLibCheck": true,
    "isolatedModules": true,
    "noEmit": true,
    "types": ["vitest/globals", "@testing-library/jest-dom"]
  },
  "include": ["src", "vite.config.ts"]
}
```

`src/test/setup.ts`:
```ts
import "@testing-library/jest-dom/vitest";

if (!window.matchMedia) {
  window.matchMedia = (query: string) =>
    ({
      matches: false,
      media: query,
      onchange: null,
      addEventListener: () => {},
      removeEventListener: () => {},
      addListener: () => {},
      removeListener: () => {},
      dispatchEvent: () => false,
    }) as MediaQueryList;
}
if (!globalThis.requestAnimationFrame) {
  globalThis.requestAnimationFrame = (cb) => setTimeout(() => cb(performance.now()), 16) as unknown as number;
  globalThis.cancelAnimationFrame = (id) => clearTimeout(id);
}
```

- [ ] **Step 4: Write the failing test for `peakToPosition`**

`src/lib/levels.test.ts`:
```ts
import { peakToPosition, MIN_DB } from "./levels";

describe("peakToPosition", () => {
  it("maps silence and negatives to 0", () => {
    expect(peakToPosition(0)).toBe(0);
    expect(peakToPosition(-1)).toBe(0);
  });
  it("maps full scale to 1 and clamps above", () => {
    expect(peakToPosition(1)).toBe(1);
    expect(peakToPosition(2)).toBe(1);
  });
  it("maps -30 dB to the midpoint of a 60 dB range", () => {
    expect(MIN_DB).toBe(-60);
    expect(peakToPosition(10 ** (-30 / 20))).toBeCloseTo(0.5, 5);
  });
  it("maps anything below MIN_DB to 0", () => {
    expect(peakToPosition(10 ** (-80 / 20))).toBe(0);
  });
});
```

- [ ] **Step 5: Run it to verify it fails**

Run: `npm.cmd test -- src/lib/levels.test.ts`
Expected: FAIL — `Failed to resolve import "./levels"`.

- [ ] **Step 6: Implement `src/lib/levels.ts`**

```ts
/** Lowest level shown on meters, in dBFS. */
export const MIN_DB = -60;

/** Converts a linear peak (0..1) to a 0..1 meter position on a dB scale. */
export function peakToPosition(peak: number): number {
  if (!(peak > 0)) return 0;
  const db = 20 * Math.log10(peak);
  return Math.min(1, Math.max(0, (db - MIN_DB) / -MIN_DB));
}
```

- [ ] **Step 7: Run the test to verify it passes**

Run: `npm.cmd test -- src/lib/levels.test.ts`
Expected: PASS (4 tests).

- [ ] **Step 8: Write the minimal React entry**

`src/styles.css`:
```css
@import "tailwindcss";
```

`src/main.tsx`:
```tsx
import React from "react";
import ReactDOM from "react-dom/client";
import { App } from "./App";
import "./styles.css";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
```

`src/App.tsx` (replaced in Task 12):
```tsx
export function App() {
  return <div className="p-6 text-white">Resonance</div>;
}
```

- [ ] **Step 9: Write the icon generator `scripts/make-icons.mjs`**

```js
// Generates src-tauri/icons/{app-icon,tray,tray-muted}.png with no dependencies.
import { deflateSync } from "node:zlib";
import { mkdirSync, writeFileSync } from "node:fs";

const crcTable = new Uint32Array(256).map((_, n) => {
  let c = n;
  for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  return c >>> 0;
});
const crc32 = (buf) => {
  let c = 0xffffffff;
  for (const b of buf) c = crcTable[(c ^ b) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
};
function chunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length);
  const body = Buffer.concat([Buffer.from(type, "ascii"), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body));
  return Buffer.concat([len, body, crc]);
}
function png(size, pixel) {
  const stride = size * 4 + 1;
  const raw = Buffer.alloc(stride * size);
  for (let y = 0; y < size; y++) {
    raw[y * stride] = 0;
    for (let x = 0; x < size; x++) {
      const [r, g, b, a] = pixel((x + 0.5) / size, (y + 0.5) / size, size);
      const o = y * stride + 1 + x * 4;
      raw[o] = r; raw[o + 1] = g; raw[o + 2] = b; raw[o + 3] = a;
    }
  }
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(size, 0);
  ihdr.writeUInt32BE(size, 4);
  ihdr[8] = 8; ihdr[9] = 6; ihdr[10] = 0; ihdr[11] = 0; ihdr[12] = 0;
  return Buffer.concat([
    Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]),
    chunk("IHDR", ihdr),
    chunk("IDAT", deflateSync(raw)),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}
const clamp = (v) => Math.max(0, Math.min(1, v));
const mix = (a, b, t) => a.map((av, i) => av + (b[i] - av) * t);
const ACCENT = [124, 92, 255];
const LIGHT = [236, 236, 246];
const BASE = [22, 22, 30];
const RED = [255, 77, 94];

/** Rounded-square badge with a centre dot and two "resonance" rings. */
function glyph({ muted }) {
  return (u, v, s) => {
    const aa = 1.5 / s;
    const r = 0.22;
    const qx = Math.abs(u - 0.5) - (0.5 - r);
    const qy = Math.abs(v - 0.5) - (0.5 - r);
    const box = Math.hypot(Math.max(qx, 0), Math.max(qy, 0)) + Math.min(Math.max(qx, qy), 0) - r;
    let alpha = clamp(0.5 - box / aa);
    let col = BASE;
    const d = Math.hypot(u - 0.5, v - 0.5);
    const dot = clamp(0.5 - (d - 0.09) / aa);
    col = mix(col, ACCENT, dot);
    const ring1 = clamp(0.5 - (Math.abs(d - 0.21) - 0.025) / aa);
    col = mix(col, ACCENT, ring1);
    const ring2 = clamp(0.5 - (Math.abs(d - 0.33) - 0.02) / aa);
    col = mix(col, LIGHT, ring2);
    if (muted) {
      const md = Math.hypot(u - 0.76, v - 0.76);
      const k = clamp(0.5 - (md - 0.22) / aa);
      col = mix(col, RED, k);
      alpha = Math.max(alpha, k);
    }
    return [...col.map(Math.round), Math.round(alpha * 255)];
  };
}

mkdirSync("src-tauri/icons", { recursive: true });
writeFileSync("src-tauri/icons/app-icon.png", png(1024, glyph({ muted: false })));
writeFileSync("src-tauri/icons/tray.png", png(64, glyph({ muted: false })));
writeFileSync("src-tauri/icons/tray-muted.png", png(64, glyph({ muted: true })));
console.log("icons written");
```

- [ ] **Step 10: Write the Tauri crate files**

`src-tauri/Cargo.toml`:
```toml
[package]
name = "resonance"
version = "0.1.0"
edition = "2021"

[lib]
name = "resonance_lib"
crate-type = ["staticlib", "cdylib", "rlib"]

[build-dependencies]
tauri-build = { version = "2", features = [] }

[dependencies]
tauri = { version = "2", features = ["tray-icon", "image-png"] }
tauri-plugin-global-shortcut = "2"
tauri-plugin-single-instance = "2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
tokio = { version = "1", features = ["sync"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
tracing-appender = "0.2"
png = "0.18"
base64 = "0.22"

[target.'cfg(windows)'.dependencies]
windows-core = "0.62"
windows = { version = "0.62", features = [
  "Win32_Foundation",
  "Win32_Media_Audio",
  "Win32_Media_Audio_Endpoints",
  "Win32_System_Com",
  "Win32_System_Com_StructuredStorage",
  "Win32_System_Variant",
  "Win32_UI_Shell_PropertiesSystem",
  "Win32_Devices_FunctionDiscovery",
  "Win32_System_Threading",
  "Win32_Storage_FileSystem",
  "Win32_System_WinRT",
  "Win32_UI_Shell",
  "Win32_UI_WindowsAndMessaging",
  "Win32_Graphics_Gdi",
  "Win32_System_Registry",
] }

[dev-dependencies]
tempfile = "3"
```

`src-tauri/build.rs`:
```rust
fn main() {
    tauri_build::build()
}
```

`src-tauri/src/main.rs`:
```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    resonance_lib::run()
}
```

`src-tauri/src/lib.rs` (grows in later tasks):
```rust
pub fn run() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("error while running Resonance");
}
```

`src-tauri/tauri.conf.json`:
```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "Resonance",
  "version": "0.1.0",
  "identifier": "com.resonance.app",
  "build": {
    "beforeDevCommand": "npm run dev",
    "devUrl": "http://localhost:1420",
    "beforeBuildCommand": "npm run build",
    "frontendDist": "../dist"
  },
  "app": {
    "windows": [
      {
        "label": "main",
        "title": "Resonance",
        "url": "index.html",
        "width": 1120,
        "height": 720,
        "minWidth": 880,
        "minHeight": 560,
        "center": true,
        "decorations": false,
        "transparent": true,
        "visible": false,
        "windowEffects": { "effects": ["mica"] }
      },
      {
        "label": "flyout",
        "title": "Resonance",
        "url": "index.html?view=flyout",
        "width": 380,
        "height": 560,
        "decorations": false,
        "transparent": true,
        "visible": false,
        "resizable": false,
        "skipTaskbar": true,
        "alwaysOnTop": true,
        "windowEffects": { "effects": ["acrylic"] }
      }
    ],
    "security": { "csp": null }
  },
  "bundle": {
    "active": true,
    "targets": ["nsis"],
    "icon": ["icons/32x32.png", "icons/128x128.png", "icons/128x128@2x.png", "icons/icon.ico"]
  }
}
```

`src-tauri/capabilities/default.json`:
```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "Resonance windows",
  "windows": ["main", "flyout"],
  "permissions": [
    "core:default",
    "core:window:allow-start-dragging",
    "core:window:allow-minimize",
    "core:window:allow-toggle-maximize",
    "core:window:allow-close",
    "core:window:allow-hide",
    "core:window:allow-set-theme"
  ]
}
```

- [ ] **Step 11: Generate icons and verify everything builds**

Run:
```powershell
npm.cmd run icons
npm.cmd run build
cd src-tauri; cargo check; cd ..
```
Expected: `icons written`; tauri icon lists generated files including `icon.ico`; Vite build succeeds; `cargo check` ends with `Finished`.

- [ ] **Step 12: Record the deviations in the spec**

In `docs/superpowers/specs/2026-10-06-resonance-design.md`, append this section at the end:

```markdown
## 11. Decisions made during Phase 1 planning

- OS change detection polls every 250 ms instead of COM notification callbacks (single path that also handles
  audio-service restarts and sleep/resume).
- The worker emits the full `UiState` on change (`state` event) instead of patches.
- Platform access goes through one `AudioBackend` trait (WASAPI implementation + fake for tests).
- Per-app output routing is persisted by Windows itself; `settings.json` keeps only per-app volume/mute.
- Settings JSON uses camelCase (e.g. `startMinimized`). Mic meters open a capture stream only while a window is visible.
- Hotkey hold modes use tauri-plugin-global-shortcut press/release events instead of a low-level keyboard hook.
```

- [ ] **Step 13: Commit**

```powershell
git add -A
git commit -m "chore: scaffold Tauri + React project, icons, spec decisions"
```

---

### Task 2: Shared model types and errors

**Files:**
- Create: `src-tauri/src/model.rs`, `src-tauri/src/error.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Produces (all `pub`, in `resonance_lib::model`): `Flow {Render, Capture}`, `DeviceState {Active, Unplugged}`, `Role {Console, Communications}`, `Boost {db, min, max, step: f32}`, `RawDevice`, `RawSession`, `Snapshot`, `Peaks`, `Device`, `App`, `HotkeyMode {Toggle, PushToTalk, PushToMute}`, `MicConfig {hotkey: Option<String>, mode}`, `Theme {System, Light, Dark}`, `UiSettings {theme, start_minimized}`, `UiState`, `MeterFrame`, `Command` (variants listed in code).
- Produces `resonance_lib::error::{AudioError, Result}`; `AudioError::code() -> &'static str`; serializes as `{ "code", "message" }`.

- [ ] **Step 1: Write `src-tauri/src/error.rs`**

```rust
use serde::{ser::SerializeStruct, Serialize, Serializer};

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum AudioError {
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Unavailable(String),
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    Os(String),
}

impl AudioError {
    pub fn code(&self) -> &'static str {
        match self {
            AudioError::NotFound(_) => "notFound",
            AudioError::Unavailable(_) => "unavailable",
            AudioError::Invalid(_) => "invalid",
            AudioError::Os(_) => "os",
        }
    }
}

impl Serialize for AudioError {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        let mut s = serializer.serialize_struct("AudioError", 2)?;
        s.serialize_field("code", self.code())?;
        s.serialize_field("message", &self.to_string())?;
        s.end()
    }
}

#[cfg(windows)]
impl From<windows::core::Error> for AudioError {
    fn from(e: windows::core::Error) -> Self {
        AudioError::Os(format!("{} (0x{:08X})", e.message(), e.code().0 as u32))
    }
}

pub type Result<T> = std::result::Result<T, AudioError>;
```

- [ ] **Step 2: Write the failing tests at the bottom of a new `src-tauri/src/model.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_deserializes_from_tagged_camel_case_json() {
        let c: Command =
            serde_json::from_str(r#"{"type":"setAppVolume","key":"spotify.exe","volume":0.5}"#).unwrap();
        assert_eq!(c, Command::SetAppVolume { key: "spotify.exe".into(), volume: 0.5 });

        let c: Command =
            serde_json::from_str(r#"{"type":"setAppOutput","key":"a.exe","deviceId":null}"#).unwrap();
        assert_eq!(c, Command::SetAppOutput { key: "a.exe".into(), device_id: None });

        let c: Command =
            serde_json::from_str(r#"{"type":"setDefaultDevice","id":"x","role":"communications"}"#).unwrap();
        assert_eq!(c, Command::SetDefaultDevice { id: "x".into(), role: Role::Communications });

        let c: Command = serde_json::from_str(r#"{"type":"toggleMicMute"}"#).unwrap();
        assert_eq!(c, Command::ToggleMicMute);

        let c: Command = serde_json::from_str(
            r#"{"type":"setMicConfig","config":{"hotkey":"Ctrl+F13","mode":"pushToTalk"}}"#,
        )
        .unwrap();
        assert_eq!(
            c,
            Command::SetMicConfig {
                config: MicConfig { hotkey: Some("Ctrl+F13".into()), mode: HotkeyMode::PushToTalk }
            }
        );
    }

    #[test]
    fn ui_state_serializes_camel_case() {
        let s = UiState { routing_available: true, ..Default::default() };
        let v = serde_json::to_value(&s).unwrap();
        assert_eq!(v["routingAvailable"], true);
        assert_eq!(v["ui"]["startMinimized"], false);
        assert_eq!(v["mic"]["hotkey"], "Ctrl+Shift+M");
        assert_eq!(v["mic"]["mode"], "toggle");
    }

    #[test]
    fn audio_error_serializes_code_and_message() {
        let v = serde_json::to_value(crate::error::AudioError::NotFound("gone".into())).unwrap();
        assert_eq!(v["code"], "notFound");
        assert_eq!(v["message"], "gone");
    }
}
```

And add to `src-tauri/src/lib.rs` above `run`:
```rust
pub mod error;
pub mod model;
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test --lib model::`
Expected: FAIL to compile — `cannot find type Command`.

- [ ] **Step 4: Implement the types at the top of `src-tauri/src/model.rs`**

```rust
use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Flow {
    Render,
    Capture,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DeviceState {
    Active,
    Unplugged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Role {
    Console,
    Communications,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Boost {
    pub db: f32,
    pub min: f32,
    pub max: f32,
    pub step: f32,
}

/// A device exactly as the OS reports it.
#[derive(Debug, Clone, PartialEq)]
pub struct RawDevice {
    pub id: String,
    pub name: String,
    pub flow: Flow,
    pub state: DeviceState,
    pub default_console: bool,
    pub default_comms: bool,
    pub volume: f32,
    pub muted: bool,
    pub boost: Option<Boost>,
}

/// An audio session exactly as the OS reports it. `display_name` is the session's own name or,
/// failing that, the executable's FileDescription.
#[derive(Debug, Clone, PartialEq)]
pub struct RawSession {
    pub id: String,
    pub pid: u32,
    pub exe_path: Option<String>,
    pub display_name: Option<String>,
    pub is_system: bool,
    pub device_id: String,
    pub volume: f32,
    pub muted: bool,
    pub output: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Snapshot {
    pub devices: Vec<RawDevice>,
    pub sessions: Vec<RawSession>,
}

/// Linear peak values (0..1) keyed by session id.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Peaks {
    pub sessions: HashMap<String, f32>,
    pub master: f32,
    pub mic: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    pub id: String,
    /// Alias if set, otherwise the OS name.
    pub name: String,
    pub original_name: String,
    pub alias: Option<String>,
    pub hidden: bool,
    pub flow: Flow,
    pub state: DeviceState,
    pub default_console: bool,
    pub default_comms: bool,
    pub volume: f32,
    pub muted: bool,
    pub boost: Option<Boost>,
}

/// All sessions belonging to one executable, controlled together.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct App {
    pub key: String,
    pub name: String,
    pub exe_path: Option<String>,
    pub is_system: bool,
    pub pids: Vec<u32>,
    pub session_ids: Vec<String>,
    pub volume: f32,
    pub muted: bool,
    pub output: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HotkeyMode {
    #[default]
    Toggle,
    PushToTalk,
    PushToMute,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MicConfig {
    pub hotkey: Option<String>,
    pub mode: HotkeyMode,
}

impl Default for MicConfig {
    fn default() -> Self {
        Self { hotkey: Some("Ctrl+Shift+M".into()), mode: HotkeyMode::Toggle }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct UiSettings {
    pub theme: Theme,
    pub start_minimized: bool,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UiState {
    pub devices: Vec<Device>,
    pub apps: Vec<App>,
    pub routing_available: bool,
    pub mic: MicConfig,
    pub ui: UiSettings,
    /// Windows accent colour as `#rrggbb`.
    pub accent: String,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeterFrame {
    /// Linear peak per app key.
    pub apps: HashMap<String, f32>,
    pub master: f32,
    pub mic: f32,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Command {
    SetAppVolume { key: String, volume: f32 },
    SetAppMute { key: String, muted: bool },
    SetAppOutput { key: String, device_id: Option<String> },
    SetDeviceVolume { id: String, volume: f32 },
    SetDeviceMute { id: String, muted: bool },
    SetDefaultDevice { id: String, role: Role },
    SetDeviceAlias { id: String, alias: Option<String> },
    SetDeviceHidden { id: String, hidden: bool },
    SetMicBoost { id: String, db: f32 },
    SetMicMute { muted: bool },
    ToggleMicMute,
    SetMicConfig { config: MicConfig },
    SetUiSettings { ui: UiSettings },
    ResetAppMemory,
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --lib model::`
Expected: PASS (3 tests).

- [ ] **Step 6: Commit**

```powershell
git add -A
git commit -m "feat(core): shared model types, commands and errors"
```

---

### Task 3: Settings persistence and paths

**Files:**
- Create: `src-tauri/src/settings.rs`, `src-tauri/src/paths.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: `MicConfig`, `UiSettings` from Task 2.
- Produces: `settings::{Settings, AppPrefs, DevicePrefs, SETTINGS_VERSION}`; `Settings::load(&Path) -> Settings` (never fails; corrupt files backed up as `settings.bad-<unix>.json`); `Settings::save(&self, &Path) -> std::io::Result<()>` (atomic tmp+rename). `paths::{data_dir, settings_path, logs_dir}() -> PathBuf`.

- [ ] **Step 1: Write failing tests at the bottom of a new `src-tauri/src/settings.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{HotkeyMode, Theme};

    #[test]
    fn missing_file_gives_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let s = Settings::load(&dir.path().join("settings.json"));
        assert_eq!(s, Settings::default());
        assert_eq!(s.version, SETTINGS_VERSION);
        assert_eq!(s.mic.hotkey.as_deref(), Some("Ctrl+Shift+M"));
    }

    #[test]
    fn round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("settings.json");
        let mut s = Settings::default();
        s.apps.insert("spotify.exe".into(), AppPrefs { volume: 0.4, muted: true });
        s.devices.insert("dev".into(), DevicePrefs { alias: Some("Desk".into()), hidden: true });
        s.mic.mode = HotkeyMode::PushToMute;
        s.ui.theme = Theme::Light;
        s.save(&path).unwrap();
        assert_eq!(Settings::load(&path), s);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"startMinimized\""));
    }

    #[test]
    fn corrupt_file_is_backed_up_and_defaults_used() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, "{ not json").unwrap();
        let s = Settings::load(&path);
        assert_eq!(s, Settings::default());
        assert!(!path.exists());
        let backups: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with("settings.bad-"))
            .collect();
        assert_eq!(backups.len(), 1);
    }

    #[test]
    fn unknown_and_missing_fields_are_tolerated() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, r#"{"version":1,"apps":{"a.exe":{"volume":0.2}},"future":true}"#).unwrap();
        let s = Settings::load(&path);
        assert_eq!(s.apps["a.exe"], AppPrefs { volume: 0.2, muted: false });
        assert_eq!(s.mic, crate::model::MicConfig::default());
    }
}
```

Add to `lib.rs`: `pub mod paths;` and `pub mod settings;`

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib settings::`
Expected: FAIL to compile — `cannot find type Settings`.

- [ ] **Step 3: Implement `settings.rs` (top of file)**

```rust
use std::{
    collections::BTreeMap,
    fs, io,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

use crate::model::{MicConfig, UiSettings};

pub const SETTINGS_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppPrefs {
    pub volume: f32,
    pub muted: bool,
}

impl Default for AppPrefs {
    fn default() -> Self {
        Self { volume: 1.0, muted: false }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DevicePrefs {
    pub alias: Option<String>,
    pub hidden: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub version: u32,
    pub apps: BTreeMap<String, AppPrefs>,
    pub devices: BTreeMap<String, DevicePrefs>,
    pub mic: MicConfig,
    pub ui: UiSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: SETTINGS_VERSION,
            apps: BTreeMap::new(),
            devices: BTreeMap::new(),
            mic: MicConfig::default(),
            ui: UiSettings::default(),
        }
    }
}

impl Settings {
    /// Loads settings; never fails. A corrupt file is renamed to `settings.bad-<unix>.json`.
    pub fn load(path: &Path) -> Settings {
        let text = match fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Settings::default(),
            Err(e) => {
                tracing::warn!("couldn't read settings {}: {e}", path.display());
                return Settings::default();
            }
        };
        match serde_json::from_str::<Settings>(&text) {
            Ok(s) => s,
            Err(e) => {
                let ts = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
                let backup = path.with_file_name(format!("settings.bad-{ts}.json"));
                tracing::warn!("settings file is corrupt ({e}); moved to {}", backup.display());
                let _ = fs::rename(path, backup);
                Settings::default()
            }
        }
    }

    /// Writes atomically (temp file + rename), creating the directory if needed.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, serde_json::to_vec_pretty(self).map_err(io::Error::other)?)?;
        fs::rename(&tmp, path)
    }
}
```

`src-tauri/src/paths.rs`:
```rust
use std::path::PathBuf;

/// `%APPDATA%\Resonance` (falls back to the current directory if APPDATA is unset).
pub fn data_dir() -> PathBuf {
    std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(".")).join("Resonance")
}

pub fn settings_path() -> PathBuf {
    data_dir().join("settings.json")
}

pub fn logs_dir() -> PathBuf {
    data_dir().join("logs")
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib settings::`
Expected: PASS (4 tests).

- [ ] **Step 5: Commit**

```powershell
git add -A
git commit -m "feat(core): settings persistence with corruption recovery"
```

---

### Task 4: AudioBackend trait, fake backend, session grouping

**Files:**
- Create: `src-tauri/src/backend.rs`, `src-tauri/src/fake.rs`, `src-tauri/src/app_state/group.rs`, `src-tauri/src/app_state/mod.rs` (temporary: only `mod group; pub use group::*;`)
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Produces `backend::AudioBackend` trait (exact methods below).
- Produces `fake::FakeBackend` (cfg(test)) with pub fields `devices, sessions, routing, calls: Vec<String>, snapshots: usize, peaks: Peaks, fail_next_snapshot: bool` and helpers `FakeBackend::device(id, flow) -> RawDevice`, `FakeBackend::session(id, pid, exe: Option<&str>) -> RawSession`.
- Produces `app_state::{SYSTEM_KEY, app_key(&RawSession) -> String, group_sessions(&[RawSession]) -> Vec<App>}`.

- [ ] **Step 1: Write `src-tauri/src/backend.rs`**

```rust
use crate::{
    error::Result,
    model::{Peaks, Role, Snapshot},
};

/// Everything Resonance needs from the OS audio stack. Implemented by `win::WasapiBackend`
/// and by `fake::FakeBackend` in tests. Not `Send`: lives on the audio worker thread.
pub trait AudioBackend {
    fn snapshot(&mut self) -> Result<Snapshot>;
    fn set_session_volume(&mut self, session_id: &str, volume: f32) -> Result<()>;
    fn set_session_mute(&mut self, session_id: &str, muted: bool) -> Result<()>;
    fn set_device_volume(&mut self, device_id: &str, volume: f32) -> Result<()>;
    fn set_device_mute(&mut self, device_id: &str, muted: bool) -> Result<()>;
    fn set_default_device(&mut self, device_id: &str, role: Role) -> Result<()>;
    fn routing_available(&self) -> bool;
    /// `None` = follow the system default.
    fn set_app_output(&mut self, pid: u32, device_id: Option<&str>) -> Result<()>;
    fn set_mic_boost(&mut self, device_id: &str, db: f32) -> Result<()>;
    /// Peaks for sessions from the last snapshot, the default output, and `mic_device_id`
    /// (opening a capture stream for it if needed).
    fn peaks(&mut self, mic_device_id: Option<&str>) -> Peaks;
    /// Releases any capture stream opened for metering.
    fn stop_metering(&mut self);
}
```

- [ ] **Step 2: Write `src-tauri/src/fake.rs`**

```rust
use crate::{
    backend::AudioBackend,
    error::{AudioError, Result},
    model::*,
};

#[derive(Default)]
pub struct FakeBackend {
    pub devices: Vec<RawDevice>,
    pub sessions: Vec<RawSession>,
    pub routing: bool,
    pub calls: Vec<String>,
    pub snapshots: usize,
    pub peaks: Peaks,
    pub fail_next_snapshot: bool,
}

impl FakeBackend {
    pub fn device(id: &str, flow: Flow) -> RawDevice {
        RawDevice {
            id: id.into(),
            name: format!("Device {id}"),
            flow,
            state: DeviceState::Active,
            default_console: false,
            default_comms: false,
            volume: 0.5,
            muted: false,
            boost: None,
        }
    }

    pub fn session(id: &str, pid: u32, exe: Option<&str>) -> RawSession {
        RawSession {
            id: id.into(),
            pid,
            exe_path: exe.map(|e| format!("C:\\Apps\\{e}")),
            display_name: None,
            is_system: false,
            device_id: "out1".into(),
            volume: 1.0,
            muted: false,
            output: None,
        }
    }

    fn device_mut(&mut self, id: &str) -> Result<&mut RawDevice> {
        self.devices.iter_mut().find(|d| d.id == id).ok_or_else(|| AudioError::NotFound(id.into()))
    }

    fn session_mut(&mut self, id: &str) -> Result<&mut RawSession> {
        self.sessions.iter_mut().find(|s| s.id == id).ok_or_else(|| AudioError::NotFound(id.into()))
    }
}

impl AudioBackend for FakeBackend {
    fn snapshot(&mut self) -> Result<Snapshot> {
        self.snapshots += 1;
        if std::mem::take(&mut self.fail_next_snapshot) {
            return Err(AudioError::Os("fake failure".into()));
        }
        Ok(Snapshot { devices: self.devices.clone(), sessions: self.sessions.clone() })
    }

    fn set_session_volume(&mut self, id: &str, volume: f32) -> Result<()> {
        self.calls.push(format!("session_volume {id} {volume:.2}"));
        self.session_mut(id)?.volume = volume;
        Ok(())
    }

    fn set_session_mute(&mut self, id: &str, muted: bool) -> Result<()> {
        self.calls.push(format!("session_mute {id} {muted}"));
        self.session_mut(id)?.muted = muted;
        Ok(())
    }

    fn set_device_volume(&mut self, id: &str, volume: f32) -> Result<()> {
        self.calls.push(format!("device_volume {id} {volume:.2}"));
        self.device_mut(id)?.volume = volume;
        Ok(())
    }

    fn set_device_mute(&mut self, id: &str, muted: bool) -> Result<()> {
        self.calls.push(format!("device_mute {id} {muted}"));
        self.device_mut(id)?.muted = muted;
        Ok(())
    }

    fn set_default_device(&mut self, id: &str, role: Role) -> Result<()> {
        self.calls.push(format!("default {id} {role:?}"));
        let flow = self.device_mut(id)?.flow;
        for d in self.devices.iter_mut().filter(|d| d.flow == flow) {
            match role {
                Role::Console => d.default_console = d.id == id,
                Role::Communications => d.default_comms = d.id == id,
            }
        }
        Ok(())
    }

    fn routing_available(&self) -> bool {
        self.routing
    }

    fn set_app_output(&mut self, pid: u32, device_id: Option<&str>) -> Result<()> {
        if !self.routing {
            return Err(AudioError::Unavailable("routing".into()));
        }
        self.calls.push(format!("output {pid} {device_id:?}"));
        for s in self.sessions.iter_mut().filter(|s| s.pid == pid) {
            s.output = device_id.map(String::from);
        }
        Ok(())
    }

    fn set_mic_boost(&mut self, id: &str, db: f32) -> Result<()> {
        self.calls.push(format!("boost {id} {db:.1}"));
        let d = self.device_mut(id)?;
        let b = d.boost.as_mut().ok_or_else(|| AudioError::Unavailable("no boost".into()))?;
        b.db = db;
        Ok(())
    }

    fn peaks(&mut self, mic_device_id: Option<&str>) -> Peaks {
        self.calls.push(format!("peaks {mic_device_id:?}"));
        self.peaks.clone()
    }

    fn stop_metering(&mut self) {
        self.calls.push("stop_metering".into());
    }
}
```

- [ ] **Step 3: Write failing tests at the bottom of a new `src-tauri/src/app_state/group.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake::FakeBackend;

    #[test]
    fn keys_by_lowercase_exe_name_system_and_pid() {
        let mut sys = FakeBackend::session("s", 0, None);
        sys.is_system = true;
        assert_eq!(app_key(&sys), SYSTEM_KEY);
        assert_eq!(app_key(&FakeBackend::session("a", 1, Some("Spotify.EXE"))), "spotify.exe");
        assert_eq!(app_key(&FakeBackend::session("b", 42, None)), "pid:42");
    }

    #[test]
    fn groups_sessions_of_the_same_exe() {
        let mut a1 = FakeBackend::session("a1", 10, Some("chrome.exe"));
        a1.volume = 0.3;
        let a2 = FakeBackend::session("a2", 11, Some("chrome.exe"));
        let b = FakeBackend::session("b", 20, Some("discord.exe"));
        let apps = group_sessions(&[a1, a2, b]);
        assert_eq!(apps.len(), 2);
        let chrome = apps.iter().find(|a| a.key == "chrome.exe").unwrap();
        assert_eq!(chrome.session_ids, vec!["a1", "a2"]);
        assert_eq!(chrome.pids, vec![10, 11]);
        assert_eq!(chrome.volume, 0.3, "group volume comes from the first session");
    }

    #[test]
    fn names_prefer_display_name_then_exe_stem() {
        let mut a = FakeBackend::session("a", 1, Some("spotify.exe"));
        a.display_name = Some("Spotify Music".into());
        let b = FakeBackend::session("b", 2, Some("obs64.exe"));
        let mut sys = FakeBackend::session("s", 0, None);
        sys.is_system = true;
        let c = FakeBackend::session("c", 77, None);
        let apps = group_sessions(&[sys, a, b, c]);
        let names: Vec<_> = apps.iter().map(|a| a.name.as_str()).collect();
        // sorted alphabetically, system sounds last
        assert_eq!(names, vec!["Obs64", "Process 77", "Spotify Music", "System sounds"]);
    }

    #[test]
    fn output_comes_from_any_session_that_has_one() {
        let a1 = FakeBackend::session("a1", 10, Some("game.exe"));
        let mut a2 = FakeBackend::session("a2", 10, Some("game.exe"));
        a2.output = Some("out2".into());
        let apps = group_sessions(&[a1, a2]);
        assert_eq!(apps[0].output.as_deref(), Some("out2"));
        assert_eq!(apps[0].pids, vec![10]);
    }
}
```

`src-tauri/src/app_state/mod.rs` (temporary contents):
```rust
mod group;

pub use group::{app_key, group_sessions, SYSTEM_KEY};
```

Add to `lib.rs`:
```rust
pub mod app_state;
pub mod backend;
#[cfg(test)]
mod fake;
```

- [ ] **Step 4: Run tests to verify they fail**

Run: `cargo test --lib app_state::`
Expected: FAIL to compile — `cannot find function app_key`.

- [ ] **Step 5: Implement grouping (top of `group.rs`)**

```rust
use std::collections::BTreeMap;

use crate::model::{App, RawSession};

pub const SYSTEM_KEY: &str = "#system";

fn file_name(path: &str) -> Option<&str> {
    path.rsplit(['\\', '/']).next().filter(|s| !s.is_empty())
}

/// Stable per-app key: lowercase exe file name, `#system`, or `pid:<n>` when the exe is unknown.
pub fn app_key(s: &RawSession) -> String {
    if s.is_system {
        return SYSTEM_KEY.into();
    }
    match s.exe_path.as_deref().and_then(file_name) {
        Some(f) => f.to_lowercase(),
        None => format!("pid:{}", s.pid),
    }
}

fn default_name(a: &App) -> String {
    if a.is_system {
        return "System sounds".into();
    }
    if let Some(f) = a.exe_path.as_deref().and_then(file_name) {
        let stem = f.rsplit_once('.').map(|(s, _)| s).unwrap_or(f);
        let mut chars = stem.chars();
        return match chars.next() {
            Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            None => f.to_string(),
        };
    }
    format!("Process {}", a.pids.first().copied().unwrap_or(0))
}

/// Groups sessions by app key. Sorted by name, system sounds last.
pub fn group_sessions(sessions: &[RawSession]) -> Vec<App> {
    let mut map: BTreeMap<String, App> = BTreeMap::new();
    for s in sessions {
        let key = app_key(s);
        let app = map.entry(key.clone()).or_insert_with(|| App {
            key,
            name: String::new(),
            exe_path: s.exe_path.clone(),
            is_system: s.is_system,
            pids: Vec::new(),
            session_ids: Vec::new(),
            volume: s.volume,
            muted: s.muted,
            output: None,
        });
        if app.name.is_empty() {
            if let Some(n) = s.display_name.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
                app.name = n.to_string();
            }
        }
        if s.pid != 0 && !app.pids.contains(&s.pid) {
            app.pids.push(s.pid);
        }
        app.session_ids.push(s.id.clone());
        if app.output.is_none() {
            app.output = s.output.clone();
        }
    }
    let mut apps: Vec<App> = map
        .into_values()
        .map(|mut a| {
            if a.name.is_empty() {
                a.name = default_name(&a);
            }
            a
        })
        .collect();
    apps.sort_by(|a, b| {
        a.is_system.cmp(&b.is_system).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    apps
}
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test --lib app_state::`
Expected: PASS (4 tests).

- [ ] **Step 7: Commit**

```powershell
git add -A
git commit -m "feat(core): AudioBackend trait, fake backend, session grouping"
```

---

### Task 5: AppCore — state building, per-app memory, commands, meters

**Files:**
- Modify: `src-tauri/src/app_state/mod.rs` (full implementation)
- Create: `src-tauri/src/app_state/tests.rs`

**Interfaces:**
- Consumes: Tasks 2–4.
- Produces `app_state::AppCore<B: AudioBackend>` with:
  `new(backend: B, settings: Settings, accent: String) -> Self`, `state(&self) -> &UiState`, `settings(&self) -> &Settings`,
  `take_settings_dirty(&mut self) -> bool`, `refresh(&mut self) -> Result<bool>` (true if state changed),
  `apply(&mut self, Command) -> Result<()>`, `mic_muted(&self) -> Option<bool>`, `meters(&mut self) -> MeterFrame`,
  `stop_metering(&mut self)`, `#[cfg(test)] backend_mut(&mut self) -> &mut B`.

- [ ] **Step 1: Write the failing tests in `src-tauri/src/app_state/tests.rs`**

```rust
use super::*;
use crate::{fake::FakeBackend, settings::DevicePrefs};

fn fake() -> FakeBackend {
    let mut out1 = FakeBackend::device("out1", Flow::Render);
    out1.default_console = true;
    out1.default_comms = true;
    let out2 = FakeBackend::device("out2", Flow::Render);
    let mut mic = FakeBackend::device("mic1", Flow::Capture);
    mic.default_console = true;
    mic.boost = Some(Boost { db: 0.0, min: 0.0, max: 30.0, step: 10.0 });
    FakeBackend {
        devices: vec![out1, out2, mic],
        sessions: vec![
            FakeBackend::session("s1", 10, Some("spotify.exe")),
            FakeBackend::session("c1", 20, Some("chrome.exe")),
            FakeBackend::session("c2", 21, Some("chrome.exe")),
        ],
        routing: true,
        ..Default::default()
    }
}

fn core_with(settings: Settings) -> AppCore<FakeBackend> {
    let mut c = AppCore::new(fake(), settings, "#112233".into());
    c.refresh().unwrap();
    c
}

fn app<'a>(c: &'a AppCore<FakeBackend>, key: &str) -> &'a App {
    c.state().apps.iter().find(|a| a.key == key).unwrap()
}

#[test]
fn refresh_builds_state_with_device_prefs_and_sorting() {
    let mut s = Settings::default();
    s.devices.insert("out2".into(), DevicePrefs { alias: Some("Desk".into()), hidden: true });
    let c = core_with(s);
    let st = c.state();
    assert_eq!(st.accent, "#112233");
    assert!(st.routing_available);
    let flows: Vec<_> = st.devices.iter().map(|d| d.flow).collect();
    assert_eq!(flows, vec![Flow::Render, Flow::Render, Flow::Capture]);
    let desk = st.devices.iter().find(|d| d.id == "out2").unwrap();
    assert_eq!(desk.name, "Desk");
    assert_eq!(desk.original_name, "Device out2");
    assert!(desk.hidden);
    assert_eq!(st.apps.len(), 2);
}

#[test]
fn refresh_reports_change_only_when_state_differs() {
    let mut c = core_with(Settings::default());
    assert!(!c.refresh().unwrap());
    c.backend_mut().devices[0].volume = 0.9;
    assert!(c.refresh().unwrap());
}

#[test]
fn saved_prefs_are_applied_to_new_sessions() {
    let mut s = Settings::default();
    s.apps.insert("spotify.exe".into(), AppPrefs { volume: 0.25, muted: true });
    let mut c = core_with(s);
    assert_eq!(app(&c, "spotify.exe").volume, 0.25);
    assert!(app(&c, "spotify.exe").muted);
    // a new chrome session appears after the user set chrome to 0.4
    c.apply(Command::SetAppVolume { key: "chrome.exe".into(), volume: 0.4 }).unwrap();
    c.refresh().unwrap();
    c.backend_mut().sessions.push(FakeBackend::session("c3", 22, Some("chrome.exe")));
    c.refresh().unwrap();
    let c3 = c.backend_mut().sessions.iter().find(|s| s.id == "c3").unwrap().volume;
    assert!((c3 - 0.4).abs() < 1e-6);
}

#[test]
fn existing_sessions_are_not_reapplied_every_refresh() {
    let mut c = core_with(Settings::default());
    c.backend_mut().calls.clear();
    // an external app changes spotify's volume; we must not fight it
    c.backend_mut().sessions[0].volume = 0.7;
    c.refresh().unwrap();
    assert!(c.backend_mut().calls.iter().all(|call| !call.starts_with("session_volume")));
    assert_eq!(c.settings().apps["spotify.exe"].volume, 0.7);
    assert!(c.take_settings_dirty());
    assert!(!c.take_settings_dirty());
}

#[test]
fn pid_keys_are_never_remembered() {
    let mut f = fake();
    f.sessions.push(FakeBackend::session("x", 99, None));
    let mut c = AppCore::new(f, Settings::default(), String::new());
    c.refresh().unwrap();
    assert!(c.settings().apps.keys().all(|k| !k.starts_with("pid:")));
}

#[test]
fn set_app_volume_hits_every_session_and_clamps_without_resnapshot() {
    let mut c = core_with(Settings::default());
    let before = c.backend_mut().snapshots;
    c.apply(Command::SetAppVolume { key: "chrome.exe".into(), volume: 1.7 }).unwrap();
    assert_eq!(c.backend_mut().snapshots, before, "volume changes patch state directly");
    assert!(c.backend_mut().calls.contains(&"session_volume c1 1.00".to_string()));
    assert!(c.backend_mut().calls.contains(&"session_volume c2 1.00".to_string()));
    assert_eq!(app(&c, "chrome.exe").volume, 1.0);
}

#[test]
fn invalid_volume_is_rejected() {
    let mut c = core_with(Settings::default());
    let err = c.apply(Command::SetAppVolume { key: "chrome.exe".into(), volume: f32::NAN }).unwrap_err();
    assert_eq!(err.code(), "invalid");
}

#[test]
fn unknown_app_is_not_found() {
    let mut c = core_with(Settings::default());
    let err = c.apply(Command::SetAppMute { key: "nope.exe".into(), muted: true }).unwrap_err();
    assert_eq!(err.code(), "notFound");
}

#[test]
fn set_app_output_routes_every_pid_and_refreshes() {
    let mut c = core_with(Settings::default());
    c.apply(Command::SetAppOutput { key: "chrome.exe".into(), device_id: Some("out2".into()) }).unwrap();
    assert!(c.backend_mut().calls.contains(&"output 20 Some(\"out2\")".to_string()));
    assert!(c.backend_mut().calls.contains(&"output 21 Some(\"out2\")".to_string()));
    assert_eq!(app(&c, "chrome.exe").output.as_deref(), Some("out2"));
}

#[test]
fn set_app_output_fails_cleanly_without_routing_or_with_unknown_device() {
    let mut c = core_with(Settings::default());
    let err = c.apply(Command::SetAppOutput { key: "chrome.exe".into(), device_id: Some("zzz".into()) }).unwrap_err();
    assert_eq!(err.code(), "notFound");
    c.backend_mut().routing = false;
    let err = c.apply(Command::SetAppOutput { key: "chrome.exe".into(), device_id: None }).unwrap_err();
    assert_eq!(err.code(), "unavailable");
}

#[test]
fn set_default_device_refreshes_flags() {
    let mut c = core_with(Settings::default());
    c.apply(Command::SetDefaultDevice { id: "out2".into(), role: Role::Console }).unwrap();
    let out2 = c.state().devices.iter().find(|d| d.id == "out2").unwrap();
    assert!(out2.default_console);
    assert!(!out2.default_comms);
}

#[test]
fn mic_mute_toggle_and_set() {
    let mut c = core_with(Settings::default());
    assert_eq!(c.mic_muted(), Some(false));
    c.apply(Command::ToggleMicMute).unwrap();
    assert_eq!(c.mic_muted(), Some(true));
    assert!(c.backend_mut().calls.contains(&"device_mute mic1 true".to_string()));
    c.apply(Command::SetMicMute { muted: false }).unwrap();
    assert_eq!(c.mic_muted(), Some(false));
}

#[test]
fn mic_commands_fail_without_a_default_mic() {
    let mut f = fake();
    f.devices.retain(|d| d.flow == Flow::Render);
    let mut c = AppCore::new(f, Settings::default(), String::new());
    c.refresh().unwrap();
    assert_eq!(c.mic_muted(), None);
    assert_eq!(c.apply(Command::ToggleMicMute).unwrap_err().code(), "notFound");
}

#[test]
fn mic_boost_is_clamped_to_range() {
    let mut c = core_with(Settings::default());
    c.apply(Command::SetMicBoost { id: "mic1".into(), db: 99.0 }).unwrap();
    assert!(c.backend_mut().calls.contains(&"boost mic1 30.0".to_string()));
    let mic = c.state().devices.iter().find(|d| d.id == "mic1").unwrap();
    assert_eq!(mic.boost.as_ref().unwrap().db, 30.0);
    let err = c.apply(Command::SetMicBoost { id: "out1".into(), db: 10.0 }).unwrap_err();
    assert_eq!(err.code(), "unavailable");
}

#[test]
fn alias_is_trimmed_and_blank_clears_it() {
    let mut c = core_with(Settings::default());
    c.take_settings_dirty();
    c.apply(Command::SetDeviceAlias { id: "out1".into(), alias: Some("  Headphones ".into()) }).unwrap();
    assert_eq!(c.state().devices.iter().find(|d| d.id == "out1").unwrap().name, "Headphones");
    assert!(c.take_settings_dirty());
    c.apply(Command::SetDeviceAlias { id: "out1".into(), alias: Some("   ".into()) }).unwrap();
    let d = c.state().devices.iter().find(|d| d.id == "out1").unwrap();
    assert_eq!(d.name, "Device out1");
    assert_eq!(d.alias, None);
}

#[test]
fn unplugged_devices_stay_in_state() {
    let mut c = core_with(Settings::default());
    c.backend_mut().devices[1].state = DeviceState::Unplugged;
    c.refresh().unwrap();
    let d = c.state().devices.iter().find(|d| d.id == "out2").unwrap();
    assert_eq!(d.state, DeviceState::Unplugged);
    assert_eq!(c.state().devices.last().unwrap().flow, Flow::Capture);
}

#[test]
fn config_commands_update_settings_and_state() {
    let mut c = core_with(Settings::default());
    c.take_settings_dirty();
    let cfg = MicConfig { hotkey: None, mode: HotkeyMode::PushToTalk };
    c.apply(Command::SetMicConfig { config: cfg.clone() }).unwrap();
    assert_eq!(c.state().mic, cfg);
    assert_eq!(c.settings().mic, cfg);
    let ui = UiSettings { theme: Theme::Dark, start_minimized: true };
    c.apply(Command::SetUiSettings { ui: ui.clone() }).unwrap();
    assert_eq!(c.state().ui, ui);
    c.apply(Command::ResetAppMemory).unwrap();
    assert!(c.settings().apps.is_empty());
    assert!(c.take_settings_dirty());
}

#[test]
fn meters_aggregate_max_peak_per_app() {
    let mut c = core_with(Settings::default());
    let f = c.backend_mut();
    f.peaks.sessions.insert("c1".into(), 0.2);
    f.peaks.sessions.insert("c2".into(), 0.6);
    f.peaks.master = 0.5;
    f.peaks.mic = 0.1;
    let m = c.meters();
    assert_eq!(m.apps["chrome.exe"], 0.6);
    assert_eq!(m.apps["spotify.exe"], 0.0);
    assert_eq!(m.master, 0.5);
    assert_eq!(m.mic, 0.1);
    assert!(c.backend_mut().calls.contains(&"peaks Some(\"mic1\")".to_string()));
}

#[test]
fn snapshot_errors_propagate_and_keep_old_state() {
    let mut c = core_with(Settings::default());
    let before = c.state().clone();
    c.backend_mut().fail_next_snapshot = true;
    assert!(c.refresh().is_err());
    assert_eq!(c.state(), &before);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib app_state::tests`
Expected: FAIL to compile — `cannot find type AppCore`.

- [ ] **Step 3: Implement `src-tauri/src/app_state/mod.rs`**

```rust
mod group;
#[cfg(test)]
mod tests;

pub use group::{app_key, group_sessions, SYSTEM_KEY};

use std::collections::{HashMap, HashSet};

use crate::{
    backend::AudioBackend,
    error::{AudioError, Result},
    model::*,
    settings::{AppPrefs, Settings},
};

/// Pure application logic on top of an `AudioBackend`. Owned by the audio worker thread.
pub struct AppCore<B: AudioBackend> {
    backend: B,
    settings: Settings,
    state: UiState,
    seen_sessions: HashSet<String>,
    settings_dirty: bool,
}

fn clamp01(v: f32) -> Result<f32> {
    if v.is_finite() {
        Ok(v.clamp(0.0, 1.0))
    } else {
        Err(AudioError::Invalid(format!("Invalid volume {v}")))
    }
}

impl<B: AudioBackend> AppCore<B> {
    pub fn new(backend: B, settings: Settings, accent: String) -> Self {
        let state = UiState {
            routing_available: backend.routing_available(),
            mic: settings.mic.clone(),
            ui: settings.ui.clone(),
            accent,
            ..Default::default()
        };
        Self { backend, settings, state, seen_sessions: HashSet::new(), settings_dirty: false }
    }

    pub fn state(&self) -> &UiState {
        &self.state
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    pub fn take_settings_dirty(&mut self) -> bool {
        std::mem::take(&mut self.settings_dirty)
    }

    #[cfg(test)]
    pub fn backend_mut(&mut self) -> &mut B {
        &mut self.backend
    }

    /// Re-reads the OS, restores saved prefs onto new sessions, remembers current values.
    pub fn refresh(&mut self) -> Result<bool> {
        let mut snap = self.backend.snapshot()?;
        if self.restore_new_sessions(&snap.sessions) {
            snap = self.backend.snapshot()?;
        }
        let apps = group_sessions(&snap.sessions);
        self.remember(&apps);
        let next = self.build_state(snap.devices, apps);
        let changed = next != self.state;
        self.state = next;
        Ok(changed)
    }

    fn restore_new_sessions(&mut self, sessions: &[RawSession]) -> bool {
        let mut applied = false;
        for s in sessions {
            if self.seen_sessions.contains(&s.id) {
                continue;
            }
            if let Some(p) = self.settings.apps.get(&app_key(s)).cloned() {
                if (s.volume - p.volume).abs() > 0.005 {
                    applied |= self.backend.set_session_volume(&s.id, p.volume).is_ok();
                }
                if s.muted != p.muted {
                    applied |= self.backend.set_session_mute(&s.id, p.muted).is_ok();
                }
            }
        }
        self.seen_sessions = sessions.iter().map(|s| s.id.clone()).collect();
        applied
    }

    fn remember(&mut self, apps: &[App]) {
        for a in apps.iter().filter(|a| !a.key.starts_with("pid:")) {
            let prefs = AppPrefs { volume: a.volume, muted: a.muted };
            if self.settings.apps.get(&a.key) != Some(&prefs) {
                self.settings.apps.insert(a.key.clone(), prefs);
                self.settings_dirty = true;
            }
        }
    }

    fn build_state(&self, raw: Vec<RawDevice>, apps: Vec<App>) -> UiState {
        let mut devices: Vec<Device> = raw
            .into_iter()
            .map(|d| {
                let prefs = self.settings.devices.get(&d.id).cloned().unwrap_or_default();
                Device {
                    name: prefs.alias.clone().unwrap_or_else(|| d.name.clone()),
                    original_name: d.name,
                    alias: prefs.alias,
                    hidden: prefs.hidden,
                    id: d.id,
                    flow: d.flow,
                    state: d.state,
                    default_console: d.default_console,
                    default_comms: d.default_comms,
                    volume: d.volume,
                    muted: d.muted,
                    boost: d.boost,
                }
            })
            .collect();
        devices.sort_by(|a, b| {
            (a.flow as u8)
                .cmp(&(b.flow as u8))
                .then((a.state as u8).cmp(&(b.state as u8)))
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        UiState {
            devices,
            apps,
            routing_available: self.backend.routing_available(),
            mic: self.settings.mic.clone(),
            ui: self.settings.ui.clone(),
            accent: self.state.accent.clone(),
        }
    }

    fn app(&self, key: &str) -> Result<&App> {
        self.state.apps.iter().find(|a| a.key == key).ok_or_else(|| AudioError::NotFound(format!("App {key} isn't playing audio")))
    }

    fn device(&self, id: &str) -> Result<&Device> {
        self.state.devices.iter().find(|d| d.id == id).ok_or_else(|| AudioError::NotFound("That device is no longer connected".into()))
    }

    fn default_mic(&self) -> Result<&Device> {
        self.state
            .devices
            .iter()
            .find(|d| d.flow == Flow::Capture && d.default_console && d.state == DeviceState::Active)
            .ok_or_else(|| AudioError::NotFound("No default microphone".into()))
    }

    pub fn mic_muted(&self) -> Option<bool> {
        self.default_mic().ok().map(|d| d.muted)
    }

    fn patch_app(&mut self, key: &str, f: impl Fn(&mut App)) {
        if let Some(a) = self.state.apps.iter_mut().find(|a| a.key == key) {
            f(a);
        }
    }

    fn patch_device(&mut self, id: &str, f: impl Fn(&mut Device)) {
        if let Some(d) = self.state.devices.iter_mut().find(|d| d.id == id) {
            f(d);
        }
    }

    fn reapply_device_prefs(&mut self) {
        for d in &mut self.state.devices {
            let p = self.settings.devices.get(&d.id).cloned().unwrap_or_default();
            d.name = p.alias.clone().unwrap_or_else(|| d.original_name.clone());
            d.alias = p.alias;
            d.hidden = p.hidden;
        }
    }

    pub fn apply(&mut self, cmd: Command) -> Result<()> {
        match cmd {
            Command::SetAppVolume { key, volume } => {
                let v = clamp01(volume)?;
                for sid in self.app(&key)?.session_ids.clone() {
                    self.backend.set_session_volume(&sid, v)?;
                }
                self.patch_app(&key, |a| a.volume = v);
            }
            Command::SetAppMute { key, muted } => {
                for sid in self.app(&key)?.session_ids.clone() {
                    self.backend.set_session_mute(&sid, muted)?;
                }
                self.patch_app(&key, |a| a.muted = muted);
            }
            Command::SetAppOutput { key, device_id } => {
                if !self.backend.routing_available() {
                    return Err(AudioError::Unavailable(
                        "Per-app output routing isn't available on this version of Windows".into(),
                    ));
                }
                if let Some(id) = &device_id {
                    self.device(id)?;
                }
                for pid in self.app(&key)?.pids.clone() {
                    self.backend.set_app_output(pid, device_id.as_deref())?;
                }
                self.refresh()?;
            }
            Command::SetDeviceVolume { id, volume } => {
                let v = clamp01(volume)?;
                self.device(&id)?;
                self.backend.set_device_volume(&id, v)?;
                self.patch_device(&id, |d| d.volume = v);
            }
            Command::SetDeviceMute { id, muted } => {
                self.device(&id)?;
                self.backend.set_device_mute(&id, muted)?;
                self.patch_device(&id, |d| d.muted = muted);
            }
            Command::SetDefaultDevice { id, role } => {
                self.device(&id)?;
                self.backend.set_default_device(&id, role)?;
                self.refresh()?;
            }
            Command::SetDeviceAlias { id, alias } => {
                let alias = alias.map(|a| a.trim().to_string()).filter(|a| !a.is_empty());
                self.settings.devices.entry(id).or_default().alias = alias;
                self.settings_dirty = true;
                self.reapply_device_prefs();
            }
            Command::SetDeviceHidden { id, hidden } => {
                self.settings.devices.entry(id).or_default().hidden = hidden;
                self.settings_dirty = true;
                self.reapply_device_prefs();
            }
            Command::SetMicBoost { id, db } => {
                let b = self
                    .device(&id)?
                    .boost
                    .clone()
                    .ok_or_else(|| AudioError::Unavailable("This microphone has no boost control".into()))?;
                let db = db.clamp(b.min, b.max);
                self.backend.set_mic_boost(&id, db)?;
                self.patch_device(&id, |d| {
                    if let Some(b) = d.boost.as_mut() {
                        b.db = db;
                    }
                });
            }
            Command::SetMicMute { muted } => {
                let id = self.default_mic()?.id.clone();
                self.backend.set_device_mute(&id, muted)?;
                self.patch_device(&id, |d| d.muted = muted);
            }
            Command::ToggleMicMute => {
                let (id, muted) = {
                    let d = self.default_mic()?;
                    (d.id.clone(), !d.muted)
                };
                self.backend.set_device_mute(&id, muted)?;
                self.patch_device(&id, |d| d.muted = muted);
            }
            Command::SetMicConfig { config } => {
                self.settings.mic = config.clone();
                self.state.mic = config;
                self.settings_dirty = true;
            }
            Command::SetUiSettings { ui } => {
                self.settings.ui = ui.clone();
                self.state.ui = ui;
                self.settings_dirty = true;
            }
            Command::ResetAppMemory => {
                self.settings.apps.clear();
                self.settings_dirty = true;
            }
        }
        Ok(())
    }

    pub fn meters(&mut self) -> MeterFrame {
        let mic_id = self.default_mic().ok().map(|d| d.id.clone());
        let peaks = self.backend.peaks(mic_id.as_deref());
        let apps: HashMap<String, f32> = self
            .state
            .apps
            .iter()
            .map(|a| {
                let p = a.session_ids.iter().filter_map(|s| peaks.sessions.get(s)).fold(0.0f32, |m, v| m.max(*v));
                (a.key.clone(), p)
            })
            .collect();
        MeterFrame { apps, master: peaks.master, mic: peaks.mic }
    }

    pub fn stop_metering(&mut self) {
        self.backend.stop_metering();
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib`
Expected: PASS (all model, settings, group and app_state tests — 30 total).

- [ ] **Step 5: Commit**

```powershell
git add -A
git commit -m "feat(core): AppCore state, per-app memory, commands and meters"
```

---

### Task 6: Audio worker thread

**Files:**
- Create: `src-tauri/src/worker.rs`
- Modify: `src-tauri/src/lib.rs` (`pub mod worker;`)

**Interfaces:**
- Consumes: `AppCore`, `AudioBackend`, `Settings`, `Command`, `UiState`, `MeterFrame`, `AudioError`.
- Produces:
  - `trait EventSink: Send + 'static { fn state(&self, &UiState); fn meters(&self, &MeterFrame); fn mic_muted(&self, bool); fn notice(&self, &AudioError); }`
  - `#[derive(Clone)] struct WorkerHandle` with `async fn command(&self, Command) -> Result<()>`, `async fn state(&self) -> Result<UiState>`, `fn send(&self, Command)` (fire-and-forget), `fn set_metering(&self, bool)`, `fn shutdown(&self)`, `#[cfg(test)] fn command_blocking(&self, Command) -> Result<()>`.
  - `fn spawn<F, B, S>(make_backend: F, settings_path: PathBuf, accent: String, sink: S) -> WorkerHandle where F: FnOnce() -> Result<B> + Send + 'static, B: AudioBackend + 'static, S: EventSink`.
  - Constants `RESYNC_INTERVAL = 250ms`, `METER_INTERVAL = 33ms`, `SAVE_DEBOUNCE = 500ms`.

- [ ] **Step 1: Write failing tests at the bottom of a new `src-tauri/src/worker.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{fake::FakeBackend, model::Flow};
    use std::sync::mpsc;

    struct TestSink(mpsc::Sender<String>);
    impl EventSink for TestSink {
        fn state(&self, s: &UiState) {
            let _ = self.0.send(format!("state apps={}", s.apps.len()));
        }
        fn meters(&self, m: &MeterFrame) {
            let _ = self.0.send(format!("meters master={:.1}", m.master));
        }
        fn mic_muted(&self, muted: bool) {
            let _ = self.0.send(format!("mic {muted}"));
        }
        fn notice(&self, e: &AudioError) {
            let _ = self.0.send(format!("notice {e}"));
        }
    }

    fn fake() -> FakeBackend {
        let mut mic = FakeBackend::device("mic1", Flow::Capture);
        mic.default_console = true;
        let mut f = FakeBackend {
            devices: vec![FakeBackend::device("out1", Flow::Render), mic],
            sessions: vec![FakeBackend::session("s1", 10, Some("spotify.exe"))],
            routing: true,
            ..Default::default()
        };
        f.peaks.master = 0.5;
        f
    }

    fn wait_for(rx: &mpsc::Receiver<String>, prefix: &str) -> String {
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            if let Ok(m) = rx.recv_timeout(Duration::from_millis(100)) {
                if m.starts_with(prefix) {
                    return m;
                }
            }
        }
        panic!("timed out waiting for {prefix}");
    }

    #[test]
    fn emits_initial_state_and_mic_status() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, rx) = mpsc::channel();
        let h = spawn(|| Ok(fake()), dir.path().join("s.json"), String::new(), TestSink(tx));
        assert_eq!(wait_for(&rx, "state"), "state apps=1");
        assert_eq!(wait_for(&rx, "mic"), "mic false");
        h.shutdown();
    }

    #[test]
    fn commands_round_trip_and_settings_are_saved_debounced() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s.json");
        let (tx, rx) = mpsc::channel();
        let h = spawn(|| Ok(fake()), path.clone(), String::new(), TestSink(tx));
        wait_for(&rx, "state");
        h.command_blocking(Command::SetAppVolume { key: "spotify.exe".into(), volume: 0.25 }).unwrap();
        let err = h.command_blocking(Command::SetAppMute { key: "nope".into(), muted: true }).unwrap_err();
        assert_eq!(err.code(), "notFound");
        h.command_blocking(Command::ToggleMicMute).unwrap();
        assert_eq!(wait_for(&rx, "mic"), "mic false");
        assert_eq!(wait_for(&rx, "mic"), "mic true");
        std::thread::sleep(Duration::from_millis(1500));
        assert_eq!(Settings::load(&path).apps["spotify.exe"].volume, 0.25);
        h.shutdown();
    }

    #[test]
    fn meters_flow_only_while_metering_is_on() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, rx) = mpsc::channel();
        let h = spawn(|| Ok(fake()), dir.path().join("s.json"), String::new(), TestSink(tx));
        wait_for(&rx, "state");
        std::thread::sleep(Duration::from_millis(200));
        assert!(rx.try_iter().all(|m| !m.starts_with("meters")));
        h.set_metering(true);
        assert_eq!(wait_for(&rx, "meters"), "meters master=0.5");
        h.set_metering(false);
        std::thread::sleep(Duration::from_millis(100));
        let _ = rx.try_iter().count();
        std::thread::sleep(Duration::from_millis(200));
        assert!(rx.try_iter().all(|m| !m.starts_with("meters")));
        h.shutdown();
    }

    #[test]
    fn backend_creation_failure_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, rx) = mpsc::channel();
        let h = spawn::<_, FakeBackend, _>(
            || Err(AudioError::Os("no audio".into())),
            dir.path().join("s.json"),
            String::new(),
            TestSink(tx),
        );
        assert_eq!(wait_for(&rx, "notice"), "notice no audio");
        assert_eq!(h.command_blocking(Command::ToggleMicMute).unwrap_err().code(), "unavailable");
    }
}
```

Add `pub mod worker;` to `lib.rs`.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib worker::`
Expected: FAIL to compile — `cannot find function spawn`.

- [ ] **Step 3: Implement the worker (top of `worker.rs`)**

```rust
use std::{
    path::PathBuf,
    sync::mpsc::{self, RecvTimeoutError},
    thread,
    time::{Duration, Instant},
};

use tokio::sync::oneshot;

use crate::{
    app_state::AppCore,
    backend::AudioBackend,
    error::{AudioError, Result},
    model::{Command, MeterFrame, UiState},
    settings::Settings,
};

pub const RESYNC_INTERVAL: Duration = Duration::from_millis(250);
pub const METER_INTERVAL: Duration = Duration::from_millis(33);
pub const SAVE_DEBOUNCE: Duration = Duration::from_millis(500);

/// Where the worker publishes changes (the Tauri app in production, a channel in tests).
pub trait EventSink: Send + 'static {
    fn state(&self, state: &UiState);
    fn meters(&self, frame: &MeterFrame);
    fn mic_muted(&self, muted: bool);
    fn notice(&self, error: &AudioError);
}

enum Msg {
    Command(Command, Option<oneshot::Sender<Result<()>>>),
    GetState(oneshot::Sender<UiState>),
    SetMetering(bool),
    Shutdown,
}

#[derive(Clone)]
pub struct WorkerHandle {
    tx: mpsc::Sender<Msg>,
}

fn stopped() -> AudioError {
    AudioError::Unavailable("The audio engine isn't running".into())
}

impl WorkerHandle {
    pub async fn command(&self, cmd: Command) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.tx.send(Msg::Command(cmd, Some(tx))).map_err(|_| stopped())?;
        rx.await.map_err(|_| stopped())?
    }

    #[cfg(test)]
    pub fn command_blocking(&self, cmd: Command) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.tx.send(Msg::Command(cmd, Some(tx))).map_err(|_| stopped())?;
        rx.blocking_recv().map_err(|_| stopped())?
    }

    pub async fn state(&self) -> Result<UiState> {
        let (tx, rx) = oneshot::channel();
        self.tx.send(Msg::GetState(tx)).map_err(|_| stopped())?;
        rx.await.map_err(|_| stopped())
    }

    /// Fire-and-forget (used by hotkeys and the tray menu).
    pub fn send(&self, cmd: Command) {
        let _ = self.tx.send(Msg::Command(cmd, None));
    }

    pub fn set_metering(&self, active: bool) {
        let _ = self.tx.send(Msg::SetMetering(active));
    }

    pub fn shutdown(&self) {
        let _ = self.tx.send(Msg::Shutdown);
    }
}

pub fn spawn<F, B, S>(make_backend: F, settings_path: PathBuf, accent: String, sink: S) -> WorkerHandle
where
    F: FnOnce() -> Result<B> + Send + 'static,
    B: AudioBackend + 'static,
    S: EventSink,
{
    let (tx, rx) = mpsc::channel::<Msg>();
    thread::Builder::new()
        .name("audio-worker".into())
        .spawn(move || {
            let backend = match make_backend() {
                Ok(b) => b,
                Err(e) => {
                    tracing::error!("audio backend failed to start: {e}");
                    sink.notice(&e);
                    return;
                }
            };
            let mut core = AppCore::new(backend, Settings::load(&settings_path), accent);
            run_loop(&mut core, &rx, &settings_path, &sink);
        })
        .expect("failed to spawn audio worker thread");
    WorkerHandle { tx }
}

fn save(core: &AppCore<impl AudioBackend>, path: &PathBuf) {
    if let Err(e) = core.settings().save(path) {
        tracing::error!("couldn't save settings: {e}");
    }
}

fn run_loop<B: AudioBackend>(core: &mut AppCore<B>, rx: &mpsc::Receiver<Msg>, path: &PathBuf, sink: &impl EventSink) {
    if let Err(e) = core.refresh() {
        tracing::warn!("initial refresh failed: {e}");
    }
    sink.state(core.state());
    let mut last_mic = core.mic_muted();
    if let Some(m) = last_mic {
        sink.mic_muted(m);
    }

    let mut metering = false;
    let mut next_resync = Instant::now() + RESYNC_INTERVAL;
    let mut next_meter = Instant::now();
    let mut dirty_since: Option<Instant> = if core.take_settings_dirty() { Some(Instant::now()) } else { None };

    loop {
        let mut deadline = next_resync;
        if metering {
            deadline = deadline.min(next_meter);
        }
        if let Some(d) = dirty_since {
            deadline = deadline.min(d + SAVE_DEBOUNCE);
        }
        let mut changed = false;
        match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(Msg::Command(cmd, reply)) => {
                let result = core.apply(cmd);
                if let Err(e) = &result {
                    tracing::info!("command failed: {e}");
                }
                if let Some(reply) = reply {
                    let _ = reply.send(result);
                }
                changed = true;
            }
            Ok(Msg::GetState(reply)) => {
                let _ = reply.send(core.state().clone());
            }
            Ok(Msg::SetMetering(on)) => {
                if metering && !on {
                    core.stop_metering();
                }
                metering = on;
                next_meter = Instant::now();
            }
            Ok(Msg::Shutdown) | Err(RecvTimeoutError::Disconnected) => {
                if dirty_since.is_some() || core.take_settings_dirty() {
                    save(core, path);
                }
                core.stop_metering();
                return;
            }
            Err(RecvTimeoutError::Timeout) => {}
        }

        let now = Instant::now();
        if now >= next_resync {
            match core.refresh() {
                Ok(c) => changed |= c,
                Err(e) => tracing::warn!("refresh failed: {e}"),
            }
            next_resync = now + RESYNC_INTERVAL;
        }
        if metering && now >= next_meter {
            sink.meters(&core.meters());
            next_meter = now + METER_INTERVAL;
        }
        if changed {
            sink.state(core.state());
            let mic = core.mic_muted();
            if mic != last_mic {
                if let Some(m) = mic {
                    sink.mic_muted(m);
                }
                last_mic = mic;
            }
        }
        if core.take_settings_dirty() {
            dirty_since = Some(now);
        }
        if let Some(d) = dirty_since {
            if now >= d + SAVE_DEBOUNCE {
                save(core, path);
                dirty_since = None;
            }
        }
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib worker::`
Expected: PASS (4 tests).

- [ ] **Step 5: Commit**

```powershell
git add -A
git commit -m "feat(core): audio worker thread with polling, metering and debounced saves"
```


---

### Task 7: WASAPI backend — devices, sessions, volume, defaults, process info

**Files:**
- Create: `src-tauri/src/win/mod.rs`, `win/util.rs`, `win/com.rs`, `win/endpoints.rs`, `win/procinfo.rs`, `win/policy.rs`, `win/backend.rs`
- Create (stubs filled in Task 8): `win/routing.rs`, `win/boost.rs`, `win/meter.rs`
- Create: `src-tauri/tests/wasapi.rs`
- Modify: `src-tauri/src/lib.rs` (`#[cfg(windows)] pub mod win;`)

**Interfaces:**
- Consumes: `AudioBackend`, model types.
- Produces: `win::WasapiBackend::new() -> Result<WasapiBackend>` (initializes COM MTA on the calling thread; implements `AudioBackend`). Task 8 adds `win::routing::Routing`, real `win::boost::{find, read, write}`, real `win::meter::MicMeter`, `win::exe_icon_png_base64`, `win::accent_hex`.

- [ ] **Step 1: Write the integration tests `src-tauri/tests/wasapi.rs`** (real hardware, `#[ignore]`)

```rust
#![cfg(windows)]
use resonance_lib::{backend::AudioBackend, model::*, win::WasapiBackend};

fn backend() -> WasapiBackend {
    WasapiBackend::new().expect("WASAPI backend")
}

#[test]
#[ignore]
fn enumerates_devices_and_sessions() {
    let mut b = backend();
    let s = b.snapshot().unwrap();
    assert!(s.devices.iter().any(|d| d.flow == Flow::Render), "no output devices");
    assert!(s.devices.iter().filter(|d| d.flow == Flow::Render && d.default_console).count() <= 1);
    for d in &s.devices {
        assert!(!d.id.is_empty() && !d.name.is_empty());
        assert!((0.0..=1.0).contains(&d.volume));
    }
    println!("{:#?}", s);
}

#[test]
#[ignore]
fn set_and_restore_default_output_volume() {
    let mut b = backend();
    let s = b.snapshot().unwrap();
    let d = s.devices.iter().find(|d| d.flow == Flow::Render && d.default_console).expect("default output").clone();
    let target = if d.volume > 0.5 { d.volume - 0.05 } else { d.volume + 0.05 };
    b.set_device_volume(&d.id, target).unwrap();
    let after = b.snapshot().unwrap().devices.into_iter().find(|x| x.id == d.id).unwrap().volume;
    b.set_device_volume(&d.id, d.volume).unwrap();
    assert!((after - target).abs() < 0.01, "expected {target}, got {after}");
}

#[test]
#[ignore]
fn toggle_and_restore_mic_mute() {
    let mut b = backend();
    let s = b.snapshot().unwrap();
    let Some(mic) = s
        .devices
        .iter()
        .find(|d| d.flow == Flow::Capture && d.default_console && d.state == DeviceState::Active)
        .cloned()
    else {
        eprintln!("no microphone; skipping");
        return;
    };
    b.set_device_mute(&mic.id, !mic.muted).unwrap();
    let after = b.snapshot().unwrap().devices.into_iter().find(|x| x.id == mic.id).unwrap().muted;
    b.set_device_mute(&mic.id, mic.muted).unwrap();
    assert_eq!(after, !mic.muted);
}

#[test]
#[ignore]
fn reads_peaks_without_panicking() {
    let mut b = backend();
    let s = b.snapshot().unwrap();
    let mic = s.devices.iter().find(|d| d.flow == Flow::Capture && d.default_console).map(|d| d.id.clone());
    let p = b.peaks(mic.as_deref());
    assert!((0.0..=1.0).contains(&p.master));
    b.stop_metering();
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test --test wasapi -- --ignored`
Expected: FAIL to compile — `could not find win in resonance_lib`.

- [ ] **Step 3: Write `win/util.rs`, `win/com.rs`, `win/endpoints.rs`**

`src-tauri/src/win/util.rs`:
```rust
use windows::core::PWSTR;
use windows::Win32::System::Com::CoTaskMemFree;

pub fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Copies a CoTaskMem-allocated wide string and frees it.
pub unsafe fn take_pwstr(p: PWSTR) -> String {
    if p.is_null() {
        return String::new();
    }
    let s = p.to_string().unwrap_or_default();
    CoTaskMemFree(Some(p.0 as *const _));
    s
}
```

`src-tauri/src/win/com.rs`:
```rust
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};

use crate::error::Result;

/// Initializes COM (MTA) for the current thread for as long as it lives.
pub struct ComGuard;

impl ComGuard {
    pub fn init_mta() -> Result<Self> {
        unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).ok()? };
        Ok(ComGuard)
    }
}

impl Drop for ComGuard {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}
```

`src-tauri/src/win/endpoints.rs`:
```rust
use windows::Win32::{
    Devices::FunctionDiscovery::PKEY_Device_FriendlyName,
    Media::Audio::{Endpoints::IAudioEndpointVolume, EDataFlow, ERole, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator},
    System::Com::{CoCreateInstance, StructuredStorage::PropVariantToStringAlloc, CLSCTX_ALL, STGM_READ},
};

use super::util::take_pwstr;
use crate::error::Result;

pub fn create_enumerator() -> Result<IMMDeviceEnumerator> {
    unsafe { Ok(CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?) }
}

pub fn device_id(d: &IMMDevice) -> Result<String> {
    unsafe { Ok(take_pwstr(d.GetId()?)) }
}

pub fn friendly_name(d: &IMMDevice) -> Result<String> {
    unsafe {
        let store = d.OpenPropertyStore(STGM_READ)?;
        let value = store.GetValue(&PKEY_Device_FriendlyName)?;
        Ok(take_pwstr(PropVariantToStringAlloc(&value)?))
    }
}

pub fn default_id(e: &IMMDeviceEnumerator, flow: EDataFlow, role: ERole) -> Option<String> {
    unsafe { e.GetDefaultAudioEndpoint(flow, role).ok().and_then(|d| device_id(&d).ok()) }
}

pub fn endpoint_volume(d: &IMMDevice) -> Result<IAudioEndpointVolume> {
    unsafe { Ok(d.Activate(CLSCTX_ALL, None)?) }
}
```

- [ ] **Step 4: Write `win/procinfo.rs`**

```rust
use std::ffi::c_void;

use windows::core::{w, PCWSTR, PWSTR};
use windows::Win32::{
    Foundation::{CloseHandle, ERROR_INVALID_PARAMETER},
    Storage::FileSystem::{GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW},
    System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION},
};

use super::util::to_wide;

#[derive(Debug, Clone)]
pub struct ProcInfo {
    pub exe_path: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone)]
pub enum ProcLookup {
    /// The process has exited (stale session).
    Dead,
    /// Alive but unreadable (e.g. protected process).
    Unknown,
    Found(ProcInfo),
}

pub fn lookup(pid: u32) -> ProcLookup {
    unsafe {
        let handle = match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
            Ok(h) => h,
            Err(e) if e.code() == ERROR_INVALID_PARAMETER.to_hresult() => return ProcLookup::Dead,
            Err(_) => return ProcLookup::Unknown,
        };
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len);
        let _ = CloseHandle(handle);
        if ok.is_err() {
            return ProcLookup::Unknown;
        }
        let exe_path = String::from_utf16_lossy(&buf[..len as usize]);
        let description = file_description(&exe_path);
        ProcLookup::Found(ProcInfo { exe_path, description })
    }
}

/// Reads the FileDescription version resource (e.g. "Google Chrome").
pub fn file_description(path: &str) -> Option<String> {
    unsafe {
        let wide = to_wide(path);
        let size = GetFileVersionInfoSizeW(PCWSTR(wide.as_ptr()), None);
        if size == 0 {
            return None;
        }
        let mut data = vec![0u8; size as usize];
        GetFileVersionInfoW(PCWSTR(wide.as_ptr()), None, size, data.as_mut_ptr() as *mut c_void).ok()?;
        let mut ptr: *mut c_void = std::ptr::null_mut();
        let mut len = 0u32;
        if !VerQueryValueW(data.as_ptr() as *const c_void, w!("\\VarFileInfo\\Translation"), &mut ptr, &mut len).as_bool()
            || len < 4
        {
            return None;
        }
        let lang = *(ptr as *const u16);
        let codepage = *(ptr as *const u16).add(1);
        let key = to_wide(&format!("\\StringFileInfo\\{lang:04x}{codepage:04x}\\FileDescription"));
        if !VerQueryValueW(data.as_ptr() as *const c_void, PCWSTR(key.as_ptr()), &mut ptr, &mut len).as_bool() || len == 0 {
            return None;
        }
        let text = std::slice::from_raw_parts(ptr as *const u16, len as usize);
        let s = String::from_utf16_lossy(text).trim_end_matches('\0').trim().to_string();
        (!s.is_empty()).then_some(s)
    }
}
```

- [ ] **Step 5: Write `win/policy.rs` (default device via undocumented IPolicyConfig)**

```rust
#![allow(non_snake_case)]
use windows::core::{IUnknown, IUnknown_Vtbl, GUID, HRESULT, PCWSTR};
use windows::Win32::{
    Media::Audio::ERole,
    System::Com::{CoCreateInstance, CLSCTX_ALL},
};

use super::util::to_wide;
use crate::error::Result;

const CLSID_POLICY_CONFIG_CLIENT: GUID = GUID::from_u128(0x870af99c_171d_4f9e_af0d_e63df40c2bc9);

/// Undocumented, stable since Windows 7. Only SetDefaultEndpoint is called; the others are
/// placeholders that keep the vtable layout correct.
#[windows_core::interface("f8679f50-850a-41cf-9c72-430f290290c8")]
unsafe trait IPolicyConfig: IUnknown {
    fn GetMixFormat(&self) -> HRESULT;
    fn GetDeviceFormat(&self) -> HRESULT;
    fn ResetDeviceFormat(&self) -> HRESULT;
    fn SetDeviceFormat(&self) -> HRESULT;
    fn GetProcessingPeriod(&self) -> HRESULT;
    fn SetProcessingPeriod(&self) -> HRESULT;
    fn GetShareMode(&self) -> HRESULT;
    fn SetShareMode(&self) -> HRESULT;
    fn GetPropertyValue(&self) -> HRESULT;
    fn SetPropertyValue(&self) -> HRESULT;
    fn SetDefaultEndpoint(&self, device_id: PCWSTR, role: ERole) -> HRESULT;
    fn SetEndpointVisibility(&self) -> HRESULT;
}

pub fn set_default_endpoint(device_id: &str, role: ERole) -> Result<()> {
    unsafe {
        let pc: IPolicyConfig = CoCreateInstance(&CLSID_POLICY_CONFIG_CLIENT, None, CLSCTX_ALL)?;
        let wide = to_wide(device_id);
        pc.SetDefaultEndpoint(PCWSTR(wide.as_ptr()), role).ok()?;
    }
    Ok(())
}
```

- [ ] **Step 6: Write stubs for the Task 8 modules so the backend compiles**

`src-tauri/src/win/routing.rs`:
```rust
use crate::error::{AudioError, Result};

pub struct Routing;

impl Routing {
    pub fn new() -> Result<Self> {
        Err(AudioError::Unavailable("not implemented yet".into()))
    }
    pub fn get(&self, _pid: u32) -> Result<Option<String>> {
        Ok(None)
    }
    pub fn set(&self, _pid: u32, _device_id: Option<&str>) -> Result<()> {
        Ok(())
    }
}
```

`src-tauri/src/win/boost.rs`:
```rust
use windows::Win32::Media::Audio::{IAudioVolumeLevel, IMMDevice};

use crate::{error::Result, model::Boost};

pub fn find(_dev: &IMMDevice) -> Result<Option<IAudioVolumeLevel>> {
    Ok(None)
}
pub fn read(_level: &IAudioVolumeLevel) -> Result<Boost> {
    Ok(Boost { db: 0.0, min: 0.0, max: 0.0, step: 0.0 })
}
pub fn write(_level: &IAudioVolumeLevel, _db: f32) -> Result<()> {
    Ok(())
}
```

`src-tauri/src/win/meter.rs`:
```rust
use windows::Win32::Media::Audio::IMMDevice;

use crate::error::{AudioError, Result};

pub struct MicMeter {
    pub device_id: String,
}

impl MicMeter {
    pub fn open(_dev: &IMMDevice, _id: &str) -> Result<Self> {
        Err(AudioError::Unavailable("not implemented yet".into()))
    }
    pub fn peak(&self) -> f32 {
        0.0
    }
}
```

- [ ] **Step 7: Write `win/mod.rs` and `win/backend.rs`**

`src-tauri/src/win/mod.rs`:
```rust
mod backend;
pub mod boost;
mod com;
mod endpoints;
pub mod meter;
mod policy;
mod procinfo;
pub mod routing;
mod util;

pub use backend::WasapiBackend;
```

`src-tauri/src/win/backend.rs`:
```rust
use std::collections::{HashMap, HashSet};

use windows::core::{Interface, S_OK};
use windows::Win32::Media::Audio::{
    eCapture, eCommunications, eConsole, eMultimedia, eRender, AudioSessionStateExpired,
    Endpoints::IAudioMeterInformation, IAudioSessionControl2, IAudioSessionManager2, IAudioVolumeLevel, IMMDevice,
    IMMDeviceEnumerator, ISimpleAudioVolume, DEVICE_STATE_ACTIVE, DEVICE_STATE_UNPLUGGED,
};
use windows::Win32::System::Com::CLSCTX_ALL;

use super::{
    boost,
    com::ComGuard,
    endpoints::{create_enumerator, default_id, device_id, endpoint_volume, friendly_name},
    meter::MicMeter,
    policy,
    procinfo::{self, ProcLookup},
    routing::Routing,
    util::take_pwstr,
};
use crate::{
    backend::AudioBackend,
    error::{AudioError, Result},
    model::*,
};

struct SessionHandle {
    volume: ISimpleAudioVolume,
    meter: IAudioMeterInformation,
}

/// WASAPI implementation of `AudioBackend`. Must be created and used on one thread.
pub struct WasapiBackend {
    enumerator: IMMDeviceEnumerator,
    endpoints: HashMap<String, IMMDevice>,
    sessions: HashMap<String, SessionHandle>,
    boosts: HashMap<String, Option<IAudioVolumeLevel>>,
    procs: HashMap<u32, ProcLookup>,
    routing: Option<Routing>,
    master_meter: Option<IAudioMeterInformation>,
    mic_meter: Option<MicMeter>,
    mic_meter_failed: Option<String>,
    // Declared last: COM is uninitialized only after every COM object above is released.
    _com: ComGuard,
}

impl WasapiBackend {
    pub fn new() -> Result<Self> {
        let com = ComGuard::init_mta()?;
        let routing = match Routing::new() {
            Ok(r) => Some(r),
            Err(e) => {
                tracing::warn!("per-app routing unavailable: {e}");
                None
            }
        };
        Ok(Self {
            enumerator: create_enumerator()?,
            endpoints: HashMap::new(),
            sessions: HashMap::new(),
            boosts: HashMap::new(),
            procs: HashMap::new(),
            routing,
            master_meter: None,
            mic_meter: None,
            mic_meter_failed: None,
            _com: com,
        })
    }

    fn endpoint(&self, id: &str) -> Result<&IMMDevice> {
        self.endpoints.get(id).ok_or_else(|| AudioError::NotFound("That device is no longer connected".into()))
    }

    fn proc(&mut self, pid: u32) -> ProcLookup {
        if let Some(p) = self.procs.get(&pid) {
            return p.clone();
        }
        let p = procinfo::lookup(pid);
        if !matches!(p, ProcLookup::Dead) {
            self.procs.insert(pid, p.clone());
        }
        p
    }

    fn boost_level(&mut self, id: &str) -> Result<Option<IAudioVolumeLevel>> {
        if let Some(cached) = self.boosts.get(id) {
            return Ok(cached.clone());
        }
        let dev = self.endpoint(id)?.clone();
        let level = boost::find(&dev).unwrap_or_else(|e| {
            tracing::debug!("boost lookup failed for {id}: {e}");
            None
        });
        self.boosts.insert(id.to_string(), level.clone());
        Ok(level)
    }

    fn canonical_device(&self, id: &str) -> Option<String> {
        self.endpoints.keys().find(|k| k.eq_ignore_ascii_case(id)).cloned()
    }

    fn snapshot_inner(&mut self) -> Result<Snapshot> {
        let e = &self.enumerator;
        let defaults = [
            default_id(e, eRender, eConsole),
            default_id(e, eRender, eCommunications),
            default_id(e, eCapture, eConsole),
            default_id(e, eCapture, eCommunications),
        ];

        // Pass 1: enumerate endpoints.
        let mut found: Vec<(String, IMMDevice, Flow, DeviceState)> = Vec::new();
        for (flow, edf) in [(Flow::Render, eRender), (Flow::Capture, eCapture)] {
            unsafe {
                let coll = self.enumerator.EnumAudioEndpoints(edf, DEVICE_STATE_ACTIVE | DEVICE_STATE_UNPLUGGED)?;
                for i in 0..coll.GetCount()? {
                    let dev = coll.Item(i)?;
                    let id = device_id(&dev)?;
                    let state =
                        if dev.GetState()? == DEVICE_STATE_ACTIVE { DeviceState::Active } else { DeviceState::Unplugged };
                    found.push((id, dev, flow, state));
                }
            }
        }
        self.endpoints = found.iter().map(|(id, dev, _, _)| (id.clone(), dev.clone())).collect();
        let active_capture: HashSet<String> = found
            .iter()
            .filter(|(_, _, f, s)| *f == Flow::Capture && *s == DeviceState::Active)
            .map(|(id, ..)| id.clone())
            .collect();
        self.boosts.retain(|id, _| active_capture.contains(id));

        // Pass 2: device properties.
        let mut devices = Vec::with_capacity(found.len());
        for (id, dev, flow, state) in &found {
            let (volume, muted) = if *state == DeviceState::Active {
                endpoint_volume(dev)
                    .and_then(|v| unsafe { Ok((v.GetMasterVolumeLevelScalar()?, v.GetMute()?.as_bool())) })
                    .unwrap_or((0.0, false))
            } else {
                (0.0, false)
            };
            let boost = if active_capture.contains(id) {
                self.boost_level(id).ok().flatten().and_then(|l| boost::read(&l).ok())
            } else {
                None
            };
            let (dc, dm) = match flow {
                Flow::Render => (&defaults[0], &defaults[1]),
                Flow::Capture => (&defaults[2], &defaults[3]),
            };
            devices.push(RawDevice {
                id: id.clone(),
                name: friendly_name(dev).unwrap_or_else(|_| "Unknown device".into()),
                flow: *flow,
                state: *state,
                default_console: dc.as_deref() == Some(id.as_str()),
                default_comms: dm.as_deref() == Some(id.as_str()),
                volume,
                muted,
                boost,
            });
        }

        // Pass 3: sessions on every active output.
        self.sessions.clear();
        let mut sessions = Vec::new();
        let mut live_pids = HashSet::new();
        let outputs: Vec<(String, IMMDevice)> = found
            .iter()
            .filter(|(_, _, f, s)| *f == Flow::Render && *s == DeviceState::Active)
            .map(|(id, dev, ..)| (id.clone(), dev.clone()))
            .collect();
        for (dev_id, dev) in outputs {
            let Ok(manager) = (unsafe { dev.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None) }) else { continue };
            unsafe {
                let list = manager.GetSessionEnumerator()?;
                for i in 0..list.GetCount()? {
                    let Ok(ctl) = list.GetSession(i) else { continue };
                    if ctl.GetState().map(|s| s == AudioSessionStateExpired).unwrap_or(true) {
                        continue;
                    }
                    let Ok(ctl2) = ctl.cast::<IAudioSessionControl2>() else { continue };
                    let is_system = ctl2.IsSystemSoundsSession() == S_OK;
                    let pid = ctl2.GetProcessId().unwrap_or(0);
                    let (exe_path, description) = if is_system || pid == 0 {
                        (None, None)
                    } else {
                        match self.proc(pid) {
                            ProcLookup::Dead => continue,
                            ProcLookup::Unknown => (None, None),
                            ProcLookup::Found(p) => (Some(p.exe_path), p.description),
                        }
                    };
                    live_pids.insert(pid);
                    let Ok(id) = ctl2.GetSessionInstanceIdentifier().map(|p| take_pwstr(p)) else { continue };
                    let (Ok(volume), Ok(meter)) = (ctl.cast::<ISimpleAudioVolume>(), ctl.cast::<IAudioMeterInformation>())
                    else {
                        continue;
                    };
                    let level = volume.GetMasterVolume().unwrap_or(1.0);
                    let muted = volume.GetMute().map(|m| m.as_bool()).unwrap_or(false);
                    let session_name = ctl
                        .GetDisplayName()
                        .ok()
                        .map(|p| take_pwstr(p))
                        .filter(|s| !s.is_empty() && !s.starts_with('@'));
                    let output = if is_system {
                        None
                    } else {
                        self.routing
                            .as_ref()
                            .and_then(|r| r.get(pid).ok().flatten())
                            .map(|o| self.canonical_device(&o).unwrap_or(o))
                    };
                    sessions.push(RawSession {
                        id: id.clone(),
                        pid,
                        exe_path,
                        display_name: session_name.or(description),
                        is_system,
                        device_id: dev_id.clone(),
                        volume: level,
                        muted,
                        output,
                    });
                    self.sessions.insert(id, SessionHandle { volume, meter });
                }
            }
        }
        self.procs.retain(|pid, _| live_pids.contains(pid));

        self.master_meter = defaults[0]
            .as_ref()
            .and_then(|id| self.endpoints.get(id))
            .and_then(|d| unsafe { d.Activate::<IAudioMeterInformation>(CLSCTX_ALL, None) }.ok());

        Ok(Snapshot { devices, sessions })
    }
}

impl AudioBackend for WasapiBackend {
    fn snapshot(&mut self) -> Result<Snapshot> {
        match self.snapshot_inner() {
            Ok(s) => Ok(s),
            Err(e) => {
                // Audio-service restart / sleep-resume invalidates the enumerator: rebuild once.
                tracing::warn!("snapshot failed ({e}); recreating device enumerator");
                self.enumerator = create_enumerator()?;
                self.boosts.clear();
                self.mic_meter = None;
                self.snapshot_inner()
            }
        }
    }

    fn set_session_volume(&mut self, id: &str, volume: f32) -> Result<()> {
        let h = self.sessions.get(id).ok_or_else(|| AudioError::NotFound("That audio session has ended".into()))?;
        unsafe { h.volume.SetMasterVolume(volume, std::ptr::null())? };
        Ok(())
    }

    fn set_session_mute(&mut self, id: &str, muted: bool) -> Result<()> {
        let h = self.sessions.get(id).ok_or_else(|| AudioError::NotFound("That audio session has ended".into()))?;
        unsafe { h.volume.SetMute(muted, std::ptr::null())? };
        Ok(())
    }

    fn set_device_volume(&mut self, id: &str, volume: f32) -> Result<()> {
        let v = endpoint_volume(self.endpoint(id)?)?;
        unsafe { v.SetMasterVolumeLevelScalar(volume, std::ptr::null())? };
        Ok(())
    }

    fn set_device_mute(&mut self, id: &str, muted: bool) -> Result<()> {
        let v = endpoint_volume(self.endpoint(id)?)?;
        unsafe { v.SetMute(muted, std::ptr::null())? };
        Ok(())
    }

    fn set_default_device(&mut self, id: &str, role: Role) -> Result<()> {
        self.endpoint(id)?;
        let roles = match role {
            Role::Console => vec![eConsole, eMultimedia],
            Role::Communications => vec![eCommunications],
        };
        for r in roles {
            policy::set_default_endpoint(id, r)?;
        }
        Ok(())
    }

    fn routing_available(&self) -> bool {
        self.routing.is_some()
    }

    fn set_app_output(&mut self, pid: u32, device_id: Option<&str>) -> Result<()> {
        self.routing
            .as_ref()
            .ok_or_else(|| AudioError::Unavailable("Per-app routing isn't available".into()))?
            .set(pid, device_id)
    }

    fn set_mic_boost(&mut self, id: &str, db: f32) -> Result<()> {
        let level =
            self.boost_level(id)?.ok_or_else(|| AudioError::Unavailable("This microphone has no boost control".into()))?;
        boost::write(&level, db)
    }

    fn peaks(&mut self, mic_device_id: Option<&str>) -> Peaks {
        let sessions =
            self.sessions.iter().map(|(id, h)| (id.clone(), unsafe { h.meter.GetPeakValue() }.unwrap_or(0.0))).collect();
        let master = self.master_meter.as_ref().and_then(|m| unsafe { m.GetPeakValue() }.ok()).unwrap_or(0.0);
        let mic = match mic_device_id {
            None => {
                self.mic_meter = None;
                0.0
            }
            Some(id) => {
                let current = self.mic_meter.as_ref().map(|m| m.device_id == id).unwrap_or(false);
                let failed_before = self.mic_meter_failed.as_deref() == Some(id);
                if !current && !failed_before {
                    self.mic_meter = None;
                    match self.endpoint(id).and_then(|d| MicMeter::open(d, id)) {
                        Ok(m) => {
                            self.mic_meter = Some(m);
                            self.mic_meter_failed = None;
                        }
                        Err(e) => {
                            tracing::warn!("couldn't open mic meter: {e}");
                            self.mic_meter_failed = Some(id.to_string());
                        }
                    }
                }
                self.mic_meter.as_ref().map(|m| m.peak()).unwrap_or(0.0)
            }
        };
        Peaks { sessions, master, mic }
    }

    fn stop_metering(&mut self) {
        self.mic_meter = None;
        self.mic_meter_failed = None;
    }
}
```

Add to `lib.rs`:
```rust
#[cfg(windows)]
pub mod win;
```

- [ ] **Step 8: Run the integration tests on real hardware**

Run: `cargo test --test wasapi -- --ignored --nocapture`
Expected: PASS (4 tests); the first prints devices and sessions — check names look right (the Space One appears as an output when connected). If a `windows` 0.62 signature differs (e.g. `SetMute` wanting `BOOL`, `Activate` taking a different context type), change the call to the signature in the compiler error and re-run.

- [ ] **Step 9: Confirm unit tests still pass**

Run: `cargo test --lib`
Expected: PASS.

- [ ] **Step 10: Commit**

```powershell
git add -A
git commit -m "feat(win): WASAPI backend for devices, sessions, volume and default devices"
```

---

### Task 8: Routing, mic boost, mic meter, app icons, accent colour

**Files:**
- Modify (replace stubs): `src-tauri/src/win/routing.rs`, `win/boost.rs`, `win/meter.rs`
- Create: `src-tauri/src/win/icon.rs`, `src-tauri/src/win/accent.rs`
- Modify: `src-tauri/src/win/mod.rs`, `src-tauri/tests/wasapi.rs`

**Interfaces:**
- Produces: `routing::{Routing, pack_device_id(&str) -> String, unpack_device_id(&str) -> Option<String>}` (same `Routing` methods as the stub); real `boost::{find, read, write}`; real `meter::MicMeter::{open, peak}`; `win::exe_icon_png_base64(path: &str) -> Option<String>`; `win::accent_hex() -> String`.

- [ ] **Step 1: Write failing tests**

Replace `src-tauri/src/win/routing.rs` entirely with the Step 3 code plus this test module at the bottom (write the tests first, with `pack_device_id`/`unpack_device_id` absent, to see the failure):
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packs_and_unpacks_render_device_ids() {
        let id = "{0.0.0.00000000}.{8b1a2c3d-0000-1111-2222-333344445555}";
        let packed = pack_device_id(id);
        assert_eq!(
            packed,
            r"\\?\SWD#MMDEVAPI#{0.0.0.00000000}.{8b1a2c3d-0000-1111-2222-333344445555}#{e6327cad-dcec-4949-ae8a-991e976a79d2}"
        );
        assert_eq!(unpack_device_id(&packed).as_deref(), Some(id));
    }

    #[test]
    fn unpack_rejects_empty_and_foreign_strings() {
        assert_eq!(unpack_device_id(""), None);
        assert_eq!(unpack_device_id("garbage"), None);
        assert_eq!(unpack_device_id(r"\\?\SWD#MMDEVAPI##{e6327cad-dcec-4949-ae8a-991e976a79d2}"), None);
    }
}
```

Append to `src-tauri/tests/wasapi.rs`:
```rust
#[test]
#[ignore]
fn routing_is_available_on_this_machine() {
    assert!(backend().routing_available(), "IAudioPolicyConfigFactory failed to activate");
}

#[test]
#[ignore]
fn icons_and_accent() {
    let icon = resonance_lib::win::exe_icon_png_base64(r"C:\Windows\explorer.exe").expect("explorer icon");
    assert!(icon.len() > 100);
    let accent = resonance_lib::win::accent_hex();
    assert!(accent.starts_with('#') && accent.len() == 7, "{accent}");
}

#[test]
#[ignore]
fn mic_meter_opens_and_reads() {
    let mut b = backend();
    let s = b.snapshot().unwrap();
    let Some(mic) =
        s.devices.iter().find(|d| d.flow == Flow::Capture && d.default_console && d.state == DeviceState::Active)
    else {
        eprintln!("no microphone; skipping");
        return;
    };
    for _ in 0..10 {
        let p = b.peaks(Some(&mic.id));
        assert!((0.0..=1.0).contains(&p.mic));
        std::thread::sleep(std::time::Duration::from_millis(30));
    }
    println!("boost: {:?}", mic.boost);
    b.stop_metering();
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test --lib win::routing`
Expected: FAIL to compile — `cannot find function pack_device_id`.

- [ ] **Step 3: Implement `routing.rs` (above the tests)**

```rust
#![allow(non_snake_case)]
use std::ffi::c_void;

use windows::core::{IUnknown, IUnknown_Vtbl, HRESULT, HSTRING};
use windows::Win32::{
    Media::Audio::{eConsole, eMultimedia, eRender, EDataFlow, ERole},
    System::WinRT::RoGetActivationFactory,
};

use crate::error::Result;

const MMDEVAPI_TOKEN: &str = r"\\?\SWD#MMDEVAPI#";
const DEVINTERFACE_AUDIO_RENDER: &str = "#{e6327cad-dcec-4949-ae8a-991e976a79d2}";

pub fn pack_device_id(id: &str) -> String {
    format!("{MMDEVAPI_TOKEN}{id}{DEVINTERFACE_AUDIO_RENDER}")
}

pub fn unpack_device_id(s: &str) -> Option<String> {
    let inner = s.strip_prefix(MMDEVAPI_TOKEN)?.strip_suffix(DEVINTERFACE_AUDIO_RENDER)?;
    (!inner.is_empty()).then(|| inner.to_string())
}

/// Undocumented `IAudioPolicyConfigFactory`, Windows 10 21H2+ layout (matches EarTrumpet's
/// IAudioPolicyConfigFactoryVariantFor21H2). Declared on IUnknown with the three IInspectable
/// slots as placeholders, then 19 unused methods, then the three we call.
#[windows_core::interface("ab3d4648-e242-459f-b02f-541c70306324")]
unsafe trait IAudioPolicyConfigFactory: IUnknown {
    fn GetIids(&self) -> HRESULT;
    fn GetRuntimeClassName(&self) -> HRESULT;
    fn GetTrustLevel(&self) -> HRESULT;
    fn add_CtxVolumeChange(&self) -> HRESULT;
    fn remove_CtxVolumeChanged(&self) -> HRESULT;
    fn add_RingerVibrateStateChanged(&self) -> HRESULT;
    fn remove_RingerVibrateStateChange(&self) -> HRESULT;
    fn SetVolumeGroupGainForId(&self) -> HRESULT;
    fn GetVolumeGroupGainForId(&self) -> HRESULT;
    fn GetActiveVolumeGroupForEndpointId(&self) -> HRESULT;
    fn GetVolumeGroupsForEndpoint(&self) -> HRESULT;
    fn GetCurrentVolumeContext(&self) -> HRESULT;
    fn SetVolumeGroupMuteForId(&self) -> HRESULT;
    fn GetVolumeGroupMuteForId(&self) -> HRESULT;
    fn SetRingerVibrateState(&self) -> HRESULT;
    fn GetRingerVibrateState(&self) -> HRESULT;
    fn SetPreferredChatApplication(&self) -> HRESULT;
    fn ResetPreferredChatApplication(&self) -> HRESULT;
    fn GetPreferredChatApplication(&self) -> HRESULT;
    fn GetCurrentChatApplications(&self) -> HRESULT;
    fn add_ChatContextChanged(&self) -> HRESULT;
    fn remove_ChatContextChanged(&self) -> HRESULT;
    fn SetPersistedDefaultAudioEndpoint(&self, pid: u32, flow: EDataFlow, role: ERole, device_id: *mut c_void) -> HRESULT;
    fn GetPersistedDefaultAudioEndpoint(&self, pid: u32, flow: EDataFlow, role: ERole, device_id: *mut *mut c_void) -> HRESULT;
    fn ClearAllPersistedApplicationDefaultEndpoints(&self) -> HRESULT;
}

pub struct Routing {
    factory: IAudioPolicyConfigFactory,
}

impl Routing {
    pub fn new() -> Result<Self> {
        let class = HSTRING::from("Windows.Media.Internal.AudioPolicyConfig");
        let factory: IAudioPolicyConfigFactory = unsafe { RoGetActivationFactory(&class)? };
        Ok(Self { factory })
    }

    /// The output Windows has persisted for this process, or `None` for "default".
    pub fn get(&self, pid: u32) -> Result<Option<String>> {
        let mut raw: *mut c_void = std::ptr::null_mut();
        unsafe { self.factory.GetPersistedDefaultAudioEndpoint(pid, eRender, eMultimedia, &mut raw).ok()? };
        // Take ownership of the returned HSTRING so it is freed on drop.
        let h: HSTRING = unsafe { std::mem::transmute::<*mut c_void, HSTRING>(raw) };
        Ok(unpack_device_id(&h.to_string()))
    }

    pub fn set(&self, pid: u32, device_id: Option<&str>) -> Result<()> {
        let packed = device_id.map(|id| HSTRING::from(pack_device_id(id)));
        let raw: *mut c_void = match &packed {
            Some(h) => unsafe { std::mem::transmute_copy::<HSTRING, *mut c_void>(h) },
            None => std::ptr::null_mut(),
        };
        for role in [eMultimedia, eConsole] {
            unsafe { self.factory.SetPersistedDefaultAudioEndpoint(pid, eRender, role, raw).ok()? };
        }
        Ok(())
    }
}
```

- [ ] **Step 4: Implement `boost.rs` (replace stub)**

```rust
use std::collections::HashSet;

use windows::core::Interface;
use windows::Win32::{
    Media::Audio::{IAudioVolumeLevel, IDeviceTopology, IMMDevice, IPart},
    System::Com::CLSCTX_ALL,
};

use super::util::take_pwstr;
use crate::{error::Result, model::Boost};

/// Finds the "Microphone Boost" control upstream of a capture endpoint, if the driver exposes one.
pub fn find(dev: &IMMDevice) -> Result<Option<IAudioVolumeLevel>> {
    unsafe {
        let topo: IDeviceTopology = dev.Activate(CLSCTX_ALL, None)?;
        let connector = topo.GetConnector(0)?;
        let Ok(other) = connector.GetConnectedTo() else { return Ok(None) };
        let part: IPart = other.cast()?;
        let mut visited = HashSet::new();
        Ok(walk(&part, &mut visited, 0))
    }
}

unsafe fn walk(part: &IPart, visited: &mut HashSet<u32>, depth: u32) -> Option<IAudioVolumeLevel> {
    if depth > 16 {
        return None;
    }
    let id = part.GetLocalId().ok()?;
    if !visited.insert(id) {
        return None;
    }
    if let Ok(name) = part.GetName() {
        if take_pwstr(name).to_lowercase().contains("boost") {
            if let Ok(level) = part.Activate::<IAudioVolumeLevel>(CLSCTX_ALL.0 as u32) {
                return Some(level);
            }
        }
    }
    let parts = part.EnumPartsIncoming().ok()?;
    for i in 0..parts.GetCount().ok()? {
        if let Ok(p) = parts.GetPart(i) {
            if let Some(level) = walk(&p, visited, depth + 1) {
                return Some(level);
            }
        }
    }
    None
}

pub fn read(level: &IAudioVolumeLevel) -> Result<Boost> {
    let (mut min, mut max, mut step) = (0f32, 0f32, 0f32);
    unsafe {
        level.GetLevelRange(0, &mut min, &mut max, &mut step)?;
        let db = level.GetLevel(0)?;
        Ok(Boost { db, min, max, step })
    }
}

pub fn write(level: &IAudioVolumeLevel, db: f32) -> Result<()> {
    unsafe { level.SetLevelUniform(db, std::ptr::null())? };
    Ok(())
}
```

- [ ] **Step 5: Implement `meter.rs` (replace stub)**

```rust
use windows::Win32::{
    Media::Audio::{Endpoints::IAudioMeterInformation, IAudioCaptureClient, IAudioClient, IMMDevice, AUDCLNT_SHAREMODE_SHARED},
    System::Com::{CoTaskMemFree, CLSCTX_ALL},
};

use crate::error::Result;

/// Capture endpoints only report peaks while a stream is open, so this opens a shared-mode
/// stream and discards the data. Alive only while a Resonance window is visible.
pub struct MicMeter {
    pub device_id: String,
    client: IAudioClient,
    capture: IAudioCaptureClient,
    meter: IAudioMeterInformation,
}

impl MicMeter {
    pub fn open(dev: &IMMDevice, id: &str) -> Result<Self> {
        unsafe {
            let client: IAudioClient = dev.Activate(CLSCTX_ALL, None)?;
            let format = client.GetMixFormat()?;
            let init = client.Initialize(AUDCLNT_SHAREMODE_SHARED, 0, 10_000_000, 0, format, None);
            CoTaskMemFree(Some(format as *const _));
            init?;
            let capture: IAudioCaptureClient = client.GetService()?;
            let meter: IAudioMeterInformation = dev.Activate(CLSCTX_ALL, None)?;
            client.Start()?;
            Ok(Self { device_id: id.to_string(), client, capture, meter })
        }
    }

    pub fn peak(&self) -> f32 {
        unsafe {
            // Drain and discard captured packets so the buffer never overflows.
            while let Ok(n) = self.capture.GetNextPacketSize() {
                if n == 0 {
                    break;
                }
                let mut data = std::ptr::null_mut();
                let mut frames = 0u32;
                let mut flags = 0u32;
                if self.capture.GetBuffer(&mut data, &mut frames, &mut flags, None, None).is_err() {
                    break;
                }
                let _ = self.capture.ReleaseBuffer(frames);
            }
            self.meter.GetPeakValue().unwrap_or(0.0)
        }
    }
}

impl Drop for MicMeter {
    fn drop(&mut self) {
        unsafe {
            let _ = self.client.Stop();
        }
    }
}
```

- [ ] **Step 6: Implement `icon.rs`, `accent.rs`, update `mod.rs`**

`src-tauri/src/win/icon.rs`:
```rust
use base64::{engine::general_purpose::STANDARD, Engine};
use windows::core::PCWSTR;
use windows::Win32::{
    Graphics::Gdi::{
        DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
    },
    UI::{
        Shell::ExtractIconExW,
        WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON, ICONINFO},
    },
};

use super::util::to_wide;

/// The executable's large icon as a base64 PNG, or `None` if it has none.
pub fn exe_icon_png_base64(path: &str) -> Option<String> {
    unsafe {
        let wide = to_wide(path);
        let mut large = HICON::default();
        let count = ExtractIconExW(PCWSTR(wide.as_ptr()), 0, Some(&mut large), None, 1);
        if count == 0 || large.is_invalid() {
            return None;
        }
        let png = icon_to_png(large);
        let _ = DestroyIcon(large);
        png.map(|bytes| STANDARD.encode(bytes))
    }
}

unsafe fn icon_to_png(icon: HICON) -> Option<Vec<u8>> {
    let mut info = ICONINFO::default();
    GetIconInfo(icon, &mut info).ok()?;
    let result = (|| {
        let mut bmp = BITMAP::default();
        if GetObjectW(info.hbmColor.into(), std::mem::size_of::<BITMAP>() as i32, Some(&mut bmp as *mut _ as *mut _)) == 0 {
            return None;
        }
        let (w, h) = (bmp.bmWidth, bmp.bmHeight);
        let mut bi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut buf = vec![0u8; (w * h * 4) as usize];
        let hdc = GetDC(None);
        let lines = GetDIBits(hdc, info.hbmColor, 0, h as u32, Some(buf.as_mut_ptr() as *mut _), &mut bi, DIB_RGB_COLORS);
        ReleaseDC(None, hdc);
        if lines == 0 {
            return None;
        }
        let has_alpha = buf.chunks_exact(4).any(|p| p[3] != 0);
        for px in buf.chunks_exact_mut(4) {
            px.swap(0, 2); // BGRA -> RGBA
            if !has_alpha {
                px[3] = 255;
            }
        }
        encode_png(&buf, w as u32, h as u32)
    })();
    let _ = DeleteObject(info.hbmColor.into());
    let _ = DeleteObject(info.hbmMask.into());
    result
}

fn encode_png(rgba: &[u8], w: u32, h: u32) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header().ok()?;
        writer.write_image_data(rgba).ok()?;
    }
    Some(out)
}
```

`src-tauri/src/win/accent.rs`:
```rust
use windows::core::w;
use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};

const FALLBACK: &str = "#7c5cff";

/// The Windows accent colour (`HKCU\Software\Microsoft\Windows\DWM\AccentColor`, 0xAABBGGRR) as `#rrggbb`.
pub fn accent_hex() -> String {
    let mut data: u32 = 0;
    let mut size = std::mem::size_of::<u32>() as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\DWM"),
            w!("AccentColor"),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut data as *mut u32 as *mut _),
            Some(&mut size),
        )
    };
    if status.is_err() {
        return FALLBACK.into();
    }
    let (r, g, b) = (data & 0xff, (data >> 8) & 0xff, (data >> 16) & 0xff);
    format!("#{r:02x}{g:02x}{b:02x}")
}
```

`src-tauri/src/win/mod.rs`:
```rust
mod accent;
mod backend;
pub mod boost;
mod com;
mod endpoints;
mod icon;
pub mod meter;
mod policy;
mod procinfo;
pub mod routing;
mod util;

pub use accent::accent_hex;
pub use backend::WasapiBackend;
pub use icon::exe_icon_png_base64;
```

- [ ] **Step 7: Run unit and hardware tests**

Run:
```powershell
cargo test --lib
cargo test --test wasapi -- --ignored --nocapture
```
Expected: all PASS. `routing_is_available_on_this_machine` passing proves the vtable layout matches this Windows build. (If `RegGetValueW`'s `WIN32_ERROR` has no `is_err()`, compare with `ERROR_SUCCESS`.)

- [ ] **Step 8: Manual check with a real app**

Play audio in a browser, then run `cargo test --test wasapi enumerates -- --ignored --nocapture`. Confirm the browser session lists an `exe_path` and a friendly `display_name` (e.g. "Google Chrome").

- [ ] **Step 9: Commit**

```powershell
git add -A
git commit -m "feat(win): per-app routing, mic boost, mic meter, app icons, accent colour"
```

---

### Task 9: Tauri wiring — commands, tray, flyout, hotkeys, logging, metering

**Files:**
- Create: `src-tauri/src/app/mod.rs`, `app/commands.rs`, `app/sink.rs`, `app/tray.rs`, `app/wnd.rs`, `app/hotkeys.rs`, `app/logging.rs`
- Modify: `src-tauri/src/lib.rs` (final form)

**Interfaces:**
- Consumes: `worker::{spawn, WorkerHandle, EventSink}`, `win::{WasapiBackend, accent_hex, exe_icon_png_base64}`, `Settings`, `paths`.
- Produces Tauri commands (JS names / args): `command({ cmd })`, `get_state() -> UiState`, `get_app_icon({ exePath }) -> string | null`, `set_mic_hotkey({ config })`, `take_notices() -> AudioError[]`, `show_main()`.
- Produces events: `state` (UiState), `meters` (MeterFrame), `mic-muted` (bool), `notice` (AudioError).
- Produces `app::hotkeys::hotkey_action(mode: HotkeyMode, pressed: bool) -> Option<Command>`.

- [ ] **Step 1: Write the failing hotkey-mapping test (bottom of a new `app/hotkeys.rs`)**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_modes_and_key_states_to_commands() {
        assert_eq!(hotkey_action(HotkeyMode::Toggle, true), Some(Command::ToggleMicMute));
        assert_eq!(hotkey_action(HotkeyMode::Toggle, false), None);
        assert_eq!(hotkey_action(HotkeyMode::PushToTalk, true), Some(Command::SetMicMute { muted: false }));
        assert_eq!(hotkey_action(HotkeyMode::PushToTalk, false), Some(Command::SetMicMute { muted: true }));
        assert_eq!(hotkey_action(HotkeyMode::PushToMute, true), Some(Command::SetMicMute { muted: true }));
        assert_eq!(hotkey_action(HotkeyMode::PushToMute, false), Some(Command::SetMicMute { muted: false }));
    }
}
```

Create `src-tauri/src/app/mod.rs` containing only `mod hotkeys;` for now, and add `mod app;` to `lib.rs`.

- [ ] **Step 2: Run to verify failure**

Run: `cargo test --lib app::hotkeys`
Expected: FAIL to compile — `cannot find function hotkey_action`.

- [ ] **Step 3: Implement `app/hotkeys.rs` (top of file)**

```rust
use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use crate::{
    model::{Command, HotkeyMode, MicConfig},
    worker::WorkerHandle,
};

pub fn hotkey_action(mode: HotkeyMode, pressed: bool) -> Option<Command> {
    match (mode, pressed) {
        (HotkeyMode::Toggle, true) => Some(Command::ToggleMicMute),
        (HotkeyMode::Toggle, false) => None,
        (HotkeyMode::PushToTalk, p) => Some(Command::SetMicMute { muted: !p }),
        (HotkeyMode::PushToMute, p) => Some(Command::SetMicMute { muted: p }),
    }
}

/// Replaces the registered mic hotkey. Validates by registering; returns a readable error.
pub fn apply(app: &AppHandle, cfg: &MicConfig) -> Result<(), String> {
    let gs = app.global_shortcut();
    gs.unregister_all().map_err(|e| format!("Couldn't clear hotkeys: {e}"))?;
    let Some(hk) = cfg.hotkey.as_deref() else { return Ok(()) };
    let shortcut: Shortcut = hk.parse().map_err(|e| format!("Invalid hotkey \"{hk}\": {e}"))?;
    let worker = app.state::<WorkerHandle>().inner().clone();
    let mode = cfg.mode;
    gs.on_shortcut(shortcut, move |_app, _shortcut, event| {
        if let Some(cmd) = hotkey_action(mode, event.state() == ShortcutState::Pressed) {
            worker.send(cmd);
        }
    })
    .map_err(|e| format!("Couldn't register \"{hk}\" (is another app using it?): {e}"))?;
    if mode == HotkeyMode::PushToTalk {
        app.state::<WorkerHandle>().send(Command::SetMicMute { muted: true });
    }
    Ok(())
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test --lib app::hotkeys`
Expected: PASS.

- [ ] **Step 5: Write `app/logging.rs`, `app/sink.rs`, `app/tray.rs`, `app/wnd.rs`, `app/commands.rs`**

`src-tauri/src/app/logging.rs`:
```rust
use std::path::Path;

use tracing_appender::{non_blocking::WorkerGuard, rolling};
use tracing_subscriber::EnvFilter;

/// Daily-rotated log files in `dir`, keeping 7. Keep the guard alive for the app's lifetime.
pub fn init(dir: &Path) -> Option<WorkerGuard> {
    let appender = rolling::Builder::new()
        .rotation(rolling::Rotation::DAILY)
        .filename_prefix("resonance")
        .filename_suffix("log")
        .max_log_files(7)
        .build(dir)
        .ok()?;
    let (writer, guard) = tracing_appender::non_blocking(appender);
    tracing_subscriber::fmt()
        .with_writer(writer)
        .with_ansi(false)
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .try_init()
        .ok()?;
    Some(guard)
}
```

`src-tauri/src/app/sink.rs`:
```rust
use std::sync::Mutex;

use tauri::{AppHandle, Emitter, Manager};

use super::tray;
use crate::{
    error::AudioError,
    model::{MeterFrame, UiState},
    worker::EventSink,
};

/// Notices raised before any window was listening; drained by `take_notices`.
#[derive(Default)]
pub struct Notices(pub Mutex<Vec<AudioError>>);

pub fn push_notice(app: &AppHandle, error: AudioError) {
    if let Ok(mut n) = app.state::<Notices>().0.lock() {
        n.push(error.clone());
    }
    let _ = app.emit("notice", error);
}

pub struct TauriSink {
    pub app: AppHandle,
}

impl EventSink for TauriSink {
    fn state(&self, state: &UiState) {
        let _ = self.app.emit("state", state);
    }

    fn meters(&self, frame: &MeterFrame) {
        let _ = self.app.emit("meters", frame);
    }

    fn mic_muted(&self, muted: bool) {
        let app = self.app.clone();
        let _ = self.app.run_on_main_thread(move || tray::set_mic_muted(&app, muted));
        let _ = self.app.emit("mic-muted", muted);
    }

    fn notice(&self, error: &AudioError) {
        push_notice(&self.app, error.clone());
    }
}
```

`src-tauri/src/app/tray.rs`:
```rust
use tauri::{
    image::Image,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager,
};

use super::wnd;
use crate::{model::Command, worker::WorkerHandle};

pub const TRAY_ID: &str = "resonance";
const ICON: &[u8] = include_bytes!("../../icons/tray.png");
const ICON_MUTED: &[u8] = include_bytes!("../../icons/tray-muted.png");

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open Resonance", true, None::<&str>)?;
    let mute = MenuItem::with_id(app, "mute", "Toggle microphone mute", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Resonance", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&open, &mute, &sep, &quit])?;
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(Image::from_bytes(ICON)?)
        .tooltip("Resonance")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => wnd::show_main(app),
            "mute" => app.state::<WorkerHandle>().send(Command::ToggleMicMute),
            "quit" => {
                app.state::<WorkerHandle>().shutdown();
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, rect, .. } = event {
                wnd::toggle_flyout(tray.app_handle(), rect);
            }
        })
        .build(app)?;
    Ok(())
}

pub fn set_mic_muted(app: &AppHandle, muted: bool) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else { return };
    if let Ok(img) = Image::from_bytes(if muted { ICON_MUTED } else { ICON }) {
        let _ = tray.set_icon(Some(img));
    }
    let _ = tray.set_tooltip(Some(if muted { "Resonance — microphone muted" } else { "Resonance" }));
}
```

`src-tauri/src/app/wnd.rs`:
```rust
use std::{
    sync::Mutex,
    time::{Duration, Instant},
};

use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, Rect, Window, WindowEvent};

use crate::worker::WorkerHandle;

const FLYOUT_W: f64 = 380.0;
const FLYOUT_H: f64 = 560.0;
const MARGIN: f64 = 12.0;

/// When the flyout last hid on focus loss: the tray click that caused it must not reopen it.
static LAST_FLYOUT_HIDE: Mutex<Option<Instant>> = Mutex::new(None);

/// Meters (and the mic capture stream) run only while some window is actually visible.
pub fn update_metering(app: &AppHandle) {
    let active = ["main", "flyout"]
        .iter()
        .filter_map(|l| app.get_webview_window(l))
        .any(|w| w.is_visible().unwrap_or(false) && !w.is_minimized().unwrap_or(false));
    if let Some(worker) = app.try_state::<WorkerHandle>() {
        worker.set_metering(active);
    }
}

pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
    update_metering(app);
}

pub fn toggle_flyout(app: &AppHandle, icon: Rect) {
    let Some(w) = app.get_webview_window("flyout") else { return };
    let just_hid = LAST_FLYOUT_HIDE
        .lock()
        .ok()
        .and_then(|t| *t)
        .map(|t| t.elapsed() < Duration::from_millis(300))
        .unwrap_or(false);
    if w.is_visible().unwrap_or(false) || just_hid {
        let _ = w.hide();
        update_metering(app);
        return;
    }
    let pos = icon.position.to_physical::<f64>(1.0);
    let size = icon.size.to_physical::<f64>(1.0);
    let (cx, cy) = (pos.x + size.width / 2.0, pos.y + size.height / 2.0);
    if let Ok(Some(mon)) = app.monitor_from_point(cx, cy) {
        let scale = mon.scale_factor();
        let wa = mon.work_area();
        let (fw, fh, m) = (FLYOUT_W * scale, FLYOUT_H * scale, MARGIN * scale);
        let (left, top) = (wa.position.x as f64, wa.position.y as f64);
        let (right, bottom) = (left + wa.size.width as f64, top + wa.size.height as f64);
        let x = (cx - fw / 2.0).clamp(left + m, (right - fw - m).max(left + m));
        // Taskbar at the top → open below it; otherwise open above the bottom edge.
        let y = if cy < top { top + m } else { bottom - fh - m };
        let _ = w.set_size(PhysicalSize::new(fw.round() as u32, fh.round() as u32));
        let _ = w.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
    }
    let _ = w.show();
    let _ = w.set_focus();
    update_metering(app);
}

pub fn on_window_event(window: &Window, event: &WindowEvent) {
    match event {
        WindowEvent::CloseRequested { api, .. } if window.label() == "main" => {
            api.prevent_close();
            let _ = window.hide();
        }
        WindowEvent::Focused(false) if window.label() == "flyout" => {
            let _ = window.hide();
            if let Ok(mut t) = LAST_FLYOUT_HIDE.lock() {
                *t = Some(Instant::now());
            }
        }
        _ => {}
    }
    if matches!(event, WindowEvent::CloseRequested { .. } | WindowEvent::Focused(_) | WindowEvent::Resized(_)) {
        update_metering(window.app_handle());
    }
}
```

`src-tauri/src/app/commands.rs`:
```rust
use std::{collections::HashMap, sync::Mutex};

use tauri::{AppHandle, Manager, State};

use super::{hotkeys, sink::Notices, wnd};
use crate::{
    error::{AudioError, Result},
    model::{Command, MicConfig, UiState},
    worker::WorkerHandle,
};

#[derive(Default)]
pub struct IconCache(Mutex<HashMap<String, Option<String>>>);

#[tauri::command]
pub async fn command(worker: State<'_, WorkerHandle>, cmd: Command) -> Result<()> {
    worker.command(cmd).await
}

#[tauri::command]
pub async fn get_state(worker: State<'_, WorkerHandle>) -> Result<UiState> {
    worker.state().await
}

#[tauri::command]
pub async fn get_app_icon(cache: State<'_, IconCache>, exe_path: String) -> Result<Option<String>> {
    if let Some(hit) = cache.0.lock().ok().and_then(|c| c.get(&exe_path).cloned()) {
        return Ok(hit);
    }
    let path = exe_path.clone();
    let icon = tauri::async_runtime::spawn_blocking(move || crate::win::exe_icon_png_base64(&path)).await.unwrap_or(None);
    if let Ok(mut c) = cache.0.lock() {
        c.insert(exe_path, icon.clone());
    }
    Ok(icon)
}

#[tauri::command]
pub async fn set_mic_hotkey(app: AppHandle, worker: State<'_, WorkerHandle>, config: MicConfig) -> Result<()> {
    hotkeys::apply(&app, &config).map_err(AudioError::Invalid)?;
    worker.command(Command::SetMicConfig { config }).await
}

#[tauri::command]
pub fn take_notices(notices: State<'_, Notices>) -> Vec<AudioError> {
    notices.0.lock().map(|mut n| std::mem::take(&mut *n)).unwrap_or_default()
}

#[tauri::command]
pub fn show_main(app: AppHandle) {
    if let Some(f) = app.get_webview_window("flyout") {
        let _ = f.hide();
    }
    wnd::show_main(&app);
}
```

- [ ] **Step 6: Write `app/mod.rs` and the final `lib.rs`**

`src-tauri/src/app/mod.rs`:
```rust
mod commands;
mod hotkeys;
mod logging;
mod sink;
mod tray;
mod wnd;

use tauri::Manager;

use crate::{error::AudioError, paths, settings::Settings, worker};

pub fn run() {
    let _log_guard = logging::init(&paths::logs_dir());
    let settings_path = paths::settings_path();
    let initial = Settings::load(&settings_path);
    tracing::info!("Resonance starting");

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| wnd::show_main(app)))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(commands::IconCache::default())
        .manage(sink::Notices::default())
        .setup(move |app| {
            let handle = app.handle().clone();
            let worker = worker::spawn(
                crate::win::WasapiBackend::new,
                settings_path.clone(),
                crate::win::accent_hex(),
                sink::TauriSink { app: handle.clone() },
            );
            app.manage(worker);
            tray::create(&handle)?;
            if let Err(e) = hotkeys::apply(&handle, &initial.mic) {
                tracing::warn!("{e}");
                sink::push_notice(&handle, AudioError::Invalid(e));
            }
            if !initial.ui.start_minimized {
                wnd::show_main(&handle);
            }
            Ok(())
        })
        .on_window_event(wnd::on_window_event)
        .invoke_handler(tauri::generate_handler![
            commands::command,
            commands::get_state,
            commands::get_app_icon,
            commands::set_mic_hotkey,
            commands::take_notices,
            commands::show_main,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Resonance");
}
```

`src-tauri/src/lib.rs`:
```rust
mod app;
pub mod app_state;
pub mod backend;
pub mod error;
#[cfg(test)]
mod fake;
pub mod model;
pub mod paths;
pub mod settings;
#[cfg(windows)]
pub mod win;
pub mod worker;

pub use app::run;
```

- [ ] **Step 7: Run tests and build**

Run:
```powershell
cargo test --lib
cargo build
```
Expected: all tests PASS; build `Finished`. If a tauri 2.x name differs (e.g. `show_menu_on_left_click`), apply the compiler's suggestion.

- [ ] **Step 8: Smoke-run**

Run from `M:\Resonance`: `npm.cmd run tauri dev`
Expected: main window shows the placeholder; tray icon appears; left-click shows the flyout above the taskbar and clicking elsewhere hides it; closing the main window hides it (process stays in tray); tray "Quit Resonance" exits; `%APPDATA%\Resonance\logs\` has a log containing "Resonance starting"; pressing Ctrl+Shift+M toggles the red badge on the tray icon.

- [ ] **Step 9: Commit**

```powershell
git add -A
git commit -m "feat(app): Tauri wiring — commands, tray, flyout, hotkeys, logging, metering"
```


---

### Task 10: Frontend foundation — types, API, store, throttle, meter engine, hotkey format

**Files:**
- Create: `src/lib/types.ts`, `src/lib/api.ts`, `src/lib/store.ts`, `src/lib/throttle.ts`, `src/lib/meters.ts`, `src/lib/hotkey.ts`, `src/test/fixtures.ts`
- Test: `src/lib/throttle.test.ts`, `src/lib/meters.test.ts`, `src/lib/store.test.ts`, `src/lib/hotkey.test.ts`

**Interfaces:**
- Consumes: Tauri commands/events from Task 9; `peakToPosition` from Task 1.
- Produces:
  - `types.ts`: `Flow, DeviceState, Role, HotkeyMode, Theme, Boost, Device, App, MicConfig, UiSettings, UiState, MeterFrame, Command, CmdError` (mirror Rust camelCase).
  - `api.ts`: `send(cmd)`, `getState()`, `getAppIcon(exePath)`, `setMicHotkey(config)`, `takeNotices()`, `showMain()`, `onState(cb)`, `onMeters(cb)`, `onNotice(cb)`.
  - `store.ts`: `useStore` (`state`, `toasts`, `setState`, `pushToast`, `dismissToast`), `run(cmd): Promise<boolean>`, `saveMicConfig(config): Promise<boolean>`, `errorMessage(e)`, selectors `outputs, inputs, defaultOutput, defaultMic`.
  - `throttle(fn, ms)`; `meters.ts`: `stepMeter, ingestFrame, readLevel, registerMeter, PEAK_HOLD_MS`; `hotkey.ts`: `formatHotkey(e)`.
  - `fixtures.ts`: `makeDevice, makeApp, makeState`.

- [ ] **Step 1: Write `src/lib/types.ts`**

```ts
export type Flow = "render" | "capture";
export type DeviceState = "active" | "unplugged";
export type Role = "console" | "communications";
export type HotkeyMode = "toggle" | "pushToTalk" | "pushToMute";
export type Theme = "system" | "light" | "dark";

export interface Boost {
  db: number;
  min: number;
  max: number;
  step: number;
}

export interface Device {
  id: string;
  name: string;
  originalName: string;
  alias: string | null;
  hidden: boolean;
  flow: Flow;
  state: DeviceState;
  defaultConsole: boolean;
  defaultComms: boolean;
  volume: number;
  muted: boolean;
  boost: Boost | null;
}

export interface App {
  key: string;
  name: string;
  exePath: string | null;
  isSystem: boolean;
  pids: number[];
  sessionIds: string[];
  volume: number;
  muted: boolean;
  output: string | null;
}

export interface MicConfig {
  hotkey: string | null;
  mode: HotkeyMode;
}

export interface UiSettings {
  theme: Theme;
  startMinimized: boolean;
}

export interface UiState {
  devices: Device[];
  apps: App[];
  routingAvailable: boolean;
  mic: MicConfig;
  ui: UiSettings;
  accent: string;
}

export interface MeterFrame {
  apps: Record<string, number>;
  master: number;
  mic: number;
}

export type Command =
  | { type: "setAppVolume"; key: string; volume: number }
  | { type: "setAppMute"; key: string; muted: boolean }
  | { type: "setAppOutput"; key: string; deviceId: string | null }
  | { type: "setDeviceVolume"; id: string; volume: number }
  | { type: "setDeviceMute"; id: string; muted: boolean }
  | { type: "setDefaultDevice"; id: string; role: Role }
  | { type: "setDeviceAlias"; id: string; alias: string | null }
  | { type: "setDeviceHidden"; id: string; hidden: boolean }
  | { type: "setMicBoost"; id: string; db: number }
  | { type: "setMicMute"; muted: boolean }
  | { type: "toggleMicMute" }
  | { type: "setMicConfig"; config: MicConfig }
  | { type: "setUiSettings"; ui: UiSettings }
  | { type: "resetAppMemory" };

export interface CmdError {
  code: "notFound" | "unavailable" | "invalid" | "os";
  message: string;
}
```

- [ ] **Step 2: Write `src/lib/api.ts` and `src/test/fixtures.ts`**

`src/lib/api.ts`:
```ts
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { CmdError, Command, MeterFrame, MicConfig, UiState } from "./types";

export const send = (cmd: Command) => invoke<void>("command", { cmd });
export const getState = () => invoke<UiState>("get_state");
export const getAppIcon = (exePath: string) => invoke<string | null>("get_app_icon", { exePath });
export const setMicHotkey = (config: MicConfig) => invoke<void>("set_mic_hotkey", { config });
export const takeNotices = () => invoke<CmdError[]>("take_notices");
export const showMain = () => invoke<void>("show_main");

export const onState = (cb: (s: UiState) => void): Promise<UnlistenFn> => listen<UiState>("state", (e) => cb(e.payload));
export const onMeters = (cb: (m: MeterFrame) => void): Promise<UnlistenFn> =>
  listen<MeterFrame>("meters", (e) => cb(e.payload));
export const onNotice = (cb: (e: CmdError) => void): Promise<UnlistenFn> => listen<CmdError>("notice", (e) => cb(e.payload));
```

`src/test/fixtures.ts`:
```ts
import type { App, Device, UiState } from "../lib/types";

export function makeDevice(p: Partial<Device> & { id: string }): Device {
  return {
    name: `Device ${p.id}`,
    originalName: `Device ${p.id}`,
    alias: null,
    hidden: false,
    flow: "render",
    state: "active",
    defaultConsole: false,
    defaultComms: false,
    volume: 0.5,
    muted: false,
    boost: null,
    ...p,
  };
}

export function makeApp(p: Partial<App> & { key: string }): App {
  return {
    name: p.key,
    exePath: `C:\\Apps\\${p.key}`,
    isSystem: false,
    pids: [1],
    sessionIds: [`${p.key}-s`],
    volume: 1,
    muted: false,
    output: null,
    ...p,
  };
}

export function makeState(p: Partial<UiState> = {}): UiState {
  return {
    devices: [
      makeDevice({ id: "out1", name: "Headphones", defaultConsole: true, defaultComms: true }),
      makeDevice({ id: "out2", name: "Speakers" }),
      makeDevice({ id: "gone", name: "Old DAC", state: "unplugged" }),
      makeDevice({ id: "mic1", name: "Desk Mic", flow: "capture", defaultConsole: true, defaultComms: true }),
    ],
    apps: [makeApp({ key: "spotify.exe", name: "Spotify" }), makeApp({ key: "#system", name: "System sounds", isSystem: true, exePath: null })],
    routingAvailable: true,
    mic: { hotkey: "Ctrl+Shift+M", mode: "toggle" },
    ui: { theme: "system", startMinimized: false },
    accent: "#7c5cff",
    ...p,
  };
}
```

- [ ] **Step 3: Write the failing tests**

`src/lib/throttle.test.ts`:
```ts
import { throttle } from "./throttle";

describe("throttle", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("calls immediately, then at most once per interval with the latest args", () => {
    const fn = vi.fn();
    const t = throttle(fn, 100);
    t(1);
    t(2);
    t(3);
    expect(fn).toHaveBeenCalledTimes(1);
    expect(fn).toHaveBeenLastCalledWith(1);
    vi.advanceTimersByTime(100);
    expect(fn).toHaveBeenCalledTimes(2);
    expect(fn).toHaveBeenLastCalledWith(3);
    vi.advanceTimersByTime(500);
    expect(fn).toHaveBeenCalledTimes(2);
  });

  it("calls immediately again after the interval passes", () => {
    const fn = vi.fn();
    const t = throttle(fn, 100);
    t("a");
    vi.advanceTimersByTime(150);
    t("b");
    expect(fn).toHaveBeenCalledTimes(2);
    expect(fn).toHaveBeenLastCalledWith("b");
  });
});
```

`src/lib/meters.test.ts`:
```ts
import { ingestFrame, readLevel, stepMeter, PEAK_HOLD_MS } from "./meters";

describe("stepMeter", () => {
  const zero = { level: 0, peak: 0, peakAt: 0 };

  it("attacks instantly and records the peak", () => {
    const s = stepMeter(zero, 0.8, 1000);
    expect(s).toEqual({ level: 0.8, peak: 0.8, peakAt: 1000 });
  });

  it("decays smoothly instead of dropping", () => {
    const s = stepMeter({ level: 0.8, peak: 0.8, peakAt: 1000 }, 0, 1016);
    expect(s.level).toBeLessThan(0.8);
    expect(s.level).toBeGreaterThan(0.5);
  });

  it("holds the peak, then lets it fall", () => {
    const held = stepMeter({ level: 0.2, peak: 0.9, peakAt: 1000 }, 0.2, 1000 + PEAK_HOLD_MS - 1);
    expect(held.peak).toBe(0.9);
    const falling = stepMeter({ level: 0.2, peak: 0.9, peakAt: 1000 }, 0.2, 1000 + PEAK_HOLD_MS + 1);
    expect(falling.peak).toBeLessThan(0.9);
    expect(falling.peak).toBeGreaterThanOrEqual(falling.level);
  });
});

describe("ingestFrame", () => {
  it("stores master, mic and per-app values and zeroes apps that disappear", () => {
    ingestFrame({ apps: { "a.exe": 0.5, "b.exe": 0.25 }, master: 0.7, mic: 0.1 });
    expect(readLevel("master")).toBe(0.7);
    expect(readLevel("mic")).toBe(0.1);
    expect(readLevel("app:a.exe")).toBe(0.5);
    ingestFrame({ apps: { "b.exe": 0.3 }, master: 0, mic: 0 });
    expect(readLevel("app:a.exe")).toBe(0);
    expect(readLevel("app:b.exe")).toBe(0.3);
    expect(readLevel("app:never")).toBe(0);
  });
});
```

`src/lib/hotkey.test.ts`:
```ts
import { formatHotkey } from "./hotkey";

const key = (code: string, mods: Partial<{ ctrlKey: boolean; shiftKey: boolean; altKey: boolean; metaKey: boolean }> = {}) => ({
  code,
  ctrlKey: false,
  shiftKey: false,
  altKey: false,
  metaKey: false,
  ...mods,
});

describe("formatHotkey", () => {
  it("formats modifiers in a stable order with letter and digit codes simplified", () => {
    expect(formatHotkey(key("KeyM", { ctrlKey: true, shiftKey: true }))).toBe("Ctrl+Shift+M");
    expect(formatHotkey(key("Digit1", { altKey: true, ctrlKey: true }))).toBe("Ctrl+Alt+1");
    expect(formatHotkey(key("KeyK", { metaKey: true }))).toBe("Super+K");
  });

  it("allows bare function keys but not bare letters", () => {
    expect(formatHotkey(key("F13"))).toBe("F13");
    expect(formatHotkey(key("KeyM"))).toBeNull();
  });

  it("ignores lone modifier presses", () => {
    expect(formatHotkey(key("ShiftLeft", { shiftKey: true }))).toBeNull();
    expect(formatHotkey(key("ControlRight", { ctrlKey: true }))).toBeNull();
  });
});
```

`src/lib/store.test.ts`:
```ts
import { defaultMic, defaultOutput, errorMessage, inputs, outputs, run, saveMicConfig, useStore } from "./store";
import { makeDevice, makeState } from "../test/fixtures";
import * as api from "./api";

vi.mock("./api", () => ({
  send: vi.fn(),
  setMicHotkey: vi.fn(),
}));

describe("selectors", () => {
  it("lists active, visible outputs and inputs and finds defaults", () => {
    const s = makeState();
    s.devices.push(makeDevice({ id: "hid", hidden: true }));
    expect(outputs(s).map((d) => d.id)).toEqual(["out1", "out2"]);
    expect(inputs(s).map((d) => d.id)).toEqual(["mic1"]);
    expect(defaultOutput(s)?.id).toBe("out1");
    expect(defaultMic(s)?.id).toBe("mic1");
  });
});

describe("run", () => {
  beforeEach(() => useStore.setState({ toasts: [] }));

  it("returns true on success", async () => {
    vi.mocked(api.send).mockResolvedValueOnce(undefined);
    expect(await run({ type: "toggleMicMute" })).toBe(true);
    expect(useStore.getState().toasts).toEqual([]);
  });

  it("shows the backend error message as a toast on failure", async () => {
    vi.mocked(api.send).mockRejectedValueOnce({ code: "notFound", message: "That device is no longer connected" });
    expect(await run({ type: "toggleMicMute" })).toBe(false);
    expect(useStore.getState().toasts.map((t) => t.message)).toEqual(["That device is no longer connected"]);
  });

  it("saveMicConfig toasts hotkey registration errors", async () => {
    vi.mocked(api.setMicHotkey).mockRejectedValueOnce({ code: "invalid", message: "Couldn't register" });
    expect(await saveMicConfig({ hotkey: "Ctrl+F1", mode: "toggle" })).toBe(false);
    expect(useStore.getState().toasts.at(-1)?.message).toBe("Couldn't register");
  });
});

describe("toasts", () => {
  it("dedupes identical messages and keeps at most 4", () => {
    useStore.setState({ toasts: [] });
    const { pushToast } = useStore.getState();
    pushToast("error", "same");
    pushToast("error", "same");
    expect(useStore.getState().toasts).toHaveLength(1);
    for (let i = 0; i < 6; i++) pushToast("info", `m${i}`);
    expect(useStore.getState().toasts).toHaveLength(4);
  });

  it("errorMessage handles strings, error objects and junk", () => {
    expect(errorMessage("x")).toBe("x");
    expect(errorMessage(new Error("boom"))).toBe("boom");
    expect(errorMessage(42)).toBe("Something went wrong");
  });
});
```

- [ ] **Step 4: Run tests to verify they fail**

Run: `npm.cmd test`
Expected: FAIL — unresolved imports `./throttle`, `./meters`, `./hotkey`, `./store`.

- [ ] **Step 5: Implement `throttle.ts`, `meters.ts`, `hotkey.ts`, `store.ts`**

`src/lib/throttle.ts`:
```ts
/** Leading + trailing throttle: fires now, then at most once per `ms` with the latest args. */
export function throttle<A extends unknown[]>(fn: (...args: A) => void, ms: number): (...args: A) => void {
  let last = 0;
  let timer: ReturnType<typeof setTimeout> | null = null;
  let pending: A | null = null;
  const fire = (args: A) => {
    last = Date.now();
    fn(...args);
  };
  return (...args: A) => {
    const wait = ms - (Date.now() - last);
    if (wait <= 0) {
      if (timer) {
        clearTimeout(timer);
        timer = null;
      }
      pending = null;
      fire(args);
      return;
    }
    pending = args;
    if (!timer) {
      timer = setTimeout(() => {
        timer = null;
        if (pending) {
          const p = pending;
          pending = null;
          fire(p);
        }
      }, wait);
    }
  };
}
```

`src/lib/meters.ts`:
```ts
import { peakToPosition } from "./levels";
import type { MeterFrame } from "./types";

export const PEAK_HOLD_MS = 1200;
const DECAY = 0.12;
const PEAK_FALL = 0.015;

export interface MeterState {
  level: number;
  peak: number;
  peakAt: number;
}

/** One animation step: instant attack, exponential release, held peak marker. */
export function stepMeter(s: MeterState, target: number, now: number): MeterState {
  const level = target >= s.level ? target : s.level + (target - s.level) * DECAY;
  if (target >= s.peak) return { level, peak: target, peakAt: now };
  if (now - s.peakAt > PEAK_HOLD_MS) return { level, peak: Math.max(level, s.peak - PEAK_FALL), peakAt: s.peakAt };
  return { level, peak: s.peak, peakAt: s.peakAt };
}

const latest = new Map<string, number>();

/** Sources: "master", "mic", "app:<key>". */
export function ingestFrame(f: MeterFrame) {
  latest.set("master", f.master);
  latest.set("mic", f.mic);
  for (const k of latest.keys()) {
    if (k.startsWith("app:") && !(k.slice(4) in f.apps)) latest.set(k, 0);
  }
  for (const [k, v] of Object.entries(f.apps)) latest.set(`app:${k}`, v);
}

export function readLevel(source: string): number {
  return latest.get(source) ?? 0;
}

interface Registration {
  source: string;
  vertical: boolean;
  fill: HTMLElement;
  peak: HTMLElement;
  state: MeterState;
}

const registrations = new Set<Registration>();
let raf = 0;

function tick(now: number) {
  for (const r of registrations) {
    r.state = stepMeter(r.state, peakToPosition(readLevel(r.source)), now);
    const lv = r.state.level * 100;
    const pk = r.state.peak * 100;
    r.peak.style.opacity = pk > 1 ? "0.8" : "0";
    if (r.vertical) {
      r.fill.style.clipPath = `inset(${100 - lv}% 0 0 0)`;
      r.peak.style.bottom = `${pk}%`;
    } else {
      r.fill.style.clipPath = `inset(0 ${100 - lv}% 0 0)`;
      r.peak.style.left = `calc(${pk}% - 1px)`;
    }
  }
  raf = registrations.size ? requestAnimationFrame(tick) : 0;
}

/** Drives a meter's DOM directly (no React renders). Returns an unregister function. */
export function registerMeter(source: string, vertical: boolean, fill: HTMLElement, peak: HTMLElement): () => void {
  const r: Registration = { source, vertical, fill, peak, state: { level: 0, peak: 0, peakAt: 0 } };
  registrations.add(r);
  if (!raf) raf = requestAnimationFrame(tick);
  return () => {
    registrations.delete(r);
  };
}
```

`src/lib/hotkey.ts`:
```ts
const MODIFIER_CODES = new Set([
  "ControlLeft", "ControlRight", "ShiftLeft", "ShiftRight", "AltLeft", "AltRight", "MetaLeft", "MetaRight",
]);

interface KeyLike {
  code: string;
  ctrlKey: boolean;
  shiftKey: boolean;
  altKey: boolean;
  metaKey: boolean;
}

/** Converts a key press into the global-shortcut format ("Ctrl+Shift+M"), or null if incomplete. */
export function formatHotkey(e: KeyLike): string | null {
  if (MODIFIER_CODES.has(e.code)) return null;
  const mods: string[] = [];
  if (e.ctrlKey) mods.push("Ctrl");
  if (e.altKey) mods.push("Alt");
  if (e.shiftKey) mods.push("Shift");
  if (e.metaKey) mods.push("Super");
  const key = e.code.startsWith("Key") ? e.code.slice(3) : e.code.startsWith("Digit") ? e.code.slice(5) : e.code;
  const isFunctionKey = /^F\d{1,2}$/.test(key);
  if (mods.length === 0 && !isFunctionKey) return null;
  return [...mods, key].join("+");
}
```

`src/lib/store.ts`:
```ts
import { create } from "zustand";
import { send, setMicHotkey } from "./api";
import type { Command, Device, MicConfig, UiState } from "./types";

export interface Toast {
  id: number;
  kind: "error" | "info";
  message: string;
}

interface Store {
  state: UiState | null;
  toasts: Toast[];
  setState: (s: UiState) => void;
  pushToast: (kind: Toast["kind"], message: string) => void;
  dismissToast: (id: number) => void;
}

let nextToastId = 1;

export const useStore = create<Store>((set) => ({
  state: null,
  toasts: [],
  setState: (state) => set({ state }),
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

/** Sends a command; failures become toasts. */
export async function run(cmd: Command): Promise<boolean> {
  try {
    await send(cmd);
    return true;
  } catch (e) {
    useStore.getState().pushToast("error", errorMessage(e));
    return false;
  }
}

/** Registers the hotkey (validated by the backend) and persists it. */
export async function saveMicConfig(config: MicConfig): Promise<boolean> {
  try {
    await setMicHotkey(config);
    return true;
  } catch (e) {
    useStore.getState().pushToast("error", errorMessage(e));
    return false;
  }
}

export const outputs = (s: UiState): Device[] =>
  s.devices.filter((d) => d.flow === "render" && d.state === "active" && !d.hidden);
export const inputs = (s: UiState): Device[] =>
  s.devices.filter((d) => d.flow === "capture" && d.state === "active" && !d.hidden);
export const defaultOutput = (s: UiState): Device | undefined =>
  s.devices.find((d) => d.flow === "render" && d.defaultConsole && d.state === "active");
export const defaultMic = (s: UiState): Device | undefined =>
  s.devices.find((d) => d.flow === "capture" && d.defaultConsole && d.state === "active");
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `npm.cmd test`
Expected: PASS (levels, throttle, meters, hotkey, store — 17 tests).

- [ ] **Step 7: Commit**

```powershell
git add -A
git commit -m "feat(ui): types, API bindings, store, throttle, meter engine, hotkey formatting"
```

---

### Task 11: UI components and visual system

> Before writing styles, invoke the `frontend-design:frontend-design` skill and apply its guidance within this design direction: **"studio hardware on glass"** — dark graphite translucent surfaces over Mica, the Windows accent colour as the only chromatic accent, Segoe UI Variable for text, Cascadia Mono tabular numerals for levels, green→amber→red meters, generous radii, no gratuitous gradients.

**Files:**
- Modify: `src/styles.css` (full visual system)
- Create: `src/components/Slider.tsx`, `Meter.tsx`, `MuteButton.tsx`, `AppIcon.tsx`, `DeviceSelect.tsx`, `AppRow.tsx`, `MicStrip.tsx`, `Toasts.tsx`, `ui.tsx`
- Test: `src/components/AppRow.test.tsx`, `src/components/DeviceSelect.test.tsx`

**Interfaces:**
- Consumes: Task 10 modules.
- Produces components: `Slider({value, onChange, label, orientation?, disabled?, className?})`, `Meter({source, orientation?, className?})`, `MuteButton({muted, onToggle, label, kind?, size?})`, `AppIcon({app, size?})`, `DeviceSelect({app, compact?})`, `AppRow({app})`, `MicStrip({mic})`, `Toasts()`, and from `ui.tsx`: `ViewHeader, Banner, Badge, IconButton, Segmented, Toggle, Field, Logo`.

- [ ] **Step 1: Write the failing component tests**

`src/components/AppRow.test.tsx`:
```tsx
import { fireEvent, render, screen } from "@testing-library/react";
import { AppRow } from "./AppRow";
import { makeApp, makeState } from "../test/fixtures";
import { useStore } from "../lib/store";
import * as api from "../lib/api";

vi.mock("../lib/api", () => ({
  send: vi.fn().mockResolvedValue(undefined),
  getAppIcon: vi.fn().mockResolvedValue(null),
}));

describe("AppRow", () => {
  beforeEach(() => {
    vi.mocked(api.send).mockClear();
    useStore.setState({ state: makeState() });
  });

  it("shows the app name and volume", () => {
    render(<AppRow app={makeApp({ key: "spotify.exe", name: "Spotify", volume: 0.42 })} />);
    expect(screen.getByText("Spotify")).toBeInTheDocument();
    expect(screen.getByText("42")).toBeInTheDocument();
  });

  it("toggles mute", () => {
    render(<AppRow app={makeApp({ key: "spotify.exe", name: "Spotify" })} />);
    fireEvent.click(screen.getByRole("button", { name: "Mute Spotify" }));
    expect(api.send).toHaveBeenCalledWith({ type: "setAppMute", key: "spotify.exe", muted: true });
  });

  it("sends volume changes from the slider", () => {
    render(<AppRow app={makeApp({ key: "spotify.exe", name: "Spotify" })} />);
    fireEvent.change(screen.getByRole("slider", { name: "Spotify volume" }), { target: { value: "40" } });
    expect(api.send).toHaveBeenCalledWith({ type: "setAppVolume", key: "spotify.exe", volume: 0.4 });
  });

  it("shows a letter avatar when there is no icon", () => {
    render(<AppRow app={makeApp({ key: "zoom.exe", name: "Zoom" })} />);
    expect(screen.getByText("Z")).toBeInTheDocument();
  });
});
```

`src/components/DeviceSelect.test.tsx`:
```tsx
import { fireEvent, render, screen } from "@testing-library/react";
import { DeviceSelect } from "./DeviceSelect";
import { makeApp, makeState } from "../test/fixtures";
import { useStore } from "../lib/store";
import * as api from "../lib/api";

vi.mock("../lib/api", () => ({ send: vi.fn().mockResolvedValue(undefined) }));

describe("DeviceSelect", () => {
  beforeEach(() => vi.mocked(api.send).mockClear());

  it("renders nothing when routing is unavailable", () => {
    useStore.setState({ state: makeState({ routingAvailable: false }) });
    const { container } = render(<DeviceSelect app={makeApp({ key: "a.exe" })} />);
    expect(container).toBeEmptyDOMElement();
  });

  it("lists Default plus active outputs, excluding unplugged ones", () => {
    useStore.setState({ state: makeState() });
    render(<DeviceSelect app={makeApp({ key: "a.exe", name: "A" })} />);
    const options = screen.getAllByRole("option").map((o) => o.textContent);
    expect(options).toEqual(["Default · Headphones", "Headphones", "Speakers"]);
  });

  it("routes to a device and back to default", () => {
    useStore.setState({ state: makeState() });
    const { rerender } = render(<DeviceSelect app={makeApp({ key: "a.exe", name: "A" })} />);
    fireEvent.change(screen.getByRole("combobox", { name: "Output device for A" }), { target: { value: "out2" } });
    expect(api.send).toHaveBeenCalledWith({ type: "setAppOutput", key: "a.exe", deviceId: "out2" });
    rerender(<DeviceSelect app={makeApp({ key: "a.exe", name: "A", output: "out2" })} />);
    fireEvent.change(screen.getByRole("combobox", { name: "Output device for A" }), { target: { value: "" } });
    expect(api.send).toHaveBeenLastCalledWith({ type: "setAppOutput", key: "a.exe", deviceId: null });
  });

  it("keeps an unavailable routed device visible", () => {
    useStore.setState({ state: makeState() });
    render(<DeviceSelect app={makeApp({ key: "a.exe", name: "A", output: "gone" })} />);
    expect(screen.getByRole("option", { name: "Old DAC (unavailable)" })).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `npm.cmd test -- src/components`
Expected: FAIL — `Failed to resolve import "./AppRow"`.

- [ ] **Step 3: Write `src/styles.css`**

```css
@import "tailwindcss";

@theme {
  --font-sans: "Segoe UI Variable Text", "Segoe UI", system-ui, sans-serif;
  --font-display: "Segoe UI Variable Display", "Segoe UI", system-ui, sans-serif;
  --font-mono: "Cascadia Mono", Consolas, ui-monospace, monospace;
  --color-base: var(--base);
  --color-surface: var(--surface);
  --color-surface-2: var(--surface-2);
  --color-line: var(--line);
  --color-ink: var(--ink);
  --color-ink-2: var(--ink-2);
  --color-ink-3: var(--ink-3);
  --color-accent: var(--accent);
  --color-danger: var(--danger);
}

:root {
  --accent: #7c5cff;
  --danger: #ff5468;
  --meter-low: #39d98a;
  --meter-mid: #ffc23d;
  --meter-high: #ff5468;
}

:root,
:root[data-theme="dark"] {
  color-scheme: dark;
  --base: rgb(14 15 19 / 0.78);
  --surface: rgb(255 255 255 / 0.04);
  --surface-2: rgb(255 255 255 / 0.08);
  --line: rgb(255 255 255 / 0.07);
  --ink: #f3f4f7;
  --ink-2: #a8adb8;
  --ink-3: #6b707c;
  --track: rgb(255 255 255 / 0.1);
  --thumb: #ffffff;
  --option-bg: #1b1c22;
}

:root[data-theme="light"] {
  color-scheme: light;
  --base: rgb(247 248 251 / 0.84);
  --surface: rgb(0 0 0 / 0.03);
  --surface-2: rgb(0 0 0 / 0.06);
  --line: rgb(0 0 0 / 0.08);
  --ink: #14161b;
  --ink-2: #555b67;
  --ink-3: #8c919c;
  --track: rgb(0 0 0 / 0.1);
  --thumb: #ffffff;
  --option-bg: #ffffff;
}

html,
body,
#root {
  height: 100%;
}

body {
  margin: 0;
  background: var(--base);
  color: var(--ink);
  font-family: var(--font-sans);
  font-size: 14px;
  -webkit-font-smoothing: antialiased;
  user-select: none;
  overflow: hidden;
}

button,
select,
input {
  font: inherit;
  color: inherit;
}

:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 2px;
  border-radius: 6px;
}

.tabular {
  font-family: var(--font-mono);
  font-variant-numeric: tabular-nums;
}

/* ---------- Range slider ---------- */
.rs-slider {
  -webkit-appearance: none;
  appearance: none;
  background: transparent;
  width: 100%;
  height: 20px;
  margin: 0;
  cursor: pointer;
  --fill: 50%;
}
.rs-slider::-webkit-slider-runnable-track {
  height: 4px;
  border-radius: 999px;
  background: linear-gradient(to right, var(--accent) var(--fill), var(--track) var(--fill));
}
.rs-slider::-webkit-slider-thumb {
  -webkit-appearance: none;
  width: 14px;
  height: 14px;
  margin-top: -5px;
  border-radius: 50%;
  background: var(--thumb);
  box-shadow: 0 0 0 3px var(--accent), 0 2px 6px rgb(0 0 0 / 0.35);
  transition: transform 0.12s ease;
}
.rs-slider:hover::-webkit-slider-thumb,
.rs-slider:active::-webkit-slider-thumb {
  transform: scale(1.15);
}
.rs-slider:disabled {
  cursor: default;
  opacity: 0.4;
}
.rs-slider.vertical {
  writing-mode: vertical-lr;
  direction: rtl;
  width: 20px;
  height: 100%;
}
.rs-slider.vertical::-webkit-slider-runnable-track {
  width: 4px;
  height: 100%;
  background: linear-gradient(to top, var(--accent) var(--fill), var(--track) var(--fill));
}
.rs-slider.vertical::-webkit-slider-thumb {
  margin-top: 0;
  margin-left: -5px;
}

/* ---------- Meters ---------- */
.meter-fill {
  background: linear-gradient(to right, var(--meter-low) 0%, var(--meter-low) 65%, var(--meter-mid) 85%, var(--meter-high) 100%);
}
.meter.vertical .meter-fill {
  background: linear-gradient(to top, var(--meter-low) 0%, var(--meter-low) 65%, var(--meter-mid) 85%, var(--meter-high) 100%);
}

/* ---------- Select ---------- */
.rs-select {
  -webkit-appearance: none;
  appearance: none;
  max-width: 100%;
  padding: 5px 26px 5px 10px;
  border: 1px solid var(--line);
  border-radius: 8px;
  background: var(--surface-2)
    url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 6'%3E%3Cpath d='M1 1l4 4 4-4' fill='none' stroke='%23999' stroke-width='1.5'/%3E%3C/svg%3E")
    no-repeat right 9px center / 10px;
  font-size: 12px;
  text-overflow: ellipsis;
  cursor: pointer;
}
.rs-select option {
  background: var(--option-bg);
  color: var(--ink);
}

/* ---------- Scrollbars ---------- */
::-webkit-scrollbar {
  width: 8px;
  height: 8px;
}
::-webkit-scrollbar-thumb {
  background: var(--surface-2);
  border-radius: 8px;
}
::-webkit-scrollbar-track {
  background: transparent;
}
```

- [ ] **Step 4: Write the components**

`src/components/Slider.tsx`:
```tsx
import { useMemo, useRef, useState, type CSSProperties } from "react";
import { throttle } from "../lib/throttle";

interface Props {
  value: number;
  onChange: (value: number) => void;
  label: string;
  orientation?: "horizontal" | "vertical";
  disabled?: boolean;
  className?: string;
}

/** 0..1 slider. While dragging it shows the local value so backend echoes can't make it jump. */
export function Slider({ value, onChange, label, orientation = "horizontal", disabled, className = "" }: Props) {
  const [local, setLocal] = useState<number | null>(null);
  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;
  const emit = useMemo(() => throttle((v: number) => onChangeRef.current(v), 16), []);
  const pct = Math.round((local ?? value) * 100);

  const release = () => {
    if (local !== null) onChangeRef.current(local);
    setLocal(null);
  };

  return (
    <input
      type="range"
      min={0}
      max={100}
      step={1}
      value={pct}
      aria-label={label}
      aria-valuetext={`${pct}%`}
      disabled={disabled}
      className={`rs-slider ${orientation === "vertical" ? "vertical" : ""} ${className}`}
      style={{ "--fill": `${pct}%` } as CSSProperties}
      onChange={(e) => {
        const v = Number(e.currentTarget.value) / 100;
        setLocal(v);
        emit(v);
      }}
      onPointerUp={release}
      onKeyUp={release}
      onBlur={() => setLocal(null)}
    />
  );
}
```

`src/components/Meter.tsx`:
```tsx
import { useEffect, useRef } from "react";
import { registerMeter } from "../lib/meters";

interface Props {
  /** "master", "mic" or "app:<key>" */
  source: string;
  orientation?: "horizontal" | "vertical";
  className?: string;
}

export function Meter({ source, orientation = "horizontal", className = "" }: Props) {
  const fill = useRef<HTMLDivElement>(null);
  const peak = useRef<HTMLDivElement>(null);
  const vertical = orientation === "vertical";

  useEffect(() => {
    if (!fill.current || !peak.current) return;
    return registerMeter(source, vertical, fill.current, peak.current);
  }, [source, vertical]);

  return (
    <div
      aria-hidden
      className={`meter relative overflow-hidden rounded-full bg-[var(--track)] ${vertical ? "vertical h-full w-1.5" : "h-1 w-full"} ${className}`}
    >
      <div
        ref={fill}
        className="meter-fill absolute inset-0"
        style={{ clipPath: vertical ? "inset(100% 0 0 0)" : "inset(0 100% 0 0)" }}
      />
      <div
        ref={peak}
        className={`absolute bg-[var(--ink)] opacity-0 ${vertical ? "inset-x-0 h-px" : "inset-y-0 w-px"}`}
      />
    </div>
  );
}
```

`src/components/MuteButton.tsx`:
```tsx
import { Mic, MicOff, Volume2, VolumeX } from "lucide-react";

interface Props {
  muted: boolean;
  onToggle: () => void;
  label: string;
  kind?: "speaker" | "mic";
  size?: "md" | "lg";
}

export function MuteButton({ muted, onToggle, label, kind = "speaker", size = "md" }: Props) {
  const Icon = kind === "mic" ? (muted ? MicOff : Mic) : muted ? VolumeX : Volume2;
  const dims = size === "lg" ? "h-11 w-11" : "h-8 w-8";
  return (
    <button
      type="button"
      aria-pressed={muted}
      aria-label={`${muted ? "Unmute" : "Mute"} ${label}`}
      title={muted ? "Unmute" : "Mute"}
      onClick={onToggle}
      className={`grid shrink-0 place-items-center rounded-lg transition-colors ${dims} ${
        muted ? "bg-danger/15 text-danger hover:bg-danger/25" : "text-ink-2 hover:bg-surface-2 hover:text-ink"
      }`}
    >
      <Icon size={size === "lg" ? 20 : 16} strokeWidth={2} />
    </button>
  );
}
```

`src/components/AppIcon.tsx`:
```tsx
import { useEffect, useState } from "react";
import { Bell } from "lucide-react";
import { getAppIcon } from "../lib/api";
import type { App } from "../lib/types";

const cache = new Map<string, Promise<string | null>>();

function loadIcon(path: string): Promise<string | null> {
  let p = cache.get(path);
  if (!p) {
    p = getAppIcon(path).catch(() => null);
    cache.set(path, p);
  }
  return p;
}

export function AppIcon({ app, size = 32 }: { app: App; size?: number }) {
  const [src, setSrc] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    if (app.exePath && !app.isSystem) {
      loadIcon(app.exePath).then((b64) => alive && setSrc(b64 ? `data:image/png;base64,${b64}` : null));
    }
    return () => {
      alive = false;
    };
  }, [app.exePath, app.isSystem]);

  const box = { width: size, height: size };
  if (app.isSystem) {
    return (
      <div style={box} className="grid shrink-0 place-items-center rounded-lg bg-surface-2 text-ink-2">
        <Bell size={size * 0.5} />
      </div>
    );
  }
  if (src) return <img src={src} style={box} alt="" draggable={false} className="shrink-0 rounded-md" />;
  return (
    <div
      style={{ ...box, fontSize: size * 0.45 }}
      className="grid shrink-0 place-items-center rounded-lg bg-accent/20 font-semibold text-accent"
    >
      {app.name.charAt(0).toUpperCase()}
    </div>
  );
}
```

`src/components/DeviceSelect.tsx`:
```tsx
import { outputs, run, useStore } from "../lib/store";
import type { App } from "../lib/types";

export function DeviceSelect({ app, compact = false }: { app: App; compact?: boolean }) {
  const state = useStore((s) => s.state);
  if (!state?.routingAvailable) return null;
  const list = outputs(state);
  const def = list.find((d) => d.defaultConsole);
  const routedElsewhere = app.output && !list.some((d) => d.id === app.output);
  const missing = routedElsewhere ? state.devices.find((d) => d.id === app.output) : undefined;

  return (
    <select
      aria-label={`Output device for ${app.name}`}
      title="Output device"
      value={app.output ?? ""}
      onChange={(e) => run({ type: "setAppOutput", key: app.key, deviceId: e.currentTarget.value || null })}
      className={`rs-select ${compact ? "w-full" : "w-44"}`}
    >
      <option value="">{def ? `Default · ${def.name}` : "Default"}</option>
      {list.map((d) => (
        <option key={d.id} value={d.id}>
          {d.name}
        </option>
      ))}
      {routedElsewhere && <option value={app.output!}>{`${missing?.name ?? "Device"} (unavailable)`}</option>}
    </select>
  );
}
```

`src/components/AppRow.tsx`:
```tsx
import { run } from "../lib/store";
import type { App } from "../lib/types";
import { AppIcon } from "./AppIcon";
import { Meter } from "./Meter";
import { MuteButton } from "./MuteButton";
import { Slider } from "./Slider";

export function AppRow({ app }: { app: App }) {
  return (
    <div className="group flex items-center gap-3 rounded-xl px-2.5 py-2 transition-colors hover:bg-surface">
      <AppIcon app={app} size={28} />
      <div className="min-w-0 flex-1">
        <div className="flex items-baseline justify-between gap-2">
          <span className={`truncate text-[13px] ${app.muted ? "text-ink-3" : ""}`}>{app.name}</span>
          <span className="tabular text-[11px] text-ink-3">{Math.round(app.volume * 100)}</span>
        </div>
        <Slider
          value={app.volume}
          label={`${app.name} volume`}
          onChange={(v) => run({ type: "setAppVolume", key: app.key, volume: v })}
        />
        <Meter source={`app:${app.key}`} />
      </div>
      <MuteButton muted={app.muted} label={app.name} onToggle={() => run({ type: "setAppMute", key: app.key, muted: !app.muted })} />
    </div>
  );
}
```

`src/components/MicStrip.tsx`:
```tsx
import { run } from "../lib/store";
import type { Device } from "../lib/types";
import { Meter } from "./Meter";
import { MuteButton } from "./MuteButton";
import { Slider } from "./Slider";

/** Compact mic control used in the flyout. */
export function MicStrip({ mic }: { mic: Device }) {
  return (
    <section className={`rounded-2xl border p-3 ${mic.muted ? "border-danger/40 bg-danger/10" : "border-line bg-surface"}`}>
      <div className="mb-1.5 flex items-center justify-between text-[11px] uppercase tracking-wider text-ink-3">
        <span>Microphone</span>
        <span className="tabular normal-case">{mic.muted ? "Muted" : `${Math.round(mic.volume * 100)}%`}</span>
      </div>
      <div className="flex items-center gap-2">
        <MuteButton kind="mic" muted={mic.muted} label="microphone" onToggle={() => run({ type: "setMicMute", muted: !mic.muted })} />
        <div className="min-w-0 flex-1">
          <div className="truncate text-[13px]">{mic.name}</div>
          <Slider value={mic.volume} label="Microphone level" onChange={(v) => run({ type: "setDeviceVolume", id: mic.id, volume: v })} />
          <Meter source="mic" />
        </div>
      </div>
    </section>
  );
}
```

`src/components/Toasts.tsx`:
```tsx
import { useEffect } from "react";
import { AlertTriangle, Info, X } from "lucide-react";
import { useStore, type Toast } from "../lib/store";

function ToastItem({ toast }: { toast: Toast }) {
  const dismiss = useStore((s) => s.dismissToast);
  useEffect(() => {
    const t = setTimeout(() => dismiss(toast.id), 6000);
    return () => clearTimeout(t);
  }, [toast.id, dismiss]);
  const Icon = toast.kind === "error" ? AlertTriangle : Info;
  return (
    <div
      role={toast.kind === "error" ? "alert" : "status"}
      className="pointer-events-auto flex items-start gap-3 rounded-xl border border-line bg-[var(--option-bg)] px-4 py-3 text-[13px] shadow-2xl"
    >
      <Icon size={16} className={toast.kind === "error" ? "mt-0.5 text-danger" : "mt-0.5 text-accent"} />
      <span className="flex-1">{toast.message}</span>
      <button type="button" aria-label="Dismiss" onClick={() => dismiss(toast.id)} className="text-ink-3 hover:text-ink">
        <X size={14} />
      </button>
    </div>
  );
}

export function Toasts() {
  const toasts = useStore((s) => s.toasts);
  return (
    <div className="pointer-events-none fixed inset-x-3 bottom-3 z-50 flex flex-col items-end gap-2">
      {toasts.map((t) => (
        <div key={t.id} className="w-full max-w-sm">
          <ToastItem toast={t} />
        </div>
      ))}
    </div>
  );
}
```

`src/components/ui.tsx`:
```tsx
import type { ReactNode } from "react";

export function Logo({ size = 18 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" aria-hidden>
      <circle cx="12" cy="12" r="2.6" fill="var(--accent)" />
      <circle cx="12" cy="12" r="6" fill="none" stroke="var(--accent)" strokeWidth="1.6" />
      <circle cx="12" cy="12" r="9.6" fill="none" stroke="currentColor" strokeOpacity="0.55" strokeWidth="1.4" />
    </svg>
  );
}

export function ViewHeader({ title, subtitle, children }: { title: string; subtitle?: string; children?: ReactNode }) {
  return (
    <div className="flex items-end justify-between gap-4">
      <div>
        <h1 className="font-display text-2xl font-semibold tracking-tight">{title}</h1>
        {subtitle && <p className="mt-0.5 text-[13px] text-ink-2">{subtitle}</p>}
      </div>
      {children}
    </div>
  );
}

export function Banner({ children }: { children: ReactNode }) {
  return <div className="rounded-xl border border-line bg-surface-2 px-4 py-3 text-[13px] text-ink-2">{children}</div>;
}

export function Badge({ children, tone = "neutral" }: { children: ReactNode; tone?: "neutral" | "accent" | "muted" }) {
  const tones = {
    neutral: "bg-surface-2 text-ink-2",
    accent: "bg-accent/20 text-accent",
    muted: "bg-surface text-ink-3",
  };
  return <span className={`rounded-md px-1.5 py-0.5 text-[11px] font-medium ${tones[tone]}`}>{children}</span>;
}

export function IconButton({ label, onClick, children }: { label: string; onClick: () => void; children: ReactNode }) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      onClick={onClick}
      className="grid h-8 w-8 place-items-center rounded-lg text-ink-2 transition-colors hover:bg-surface-2 hover:text-ink"
    >
      {children}
    </button>
  );
}

export function Segmented<T extends string | number>({
  value,
  options,
  onChange,
  label,
}: {
  value: T;
  options: { value: T; label: string }[];
  onChange: (v: T) => void;
  label: string;
}) {
  return (
    <div role="radiogroup" aria-label={label} className="inline-flex rounded-lg border border-line bg-surface p-0.5">
      {options.map((o) => (
        <button
          key={String(o.value)}
          type="button"
          role="radio"
          aria-checked={o.value === value}
          onClick={() => onChange(o.value)}
          className={`rounded-md px-3 py-1 text-[13px] transition-colors ${
            o.value === value ? "bg-accent text-white shadow" : "text-ink-2 hover:text-ink"
          }`}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

export function Toggle({ checked, onChange, label }: { checked: boolean; onChange: (v: boolean) => void; label: string }) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      onClick={() => onChange(!checked)}
      className={`relative h-6 w-11 rounded-full transition-colors ${checked ? "bg-accent" : "bg-surface-2"}`}
    >
      <span
        className={`absolute top-1 h-4 w-4 rounded-full bg-white shadow transition-transform ${checked ? "translate-x-6" : "translate-x-1"}`}
      />
    </button>
  );
}

export function Field({ label, hint, children }: { label: string; hint?: string; children: ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-6 border-b border-line py-4 last:border-b-0">
      <div>
        <div className="text-[14px] font-medium">{label}</div>
        {hint && <div className="mt-0.5 text-[12px] text-ink-3">{hint}</div>}
      </div>
      <div className="shrink-0">{children}</div>
    </div>
  );
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `npm.cmd test`
Expected: PASS (all previous + 8 component tests).

- [ ] **Step 6: Commit**

```powershell
git add -A
git commit -m "feat(ui): visual system and core components"
```

---

### Task 12: App shell, bootstrap, theme and the tray flyout

**Files:**
- Create: `src/lib/bootstrap.ts`, `src/lib/theme.ts`, `src/views/Flyout.tsx`
- Modify: `src/App.tsx`
- Test: `src/views/Flyout.test.tsx`

**Interfaces:**
- Consumes: Tasks 10–11.
- Produces: `useBootstrap()`, `useTheme(state)`, `<Flyout />`; `App` routes `?view=flyout` to the flyout, everything else to `MainWindow` (Task 13 — until then App imports a temporary placeholder; Step 5 shows it).

- [ ] **Step 1: Write the failing Flyout test**

`src/views/Flyout.test.tsx`:
```tsx
import { fireEvent, render, screen } from "@testing-library/react";
import { Flyout } from "./Flyout";
import { makeState } from "../test/fixtures";
import { useStore } from "../lib/store";
import * as api from "../lib/api";

vi.mock("../lib/api", () => ({
  send: vi.fn().mockResolvedValue(undefined),
  getAppIcon: vi.fn().mockResolvedValue(null),
  showMain: vi.fn().mockResolvedValue(undefined),
}));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => ({ hide: vi.fn() }) }));

describe("Flyout", () => {
  it("shows a loading state before the first state arrives", () => {
    useStore.setState({ state: null });
    render(<Flyout />);
    expect(screen.getByText(/connecting/i)).toBeInTheDocument();
  });

  it("shows master device, mic and apps", () => {
    useStore.setState({ state: makeState() });
    render(<Flyout />);
    expect(screen.getByText("Headphones")).toBeInTheDocument();
    expect(screen.getByText("Desk Mic")).toBeInTheDocument();
    expect(screen.getByText("Spotify")).toBeInTheDocument();
    expect(screen.getByText("System sounds")).toBeInTheDocument();
  });

  it("mutes the master device and opens the main window", () => {
    useStore.setState({ state: makeState() });
    render(<Flyout />);
    fireEvent.click(screen.getByRole("button", { name: "Mute Headphones" }));
    expect(api.send).toHaveBeenCalledWith({ type: "setDeviceMute", id: "out1", muted: true });
    fireEvent.click(screen.getByRole("button", { name: "Open Resonance" }));
    expect(api.showMain).toHaveBeenCalled();
  });

  it("explains when nothing is playing", () => {
    useStore.setState({ state: makeState({ apps: [] }) });
    render(<Flyout />);
    expect(screen.getByText(/no apps are playing audio/i)).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run to verify failure**

Run: `npm.cmd test -- src/views`
Expected: FAIL — `Failed to resolve import "./Flyout"`.

- [ ] **Step 3: Implement `bootstrap.ts`, `theme.ts`, `Flyout.tsx`**

`src/lib/bootstrap.ts`:
```ts
import { useEffect } from "react";
import { getState, onMeters, onNotice, onState, takeNotices } from "./api";
import { ingestFrame } from "./meters";
import { errorMessage, useStore } from "./store";

/** Loads the initial state and subscribes to backend events for this window. */
export function useBootstrap() {
  useEffect(() => {
    let disposed = false;
    const { setState, pushToast } = useStore.getState();
    const unlisteners = [
      onState((s) => setState(s)),
      onMeters(ingestFrame),
      onNotice((e) => pushToast("error", e.message)),
    ];
    getState()
      .then((s) => !disposed && setState(s))
      .catch((e) => pushToast("error", errorMessage(e)));
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

`src/lib/theme.ts`:
```ts
import { useEffect } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { UiState } from "./types";

/** Applies light/dark (system or forced) and the Windows accent colour. */
export function useTheme(state: UiState | null) {
  const pref = state?.ui.theme ?? "system";
  const accent = state?.accent;

  useEffect(() => {
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const apply = () => {
      const dark = pref === "dark" || (pref === "system" && mq.matches);
      document.documentElement.dataset.theme = dark ? "dark" : "light";
    };
    apply();
    try {
      getCurrentWindow()
        .setTheme(pref === "system" ? null : pref)
        .catch(() => {});
    } catch {
      /* not running inside Tauri (tests) */
    }
    mq.addEventListener("change", apply);
    return () => mq.removeEventListener("change", apply);
  }, [pref]);

  useEffect(() => {
    if (accent) document.documentElement.style.setProperty("--accent", accent);
  }, [accent]);
}
```

`src/views/Flyout.tsx`:
```tsx
import { useEffect } from "react";
import { Maximize2 } from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { showMain } from "../lib/api";
import { defaultMic, defaultOutput, run, useStore } from "../lib/store";
import { AppRow } from "../components/AppRow";
import { Meter } from "../components/Meter";
import { MicStrip } from "../components/MicStrip";
import { MuteButton } from "../components/MuteButton";
import { Slider } from "../components/Slider";
import { Toasts } from "../components/Toasts";
import { Logo } from "../components/ui";

export function Flyout() {
  const state = useStore((s) => s.state);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") getCurrentWindow().hide();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  if (!state) {
    return <div className="grid h-full place-items-center text-[13px] text-ink-3">Connecting to audio…</div>;
  }
  const out = defaultOutput(state);
  const mic = defaultMic(state);

  return (
    <div className="flex h-full flex-col gap-2.5 p-3">
      <header className="flex items-center justify-between px-1 pt-0.5">
        <div className="flex items-center gap-2 text-[13px] font-semibold tracking-wide">
          <Logo size={16} /> Resonance
        </div>
        <button
          type="button"
          aria-label="Open Resonance"
          title="Open Resonance"
          onClick={() => showMain()}
          className="grid h-7 w-7 place-items-center rounded-lg text-ink-2 hover:bg-surface-2 hover:text-ink"
        >
          <Maximize2 size={14} />
        </button>
      </header>

      {out && (
        <section className="rounded-2xl border border-line bg-surface-2 p-3">
          <div className="mb-1.5 flex items-center justify-between text-[11px] uppercase tracking-wider text-ink-3">
            <span>Output</span>
            <span className="tabular normal-case">{Math.round(out.volume * 100)}%</span>
          </div>
          <div className="flex items-center gap-2">
            <MuteButton
              muted={out.muted}
              label={out.name}
              onToggle={() => run({ type: "setDeviceMute", id: out.id, muted: !out.muted })}
            />
            <div className="min-w-0 flex-1">
              <div className="truncate text-[13px]">{out.name}</div>
              <Slider
                value={out.volume}
                label={`${out.name} volume`}
                onChange={(v) => run({ type: "setDeviceVolume", id: out.id, volume: v })}
              />
              <Meter source="master" />
            </div>
          </div>
        </section>
      )}

      {mic && <MicStrip mic={mic} />}

      <section className="min-h-0 flex-1 overflow-y-auto rounded-2xl border border-line bg-surface p-1">
        {state.apps.length === 0 ? (
          <p className="px-4 py-8 text-center text-[13px] text-ink-3">No apps are playing audio right now.</p>
        ) : (
          state.apps.map((a) => <AppRow key={a.key} app={a} />)
        )}
      </section>
      <Toasts />
    </div>
  );
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `npm.cmd test`
Expected: PASS.

- [ ] **Step 5: Wire `App.tsx` (temporary main view until Task 13)**

```tsx
import { useBootstrap } from "./lib/bootstrap";
import { useStore } from "./lib/store";
import { useTheme } from "./lib/theme";
import { Flyout } from "./views/Flyout";

const isFlyout = new URLSearchParams(window.location.search).get("view") === "flyout";

export function App() {
  useBootstrap();
  const state = useStore((s) => s.state);
  useTheme(state);
  return isFlyout ? <Flyout /> : <div className="p-6">Main window — Task 13</div>;
}
```

- [ ] **Step 6: See it running**

Run: `npm.cmd run tauri dev`, play audio in any app, left-click the tray icon.
Expected: the flyout shows your default output, your mic, and the playing app with a moving meter; dragging a slider changes that app's volume (verify in the Windows volume mixer); muting the mic turns its strip red and the tray icon gets the red badge.

- [ ] **Step 7: Commit**

```powershell
git add -A
git commit -m "feat(ui): bootstrap, theming and tray flyout"
```

---

### Task 13: Main window — titlebar, sidebar, Mixer, Devices, Microphone, Settings

**Files:**
- Create: `src/views/MainWindow.tsx`, `Titlebar.tsx`, `Sidebar.tsx`, `MixerView.tsx`, `ChannelStrip.tsx`, `DevicesView.tsx`, `MicView.tsx`, `SettingsView.tsx`, `src/components/HotkeyRecorder.tsx`
- Modify: `src/App.tsx`
- Test: `src/views/MixerView.test.tsx`, `src/views/DevicesView.test.tsx`, `src/views/MicView.test.tsx`

**Interfaces:**
- Consumes: everything above.
- Produces: `<MainWindow />` with views `"mixer" | "devices" | "mic" | "settings"`.

- [ ] **Step 1: Write the failing view tests**

`src/views/MixerView.test.tsx`:
```tsx
import { fireEvent, render, screen } from "@testing-library/react";
import { MixerView } from "./MixerView";
import { makeState } from "../test/fixtures";
import { useStore } from "../lib/store";
import * as api from "../lib/api";

vi.mock("../lib/api", () => ({
  send: vi.fn().mockResolvedValue(undefined),
  getAppIcon: vi.fn().mockResolvedValue(null),
}));

describe("MixerView", () => {
  it("renders a master strip and one strip per app", () => {
    const s = makeState();
    useStore.setState({ state: s });
    render(<MixerView state={s} />);
    expect(screen.getByText("Master")).toBeInTheDocument();
    expect(screen.getByRole("slider", { name: "Spotify volume" })).toBeInTheDocument();
    expect(screen.getByRole("slider", { name: "System sounds volume" })).toBeInTheDocument();
  });

  it("only offers output routing for non-system apps", () => {
    const s = makeState();
    useStore.setState({ state: s });
    render(<MixerView state={s} />);
    expect(screen.getByRole("combobox", { name: "Output device for Spotify" })).toBeInTheDocument();
    expect(screen.queryByRole("combobox", { name: "Output device for System sounds" })).toBeNull();
  });

  it("explains when routing is unavailable", () => {
    const s = makeState({ routingAvailable: false });
    useStore.setState({ state: s });
    render(<MixerView state={s} />);
    expect(screen.getByText(/isn't available on this Windows build/i)).toBeInTheDocument();
  });

  it("master strip controls the default device", () => {
    const s = makeState();
    useStore.setState({ state: s });
    render(<MixerView state={s} />);
    fireEvent.click(screen.getByRole("button", { name: "Mute Headphones" }));
    expect(api.send).toHaveBeenCalledWith({ type: "setDeviceMute", id: "out1", muted: true });
  });
});
```

`src/views/DevicesView.test.tsx`:
```tsx
import { fireEvent, render, screen } from "@testing-library/react";
import { DevicesView } from "./DevicesView";
import { makeDevice, makeState } from "../test/fixtures";
import * as api from "../lib/api";

vi.mock("../lib/api", () => ({ send: vi.fn().mockResolvedValue(undefined) }));

describe("DevicesView", () => {
  beforeEach(() => vi.mocked(api.send).mockClear());

  it("groups outputs and inputs and labels defaults and unplugged devices", () => {
    render(<DevicesView state={makeState()} />);
    expect(screen.getByRole("heading", { name: "Outputs" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Inputs" })).toBeInTheDocument();
    expect(screen.getAllByText("Default").length).toBeGreaterThan(0);
    expect(screen.getByText("Unplugged")).toBeInTheDocument();
  });

  it("sets default and communications devices", () => {
    render(<DevicesView state={makeState()} />);
    fireEvent.click(screen.getByRole("button", { name: "Set Speakers as default" }));
    expect(api.send).toHaveBeenCalledWith({ type: "setDefaultDevice", id: "out2", role: "console" });
    fireEvent.click(screen.getByRole("button", { name: "Set Speakers as communications device" }));
    expect(api.send).toHaveBeenCalledWith({ type: "setDefaultDevice", id: "out2", role: "communications" });
  });

  it("renames a device with Enter", () => {
    render(<DevicesView state={makeState()} />);
    fireEvent.click(screen.getByRole("button", { name: "Rename Speakers" }));
    const input = screen.getByRole("textbox", { name: "Device name" });
    fireEvent.change(input, { target: { value: "Desk speakers" } });
    fireEvent.submit(input.closest("form")!);
    expect(api.send).toHaveBeenCalledWith({ type: "setDeviceAlias", id: "out2", alias: "Desk speakers" });
  });

  it("collapses hidden devices behind a toggle", () => {
    const s = makeState();
    s.devices.push(makeDevice({ id: "h", name: "Virtual Cable", hidden: true }));
    render(<DevicesView state={s} />);
    expect(screen.queryByText("Virtual Cable")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: /show 1 hidden/i }));
    expect(screen.getByText("Virtual Cable")).toBeInTheDocument();
  });
});
```

`src/views/MicView.test.tsx`:
```tsx
import { fireEvent, render, screen } from "@testing-library/react";
import { MicView } from "./MicView";
import { makeDevice, makeState } from "../test/fixtures";
import * as api from "../lib/api";

vi.mock("../lib/api", () => ({
  send: vi.fn().mockResolvedValue(undefined),
  setMicHotkey: vi.fn().mockResolvedValue(undefined),
}));

describe("MicView", () => {
  beforeEach(() => {
    vi.mocked(api.send).mockClear();
    vi.mocked(api.setMicHotkey).mockClear();
  });

  it("explains when there is no microphone", () => {
    const s = makeState();
    s.devices = s.devices.filter((d) => d.flow === "render");
    render(<MicView state={s} />);
    expect(screen.getByText(/no microphone found/i)).toBeInTheDocument();
  });

  it("mutes the mic and shows the muted warning", () => {
    const s = makeState();
    const { rerender } = render(<MicView state={s} />);
    fireEvent.click(screen.getByRole("button", { name: "Mute microphone" }));
    expect(api.send).toHaveBeenCalledWith({ type: "setMicMute", muted: true });
    s.devices = s.devices.map((d) => (d.id === "mic1" ? { ...d, muted: true } : d));
    rerender(<MicView state={{ ...s }} />);
    expect(screen.getByText(/your microphone is muted/i)).toBeInTheDocument();
  });

  it("offers boost steps when the driver supports boost", () => {
    const s = makeState();
    s.devices = s.devices.map((d) => (d.id === "mic1" ? { ...d, boost: { db: 10, min: 0, max: 30, step: 10 } } : d));
    render(<MicView state={s} />);
    fireEvent.click(screen.getByRole("radio", { name: "+20 dB" }));
    expect(api.send).toHaveBeenCalledWith({ type: "setMicBoost", id: "mic1", db: 20 });
  });

  it("changes hotkey mode and records a new hotkey", () => {
    render(<MicView state={makeState()} />);
    fireEvent.click(screen.getByRole("radio", { name: "Push to talk" }));
    expect(api.setMicHotkey).toHaveBeenCalledWith({ hotkey: "Ctrl+Shift+M", mode: "pushToTalk" });
    fireEvent.click(screen.getByRole("button", { name: /ctrl/i }));
    fireEvent.keyDown(window, { code: "F13", key: "F13" });
    expect(api.setMicHotkey).toHaveBeenLastCalledWith({ hotkey: "F13", mode: "toggle" });
  });

  it("switches the default input device", () => {
    const s = makeState();
    s.devices.push(makeDevice({ id: "mic2", name: "Headset Mic", flow: "capture" }));
    render(<MicView state={s} />);
    fireEvent.change(screen.getByRole("combobox", { name: "Input device" }), { target: { value: "mic2" } });
    expect(api.send).toHaveBeenCalledWith({ type: "setDefaultDevice", id: "mic2", role: "console" });
  });
});
```

- [ ] **Step 2: Run to verify failure**

Run: `npm.cmd test -- src/views`
Expected: FAIL — unresolved `./MixerView`, `./DevicesView`, `./MicView`.

- [ ] **Step 3: Implement the views**

`src/views/ChannelStrip.tsx`:
```tsx
import { SlidersVertical } from "lucide-react";
import { run } from "../lib/store";
import type { App, Device } from "../lib/types";
import { AppIcon } from "../components/AppIcon";
import { DeviceSelect } from "../components/DeviceSelect";
import { Meter } from "../components/Meter";
import { MuteButton } from "../components/MuteButton";
import { Slider } from "../components/Slider";

type Props = { kind: "master"; device: Device } | { kind: "app"; app: App };

interface Channel {
  name: string;
  caption: string;
  volume: number;
  muted: boolean;
  source: string;
  setVolume: (v: number) => void;
  setMuted: (m: boolean) => void;
}

function channelFor(props: Props): Channel {
  if (props.kind === "master") {
    const d = props.device;
    return {
      name: d.name,
      caption: "Master",
      volume: d.volume,
      muted: d.muted,
      source: "master",
      setVolume: (v) => run({ type: "setDeviceVolume", id: d.id, volume: v }),
      setMuted: (m) => run({ type: "setDeviceMute", id: d.id, muted: m }),
    };
  }
  const a = props.app;
  return {
    name: a.name,
    caption: a.isSystem ? "System" : "App",
    volume: a.volume,
    muted: a.muted,
    source: `app:${a.key}`,
    setVolume: (v) => run({ type: "setAppVolume", key: a.key, volume: v }),
    setMuted: (m) => run({ type: "setAppMute", key: a.key, muted: m }),
  };
}

export function ChannelStrip(props: Props) {
  const ch = channelFor(props);
  const isMaster = props.kind === "master";
  return (
    <div
      className={`flex w-32 shrink-0 flex-col items-center gap-3 rounded-2xl border p-3 ${
        isMaster ? "border-accent/30 bg-surface-2" : "border-line bg-surface"
      }`}
    >
      <div className="flex h-10 items-center">
        {props.kind === "master" ? (
          <div className="grid h-9 w-9 place-items-center rounded-lg bg-accent/20 text-accent">
            <SlidersVertical size={18} />
          </div>
        ) : (
          <AppIcon app={props.app} size={36} />
        )}
      </div>
      <div className="w-full text-center">
        <div className="truncate text-[13px] font-medium" title={ch.name}>
          {ch.name}
        </div>
        <div className="text-[10px] uppercase tracking-[0.14em] text-ink-3">{ch.caption}</div>
      </div>
      <div className="flex min-h-40 flex-1 items-stretch gap-3 py-1">
        <Slider orientation="vertical" value={ch.volume} label={`${ch.name} volume`} onChange={ch.setVolume} />
        <Meter orientation="vertical" source={ch.source} />
      </div>
      <div className={`tabular text-sm ${ch.muted ? "text-danger" : "text-ink-2"}`}>{Math.round(ch.volume * 100)}</div>
      <MuteButton muted={ch.muted} label={ch.name} onToggle={() => ch.setMuted(!ch.muted)} />
      {props.kind === "app" && !props.app.isSystem && <DeviceSelect app={props.app} compact />}
    </div>
  );
}
```

`src/views/MixerView.tsx`:
```tsx
import { defaultOutput } from "../lib/store";
import type { UiState } from "../lib/types";
import { Banner, ViewHeader } from "../components/ui";
import { ChannelStrip } from "./ChannelStrip";

export function MixerView({ state }: { state: UiState }) {
  const out = defaultOutput(state);
  return (
    <div className="flex h-full flex-col gap-4">
      <ViewHeader title="Mixer" subtitle={out ? `Playing on ${out.name}` : "No output device"} />
      {!state.routingAvailable && (
        <Banner>Per-app output selection isn't available on this Windows build. Volume and mute still work.</Banner>
      )}
      <div className="flex min-h-0 flex-1 gap-3 overflow-x-auto pb-2">
        {out && <ChannelStrip kind="master" device={out} />}
        <div className="w-px shrink-0 bg-line" />
        {state.apps.length === 0 ? (
          <div className="grid flex-1 place-items-center text-[13px] text-ink-3">
            Apps appear here as soon as they play sound.
          </div>
        ) : (
          state.apps.map((a) => <ChannelStrip key={a.key} kind="app" app={a} />)
        )}
      </div>
    </div>
  );
}
```

`src/views/DevicesView.tsx`:
```tsx
import { useState } from "react";
import { Eye, EyeOff, Mic, Pencil, Phone, Speaker, Star } from "lucide-react";
import { run } from "../lib/store";
import type { Device, UiState } from "../lib/types";
import { MuteButton } from "../components/MuteButton";
import { Slider } from "../components/Slider";
import { Badge, IconButton, ViewHeader } from "../components/ui";

function DeviceCard({ device: d }: { device: Device }) {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(d.alias ?? d.name);
  const active = d.state === "active";

  return (
    <div className={`flex items-center gap-4 rounded-xl border border-line bg-surface px-4 py-3 ${active ? "" : "opacity-55"}`}>
      <div className="grid h-9 w-9 shrink-0 place-items-center rounded-lg bg-surface-2 text-ink-2">
        {d.flow === "capture" ? <Mic size={18} /> : <Speaker size={18} />}
      </div>
      <div className="min-w-0 flex-1">
        {editing ? (
          <form
            onSubmit={(e) => {
              e.preventDefault();
              run({ type: "setDeviceAlias", id: d.id, alias: draft });
              setEditing(false);
            }}
          >
            <input
              autoFocus
              aria-label="Device name"
              value={draft}
              onChange={(e) => setDraft(e.target.value)}
              onKeyDown={(e) => e.key === "Escape" && setEditing(false)}
              onBlur={() => setEditing(false)}
              placeholder={d.originalName}
              className="w-full rounded-md border border-line bg-surface-2 px-2 py-1 text-[14px] select-text"
            />
          </form>
        ) : (
          <div className="flex items-baseline gap-2">
            <span className="truncate font-medium">{d.name}</span>
            {d.alias && <span className="truncate text-[12px] text-ink-3">{d.originalName}</span>}
          </div>
        )}
        <div className="mt-1 flex flex-wrap gap-1.5">
          {d.defaultConsole && <Badge tone="accent">Default</Badge>}
          {d.defaultComms && <Badge>Communications</Badge>}
          {!active && <Badge tone="muted">Unplugged</Badge>}
        </div>
      </div>
      {active && (
        <div className="flex w-60 items-center gap-2">
          <MuteButton
            kind={d.flow === "capture" ? "mic" : "speaker"}
            muted={d.muted}
            label={d.name}
            onToggle={() => run({ type: "setDeviceMute", id: d.id, muted: !d.muted })}
          />
          <Slider value={d.volume} label={`${d.name} volume`} onChange={(v) => run({ type: "setDeviceVolume", id: d.id, volume: v })} />
          <span className="tabular w-8 text-right text-[12px] text-ink-2">{Math.round(d.volume * 100)}</span>
        </div>
      )}
      <div className="flex items-center gap-0.5">
        {active && !d.defaultConsole && (
          <IconButton label={`Set ${d.name} as default`} onClick={() => run({ type: "setDefaultDevice", id: d.id, role: "console" })}>
            <Star size={16} />
          </IconButton>
        )}
        {active && !d.defaultComms && (
          <IconButton
            label={`Set ${d.name} as communications device`}
            onClick={() => run({ type: "setDefaultDevice", id: d.id, role: "communications" })}
          >
            <Phone size={16} />
          </IconButton>
        )}
        <IconButton
          label={`Rename ${d.name}`}
          onClick={() => {
            setDraft(d.alias ?? d.name);
            setEditing(true);
          }}
        >
          <Pencil size={16} />
        </IconButton>
        <IconButton
          label={d.hidden ? `Show ${d.name}` : `Hide ${d.name}`}
          onClick={() => run({ type: "setDeviceHidden", id: d.id, hidden: !d.hidden })}
        >
          {d.hidden ? <Eye size={16} /> : <EyeOff size={16} />}
        </IconButton>
      </div>
    </div>
  );
}

function DeviceSection({ title, devices }: { title: string; devices: Device[] }) {
  const [showHidden, setShowHidden] = useState(false);
  const visible = devices.filter((d) => !d.hidden);
  const hidden = devices.filter((d) => d.hidden);
  return (
    <section className="flex flex-col gap-2">
      <h2 className="text-[11px] font-semibold uppercase tracking-[0.14em] text-ink-3">{title}</h2>
      {visible.map((d) => (
        <DeviceCard key={d.id} device={d} />
      ))}
      {visible.length === 0 && <p className="text-[13px] text-ink-3">None connected.</p>}
      {hidden.length > 0 && (
        <button type="button" onClick={() => setShowHidden(!showHidden)} className="self-start text-[12px] text-ink-3 hover:text-ink">
          {showHidden ? "Collapse hidden devices" : `Show ${hidden.length} hidden`}
        </button>
      )}
      {showHidden && hidden.map((d) => <DeviceCard key={d.id} device={d} />)}
    </section>
  );
}

export function DevicesView({ state }: { state: UiState }) {
  return (
    <div className="flex max-w-5xl flex-col gap-6">
      <ViewHeader title="Devices" subtitle="Defaults, names and levels for every output and input" />
      <DeviceSection title="Outputs" devices={state.devices.filter((d) => d.flow === "render")} />
      <DeviceSection title="Inputs" devices={state.devices.filter((d) => d.flow === "capture")} />
    </div>
  );
}
```

`src/components/HotkeyRecorder.tsx`:
```tsx
import { useEffect, useState } from "react";
import { formatHotkey } from "../lib/hotkey";
import { saveMicConfig } from "../lib/store";
import type { MicConfig } from "../lib/types";

function Keys({ combo }: { combo: string }) {
  return (
    <span className="flex gap-1">
      {combo.split("+").map((k) => (
        <kbd key={k} className="rounded-md border border-line bg-surface-2 px-1.5 py-0.5 font-mono text-[12px]">
          {k}
        </kbd>
      ))}
    </span>
  );
}

export function HotkeyRecorder({ config }: { config: MicConfig }) {
  const [recording, setRecording] = useState(false);

  useEffect(() => {
    if (!recording) return;
    const onKey = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      if (e.key === "Escape") return setRecording(false);
      if (e.key === "Backspace" || e.key === "Delete") {
        setRecording(false);
        saveMicConfig({ ...config, hotkey: null });
        return;
      }
      const hk = formatHotkey(e);
      if (hk) {
        setRecording(false);
        saveMicConfig({ ...config, hotkey: hk });
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [recording, config]);

  return (
    <div className="flex flex-col items-end gap-1">
      <button
        type="button"
        onClick={() => setRecording((r) => !r)}
        aria-label={recording ? "Recording hotkey" : config.hotkey ? `Hotkey ${config.hotkey}, click to change` : "Set hotkey"}
        className={`min-w-44 rounded-lg border px-3 py-1.5 text-[13px] transition-colors ${
          recording ? "border-accent bg-accent/15 text-ink" : "border-line bg-surface hover:bg-surface-2"
        }`}
      >
        {recording ? "Press a key combination…" : config.hotkey ? <Keys combo={config.hotkey} /> : "Not set"}
      </button>
      {recording && <span className="text-[11px] text-ink-3">Esc to cancel · Backspace to clear</span>}
    </div>
  );
}
```

`src/views/MicView.tsx`:
```tsx
import { defaultMic, run, saveMicConfig } from "../lib/store";
import type { Device, HotkeyMode, UiState } from "../lib/types";
import { HotkeyRecorder } from "../components/HotkeyRecorder";
import { Meter } from "../components/Meter";
import { MuteButton } from "../components/MuteButton";
import { Slider } from "../components/Slider";
import { Banner, Field, Segmented, ViewHeader } from "../components/ui";

function BoostControl({ mic }: { mic: Device }) {
  const b = mic.boost!;
  const steps: number[] = [];
  if (b.step > 0) for (let v = b.min; v <= b.max + 1e-3; v += b.step) steps.push(Math.round(v * 10) / 10);
  if (steps.length >= 2 && steps.length <= 7) {
    return (
      <Segmented
        label="Microphone boost"
        value={Math.round(b.db * 10) / 10}
        options={steps.map((v) => ({ value: v, label: v === 0 ? "Off" : `+${v} dB` }))}
        onChange={(db) => run({ type: "setMicBoost", id: mic.id, db })}
      />
    );
  }
  const range = b.max - b.min || 1;
  return (
    <div className="flex w-56 items-center gap-2">
      <Slider
        label="Microphone boost"
        value={(b.db - b.min) / range}
        onChange={(v) => run({ type: "setMicBoost", id: mic.id, db: b.min + v * range })}
      />
      <span className="tabular w-14 text-right text-[12px] text-ink-2">+{b.db.toFixed(1)} dB</span>
    </div>
  );
}

const MODES: { value: HotkeyMode; label: string }[] = [
  { value: "toggle", label: "Toggle" },
  { value: "pushToTalk", label: "Push to talk" },
  { value: "pushToMute", label: "Push to mute" },
];

export function MicView({ state }: { state: UiState }) {
  const mic = defaultMic(state);
  const all = state.devices.filter((d) => d.flow === "capture" && d.state === "active");
  const comms = all.find((d) => d.defaultComms);

  return (
    <div className="flex max-w-3xl flex-col gap-6">
      <ViewHeader title="Microphone" subtitle="Level, mute and hotkeys for your mic" />
      {!mic ? (
        <Banner>No microphone found. Plug one in or enable it in Windows sound settings.</Banner>
      ) : (
        <>
          <section className={`rounded-2xl border p-5 ${mic.muted ? "border-danger/40 bg-danger/10" : "border-line bg-surface"}`}>
            <div className="flex items-center gap-4">
              <MuteButton kind="mic" size="lg" muted={mic.muted} label="microphone" onToggle={() => run({ type: "setMicMute", muted: !mic.muted })} />
              <div className="min-w-0 flex-1">
                <div className="flex items-baseline justify-between">
                  <span className="truncate font-medium">{mic.name}</span>
                  <span className="tabular text-ink-2">{Math.round(mic.volume * 100)}%</span>
                </div>
                <Slider value={mic.volume} label="Microphone level" onChange={(v) => run({ type: "setDeviceVolume", id: mic.id, volume: v })} />
                <Meter source="mic" className="mt-2 !h-2" />
              </div>
            </div>
            {mic.muted && <p className="mt-3 text-[13px] text-danger">Your microphone is muted.</p>}
          </section>

          <div className="rounded-2xl border border-line bg-surface px-5">
            <Field label="Input device" hint="Used by apps set to the default microphone">
              <select
                aria-label="Input device"
                className="rs-select w-60"
                value={mic.id}
                onChange={(e) => run({ type: "setDefaultDevice", id: e.currentTarget.value, role: "console" })}
              >
                {all.map((d) => (
                  <option key={d.id} value={d.id}>
                    {d.name}
                  </option>
                ))}
              </select>
            </Field>
            <Field label="Communications device" hint="Used by Discord, Teams and calls when set to Default">
              <select
                aria-label="Communications device"
                className="rs-select w-60"
                value={comms?.id ?? ""}
                onChange={(e) => e.currentTarget.value && run({ type: "setDefaultDevice", id: e.currentTarget.value, role: "communications" })}
              >
                {!comms && <option value="">None</option>}
                {all.map((d) => (
                  <option key={d.id} value={d.id}>
                    {d.name}
                  </option>
                ))}
              </select>
            </Field>
            {mic.boost && (
              <Field label="Microphone boost" hint="Hardware gain applied before the level above">
                <BoostControl mic={mic} />
              </Field>
            )}
            <Field label="Mute hotkey" hint="Works system-wide, even in fullscreen games">
              <HotkeyRecorder config={state.mic} />
            </Field>
            <Field label="Hotkey mode" hint="Push to talk keeps you muted until you hold the key">
              <Segmented label="Hotkey mode" value={state.mic.mode} options={MODES} onChange={(mode) => saveMicConfig({ ...state.mic, mode })} />
            </Field>
          </div>
        </>
      )}
    </div>
  );
}
```

`src/views/SettingsView.tsx`:
```tsx
import { run } from "../lib/store";
import type { Theme, UiState } from "../lib/types";
import { Field, Segmented, Toggle, ViewHeader } from "../components/ui";

const THEMES: { value: Theme; label: string }[] = [
  { value: "system", label: "System" },
  { value: "light", label: "Light" },
  { value: "dark", label: "Dark" },
];

export function SettingsView({ state }: { state: UiState }) {
  const ui = state.ui;
  return (
    <div className="flex max-w-3xl flex-col gap-6">
      <ViewHeader title="Settings" />
      <div className="rounded-2xl border border-line bg-surface px-5">
        <Field label="Theme">
          <Segmented label="Theme" value={ui.theme} options={THEMES} onChange={(theme) => run({ type: "setUiSettings", ui: { ...ui, theme } })} />
        </Field>
        <Field label="Start minimized to tray" hint="Open only the tray icon when Resonance launches">
          <Toggle
            label="Start minimized to tray"
            checked={ui.startMinimized}
            onChange={(startMinimized) => run({ type: "setUiSettings", ui: { ...ui, startMinimized } })}
          />
        </Field>
        <Field label="Remembered app volumes" hint="Forget saved levels for apps that aren't running (running apps are re-remembered)">
          <button
            type="button"
            onClick={() => run({ type: "resetAppMemory" })}
            className="rounded-lg border border-line px-3 py-1.5 text-[13px] hover:bg-surface-2"
          >
            Clear
          </button>
        </Field>
      </div>
      <p className="text-[12px] text-ink-3">
        Resonance 0.1.0 · Settings and logs live in %APPDATA%\Resonance
      </p>
    </div>
  );
}
```

`src/views/Titlebar.tsx`:
```tsx
import type { ReactNode } from "react";
import { Minus, Square, X } from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Logo } from "../components/ui";

function TitleButton({ label, onClick, danger, children }: { label: string; onClick: () => void; danger?: boolean; children: ReactNode }) {
  return (
    <button
      type="button"
      aria-label={label}
      onClick={onClick}
      className={`grid h-full w-12 place-items-center text-ink-2 transition-colors ${danger ? "hover:bg-[#e81123] hover:text-white" : "hover:bg-surface-2 hover:text-ink"}`}
    >
      {children}
    </button>
  );
}

export function Titlebar() {
  const win = getCurrentWindow();
  return (
    <header data-tauri-drag-region className="flex h-10 shrink-0 items-center justify-between pl-4">
      <div data-tauri-drag-region className="flex items-center gap-2 text-[13px] font-semibold tracking-wide">
        <Logo /> Resonance
      </div>
      <div className="flex h-full">
        <TitleButton label="Minimize" onClick={() => win.minimize()}>
          <Minus size={14} />
        </TitleButton>
        <TitleButton label="Maximize" onClick={() => win.toggleMaximize()}>
          <Square size={11} />
        </TitleButton>
        <TitleButton label="Close" danger onClick={() => win.close()}>
          <X size={15} />
        </TitleButton>
      </div>
    </header>
  );
}
```

`src/views/Sidebar.tsx`:
```tsx
import { Mic, Settings, SlidersVertical, Speaker } from "lucide-react";

export type View = "mixer" | "devices" | "mic" | "settings";

const ITEMS: { id: View; label: string; Icon: typeof Mic }[] = [
  { id: "mixer", label: "Mixer", Icon: SlidersVertical },
  { id: "devices", label: "Devices", Icon: Speaker },
  { id: "mic", label: "Microphone", Icon: Mic },
  { id: "settings", label: "Settings", Icon: Settings },
];

export function Sidebar({ view, onSelect, micMuted }: { view: View; onSelect: (v: View) => void; micMuted: boolean }) {
  return (
    <nav aria-label="Sections" className="flex w-52 shrink-0 flex-col gap-1 px-3 py-2">
      {ITEMS.map(({ id, label, Icon }) => (
        <button
          key={id}
          type="button"
          aria-current={view === id ? "page" : undefined}
          onClick={() => onSelect(id)}
          className={`relative flex items-center gap-3 rounded-lg px-3 py-2 text-[13px] transition-colors ${
            view === id ? "bg-surface-2 font-medium text-ink" : "text-ink-2 hover:bg-surface hover:text-ink"
          }`}
        >
          {view === id && <span className="absolute left-0 top-2 bottom-2 w-[3px] rounded-full bg-accent" />}
          <Icon size={16} />
          {label}
          {id === "mic" && micMuted && <span className="ml-auto h-2 w-2 rounded-full bg-danger" aria-label="muted" />}
        </button>
      ))}
    </nav>
  );
}
```

`src/views/MainWindow.tsx`:
```tsx
import { useState } from "react";
import { defaultMic, useStore } from "../lib/store";
import { Toasts } from "../components/Toasts";
import { DevicesView } from "./DevicesView";
import { MicView } from "./MicView";
import { MixerView } from "./MixerView";
import { SettingsView } from "./SettingsView";
import { Sidebar, type View } from "./Sidebar";
import { Titlebar } from "./Titlebar";

export function MainWindow() {
  const [view, setView] = useState<View>("mixer");
  const state = useStore((s) => s.state);
  const micMuted = state ? (defaultMic(state)?.muted ?? false) : false;

  return (
    <div className="flex h-full flex-col">
      <Titlebar />
      <div className="flex min-h-0 flex-1">
        <Sidebar view={view} onSelect={setView} micMuted={micMuted} />
        <main className="min-w-0 flex-1 overflow-auto rounded-tl-2xl border-l border-t border-line bg-surface p-6">
          {!state ? (
            <div className="grid h-full place-items-center text-[13px] text-ink-3">Connecting to audio…</div>
          ) : view === "mixer" ? (
            <MixerView state={state} />
          ) : view === "devices" ? (
            <DevicesView state={state} />
          ) : view === "mic" ? (
            <MicView state={state} />
          ) : (
            <SettingsView state={state} />
          )}
        </main>
      </div>
      <Toasts />
    </div>
  );
}
```

Final `src/App.tsx`:
```tsx
import { useBootstrap } from "./lib/bootstrap";
import { useStore } from "./lib/store";
import { useTheme } from "./lib/theme";
import { Flyout } from "./views/Flyout";
import { MainWindow } from "./views/MainWindow";

const isFlyout = new URLSearchParams(window.location.search).get("view") === "flyout";

export function App() {
  useBootstrap();
  const state = useStore((s) => s.state);
  useTheme(state);
  return isFlyout ? <Flyout /> : <MainWindow />;
}
```

- [ ] **Step 4: Run all frontend tests and the type-check/build**

Run:
```powershell
npm.cmd test
npm.cmd run build
```
Expected: all tests PASS; `tsc --noEmit` clean; Vite build succeeds.

- [ ] **Step 5: Visual check**

Run `npm.cmd run tauri dev`. Walk every view in dark and light theme (Settings → Theme), at 100% and 150% display scaling. Fix any layout issue found (clipped strips, overflowing selects, unreadable contrast) before committing.

- [ ] **Step 6: Commit**

```powershell
git add -A
git commit -m "feat(ui): main window with mixer, devices, microphone and settings views"
```

---

### Task 14: Verification, manual checklist, release build

**Files:**
- Create: `docs/testing/phase1-manual-checklist.md`, `README.md`

- [ ] **Step 1: Write `docs/testing/phase1-manual-checklist.md`**

```markdown
# Phase 1 manual checklist

Run with `npm.cmd run tauri dev` (or the installed build). Tick each item.

## Tray & windows
- [ ] Tray icon appears at launch; tooltip says "Resonance".
- [ ] Left-click opens the flyout above the taskbar, horizontally clamped on screen.
- [ ] Clicking the tray icon again, clicking elsewhere, or pressing Esc closes the flyout.
- [ ] Flyout position is correct on a second monitor and at 125%/150% scaling.
- [ ] Main window: drag by titlebar, minimize, maximize, close → hides to tray (process keeps running).
- [ ] Tray menu: Open / Toggle microphone mute / Quit work. Launching a second copy focuses the first.

## Mixer
- [ ] Every app playing sound appears with the correct icon and friendly name; System sounds appears last.
- [ ] Volume and mute changes match the Windows volume mixer both ways (change it in Windows → Resonance updates within ~0.25 s).
- [ ] Meters move with the audio and stop when the app is silent.
- [ ] Routing an app to another output moves its sound; "Default" moves it back.
- [ ] Restart Spotify (or any app) → its saved volume/mute is restored.

## Devices
- [ ] Set default and communications devices; Windows Sound settings reflect it.
- [ ] Rename (Enter saves, Esc cancels), hide/unhide; names persist after restarting Resonance.
- [ ] Unplug/turn off the Space One → shown as Unplugged; reconnect → active again, routing restored.

## Microphone
- [ ] Level slider and mute match Windows; mic meter moves when speaking.
- [ ] Boost control appears only if the mic driver supports it and changes the Windows "Levels" boost.
- [ ] Ctrl+Shift+M toggles mute from any app, including a fullscreen game; tray icon shows the red badge.
- [ ] Push to talk: muted at rest, unmuted while held. Push to mute: the reverse.
- [ ] Recording a new hotkey works; Backspace clears it; a combo used by another app shows a readable error.

## Resilience
- [ ] Restart the "Windows Audio" service (`services.msc`) → Resonance recovers within a second.
- [ ] Sleep and wake the PC → Resonance still works.
- [ ] Corrupt `%APPDATA%\Resonance\settings.json` → app starts with defaults and a `settings.bad-*.json` backup exists.
- [ ] CPU usage with the window hidden stays near 0%.
```

- [ ] **Step 2: Write `README.md`**

```markdown
# Resonance

A modern Windows audio control centre: per-app volume and output routing, device management,
microphone control with global hotkeys — with Soundcore Space One control and a Voicemeeter-style
engine coming in later phases (see `docs/superpowers/specs/`).

## Develop

Prerequisites: Node 20+, Rust (MSVC), Visual Studio C++ Build Tools.

    npm.cmd install
    npm.cmd run tauri dev

## Test

    npm.cmd test                                   # frontend
    cd src-tauri; cargo test --lib                 # Rust logic
    cd src-tauri; cargo test --test wasapi -- --ignored   # real-hardware checks

## Build an installer

    npm.cmd run tauri build    # → src-tauri/target/release/bundle/nsis/

Settings and logs: `%APPDATA%\Resonance`.
```

- [ ] **Step 3: Run the full automated suite**

Run:
```powershell
npm.cmd test
npm.cmd run build
cd src-tauri; cargo test --lib; cargo test --test wasapi -- --ignored; cargo clippy --all-targets -- -D warnings; cd ..
```
Expected: everything passes; clippy reports no warnings (fix any it raises).

- [ ] **Step 4: Build the installer**

Run: `npm.cmd run tauri build`
Expected: `src-tauri/target/release/bundle/nsis/Resonance_0.1.0_x64-setup.exe` is produced.

- [ ] **Step 5: Walk the manual checklist** on the release build and fix anything that fails (each fix gets its own commit).

- [ ] **Step 6: Commit**

```powershell
git add -A
git commit -m "docs: README and Phase 1 manual checklist"
```
