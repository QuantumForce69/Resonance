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
