use std::{collections::HashMap, sync::Mutex};

use tauri::{AppHandle, Manager, State};

use super::{
    hotkeys,
    sink::{push_notice, Notices},
    wnd,
};
use crate::{
    error::{AudioError, Result},
    headset::{model::Connection, part::HeadsetPart, HeadsetHandle},
    model::{Command, MicConfig, UiState},
    profile::PartChoice,
    worker::WorkerHandle,
};

#[derive(Default)]
pub struct IconCache(Mutex<HashMap<String, Option<String>>>);

#[tauri::command]
pub async fn command(worker: State<'_, WorkerHandle>, cmd: Command) -> Result<()> {
    worker.command(cmd).await
}

#[tauri::command]
pub async fn get_state(worker: State<'_, WorkerHandle>) -> Result<UiState> {
    worker.state().await
}

#[tauri::command]
pub async fn get_app_icon(cache: State<'_, IconCache>, exe_path: String) -> Result<Option<String>> {
    if let Some(hit) = cache.0.lock().ok().and_then(|c| c.get(&exe_path).cloned()) {
        return Ok(hit);
    }
    let path = exe_path.clone();
    let icon = tauri::async_runtime::spawn_blocking(move || crate::win::exe_icon_png_base64(&path)).await.unwrap_or(None);
    if let Ok(mut c) = cache.0.lock() {
        c.insert(exe_path, icon.clone());
    }
    Ok(icon)
}

#[tauri::command]
pub async fn set_mic_hotkey(app: AppHandle, worker: State<'_, WorkerHandle>, config: MicConfig) -> Result<()> {
    hotkeys::apply(&app, &config).map_err(AudioError::Invalid)?;
    worker.command(Command::SetMicConfig { config }).await
}

#[tauri::command]
pub fn take_notices(notices: State<'_, Notices>) -> Vec<AudioError> {
    notices.0.lock().map(|mut n| std::mem::take(&mut *n)).unwrap_or_default()
}

#[tauri::command]
pub fn show_main(app: AppHandle) {
    if let Some(f) = app.get_webview_window("flyout") {
        let _ = f.hide();
    }
    wnd::show_main(&app);
}

#[tauri::command]
pub async fn headset_command(
    headset: State<'_, crate::headset::HeadsetHandle>,
    cmd: crate::headset::model::HeadsetCommand,
) -> Result<()> {
    headset.command(cmd).await
}

#[tauri::command]
pub async fn get_headset(headset: State<'_, crate::headset::HeadsetHandle>) -> Result<crate::headset::model::HeadsetState> {
    headset.state().await
}

/// VB-Cable's download page (the only URL this command opens).
const VB_CABLE_URL: &str = "https://vb-audio.com/Cable/";

#[tauri::command]
pub fn open_vb_cable() {
    crate::win::open_url(VB_CABLE_URL);
}

async fn headset_part(headset: &HeadsetHandle) -> Option<HeadsetPart> {
    headset.state().await.ok().and_then(|s| HeadsetPart::from_state(&s))
}

#[tauri::command]
pub async fn save_profile(
    worker: State<'_, WorkerHandle>,
    headset: State<'_, HeadsetHandle>,
    name: String,
    parts: PartChoice,
) -> Result<()> {
    let snapshot = if parts.headphones { headset_part(&headset).await } else { None };
    worker.command(Command::SaveProfile { name, parts, headset: snapshot }).await
}

#[tauri::command]
pub async fn update_profile(worker: State<'_, WorkerHandle>, headset: State<'_, HeadsetHandle>, id: String) -> Result<()> {
    let snapshot = headset_part(&headset).await;
    worker.command(Command::UpdateProfile { id, headset: snapshot }).await
}

/// Applies the audio parts on the audio worker, then the headphone part through the headset manager.
#[tauri::command]
pub async fn apply_profile(
    app: AppHandle,
    worker: State<'_, WorkerHandle>,
    headset: State<'_, HeadsetHandle>,
    id: String,
) -> Result<()> {
    let part = worker.state().await?.profiles.into_iter().find(|p| p.id == id).and_then(|p| p.parts.headphones);
    worker.command(Command::ApplyProfile { id }).await?;
    if let Some(part) = part {
        let connected = headset.state().await.is_ok_and(|s| s.connection == Connection::Connected);
        if !connected {
            push_notice(&app, AudioError::Unavailable("The Space One isn't connected, so its profile settings weren't applied".into()));
            return Ok(());
        }
        for cmd in part.commands() {
            if let Err(e) = headset.command(cmd).await {
                push_notice(&app, e);
                break;
            }
        }
    }
    Ok(())
}

#[tauri::command]
pub fn get_autostart() -> Result<bool> {
    crate::win::startup::is_enabled()
}

#[tauri::command]
pub fn set_autostart(enabled: bool) -> Result<()> {
    if enabled {
        crate::win::startup::enable()
    } else {
        crate::win::startup::disable()
    }
}
