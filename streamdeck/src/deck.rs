//! The Stream Deck side: registers with Stream Deck over its WebSocket and keeps every Mic Mute key
//! showing the mic state.

use std::{collections::BTreeSet, io::ErrorKind, sync::mpsc::Receiver, sync::OnceLock, time::Duration};

use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json, Value};
use tungstenite::{stream::MaybeTlsStream, Message};

use crate::pipe;

/// How long to wait for Stream Deck before checking for a new mic state; bounds how stale a key can be.
const POLL: Duration = Duration::from_millis(50);

/// The launch arguments Stream Deck passes every plugin.
#[derive(Debug, PartialEq)]
pub struct Args {
    port: u16,
    uuid: String,
    register_event: String,
}

impl Args {
    pub fn parse(args: impl IntoIterator<Item = String>) -> Option<Self> {
        let (mut port, mut uuid, mut register_event) = (None, None, None);
        let mut args = args.into_iter();
        while let Some(flag) = args.next() {
            let value = args.next();
            match flag.as_str() {
                "-port" => port = value.and_then(|v| v.parse().ok()),
                "-pluginUUID" => uuid = value,
                "-registerEvent" => register_event = value,
                _ => {}
            }
        }
        Some(Self { port: port?, uuid: uuid?, register_event: register_event? })
    }
}

#[derive(Debug, PartialEq)]
pub enum Effect {
    Send(Value),
    ToggleMute { context: String },
}

/// The visible Mic Mute keys and the mic state they show.
#[derive(Default)]
pub struct Keys {
    contexts: BTreeSet<String>,
    muted: Option<bool>,
}

impl Keys {
    pub fn on_event(&mut self, text: &str) -> Vec<Effect> {
        let Ok(event) = serde_json::from_str::<Value>(text) else { return Vec::new() };
        let Some(context) = event["context"].as_str() else { return Vec::new() };
        match event["event"].as_str() {
            Some("willAppear") => {
                self.contexts.insert(context.to_string());
                self.show(context)
            }
            Some("willDisappear") => {
                self.contexts.remove(context);
                Vec::new()
            }
            Some("keyDown") => vec![Effect::ToggleMute { context: context.to_string() }],
            _ => Vec::new(),
        }
    }

    pub fn on_mic(&mut self, muted: Option<bool>) -> Vec<Effect> {
        if muted == self.muted {
            return Vec::new();
        }
        self.muted = muted;
        self.contexts.iter().flat_map(|c| self.show(c)).collect()
    }

    fn show(&self, context: &str) -> Vec<Effect> {
        match self.muted {
            Some(muted) => vec![
                Effect::Send(json!({ "event": "setState", "context": context, "payload": { "state": u8::from(muted) } })),
                // No image restores the state's own image, replacing the grey one shown while Resonance was away.
                Effect::Send(json!({ "event": "setImage", "context": context, "payload": {} })),
            ],
            None => vec![Effect::Send(
                json!({ "event": "setImage", "context": context, "payload": { "image": offline_image() } }),
            )],
        }
    }
}

fn offline_image() -> &'static str {
    static IMAGE: OnceLock<String> = OnceLock::new();
    IMAGE.get_or_init(|| {
        let png = include_bytes!("../com.resonance.streamdeck.sdPlugin/imgs/mic-offline@2x.png");
        format!("data:image/png;base64,{}", STANDARD.encode(png))
    })
}

/// Talks to Stream Deck until it closes the connection.
pub fn run(args: &Args, mic: &Receiver<Option<bool>>) -> tungstenite::Result<()> {
    let (mut ws, _) = tungstenite::connect(format!("ws://127.0.0.1:{}", args.port))?;
    if let MaybeTlsStream::Plain(stream) = ws.get_mut() {
        stream.set_read_timeout(Some(POLL))?;
    }
    ws.send(Message::text(json!({ "event": args.register_event, "uuid": args.uuid }).to_string()))?;
    let mut keys = Keys::default();
    loop {
        let mut effects = Vec::new();
        while let Ok(muted) = mic.try_recv() {
            effects.extend(keys.on_mic(muted));
        }
        match ws.read() {
            Ok(Message::Text(text)) => effects.extend(keys.on_event(&text)),
            Ok(Message::Close(_)) => return Ok(()),
            Ok(_) => {}
            Err(tungstenite::Error::Io(e)) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(e) => return Err(e),
        }
        for effect in effects {
            match effect {
                Effect::Send(message) => ws.send(Message::text(message.to_string()))?,
                Effect::ToggleMute { context } => {
                    if !pipe::toggle_mute() {
                        ws.send(Message::text(json!({ "event": "showAlert", "context": context }).to_string()))?;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn appear(context: &str) -> String {
        json!({ "event": "willAppear", "action": "com.resonance.streamdeck.mic-mute", "context": context }).to_string()
    }

    fn state(context: &str, state: u8) -> Effect {
        Effect::Send(json!({ "event": "setState", "context": context, "payload": { "state": state } }))
    }

    fn reset_image(context: &str) -> Effect {
        Effect::Send(json!({ "event": "setImage", "context": context, "payload": {} }))
    }

    #[test]
    fn parses_stream_deck_arguments() {
        let args = ["-port", "28196", "-pluginUUID", "abc", "-registerEvent", "registerPlugin", "-info", "{}"];
        assert_eq!(
            Args::parse(args.map(String::from)),
            Some(Args { port: 28196, uuid: "abc".into(), register_event: "registerPlugin".into() })
        );
        assert_eq!(Args::parse(["-port", "1"].map(String::from)), None);
    }

    #[test]
    fn a_new_key_shows_grey_until_resonance_answers() {
        let mut keys = Keys::default();
        let effects = keys.on_event(&appear("k1"));
        assert_eq!(effects.len(), 1);
        let Effect::Send(message) = &effects[0] else { panic!() };
        assert_eq!(message["event"], "setImage");
        assert!(message["payload"]["image"].as_str().unwrap().starts_with("data:image/png;base64,iVBOR"));
    }

    #[test]
    fn keys_follow_the_mic_state() {
        let mut keys = Keys::default();
        keys.on_event(&appear("k1"));
        keys.on_event(&appear("k2"));
        assert_eq!(keys.on_mic(Some(true)), vec![state("k1", 1), reset_image("k1"), state("k2", 1), reset_image("k2")]);
        assert_eq!(keys.on_mic(Some(true)), vec![], "no repeat for an unchanged state");
        keys.on_event(&json!({ "event": "willDisappear", "context": "k2" }).to_string());
        assert_eq!(keys.on_mic(Some(false)), vec![state("k1", 0), reset_image("k1")]);
        // A key that appears later starts in the current state.
        assert_eq!(keys.on_event(&appear("k3")), vec![state("k3", 0), reset_image("k3")]);
    }

    #[test]
    fn a_press_toggles_the_mute() {
        let mut keys = Keys::default();
        let press = json!({ "event": "keyDown", "context": "k1", "payload": {} }).to_string();
        assert_eq!(keys.on_event(&press), vec![Effect::ToggleMute { context: "k1".into() }]);
        assert_eq!(keys.on_event("not json"), vec![]);
    }
}
