//! Resonance's control pipe: one connection stays open to hear the mic state, and each key press opens a
//! short-lived one to send a command (a synchronous pipe handle can't write while a read is pending).

use std::{
    fs::{File, OpenOptions},
    io::{self, BufRead, BufReader, Write},
    sync::mpsc::Sender,
    thread,
    time::Duration,
};

pub const PIPE_NAME: &str = r"\\.\pipe\resonance";
/// How often to look for Resonance while it isn't running.
const RETRY: Duration = Duration::from_secs(2);
const ERROR_PIPE_BUSY: i32 = 231;

/// Sends the mic state (`None` while Resonance can't be reached) each time it changes, forever.
pub fn watch(tx: Sender<Option<bool>>) {
    thread::spawn(move || loop {
        if let Ok(pipe) = open(PIPE_NAME) {
            for line in BufReader::new(pipe).lines() {
                let Ok(line) = line else { break };
                if let Some(muted) = parse_status(&line) {
                    if tx.send(Some(muted)).is_err() {
                        return;
                    }
                }
            }
        }
        if tx.send(None).is_err() {
            return;
        }
        thread::sleep(RETRY);
    });
}

/// Asks Resonance to mute or unmute the mic. False if Resonance isn't running.
pub fn toggle_mute() -> bool {
    open(PIPE_NAME).and_then(|mut pipe| pipe.write_all(b"{\"command\":\"toggleMicMute\"}\n")).is_ok()
}

/// Opens the pipe, waiting briefly if Resonance is between connections.
fn open(name: &str) -> io::Result<File> {
    let mut tries = 0;
    loop {
        match OpenOptions::new().read(true).write(true).open(name) {
            Err(e) if e.raw_os_error() == Some(ERROR_PIPE_BUSY) && tries < 25 => {
                tries += 1;
                thread::sleep(Duration::from_millis(20));
            }
            result => return result,
        }
    }
}

fn parse_status(line: &str) -> Option<bool> {
    serde_json::from_str::<serde_json::Value>(line).ok()?.get("micMuted")?.as_bool()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_mic_state() {
        assert_eq!(parse_status(r#"{"micMuted":true}"#), Some(true));
        assert_eq!(parse_status(r#"{"micMuted":false}"#), Some(false));
        assert_eq!(parse_status(r#"{"somethingElse":1}"#), None);
        assert_eq!(parse_status("garbage"), None);
    }
}
