use std::{collections::HashMap, sync::Mutex};

use tauri::{AppHandle, Manager, State};

use super::{hotkeys, sink::Notices, wnd};
use crate::{
    error::{AudioError, Result},
    model::{Command, MicConfig, UiState},
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
