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
