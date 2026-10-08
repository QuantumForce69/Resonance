//! Stream Deck plugin for Resonance: a key that mutes and unmutes the mic and shows whether it is muted
//! (green: live, red: muted, grey: Resonance isn't running). Stream Deck starts it with the WebSocket
//! port to talk on; Resonance is reached through its control pipe (see `src-tauri/src/app/control.rs`).
#![windows_subsystem = "windows"]

mod deck;
mod pipe;

use std::{fs::OpenOptions, io::Write, path::PathBuf, sync::mpsc};

fn main() {
    let Some(args) = deck::Args::parse(std::env::args().skip(1)) else {
        log("started without Stream Deck's -port/-pluginUUID/-registerEvent arguments");
        return;
    };
    let (tx, rx) = mpsc::channel();
    pipe::watch(tx);
    if let Err(e) = deck::run(&args, &rx) {
        log(&format!("lost Stream Deck: {e}"));
    }
}

/// Appends to `%APPDATA%\Resonance\logs\streamdeck-plugin.log`, next to Resonance's own logs.
fn log(message: &str) {
    let Some(dir) = std::env::var_os("APPDATA").map(|d| PathBuf::from(d).join("Resonance").join("logs")) else {
        return;
    };
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(dir.join("streamdeck-plugin.log")) {
        let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let _ = writeln!(file, "[{secs}] {message}");
    }
}
