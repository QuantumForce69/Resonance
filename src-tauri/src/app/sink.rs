use std::sync::Mutex;

use tauri::{AppHandle, Emitter, Manager};

use super::{control::MicStatus, tray, wnd::VisibleWindows};
use crate::{
    error::{AudioError, Result},
    headset::{model::HeadsetCommand, HeadsetHandle},
    model::{MeterFrame, UiState},
    worker::EventSink,
};

/// Notices raised before any window was listening; drained by `take_notices`.
#[derive(Default)]
pub struct Notices(pub Mutex<Vec<AudioError>>);

pub fn push_notice(app: &AppHandle, error: AudioError) {
    if let Ok(mut n) = app.state::<Notices>().0.lock() {
        n.push(error.clone());
    }
    let _ = app.emit("notice", error);
}

pub struct TauriSink {
    pub app: AppHandle,
}

impl EventSink for TauriSink {
    fn state(&self, state: &UiState) {
        let _ = self.app.emit("state", state);
    }

    fn meters(&self, frame: &MeterFrame) {
        let visible = self.app.state::<VisibleWindows>().0.lock().map(|v| v.clone()).unwrap_or_default();
        for label in visible {
            let _ = self.app.emit_to(label, "meters", frame);
        }
    }

    fn mic_muted(&self, muted: bool) {
        self.app.state::<MicStatus>().set(muted);
        let app = self.app.clone();
        let _ = self.app.run_on_main_thread(move || tray::set_mic_muted(&app, muted));
        let _ = self.app.emit("mic-muted", muted);
    }

    fn notice(&self, error: &AudioError) {
        push_notice(&self.app, error.clone());
    }

    fn restarted(&self, result: &Result<()>) {
        // The headphones reconnect even if the audio side couldn't be rebuilt. This runs on the worker thread.
        if let Some(headset) = self.app.try_state::<HeadsetHandle>() {
            let headset = headset.inner().clone();
            tauri::async_runtime::spawn(async move {
                let _ = headset.command(HeadsetCommand::Retry).await;
            });
        }
        if let Err(e) = result {
            push_notice(&self.app, AudioError::Unavailable(format!("Couldn't restart audio: {e}")));
        }
    }
}

pub struct HeadsetEvents {
    pub app: AppHandle,
}

impl crate::headset::HeadsetSink for HeadsetEvents {
    fn state(&self, state: &crate::headset::model::HeadsetState) {
        let _ = self.app.emit("headset", state);
    }
}
