# Restart audio: design

**Goal.** A Voicemeeter-style "Restart audio" that rebuilds Resonance's whole audio side and reconnects the
headphones, for when a device, app session, the processed mic or the call mix gets stuck.

**Decided with the user (2026-10-18):**
- Scope is audio plus headphones.
- It is triggered from the tray menu and a button on the Settings page. There is no hotkey and no Stream Deck action.
- It rebuilds in place inside the audio worker (approach A). It does not replace the worker thread or relaunch the app.

## What a restart does

All steps run on the `audio-worker` thread, which owns the COM objects.

1. Build a new `WasapiBackend` from the reusable factory. If that fails, stop here: the old backend and engine keep
   running untouched and the error is returned.
2. `core.shutdown()` stops the processing engine, then restores the Windows default mics from
   `settings.processing_restore` (only roles still on CABLE Output, as today). Swap in the new backend and a new engine
   from the engine factory, then drop the old ones.
   - Dropping the backend unregisters its device, session and volume callbacks, and its `ComGuard` drops last.
   - Dropping the engine joins the `audio-engine` thread.
3. Reset AppCore's per-run state:
   - `engine_state` goes back to a fresh `ProcState`: route, sources, `switched`, `retry_in` and `last_error` are cleared.
   - `seen_sessions` is cleared, so saved per-app volumes and routes are re-applied to every session.
4. `core.refresh()`:
   - re-enumerates devices and sessions;
   - `reconcile_processing()` restarts the processed mic if it is enabled, which switches the default mic back to CABLE Output;
   - `sync_sources()` re-opens the call sends.
5. Emit the new state and the mic-mute state, then a notice: "Audio restarted", or the failure reason.

The headphones get the existing `HeadsetCommand::Retry`, which drops the Space One link and reconnects.

**Unchanged:** settings, profiles, `pending_outputs`, hotkeys, the control pipe, and open windows.

## Components

- **`worker.rs`**
  - `spawn` takes `make_backend: impl Fn() -> Result<B>` and `make_engine: impl Fn() -> Box<dyn AudioEngine>`.
    These are called once at start and again on each restart.
  - New `Msg::Restart(Option<oneshot::Sender<Result<()>>>)`.
  - New `WorkerHandle::restart()`, which is async and returns the result.
  - `run_loop` handles the restart by calling a new `AppCore::rebuild(backend, engine)`.
- **`app_state`**
  - `AppCore::rebuild(new_backend, new_engine)` performs steps 2 to 4. The worker does step 1 (building the backend) before
    calling it.
  - Its unit tests live next to it.
- **`app/commands.rs`**
  - New Tauri command `restart_audio`. It is async and awaits `WorkerHandle::restart()`, then sends `HeadsetCommand::Retry`.
  - It is async because the restart can take up to about 3 s while the engine starts. The command must not block the
    main thread (see the `show_main` deadlock lesson).
- **`app/tray.rs`**: a "Restart audio" menu item, placed above Quit. It spawns the same restart on the async runtime.
- **Frontend**
  - `api.restartAudio()`.
  - A Settings row: "Restart audio", with the explanation "Reconnects every device, app and your headphones. Use it if
    sound stops or a device is missing."
  - While a restart is running the button reads "Restarting…" and is disabled. The result shows through the existing
    notice toast.

## Errors

- **The backend factory fails** (for example, COM is unavailable).
  - The new backend is built before anything is stopped (step 1), so a failed rebuild leaves the previous backend and
    engine running.
  - Returns the error, and the notice says "Couldn't restart audio: …".
- **The engine fails to start after a rebuild.** The existing retry path (`RETRY_REFRESHES`) and its notice handle this.
- **Default mics.** If processing is enabled, a rebuild never leaves the user on CABLE Output without the engine running.
  This is the same as the existing reconcile behaviour.
- **Commands queued during a restart** run afterwards against the new backend. Stale device or session IDs fail with the
  normal `device_gone` / `session_ended` errors.
- **A second restart while one is running** queues behind it. It is harmless, and the UI button is disabled while
  restarting.

## Testing

- **Unit tests** (FakeBackend and FakeEngine, `app_state/restart_tests.rs`):
  - A rebuild stops the old engine and starts the new one.
  - The default mic is restored, then switched again.
  - Call sources are re-sent to the new engine.
  - A saved app volume is re-applied to an existing session.
  - A failing backend factory leaves the old backend working and returns an error.
- **Worker test:** `WorkerHandle::restart()` replies, and a state event follows.
- **Frontend test:** the Settings button calls `restartAudio`, shows "Restarting…", then re-enables.
- **Hardware test** (`tests/engine_hw.rs`, ignored by default): restart while the processed mic is live. The mic is live
  again afterwards and the default capture device is CABLE Output.
- **Manual check on the user's PC:** with music playing and the processed mic on, press Restart audio. Sound returns within
  about 3 s, and the Space One reconnects.
