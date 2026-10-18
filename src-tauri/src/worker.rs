use std::{
    path::{Path, PathBuf},
    sync::mpsc::{self, RecvTimeoutError},
    thread,
    time::{Duration, Instant},
};

use tokio::sync::oneshot;

use crate::{
    app_state::AppCore,
    backend::AudioBackend,
    engine::AudioEngine,
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
    Restart(Option<oneshot::Sender<Result<()>>>),
    Shutdown(Option<mpsc::Sender<()>>),
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

    /// Restart audio: rebuilds the backend and engine on the worker thread (see `AppCore::rebuild`).
    pub async fn restart(&self) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.tx.send(Msg::Restart(Some(tx))).map_err(|_| stopped())?;
        rx.await.map_err(|_| stopped())?
    }

    pub fn shutdown(&self) {
        let _ = self.tx.send(Msg::Shutdown(None));
    }

    /// Asks the worker to stop the engine, give Windows its default mics back and save settings,
    /// waiting up to `timeout` for it to finish. Returns at once if the worker has already stopped.
    pub fn shutdown_and_wait(&self, timeout: Duration) {
        let (tx, rx) = mpsc::channel();
        if self.tx.send(Msg::Shutdown(Some(tx))).is_ok() {
            let _ = rx.recv_timeout(timeout);
        }
    }
}

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

fn save(core: &AppCore<impl AudioBackend>, path: &Path) {
    if let Err(e) = core.settings().save(path) {
        tracing::error!("couldn't save settings: {e}");
    }
}

/// Stops the engine, gives Windows its default mics back and saves anything unsaved.
fn stop<B: AudioBackend>(core: &mut AppCore<B>, path: &Path, unsaved: bool) {
    // One last read so changes made since the previous refresh are remembered.
    if let Err(e) = core.refresh() {
        tracing::warn!("final refresh failed: {e}");
    }
    core.shutdown();
    if core.take_settings_dirty() || unsaved {
        save(core, path);
    }
    core.stop_metering();
}

fn run_loop<B: AudioBackend>(
    core: &mut AppCore<B>,
    rx: &mpsc::Receiver<Msg>,
    path: &Path,
    sink: &impl EventSink,
    rebuild: &dyn Fn(&mut AppCore<B>) -> Result<()>,
) {
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
            Ok(Msg::Shutdown(done)) => {
                stop(core, path, dirty_since.is_some());
                if let Some(done) = done {
                    let _ = done.send(());
                }
                return;
            }
            Err(RecvTimeoutError::Disconnected) => {
                stop(core, path, dirty_since.is_some());
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
        for notice in core.take_notices() {
            sink.notice(&notice);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{engine::NullEngine, fake::FakeBackend, model::Flow};
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
        let h = spawn(|| Ok(fake()), || Box::new(NullEngine), dir.path().join("s.json"), TestSink(tx));
        assert_eq!(wait_for(&rx, "state"), "state apps=1");
        assert_eq!(wait_for(&rx, "mic"), "mic false");
        h.shutdown();
    }

    #[test]
    fn commands_round_trip_and_settings_are_saved_debounced() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s.json");
        let (tx, rx) = mpsc::channel();
        let h = spawn(|| Ok(fake()), || Box::new(NullEngine), path.clone(), TestSink(tx));
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
        let h = spawn(|| Ok(fake()), || Box::new(NullEngine), dir.path().join("s.json"), TestSink(tx));
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
        let h = spawn::<_, _, FakeBackend, _>(
            || Err(AudioError::Os("no audio".into())),
            || Box::new(NullEngine),
            dir.path().join("s.json"),
            TestSink(tx),
        );
        assert_eq!(wait_for(&rx, "notice"), "notice no audio");
        assert_eq!(h.command_blocking(Command::ToggleMicMute).unwrap_err().code(), "unavailable");
    }

    #[test]
    fn shutdown_and_wait_saves_before_returning() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s.json");
        let (tx, rx) = mpsc::channel();
        let h = spawn(|| Ok(fake()), || Box::new(NullEngine), path.clone(), TestSink(tx));
        wait_for(&rx, "state");
        h.command_blocking(Command::SetAppVolume { key: "spotify.exe".into(), volume: 0.3 }).unwrap();
        h.shutdown_and_wait(Duration::from_secs(3));
        assert_eq!(Settings::load(&path).apps["spotify.exe"].volume, 0.3);
        h.shutdown_and_wait(Duration::from_secs(3));
    }

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
}
