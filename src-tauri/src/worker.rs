use std::{
    path::{Path, PathBuf},
    sync::mpsc::{self, RecvTimeoutError},
    thread,
    time::{Duration, Instant},
};

use tokio::sync::oneshot;

use crate::{
    app_state::{AppCore, Watchdog},
    backend::AudioBackend,
    engine::AudioEngine,
    error::{AudioError, Result},
    model::{Command, MeterFrame, UiState},
    settings::Settings,
};

pub const RESYNC_INTERVAL: Duration = Duration::from_millis(250);
pub const METER_INTERVAL: Duration = Duration::from_millis(33);
pub const SAVE_DEBOUNCE: Duration = Duration::from_millis(500);
/// How often the worker retries creating the backend while it has none (it failed at startup).
const BACKEND_RETRY_INTERVAL: Duration = Duration::from_secs(2);

/// Where the worker publishes changes (the Tauri app in production, a channel in tests).
pub trait EventSink: Send + 'static {
    fn state(&self, state: &UiState);
    fn meters(&self, frame: &MeterFrame);
    fn mic_muted(&self, muted: bool);
    fn notice(&self, error: &AudioError);
    /// Audio was restarted (or tried to be), whatever triggered it: the button, tray, pipe, `-r` or the watchdog.
    fn restarted(&self, result: &Result<()>);
}

type Reply = oneshot::Sender<Result<()>>;

enum Msg {
    Command(Command, Option<Reply>),
    GetState(oneshot::Sender<UiState>),
    SetMetering(bool),
    /// `sent` lets the worker drop restarts that a restart finishing after they were asked for already covered.
    Restart { sent: Instant, reply: Option<Reply> },
    Shutdown(Option<mpsc::Sender<()>>),
}

impl Msg {
    fn command(cmd: Command, reply: Option<Reply>) -> Self {
        match cmd {
            Command::RestartAudio => Msg::Restart { sent: Instant::now(), reply },
            cmd => Msg::Command(cmd, reply),
        }
    }
}

#[derive(Clone)]
pub struct WorkerHandle {
    tx: mpsc::Sender<Msg>,
}

fn stopped() -> AudioError {
    AudioError::Unavailable("The audio engine isn't running".into())
}

impl WorkerHandle {
    /// `Command::RestartAudio` rebuilds the backend and engine on the worker thread (see `AppCore::rebuild`).
    pub async fn command(&self, cmd: Command) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.tx.send(Msg::command(cmd, Some(tx))).map_err(|_| stopped())?;
        rx.await.map_err(|_| stopped())?
    }

    #[cfg(test)]
    pub fn command_blocking(&self, cmd: Command) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.tx.send(Msg::command(cmd, Some(tx))).map_err(|_| stopped())?;
        rx.blocking_recv().map_err(|_| stopped())?
    }

    #[cfg(test)]
    pub fn restart_blocking(&self) -> Result<()> {
        self.command_blocking(Command::RestartAudio)
    }

    pub async fn state(&self) -> Result<UiState> {
        let (tx, rx) = oneshot::channel();
        self.tx.send(Msg::GetState(tx)).map_err(|_| stopped())?;
        rx.await.map_err(|_| stopped())
    }

    /// Fire-and-forget (used by hotkeys, the tray menu and the control pipe).
    pub fn send(&self, cmd: Command) {
        let _ = self.tx.send(Msg::command(cmd, None));
    }

    pub fn set_metering(&self, active: bool) {
        let _ = self.tx.send(Msg::SetMetering(active));
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
            let mut worker = Worker {
                core: None,
                make_backend: &make_backend,
                make_engine: &make_engine,
                path: &settings_path,
                sink: &sink,
                last_restart_done: None,
            };
            worker.run(&rx);
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

/// Sends the state, and the mic's mute state if it differs from what the sink last saw.
fn publish<B: AudioBackend>(core: &AppCore<B>, sink: &impl EventSink, last_mic: &mut Option<bool>) {
    sink.state(core.state());
    let mic = core.mic_muted();
    if mic != *last_mic {
        if let Some(m) = mic {
            sink.mic_muted(m);
        }
        *last_mic = mic;
    }
}

/// Owns the audio core. `core` is `None` while the backend couldn't start (e.g. the Windows audio service wasn't
/// ready at login); the worker then retries every 2 s and on Restart audio, and answers requests as "not running".
struct Worker<'a, B: AudioBackend, S: EventSink> {
    core: Option<AppCore<B>>,
    make_backend: &'a dyn Fn() -> Result<B>,
    make_engine: &'a dyn Fn() -> Box<dyn AudioEngine>,
    path: &'a Path,
    sink: &'a S,
    /// When the last successful restart finished; restarts requested before then are already covered by it.
    last_restart_done: Option<Instant>,
}

impl<B: AudioBackend, S: EventSink> Worker<'_, B, S> {
    fn create_core(&mut self) -> Result<()> {
        let backend = (self.make_backend)()?;
        let mut core = AppCore::new(backend, Settings::load(self.path));
        core.set_engine((self.make_engine)());
        if let Err(e) = core.refresh() {
            tracing::warn!("initial refresh failed: {e}");
        }
        self.core = Some(core);
        Ok(())
    }

    /// Rebuilds the backend and engine (or creates the core if there is none) and tells the sink.
    /// The bool is whether the core was newly created.
    fn restart(&mut self) -> (Result<()>, bool) {
        tracing::info!("restarting audio");
        let created = self.core.is_none();
        let result = match self.core.as_mut() {
            // Build the new backend first: if that fails, the current one keeps running.
            Some(core) => (self.make_backend)().and_then(|backend| core.rebuild(backend, (self.make_engine)())),
            None => self.create_core(),
        };
        match &result {
            Ok(()) => self.last_restart_done = Some(Instant::now()),
            Err(e) => tracing::warn!("restart audio failed: {e}"),
        }
        self.sink.restarted(&result);
        (result, created)
    }

    fn run(&mut self, rx: &mpsc::Receiver<Msg>) {
        let sink = self.sink;
        let mut last_mic = None;
        let mut next_resync = Instant::now();
        match self.create_core() {
            Ok(()) => {
                if let Some(core) = &self.core {
                    publish(core, sink, &mut last_mic);
                }
                next_resync += RESYNC_INTERVAL;
            }
            Err(e) => {
                tracing::error!("audio backend failed to start: {e}");
                sink.notice(&e);
                next_resync += BACKEND_RETRY_INTERVAL;
            }
        }

        let mut watchdog = Watchdog::default();
        let mut watchdog_idle = true;
        let mut metering = false;
        let mut next_meter = Instant::now();
        let mut dirty_since: Option<Instant> = None;

        loop {
            let mut deadline = next_resync;
            if self.core.is_some() {
                if metering {
                    deadline = deadline.min(next_meter);
                }
                if let Some(d) = dirty_since {
                    deadline = deadline.min(d + SAVE_DEBOUNCE);
                }
            }
            let mut changed = false;
            match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
                Ok(Msg::Command(cmd, reply)) => {
                    let result = match self.core.as_mut() {
                        Some(core) => {
                            changed = true;
                            core.apply(cmd)
                        }
                        None => Err(stopped()),
                    };
                    if let Err(e) = &result {
                        tracing::info!("command failed: {e}");
                    }
                    if let Some(reply) = reply {
                        let _ = reply.send(result);
                    }
                }
                Ok(Msg::Restart { sent, reply }) => {
                    let result = if self.last_restart_done.is_some_and(|done| sent < done) {
                        tracing::debug!("restart audio already covered by the last one");
                        Ok(())
                    } else {
                        let (result, created) = self.restart();
                        // A restart can change the mute state the sink last saw.
                        last_mic = None;
                        changed = true;
                        if created && result.is_ok() {
                            next_resync = Instant::now() + RESYNC_INTERVAL;
                        }
                        result
                    };
                    if let Some(reply) = reply {
                        let _ = reply.send(result);
                    }
                }
                Ok(Msg::GetState(reply)) => {
                    let state = self.core.as_ref().map(|c| c.state().clone()).unwrap_or_default();
                    let _ = reply.send(state);
                }
                Ok(Msg::SetMetering(on)) => {
                    if let Some(core) = self.core.as_mut() {
                        if metering && !on {
                            core.stop_metering();
                        }
                        metering = on;
                        next_meter = Instant::now();
                    }
                }
                Ok(Msg::Shutdown(done)) => {
                    if let Some(core) = self.core.as_mut() {
                        stop(core, self.path, dirty_since.is_some());
                    }
                    if let Some(done) = done {
                        let _ = done.send(());
                    }
                    return;
                }
                Err(RecvTimeoutError::Disconnected) => {
                    if let Some(core) = self.core.as_mut() {
                        stop(core, self.path, dirty_since.is_some());
                    }
                    return;
                }
                Err(RecvTimeoutError::Timeout) => {}
            }

            let now = Instant::now();
            if self.core.is_none() {
                if now >= next_resync {
                    match self.create_core() {
                        Ok(()) => {
                            last_mic = None;
                            changed = true;
                            next_resync = now + RESYNC_INTERVAL;
                        }
                        Err(e) => {
                            tracing::debug!("audio backend still unavailable: {e}");
                            next_resync = now + BACKEND_RETRY_INTERVAL;
                        }
                    }
                }
                if self.core.is_none() {
                    continue;
                }
            }

            let mut restart_wanted = false;
            if let Some(core) = self.core.as_mut().filter(|_| now >= next_resync) {
                match core.refresh() {
                    Ok(c) => changed |= c,
                    Err(e) => tracing::warn!("refresh failed: {e}"),
                }
                if core.settings().ui.auto_restart_audio {
                    watchdog_idle = false;
                    let tick = watchdog.tick(true, &core.in_use_devices(), &core.present_devices());
                    for name in tick.disconnected {
                        sink.notice(&AudioError::NotFound(format!(
                            "{name} disconnected. Resonance will restart audio when it's back."
                        )));
                    }
                    restart_wanted = tick.restart;
                } else if !watchdog_idle {
                    watchdog = Watchdog::default();
                    watchdog_idle = true;
                }
                next_resync = now + RESYNC_INTERVAL;
            }
            if restart_wanted {
                let _ = self.restart();
                last_mic = None;
                changed = true;
            }
            let Some(core) = self.core.as_mut() else { continue };
            if metering && now >= next_meter {
                sink.meters(&core.meters());
                next_meter = now + METER_INTERVAL;
            }
            if changed {
                publish(core, sink, &mut last_mic);
            }
            for notice in core.take_notices() {
                sink.notice(&notice);
            }
            if core.take_settings_dirty() {
                dirty_since = Some(now);
            }
            if let Some(d) = dirty_since {
                if now >= d + SAVE_DEBOUNCE {
                    save(core, self.path);
                    dirty_since = None;
                }
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
        fn restarted(&self, result: &Result<()>) {
            let _ = self.0.send(format!("restarted ok={}", result.is_ok()));
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
    fn restart_recovers_when_the_backend_failed_at_startup() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, rx) = mpsc::channel();
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let count = calls.clone();
        let h = spawn(
            move || {
                if count.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                    Err(AudioError::Os("audio service not ready".into()))
                } else {
                    Ok(fake())
                }
            },
            || Box::new(NullEngine),
            dir.path().join("s.json"),
            TestSink(tx),
        );
        assert_eq!(wait_for(&rx, "notice"), "notice audio service not ready");
        h.restart_blocking().unwrap();
        assert_eq!(wait_for(&rx, "state"), "state apps=1");
        h.shutdown();
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
        h.restart_blocking().unwrap();
        assert_eq!(built.load(std::sync::atomic::Ordering::SeqCst), 2, "backend built at start and on restart");
        assert_eq!(wait_for(&rx, "restarted"), "restarted ok=true");
        wait_for(&rx, "state");
        h.shutdown();
    }

    #[test]
    fn restarts_queued_together_rebuild_once() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, rx) = mpsc::channel();
        let built = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let count = built.clone();
        let h = spawn(
            move || {
                // The first restart is slow, so the other two are queued behind it.
                if count.fetch_add(1, std::sync::atomic::Ordering::SeqCst) > 0 {
                    std::thread::sleep(Duration::from_millis(300));
                }
                Ok(fake())
            },
            || Box::new(NullEngine),
            dir.path().join("s.json"),
            TestSink(tx),
        );
        wait_for(&rx, "state");
        let replies: Vec<_> = (0..3)
            .map(|_| {
                let (tx, rx) = oneshot::channel();
                h.tx.send(Msg::command(Command::RestartAudio, Some(tx))).unwrap();
                rx
            })
            .collect();
        for reply in replies {
            reply.blocking_recv().unwrap().unwrap();
        }
        assert_eq!(built.load(std::sync::atomic::Ordering::SeqCst), 2, "one rebuild for all three requests");
        h.shutdown();
    }
}
