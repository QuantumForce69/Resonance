# Restart Audio Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** a Voicemeeter-style "Restart audio" that rebuilds Resonance's audio side in place and reconnects the
headphones.
- **Triggers:** tray menu, Settings button, Ctrl+R, `resonance.exe -r`, and the control pipe.
- **Automatic:** an optional restart when an in-use device comes back after disconnecting.

**Architecture:**
- The audio worker gets reusable backend and engine factories and a `Msg::Restart`.
- `AppCore::rebuild` swaps the backend and engine on the worker thread, then re-runs the normal refresh/reconcile path.
- One async routine, `app::restart::restart_audio`, serves every trigger.
- A pure `Watchdog` in `app_state` decides when to auto-restart.

**Tech Stack:** Rust (Tauri 2, windows-rs, tokio), React 19 + TypeScript + Vitest.

**Spec:** `docs/superpowers/specs/2026-10-18-restart-audio-design.md`

## Global Constraints

- Windows-only app. Use `npm.cmd`, never `npm`.
- Never run `cargo test --release` in `src-tauri`; use plain `cargo test`.
- Every COM object (the backend and the engine) is created and dropped on the `audio-worker` thread.
- Tauri commands that wait on the worker are `async`. A synchronous command that blocks the main thread can deadlock.
- User-facing copy is sentence case and plain:
  - Menu item and button: `Restart audio`.
  - Running state: `Restarting…`.
  - Settings hint: `Reconnects every device, app and your headphones. Use it if sound stops or a device is missing.`
- Auto-restart setting label: `Restart audio when a device comes back`. It is off by default and stored as
  `ui.autoRestartAudio` / `auto_restart_audio`.
- Disconnect notice: `<device name> disconnected. Resonance will restart audio when it's back.`
- The auto-restart delay after a device returns is 2 s, which is 8 refreshes at the 250 ms `RESYNC_INTERVAL`.
- Commits end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

**Spec amendment (decided while planning):**
- The restart does **not** call `core.shutdown()`, which would restore the Windows default mics.
- It only stops the engine. The default mic stays on CABLE Output while processing is on, and Resonance re-asserts it
  when the engine restarts.
- This matches Voicemeeter, which leaves Windows defaults alone, and avoids the default mic flipping twice in Discord.
- If processing can't restart, the existing reconcile/retry logic applies unchanged.

---

## File Structure

| File | Responsibility |
|---|---|
| `src-tauri/src/app_state/restart.rs` (new) | `AppCore::rebuild`: swap the backend and engine, then reset per-run state. |
| `src-tauri/src/app_state/restart_tests.rs` (new) | Unit tests for `rebuild`. |
| `src-tauri/src/app_state/watchdog.rs` (new) | Pure `Watchdog` and `in_use_devices()`: when to auto-restart. Has its own tests. |
| `src-tauri/src/worker.rs` | Reusable factories, `Msg::Restart`, `WorkerHandle::restart()`, ticking the watchdog, `EventSink::restart_wanted`. |
| `src-tauri/src/model.rs`, `src/lib/types.ts` | `UiSettings.auto_restart_audio`. |
| `src-tauri/src/app/restart.rs` (new) | `restart_audio(app)`, used by every trigger, and `wants_restart(args)`. |
| `src-tauri/src/app/{commands,tray,mod,control,sink}.rs` | Wire up the triggers. |
| `src/lib/api.ts`, `src/views/SettingsView.tsx`, `src/lib/useRestartShortcut.ts` (new), `src/views/MainWindow.tsx`, `src/views/Flyout.tsx` | UI. |

---

### Task 1: `AppCore::rebuild`

**Files:**
- Create: `src-tauri/src/app_state/restart.rs`
- Create: `src-tauri/src/app_state/restart_tests.rs`
- Modify: `src-tauri/src/app_state/mod.rs`: add the modules next to the others, near line 1-12.

**Interfaces:**
- Produces: `impl<B: AudioBackend> AppCore<B> { pub fn rebuild(&mut self, backend: B, engine: Box<dyn AudioEngine>) -> Result<()> }`.
  - Precondition: none. Postcondition: the old engine is stopped, the new backend and engine are in place, and `refresh()` has run.
  - Returns the refresh error, if any.

- [ ] **Step 1: Write the failing tests** in `src-tauri/src/app_state/restart_tests.rs`:

```rust
use super::*;
use crate::{
    engine::ProcessingSettings,
    fake::{FakeBackend, FakeEngine},
};

fn named(id: &str, flow: Flow, name: &str) -> RawDevice {
    let mut d = FakeBackend::device(id, flow);
    d.name = name.into();
    d
}

/// Speakers, a desk mic (default for both roles), VB-Cable, and one Spotify session.
fn backend() -> FakeBackend {
    let mut out1 = FakeBackend::device("out1", Flow::Render);
    out1.default_console = true;
    out1.default_comms = true;
    let mut mic1 = named("mic1", Flow::Capture, "Desk Mic");
    mic1.default_console = true;
    mic1.default_comms = true;
    FakeBackend {
        devices: vec![
            out1,
            mic1,
            named("cin", Flow::Render, "CABLE Input (VB-Audio Virtual Cable)"),
            named("cout", Flow::Capture, "CABLE Output (VB-Audio Virtual Cable)"),
        ],
        sessions: vec![FakeBackend::session("s1", 10, Some("spotify.exe"))],
        ..Default::default()
    }
}

fn processing_on() -> (AppCore<FakeBackend>, FakeEngine) {
    let engine = FakeEngine::default();
    let mut c = AppCore::new(backend(), Settings::default());
    c.set_engine(Box::new(engine.clone()));
    c.refresh().unwrap();
    let mut p: ProcessingSettings = c.settings().processing.clone();
    p.enabled = true;
    c.apply(Command::SetProcessing { processing: p }).unwrap();
    c.refresh().unwrap();
    (c, engine)
}

#[test]
fn rebuild_stops_the_old_engine_and_starts_the_new_one() {
    let (mut c, old) = processing_on();
    assert!(old.0.lock().unwrap().running.is_some());
    let new = FakeEngine::default();
    c.rebuild(backend(), Box::new(new.clone())).unwrap();
    assert!(old.0.lock().unwrap().running.is_none(), "old engine stopped");
    assert_eq!(new.0.lock().unwrap().starts, 1, "new engine started");
    assert!(new.0.lock().unwrap().running.is_some());
}

#[test]
fn rebuild_keeps_the_default_mic_on_the_cable_and_the_restore_point() {
    let (mut c, _) = processing_on();
    let restore = c.settings().processing_restore.clone();
    assert!(restore.is_some());
    let mut fresh = backend();
    // Windows still has CABLE Output as default (the OS kept it; only Resonance's objects were rebuilt).
    for d in &mut fresh.devices {
        d.default_console = d.id == "cout";
        d.default_comms = d.id == "cout";
    }
    c.rebuild(fresh, Box::new(FakeEngine::default())).unwrap();
    assert_eq!(c.settings().processing_restore, restore, "restore point unchanged");
    let default_mic = c.backend_mut().devices.iter().find(|d| d.flow == Flow::Capture && d.default_console).unwrap().id.clone();
    assert_eq!(default_mic, "cout");
}

#[test]
fn rebuild_reopens_call_sources_in_the_new_engine() {
    let (mut c, _) = processing_on();
    let key = c.state().apps[0].key.clone();
    c.apply(Command::SetAppSend { key, level: Some(0.7) }).unwrap();
    c.refresh().unwrap();
    let new = FakeEngine::default();
    c.rebuild(backend(), Box::new(new.clone())).unwrap();
    assert_eq!(new.0.lock().unwrap().sources.len(), 1, "sent app captured again");
}

#[test]
fn rebuild_reapplies_saved_app_volumes() {
    let (mut c, _) = processing_on();
    let key = c.state().apps[0].key.clone();
    c.apply(Command::SetAppVolume { key, volume: 0.3 }).unwrap();
    c.refresh().unwrap();
    let mut fresh = backend();
    fresh.sessions[0].volume = 1.0; // the OS forgot, e.g. after a driver reset
    c.rebuild(fresh, Box::new(FakeEngine::default())).unwrap();
    assert!((c.state().apps[0].volume - 0.3).abs() < 1e-6, "saved volume re-applied");
}
```

- [ ] **Step 2:** Add the modules in `src-tauri/src/app_state/mod.rs` after `mod processing_tests;`:

```rust
mod restart;
#[cfg(test)]
mod restart_tests;
```

Run: `cargo test --lib restart_tests`
Expected: FAIL to compile with "no method named `rebuild`".

- [ ] **Step 3: Implement** `src-tauri/src/app_state/restart.rs`:

```rust
use super::AppCore;
use crate::{backend::AudioBackend, engine::AudioEngine, error::Result};

impl<B: AudioBackend> AppCore<B> {
    /// Restart audio: stops the engine, swaps in a freshly built backend and engine, forgets per-run state, then
    /// re-reads everything. Saved app levels and routes are re-applied, and processing and call sends restart if on.
    /// Windows' default mics are left alone (the engine comes straight back), so calls don't see the mic flip.
    pub fn rebuild(&mut self, backend: B, engine: Box<dyn AudioEngine>) -> Result<()> {
        self.set_engine(engine);
        self.engine_state = Default::default();
        self.backend = backend;
        self.seen_sessions.clear();
        self.state.routing_available = self.backend.routing_available();
        self.refresh().map(|_| ())
    }
}
```

`set_engine` already calls `stop_engine()`, which stops the old engine and clears its sources. Resetting `engine_state`
clears `switched`, `retry_in`, `status` and `last_error`, so `reconcile_processing` starts cleanly. `seen_sessions` and
`backend` are private fields of `AppCore`; this file is a child module of `app_state`, so it can reach them.

- [ ] **Step 4:** Run `cargo test --lib restart_tests`. Expected: 4 passed.

If `rebuild_reapplies_saved_app_volumes` fails because `FakeBackend::session` has no `volume` field, look at
`src-tauri/src/fake.rs` for the session volume field name and use that.

- [ ] **Step 5:** Run `cargo test --lib`. Expected: all pass.

Commit with the message `feat(audio): AppCore::rebuild swaps backend and engine in place`.

---

### Task 2: Worker restart

**Files:**
- Modify: `src-tauri/src/worker.rs`.
- Modify: `src-tauri/src/app/mod.rs:28-33`, the `worker::spawn` call.

**Interfaces:**
- Consumes: `AppCore::rebuild` (Task 1).
- Produces:
  - `pub fn spawn<F, E, B, S>(make_backend: F, make_engine: E, settings_path: PathBuf, sink: S) -> WorkerHandle`,
    where `F: Fn() -> Result<B> + Send + 'static` and `E: Fn() -> Box<dyn AudioEngine> + Send + 'static`.
  - `impl WorkerHandle { pub async fn restart(&self) -> Result<()> }`.

- [ ] **Step 1: Write the failing test** at the end of `mod tests` in `src-tauri/src/worker.rs`:

```rust
    #[test]
    fn restart_rebuilds_and_republishes_state() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, rx) = mpsc::channel();
        let built = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let count = built.clone();
        let h = spawn(
            move || {
                count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Ok(fake())
            },
            || Box::new(NullEngine),
            dir.path().join("s.json"),
            TestSink(tx),
        );
        wait_for(&rx, "state");
        let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
        rt.block_on(h.restart()).unwrap();
        assert_eq!(built.load(std::sync::atomic::Ordering::SeqCst), 2, "backend built at start and on restart");
        wait_for(&rx, "state");
        h.shutdown();
    }
```

Change the four existing `spawn(|| Ok(fake()), Box::new(NullEngine), …)` calls to
`spawn(|| Ok(fake()), || Box::new(NullEngine), …)`.

Run: `cargo test --lib worker`
Expected: FAIL to compile (the signature and `restart` don't exist).

- [ ] **Step 2: Implement** in `src-tauri/src/worker.rs`.

Add the message variant:

```rust
enum Msg {
    Command(Command, Option<oneshot::Sender<Result<()>>>),
    GetState(oneshot::Sender<UiState>),
    SetMetering(bool),
    Restart(Option<oneshot::Sender<Result<()>>>),
    Shutdown(Option<mpsc::Sender<()>>),
}
```

Add to `impl WorkerHandle`:

```rust
    /// Restart audio: rebuilds the backend and engine on the worker thread (see `AppCore::rebuild`).
    pub async fn restart(&self) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.tx.send(Msg::Restart(Some(tx))).map_err(|_| stopped())?;
        rx.await.map_err(|_| stopped())?
    }
```

Replace `spawn`:

```rust
/// Starts the audio worker. The factories are called once now and again on every restart, always on the
/// worker thread (the backend's COM objects belong to it).
pub fn spawn<F, E, B, S>(make_backend: F, make_engine: E, settings_path: PathBuf, sink: S) -> WorkerHandle
where
    F: Fn() -> Result<B> + Send + 'static,
    E: Fn() -> Box<dyn AudioEngine> + Send + 'static,
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
            let mut core = AppCore::new(backend, Settings::load(&settings_path));
            core.set_engine(make_engine());
            let rebuild = |core: &mut AppCore<B>| -> Result<()> {
                // Build the new backend first: if that fails, the current one keeps running.
                let backend = make_backend()?;
                core.rebuild(backend, make_engine())
            };
            run_loop(&mut core, &rx, &settings_path, &sink, &rebuild);
        })
        .expect("failed to spawn audio worker thread");
    WorkerHandle { tx }
}
```

Give `run_loop` a `rebuild: &dyn Fn(&mut AppCore<B>) -> Result<()>` parameter. Make it generic over `B` the way it
already is over the backend; its current signature is `fn run_loop<B: AudioBackend>(core: &mut AppCore<B>, …)`. Then add
this match arm next to `Msg::Command`:

```rust
            Ok(Msg::Restart(reply)) => {
                tracing::info!("restarting audio");
                let result = rebuild(core);
                if let Err(e) = &result {
                    tracing::warn!("restart audio failed: {e}");
                }
                if let Some(reply) = reply {
                    let _ = reply.send(result);
                }
                changed = true;
                // A restart can change the mute state the sink last saw.
                last_mic = None;
            }
```

`last_mic` is declared `let mut last_mic = core.mic_muted();` before the loop. Setting it to `None` makes the existing
`if mic != last_mic` block re-send the mute state.

- [ ] **Step 3:** Update the production call in `src-tauri/src/app/mod.rs`:

```rust
            let worker = worker::spawn(
                crate::win::WasapiBackend::new,
                || Box::new(crate::win::WasapiEngine::new()) as Box<dyn crate::engine::AudioEngine>,
                settings_path.clone(),
                sink::TauriSink { app: handle.clone() },
            );
```

- [ ] **Step 4:** Run `cargo test --lib worker`, then `cargo test --lib`, then `cargo clippy --lib --tests`.
Expected: all pass, no warnings.

- [ ] **Step 5:** Commit with the message `feat(audio): the worker can restart audio with fresh backend and engine`.

---

### Task 3: `restart_audio` routine, Tauri command, tray, `-r`, and the pipe

**Files:**
- Create: `src-tauri/src/app/restart.rs`.
- Modify:
  - `src-tauri/src/app/mod.rs`: `mod restart;`, the single-instance callback, the launch flag, and the invoke handler.
  - `src-tauri/src/app/commands.rs`: new command.
  - `src-tauri/src/app/tray.rs`: the menu item.
  - `src-tauri/src/app/control.rs`: the `Control` enum and the `restartAudio` request.

**Interfaces:**
- Consumes: `WorkerHandle::restart()` (Task 2) and `HeadsetHandle::command(HeadsetCommand::Retry)`, which already exists.
- Produces:
  - `pub async fn restart_audio(app: &AppHandle) -> crate::error::Result<()>`.
  - `pub fn wants_restart(args: &[String]) -> bool`.
  - `pub const RESTART_ARGS: [&str; 2] = ["-r", "--restart-audio"]`.
  - Tauri command `restart_audio`.
  - `control::Control { Worker(Command), RestartAudio }`.

- [ ] **Step 1: Write the failing tests.**

In the new `src-tauri/src/app/restart.rs`, add a test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_the_restart_flags() {
        let args = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(wants_restart(&args(&["resonance.exe", "-r"])));
        assert!(wants_restart(&args(&["resonance.exe", "--restart-audio"])));
        assert!(!wants_restart(&args(&["resonance.exe", "--minimized"])));
        assert!(!wants_restart(&args(&["resonance.exe"])));
    }
}
```

In `src-tauri/src/app/control.rs` `mod tests`:
- extend `parses_requests` with
  `assert_eq!(serde_json::from_str::<Request>(r#"{"command":"restartAudio"}"#).unwrap(), Request::RestartAudio);`;
- change the `start()` helper to take `mpsc::UnboundedSender<Control>`, and add:

```rust
    #[tokio::test]
    async fn restart_audio_reaches_the_handler() {
        let name = r"\\.\pipe\resonance-test-restart";
        let (status, mut controls) = start(name);
        status.set(false);
        let mut client = connect(name).await;
        client.write_all(b"{\"command\":\"restartAudio\"}\n").await.unwrap();
        drop(client);
        let got = timeout(Duration::from_secs(2), controls.recv()).await.unwrap();
        assert_eq!(got, Some(Control::RestartAudio));
    }
```

Update the existing pipe tests to expect `Some(Control::Worker(Command::ToggleMicMute))` and
`Some(Control::Worker(Command::SetMicMute { muted: true }))`.

Run: `cargo test --lib app::`
Expected: FAIL to compile.

- [ ] **Step 2: Implement `src-tauri/src/app/restart.rs`:**

```rust
//! Restart audio (Voicemeeter's "Restart Audio Engine"): rebuild every device, app session, the processed
//! mic and call mix, and reconnect the headphones. Every trigger ends up here.

use tauri::{AppHandle, Manager};

use super::sink::push_notice;
use crate::{
    error::Result,
    headset::{model::HeadsetCommand, HeadsetHandle},
    worker::WorkerHandle,
};

/// Launch flags that ask a running Resonance to restart audio (like `Voicemeeter.exe -r`).
pub const RESTART_ARGS: [&str; 2] = ["-r", "--restart-audio"];

pub fn wants_restart(args: &[String]) -> bool {
    args.iter().any(|a| RESTART_ARGS.contains(&a.as_str()))
}

pub async fn restart_audio(app: &AppHandle) -> Result<()> {
    let worker = app.state::<WorkerHandle>().inner().clone();
    let headset = app.state::<HeadsetHandle>().inner().clone();
    let result = worker.restart().await;
    // The headphones reconnect even if the audio side couldn't be rebuilt.
    let _ = headset.command(HeadsetCommand::Retry).await;
    if let Err(e) = &result {
        push_notice(app, crate::error::AudioError::Unavailable(format!("Couldn't restart audio: {e}")));
    }
    result
}

/// For triggers that can't wait (tray menu, a second launch with `-r`, the control pipe).
pub fn spawn_restart(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let _ = restart_audio(&app).await;
    });
}
```

Check the real paths of `HeadsetCommand` and `HeadsetHandle` with
`grep -n "pub enum HeadsetCommand\|pub use" src-tauri/src/headset/mod.rs` and adjust the `use` line.
`commands.rs` already imports `crate::headset::HeadsetHandle`.

- [ ] **Step 3: Tauri command** in `src-tauri/src/app/commands.rs`:

```rust
/// Async: the restart waits on the audio worker (up to a few seconds while the engine starts).
#[tauri::command]
pub async fn restart_audio(app: AppHandle) -> Result<()> {
    super::restart::restart_audio(&app).await
}
```

In `src-tauri/src/app/mod.rs`:
- add `mod restart;`;
- add `commands::restart_audio,` to `generate_handler!`;
- replace the single-instance plugin line with:

```rust
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            if restart::wants_restart(&args) {
                restart::spawn_restart(app);
            } else {
                wnd::show_main(app);
            }
        }))
```

- in `run()`, treat a first launch with `-r` like `--minimized` (start in the tray), since there is nothing to restart:

```rust
    let launched_minimized = std::env::args()
        .any(|a| a == crate::win::startup::MINIMIZED_ARG || restart::RESTART_ARGS.contains(&a.as_str()));
```

- [ ] **Step 4: Tray** in `src-tauri/src/app/tray.rs`. Add the item and handle it:

```rust
    let restart = MenuItem::with_id(app, "restart", "Restart audio", true, None::<&str>)?;
    // …
    let menu = Menu::with_items(app, &[&open, &mute, &restart, &sep, &quit])?;
    // in on_menu_event:
            "restart" => super::restart::spawn_restart(app),
```

- [ ] **Step 5: Control pipe** in `src-tauri/src/app/control.rs`:
  - add `RestartAudio` to `enum Request`;
  - add a public enum, and replace `impl From<Request> for Command` with `impl From<Request> for Control`;
  - change `serve`'s handler and the `OnCommand` alias to take `Control`;
  - update the module doc comment to list `{"command":"restartAudio"}`.

```rust
/// What a control request asks for.
#[derive(Debug, PartialEq)]
pub enum Control {
    Worker(Command),
    RestartAudio,
}

impl From<Request> for Control {
    fn from(request: Request) -> Self {
        match request {
            Request::ToggleMicMute => Control::Worker(Command::ToggleMicMute),
            Request::SetMicMute { muted } => Control::Worker(Command::SetMicMute { muted }),
            Request::RestartAudio => Control::RestartAudio,
        }
    }
}
```

In `app/mod.rs`, the serve call becomes:

```rust
            let control_app = handle.clone();
            tauri::async_runtime::spawn(async move {
                let on_control = move |c: control::Control| match c {
                    control::Control::Worker(cmd) => commands.send(cmd),
                    control::Control::RestartAudio => restart::spawn_restart(&control_app),
                };
                if let Err(e) = control::serve(control::PIPE_NAME, mic_status, on_control).await {
                    tracing::warn!("companion control channel unavailable: {e}");
                }
            });
```

- [ ] **Step 6:** Run `cargo test --lib`, then `cargo clippy --lib --tests`. Expected: all pass, no warnings.

Commit with the message `feat(app): restart audio from the tray, -r, the control pipe and a command`.

---

### Task 4: Auto-restart watchdog

**Files:**
- Create: `src-tauri/src/app_state/watchdog.rs`. Its tests go inside the same file.
- Modify:
  - `src-tauri/src/app_state/mod.rs`: `mod watchdog; pub use watchdog::Watchdog;` and an `in_use_devices` accessor.
  - `src-tauri/src/model.rs`: `UiSettings`.
  - `src-tauri/src/worker.rs`: tick the watchdog and add `EventSink::restart_wanted`.
  - `src-tauri/src/app/sink.rs`: `restart_wanted` calls `restart::spawn_restart`.

**Interfaces:**
- Consumes: `restart::spawn_restart` (Task 3).
- Produces:
  - `pub struct Watchdog`.
  - `pub struct Tick { pub disconnected: Vec<String>, pub restart: bool }`.
    `disconnected` holds device names that just dropped out.
  - `impl Watchdog { pub fn tick(&mut self, enabled: bool, in_use: &[(String, String)], present: &HashSet<String>) -> Tick }`.
    `in_use` is `(id, name)` pairs. `present` is the ids of devices that exist and are active.
  - `pub const SETTLE_TICKS: u32 = 8`.
  - `UiSettings::auto_restart_audio: bool`.
  - `EventSink::restart_wanted(&self)`.

- [ ] **Step 1: Write the failing tests** in `src-tauri/src/app_state/watchdog.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn ids(v: &[&str]) -> HashSet<String> {
        v.iter().map(|s| s.to_string()).collect()
    }
    fn used(v: &[&str]) -> Vec<(String, String)> {
        v.iter().map(|s| (s.to_string(), format!("Dev {s}"))).collect()
    }

    #[test]
    fn a_device_dropping_out_is_announced_but_does_not_restart() {
        let mut w = Watchdog::default();
        w.tick(true, &used(&["mic"]), &ids(&["mic"]));
        let t = w.tick(true, &used(&[]), &ids(&[]));
        assert_eq!(t.disconnected, vec!["Dev mic".to_string()]);
        assert!(!t.restart);
    }

    #[test]
    fn a_returning_device_restarts_once_after_settling() {
        let mut w = Watchdog::default();
        w.tick(true, &used(&["mic"]), &ids(&["mic"]));
        w.tick(true, &used(&[]), &ids(&[]));
        let mut restarts = 0;
        for _ in 0..SETTLE_TICKS + 3 {
            if w.tick(true, &used(&["mic"]), &ids(&["mic"])).restart {
                restarts += 1;
            }
        }
        assert_eq!(restarts, 1);
    }

    #[test]
    fn several_devices_returning_together_restart_once() {
        let mut w = Watchdog::default();
        w.tick(true, &used(&["a", "b"]), &ids(&["a", "b"]));
        w.tick(true, &used(&[]), &ids(&[]));
        w.tick(true, &used(&["a"]), &ids(&["a"]));
        let mut restarts = 0;
        for _ in 0..SETTLE_TICKS + 3 {
            if w.tick(true, &used(&["a", "b"]), &ids(&["a", "b"])).restart {
                restarts += 1;
            }
        }
        assert_eq!(restarts, 1);
    }

    #[test]
    fn devices_not_in_use_are_ignored() {
        let mut w = Watchdog::default();
        w.tick(true, &used(&[]), &ids(&["other"]));
        let t = w.tick(true, &used(&[]), &ids(&[]));
        assert!(t.disconnected.is_empty());
    }

    #[test]
    fn nothing_happens_while_disabled() {
        let mut w = Watchdog::default();
        w.tick(false, &used(&["mic"]), &ids(&["mic"]));
        let t = w.tick(false, &used(&[]), &ids(&[]));
        assert!(t.disconnected.is_empty() && !t.restart);
        for _ in 0..SETTLE_TICKS + 2 {
            assert!(!w.tick(false, &used(&["mic"]), &ids(&["mic"])).restart);
        }
    }
}
```

Add `mod watchdog;` and `pub use watchdog::{Watchdog, SETTLE_TICKS};` to `app_state/mod.rs`.

Run: `cargo test --lib watchdog`
Expected: FAIL to compile.

- [ ] **Step 2: Implement** the top of `src-tauri/src/app_state/watchdog.rs`:

```rust
//! Auto-restart (Voicemeeter's "restart when a device is disconnected"): remember the devices Resonance is
//! using; when one drops out, announce it; when it comes back, wait for its driver to settle, then restart
//! audio once. Pure, so it is unit-tested without a clock (one tick per worker refresh, 250 ms).

use std::collections::{HashMap, HashSet};

/// Refreshes to wait after a device returns before restarting (2 s at 250 ms).
pub const SETTLE_TICKS: u32 = 8;

#[derive(Default)]
pub struct Watchdog {
    /// In-use devices seen present, by id → name.
    watched: HashMap<String, String>,
    /// Watched devices currently missing.
    missing: HashSet<String>,
    /// Ticks left before restarting, once something came back.
    countdown: Option<u32>,
}

#[derive(Debug, Default, PartialEq)]
pub struct Tick {
    pub disconnected: Vec<String>,
    pub restart: bool,
}

impl Watchdog {
    pub fn tick(&mut self, enabled: bool, in_use: &[(String, String)], present: &HashSet<String>) -> Tick {
        if !enabled {
            *self = Self::default();
            return Tick::default();
        }
        let mut tick = Tick::default();
        for (id, name) in &self.watched {
            if !present.contains(id) && self.missing.insert(id.clone()) {
                tick.disconnected.push(name.clone());
            }
        }
        let returned: Vec<String> = self.missing.iter().filter(|id| present.contains(*id)).cloned().collect();
        if !returned.is_empty() {
            for id in &returned {
                self.missing.remove(id);
            }
            // A later return restarts the settling wait, so devices coming back together cause one restart.
            self.countdown = Some(SETTLE_TICKS);
        } else if let Some(n) = self.countdown {
            if n <= 1 {
                self.countdown = None;
                tick.restart = true;
            } else {
                self.countdown = Some(n - 1);
            }
        }
        for (id, name) in in_use {
            if present.contains(id) {
                self.watched.insert(id.clone(), name.clone());
            }
        }
        tick
    }
}
```

The watched set only grows during a run, which is fine: a device that was used once and later goes away still deserves
a restart when it comes back. `rebuild` doesn't reset the watchdog, because the worker owns it, not `AppCore`.

- [ ] **Step 3:** Run `cargo test --lib watchdog`. Expected: 5 passed.

- [ ] **Step 4: Setting.** Add the field to `UiSettings` in `src-tauri/src/model.rs`. The struct already has
`#[serde(rename_all = "camelCase", default)]`.

```rust
    /// Restart audio automatically when a device Resonance was using comes back after disconnecting.
    pub auto_restart_audio: bool,
```

Add `autoRestartAudio: boolean;` to `UiSettings` in `src/lib/types.ts`, then update `src/test/fixtures.ts`
(`makeState`) so it includes `autoRestartAudio: false` wherever `ui` is built.

- [ ] **Step 5: In-use devices.** Add to `impl AppCore` in `src-tauri/src/app_state/mod.rs`:

```rust
    /// Devices Resonance is using, as (id, name): the default playback device, the processed mic's source
    /// and VB-Cable while the engine runs, and every app's chosen output.
    pub fn in_use_devices(&self) -> Vec<(String, String)> {
        let name = |id: &str| self.state.devices.iter().find(|d| d.id == id).map(|d| d.name.clone());
        let mut ids: Vec<String> = self
            .state
            .devices
            .iter()
            .filter(|d| d.flow == Flow::Render && d.default_console)
            .map(|d| d.id.clone())
            .collect();
        if let Some(route) = &self.engine_state.route {
            ids.push(route.source_id.clone());
            ids.push(route.cable_id.clone());
        }
        ids.extend(self.state.apps.iter().filter_map(|a| a.output.clone()));
        ids.sort();
        ids.dedup();
        ids.into_iter().filter_map(|id| name(&id).map(|n| (id, n))).collect()
    }

    /// Ids of devices Windows reports as active right now.
    pub fn present_devices(&self) -> HashSet<String> {
        self.state.devices.iter().filter(|d| d.state == DeviceState::Active).map(|d| d.id.clone()).collect()
    }
```

`Flow`, `DeviceState` and `HashSet` are already in scope through `use crate::model::*` and the existing imports.

- [ ] **Step 6: Worker wiring** in `src-tauri/src/worker.rs`.

Add to `trait EventSink`:

```rust
    /// The watchdog wants audio restarted (a device in use came back).
    fn restart_wanted(&self);
```

Implement it in the test `TestSink` as `let _ = self.0.send("restart wanted".into());`.

In `run_loop`, declare `let mut watchdog = crate::app_state::Watchdog::default();` before the loop. Right after the
periodic `core.refresh()` succeeds, inside the `if now >= next_resync` block, add:

```rust
            let tick = watchdog.tick(core.settings().ui.auto_restart_audio, &core.in_use_devices(), &core.present_devices());
            for name in tick.disconnected {
                sink.notice(&AudioError::NotFound(format!(
                    "{name} disconnected. Resonance will restart audio when it's back."
                )));
            }
            if tick.restart {
                sink.restart_wanted();
            }
```

In `src-tauri/src/app/sink.rs`, implement it for `TauriSink`:

```rust
    fn restart_wanted(&self) {
        super::restart::spawn_restart(&self.app);
    }
```

The sink sends `Msg::Restart` back through the handle, so the restart runs on the worker thread like any other restart.

- [ ] **Step 7:** Run `cargo test --lib`, then `cargo clippy --lib --tests`, then `npm.cmd test`.
Expected: all pass, no warnings.

Commit with the message `feat(audio): optional auto-restart when an in-use device comes back`.

---

### Task 5: UI (Settings button and toggle, Ctrl+R)

**Files:**
- Modify: `src/lib/api.ts`, `src/views/SettingsView.tsx`, `src/views/SettingsView.test.tsx`, `src/views/MainWindow.tsx`,
  `src/views/Flyout.tsx`.
- Create: `src/lib/useRestartShortcut.ts` and `src/lib/useRestartShortcut.test.tsx`.

**Interfaces:**
- Consumes: the Tauri command `restart_audio` (Task 3) and `UiSettings.autoRestartAudio` (Task 4).
- Produces: `export const restartAudio = () => invoke<void>("restart_audio");` and
  `export function useRestartShortcut(): void`.

- [ ] **Step 1: Write the failing tests.**

In `src/views/SettingsView.test.tsx`:
- add `restartAudio: vi.fn(() => Promise.resolve())` to the existing `vi.mock("../lib/api", …)` object;
- add these tests:

```tsx
it("restarts audio from the button and shows progress", async () => {
  let finish!: () => void;
  vi.mocked(api.restartAudio).mockImplementationOnce(() => new Promise<void>((r) => (finish = r)));
  render(<SettingsView state={makeState()} />);
  fireEvent.click(screen.getByRole("button", { name: "Restart audio" }));
  expect(api.restartAudio).toHaveBeenCalled();
  expect(screen.getByRole("button", { name: "Restarting…" })).toBeDisabled();
  finish();
  await waitFor(() => expect(screen.getByRole("button", { name: "Restart audio" })).toBeEnabled());
});

it("toggles auto-restart", () => {
  render(<SettingsView state={makeState()} />);
  fireEvent.click(screen.getByRole("switch", { name: "Restart audio when a device comes back" }));
  expect(api.send).toHaveBeenCalledWith(expect.objectContaining({ type: "setUiSettings", ui: expect.objectContaining({ autoRestartAudio: true }) }));
});
```

Check how the existing "Start in the tray" toggle test finds the `Toggle` (role `switch` or `checkbox`) and how `run`
reaches `api.send`. Use the same query and assertion style.

Create `src/lib/useRestartShortcut.test.tsx`:

```tsx
import { render, fireEvent } from "@testing-library/react";
import { useRestartShortcut } from "./useRestartShortcut";
import * as api from "./api";

vi.mock("./api", () => ({ restartAudio: vi.fn(() => Promise.resolve()) }));

function Host() {
  useRestartShortcut();
  return <input aria-label="field" />;
}

it("Ctrl+R restarts audio", () => {
  render(<Host />);
  fireEvent.keyDown(window, { key: "r", ctrlKey: true });
  expect(api.restartAudio).toHaveBeenCalledTimes(1);
});

it("is ignored while typing in a field", () => {
  vi.mocked(api.restartAudio).mockClear();
  const { getByLabelText } = render(<Host />);
  fireEvent.keyDown(getByLabelText("field"), { key: "r", ctrlKey: true });
  expect(api.restartAudio).not.toHaveBeenCalled();
});
```

Run: `npm.cmd test`
Expected: the new tests FAIL.

- [ ] **Step 2: Implement.**

`src/lib/api.ts`:

```ts
export const restartAudio = () => invoke<void>("restart_audio");
```

`src/lib/useRestartShortcut.ts`:

```ts
import { useEffect } from "react";
import { restartAudio } from "./api";

/** Ctrl+R restarts audio, as in Voicemeeter. Ignored while typing, and it replaces the webview's page reload. */
export function useRestartShortcut() {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!e.ctrlKey || e.shiftKey || e.altKey || e.key.toLowerCase() !== "r") return;
      const t = e.target as HTMLElement | null;
      if (t && (t.tagName === "INPUT" || t.tagName === "TEXTAREA" || t.isContentEditable)) return;
      e.preventDefault();
      void restartAudio();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);
}
```

Call `useRestartShortcut();` at the top of `MainWindow()` in `src/views/MainWindow.tsx` and of `Flyout()` in
`src/views/Flyout.tsx`, and import it from `../lib/useRestartShortcut`.

`src/views/SettingsView.tsx`:
- add `import { useState } from "react";` and `import { restartAudio } from "../lib/api";`;
- add inside the component:

```tsx
  const [restarting, setRestarting] = useState(false);
  const restart = async () => {
    setRestarting(true);
    try {
      await restartAudio();
    } catch {
      // The failure arrives as a notice from the backend.
    } finally {
      setRestarting(false);
    }
  };
```

- add these fields to the `Panel`, after "Start in the tray":

```tsx
        <Field label="Restart audio" hint="Reconnects every device, app and your headphones. Use it if sound stops or a device is missing. Shortcut: Ctrl+R.">
          <button type="button" onClick={restart} disabled={restarting} className="key">
            {restarting ? "Restarting…" : "Restart audio"}
          </button>
        </Field>
        <Field label="Restart audio when a device comes back" hint="If a device Resonance is using disconnects, restart audio once it's plugged back in">
          <Toggle label="Restart audio when a device comes back" checked={ui.autoRestartAudio} onChange={(autoRestartAudio) => run({ type: "setUiSettings", ui: { ...ui, autoRestartAudio } })} />
        </Field>
```

- the footer line reads `Resonance 0.1.0` but the app is 0.2.0. Replace it with the real version: `getVersion()` from
  `@tauri-apps/api/app` in a `useEffect`, falling back to an empty string. Mock `@tauri-apps/api/app` in the test with
  `getVersion: () => Promise.resolve("0.2.0")`.

- [ ] **Step 3:** Run `npm.cmd test`. Expected: all pass.

Commit with the message `feat(ui): Restart audio button, auto-restart toggle and Ctrl+R`.

---

### Task 6: Hardware test, docs, and the real check

**Files:**
- Modify: `src-tauri/tests/engine_hw.rs`, `README.md`, `docs/testing/phase1-manual-checklist.md`.

- [ ] **Step 1: Hardware test.** Add it to `src-tauri/tests/engine_hw.rs`, `#[ignore]`d like the others. Use the same
  setup helpers the file already uses to build an `AppCore` with `WasapiBackend::new()` and `WasapiEngine::new()`. Then:
  1. Enable processing and refresh until `processing.status` is running.
  2. Call `core.rebuild(WasapiBackend::new().unwrap(), Box::new(WasapiEngine::new()))`.
  3. Refresh up to 20 times, 250 ms apart, until processing is running again.
  4. Assert that the Windows default capture device is CABLE Output.
  5. Call `core.shutdown()`.

  Run it with `cargo test --test engine_hw -- --ignored restart`, after quitting the installed Resonance.
  Expected: PASS.

- [ ] **Step 2: Docs.**
  - `README.md` features, under Mixer: add "**Restart audio** (tray menu, Settings, Ctrl+R or `resonance.exe -r`) rebuilds
    every device and app connection and reconnects the headphones, with an optional auto-restart when a device comes back."
  - The control-pipe sentence: list `restartAudio`.
  - Checklist: add a "Restart audio" section covering tray, button, Ctrl+R, `-r`, and the auto-restart unplug/replug test.

- [ ] **Step 3: Full verification.** Run `cargo test --lib`, `cargo clippy --lib --tests`, `npm.cmd test`, and
  `cargo test` in `streamdeck/`. Then build with `npm.cmd run tauri build`, install, and check on the user's PC:
  - with music playing and the processed mic on, the tray's "Restart audio" brings sound back within about 3 s;
  - the Space One reconnects;
  - the default mic is still CABLE Output;
  - Ctrl+R in the window does the same;
  - `resonance.exe -r` from a terminal does the same without opening a window.

- [ ] **Step 4:** Commit with the message `docs: restart audio`. Merging and pushing follow the user's usual flow, after
  `/simplify` and their confirmation.
