use std::{
    sync::Mutex,
    time::{Duration, Instant},
};

use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, Rect, Window, WindowEvent};

use crate::worker::WorkerHandle;

const FLYOUT_W: f64 = 380.0;
const FLYOUT_H: f64 = 680.0;
const MARGIN: f64 = 12.0;

/// When the flyout last hid on focus loss: the tray click that caused it must not reopen it.
static LAST_FLYOUT_HIDE: Mutex<Option<Instant>> = Mutex::new(None);

/// Meters (and the mic capture stream) run only while some window is actually visible.
pub fn update_metering(app: &AppHandle) {
    let active = ["main", "flyout"]
        .iter()
        .filter_map(|l| app.get_webview_window(l))
        .any(|w| w.is_visible().unwrap_or(false) && !w.is_minimized().unwrap_or(false));
    if let Some(worker) = app.try_state::<WorkerHandle>() {
        worker.set_metering(active);
    }
}

pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
    update_metering(app);
}

pub fn toggle_flyout(app: &AppHandle, icon: Rect) {
    let Some(w) = app.get_webview_window("flyout") else { return };
    let just_hid = LAST_FLYOUT_HIDE
        .lock()
        .ok()
        .and_then(|t| *t)
        .map(|t| t.elapsed() < Duration::from_millis(300))
        .unwrap_or(false);
    if w.is_visible().unwrap_or(false) || just_hid {
        let _ = w.hide();
        update_metering(app);
        return;
    }
    let pos = icon.position.to_physical::<f64>(1.0);
    let size = icon.size.to_physical::<f64>(1.0);
    let (cx, cy) = (pos.x + size.width / 2.0, pos.y + size.height / 2.0);
    if let Ok(Some(mon)) = app.monitor_from_point(cx, cy) {
        let scale = mon.scale_factor();
        let wa = mon.work_area();
        let (fw, fh, m) = (FLYOUT_W * scale, FLYOUT_H * scale, MARGIN * scale);
        let (left, top) = (wa.position.x as f64, wa.position.y as f64);
        let (right, bottom) = (left + wa.size.width as f64, top + wa.size.height as f64);
        let x = (cx - fw / 2.0).clamp(left + m, (right - fw - m).max(left + m));
        // Taskbar at the top → open below it; otherwise open above the bottom edge.
        let y = if cy < top { top + m } else { bottom - fh - m };
        let _ = w.set_size(PhysicalSize::new(fw.round() as u32, fh.round() as u32));
        let _ = w.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
    }
    let _ = w.show();
    let _ = w.set_focus();
    update_metering(app);
}

pub fn on_window_event(window: &Window, event: &WindowEvent) {
    match event {
        WindowEvent::CloseRequested { api, .. } if window.label() == "main" => {
            api.prevent_close();
            let _ = window.hide();
        }
        WindowEvent::Focused(false) if window.label() == "flyout" => {
            let _ = window.hide();
            if let Ok(mut t) = LAST_FLYOUT_HIDE.lock() {
                *t = Some(Instant::now());
            }
        }
        _ => {}
    }
    if matches!(event, WindowEvent::CloseRequested { .. } | WindowEvent::Focused(_) | WindowEvent::Resized(_)) {
        update_metering(window.app_handle());
    }
}
