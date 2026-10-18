//! A local control channel for companion apps such as the Stream Deck plugin: the named pipe
//! `\\.\pipe\resonance`, carrying one JSON object per line.
//!
//! Every connection is sent `{"micMuted":true}` (or `false`) as soon as the state is known and again each time
//! it changes. A connection may send `{"command":"toggleMicMute"}`, `{"command":"setMicMute","muted":true}` or
//! `{"command":"restartAudio"}`.
//! The pipe only accepts local clients, and only the signed-in user can write to it.

use std::{io, sync::Arc};

use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::windows::named_pipe::{NamedPipeServer, ServerOptions},
    sync::watch,
};

use crate::model::Command;

pub const PIPE_NAME: &str = r"\\.\pipe\resonance";

#[derive(Debug, PartialEq, Deserialize)]
#[serde(tag = "command", rename_all = "camelCase")]
enum Request {
    ToggleMicMute,
    SetMicMute { muted: bool },
    RestartAudio,
}

impl From<Request> for Command {
    fn from(request: Request) -> Self {
        match request {
            Request::ToggleMicMute => Command::ToggleMicMute,
            Request::SetMicMute { muted } => Command::SetMicMute { muted },
            Request::RestartAudio => Command::RestartAudio,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Status {
    mic_muted: bool,
}

/// The mic state companions are told about (`None` until it is known). The event sink keeps it current.
pub struct MicStatus(watch::Sender<Option<bool>>);

impl Default for MicStatus {
    fn default() -> Self {
        Self(watch::Sender::new(None))
    }
}

impl MicStatus {
    pub fn set(&self, muted: bool) {
        self.0.send_replace(Some(muted));
    }

    pub fn subscribe(&self) -> watch::Receiver<Option<bool>> {
        self.0.subscribe()
    }
}

type OnCommand = Arc<dyn Fn(Command) + Send + Sync>;

/// Serves the pipe `name` until it can't create another instance (another Resonance already owns the name).
pub async fn serve(
    name: &str,
    status: watch::Receiver<Option<bool>>,
    on_command: impl Fn(Command) + Send + Sync + 'static,
) -> io::Result<()> {
    let on_command: OnCommand = Arc::new(on_command);
    let mut server = ServerOptions::new().first_pipe_instance(true).create(name)?;
    loop {
        server.connect().await?;
        // Open the next instance before handing this one over, so a new client never finds the name missing.
        let client = std::mem::replace(&mut server, ServerOptions::new().create(name)?);
        tokio::spawn(handle(client, status.clone(), on_command.clone()));
    }
}

async fn handle(pipe: NamedPipeServer, mut status: watch::Receiver<Option<bool>>, on_command: OnCommand) {
    let (read, mut write) = tokio::io::split(pipe);
    let mut lines = BufReader::new(read).lines();
    // A client that only sends a command may close before reading anything; keep reading its commands.
    let mut can_write = true;
    status.mark_changed();
    loop {
        tokio::select! {
            changed = status.changed(), if can_write => {
                if changed.is_err() {
                    return;
                }
                let Some(muted) = *status.borrow_and_update() else { continue };
                let mut line = serde_json::to_string(&Status { mic_muted: muted }).unwrap_or_default();
                line.push('\n');
                can_write = write.write_all(line.as_bytes()).await.is_ok();
            }
            line = lines.next_line() => match line {
                Ok(Some(line)) => match serde_json::from_str::<Request>(&line) {
                    Ok(request) => on_command(request.into()),
                    Err(e) => tracing::debug!("ignored control request {line:?}: {e}"),
                },
                _ => return,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::{
        io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
        net::windows::named_pipe::{ClientOptions, NamedPipeClient},
        sync::mpsc,
        time::timeout,
    };

    use super::*;

    #[test]
    fn parses_requests() {
        assert_eq!(serde_json::from_str::<Request>(r#"{"command":"toggleMicMute"}"#).unwrap(), Request::ToggleMicMute);
        assert_eq!(
            serde_json::from_str::<Request>(r#"{"command":"setMicMute","muted":true}"#).unwrap(),
            Request::SetMicMute { muted: true }
        );
        assert!(serde_json::from_str::<Request>(r#"{"command":"formatDisk"}"#).is_err());
        assert_eq!(
            serde_json::from_str::<Request>(r#"{"command":"restartAudio"}"#).unwrap(),
            Request::RestartAudio
        );
        assert_eq!(
            Command::from(Request::SetMicMute { muted: false }),
            Command::SetMicMute { muted: false }
        );
    }

    fn start(name: &'static str) -> (MicStatus, mpsc::UnboundedReceiver<Command>) {
        let status = MicStatus::default();
        let (tx, rx) = mpsc::unbounded_channel();
        let watch = status.subscribe();
        tokio::spawn(async move { serve(name, watch, move |c| drop(tx.send(c))).await });
        (status, rx)
    }

    async fn connect(name: &str) -> NamedPipeClient {
        for _ in 0..100 {
            if let Ok(client) = ClientOptions::new().open(name) {
                return client;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("couldn't connect to {name}");
    }

    #[tokio::test]
    async fn reports_the_mic_state_and_takes_commands() {
        let name = r"\\.\pipe\resonance-test-state";
        let (status, mut commands) = start(name);
        status.set(true);
        let mut client = BufReader::new(connect(name).await);
        let mut line = String::new();
        timeout(Duration::from_secs(2), client.read_line(&mut line)).await.unwrap().unwrap();
        assert_eq!(line, "{\"micMuted\":true}\n");

        status.set(false);
        line.clear();
        timeout(Duration::from_secs(2), client.read_line(&mut line)).await.unwrap().unwrap();
        assert_eq!(line, "{\"micMuted\":false}\n");

        client.get_mut().write_all(b"nonsense\n{\"command\":\"toggleMicMute\"}\n").await.unwrap();
        let cmd = timeout(Duration::from_secs(2), commands.recv()).await.unwrap();
        assert_eq!(cmd, Some(Command::ToggleMicMute));
    }

    #[tokio::test]
    async fn takes_a_command_from_a_client_that_hangs_up_straight_away() {
        let name = r"\\.\pipe\resonance-test-oneshot";
        let (status, mut commands) = start(name);
        status.set(false);
        for _ in 0..2 {
            let mut client = connect(name).await;
            client.write_all(b"{\"command\":\"setMicMute\",\"muted\":true}\n").await.unwrap();
            drop(client);
            let cmd = timeout(Duration::from_secs(2), commands.recv()).await.unwrap();
            assert_eq!(cmd, Some(Command::SetMicMute { muted: true }));
        }
    }

    #[tokio::test]
    async fn restart_audio_reaches_the_handler() {
        let name = r"\\.\pipe\resonance-test-restart";
        let (status, mut controls) = start(name);
        status.set(false);
        let mut client = connect(name).await;
        client.write_all(b"{\"command\":\"restartAudio\"}\n").await.unwrap();
        drop(client);
        let got = timeout(Duration::from_secs(2), controls.recv()).await.unwrap();
        assert_eq!(got, Some(Command::RestartAudio));
    }

    #[tokio::test]
    async fn a_second_server_cannot_take_the_name() {
        let name = r"\\.\pipe\resonance-test-owner";
        let (_status, _commands) = start(name);
        let _client = connect(name).await;
        let second = serve(name, MicStatus::default().subscribe(), |_| {}).await;
        assert!(second.is_err());
    }
}
