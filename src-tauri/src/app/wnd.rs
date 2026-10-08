use std::{
    sync::Mutex,
    time::{Duration, Instant},
};

use tauri::{
    AppHandle, Manager, PhysicalPosition, PhysicalSize, Rect, WebviewWindow, WebviewWindowBuilder, Window, WindowEvent,
};

use crate::worker::WorkerHandle;

const FLYOUT_W: f64 = 380.0;
const FLYOUT_H: f64 = 680.0;
const MARGIN: f64 = 12.0;
/// Focus flickers (gained, lost, gained) while a new flyout's web view takes it; a loss counts only if it
/// lasts this long.
const FOCUS_SETTLE: Duration = Duration::from_millis(150);

/// When the flyout last closed on focus loss: the tray click that caused it must not reopen it.
static LAST_FLYOUT_CLOSE: Mutex<Option<Instant>> = Mutex::new(None);

/// Windows that are on screen right now; meter frames go only to these.
#[derive(Default)]
pub struct VisibleWindows(pub Mutex<Vec<&'static str>>);

/// Meters (and the mic capture stream) run only while some window is actually visible, and frames are sent
/// only to the visible ones.
pub fn update_metering(app: &AppHandle) {
    let visible: Vec<&'static str> = ["main", "flyout"]
        .into_iter()
        .filter(|l| {
            app.get_webview_window(l)
                .is_some_and(|w| w.is_visible().unwrap_or(false) && !w.is_minimized().unwrap_or(false))
        })
        .collect();
    let active = !visible.is_empty();
    if let Ok(mut v) = app.state::<VisibleWindows>().0.lock() {
        *v = visible;
    }
    if let Some(worker) = app.try_state::<WorkerHandle>() {
        worker.set_metering(active);
    }
}

/// The window `label`, creating it from its `tauri.conf.json` entry if it isn't open. Windows are closed
/// (not hidden) when they leave the screen, so a Resonance sitting in the tray keeps no web engine loaded.
fn open_window(app: &AppHandle, label: &str) -> Option<WebviewWindow> {
    if let Some(w) = app.get_webview_window(label) {
        return Some(w);
    }
    let config = app.config().app.windows.iter().find(|w| w.label == label)?.clone();
    match WebviewWindowBuilder::from_config(app, &config).and_then(|b| b.build()) {
        Ok(w) => Some(w),
        Err(e) => {
            tracing::error!("couldn't open the {label} window: {e}");
            None
        }
    }
}

pub fn show_main(app: &AppHandle) {
    if let Some(w) = open_window(app, "main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
    update_metering(app);
}

pub fn close_flyout(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("flyout") {
        let _ = w.close();
    }
}

pub fn toggle_flyout(app: &AppHandle, icon: Rect) {
    let just_closed = LAST_FLYOUT_CLOSE
        .lock()
        .ok()
        .and_then(|t| *t)
        .map(|t| t.elapsed() < Duration::from_millis(300))
        .unwrap_or(false);
    if app.get_webview_window("flyout").is_some() || just_closed {
        close_flyout(app);
        return;
    }
    let Some(w) = open_window(app, "flyout") else { return };
    let pos = icon.position.to_physical::<f64>(1.0);
    let size = icon.size.to_physical::<f64>(1.0);
    let (cx, cy) = (pos.x + size.width / 2.0, pos.y + size.height / 2.0);
    if let Ok(Some(mon)) = app.monitor_from_point(cx, cy) {
        let scale = mon.scale_factor();
        let wa = mon.work_area();
        let (fw, fh, m) = (FLYOUT_W * scale, FLYOUT_H * scale, MARGIN * scale);
        let _ = w.set_size(PhysicalSize::new(fw.round() as u32, fh.round() as u32));
        // Place by the outer size: the window has an invisible frame around its content, and if that frame
        // overlapped the taskbar it would swallow clicks on the tray icon.
        let outer = w.outer_size().map(|s| (s.width as f64, s.height as f64)).unwrap_or((fw, fh));
        let (left, top) = (wa.position.x as f64, wa.position.y as f64);
        let (right, bottom) = (left + wa.size.width as f64, top + wa.size.height as f64);
        let x = (cx - outer.0 / 2.0).clamp(left + m, (right - outer.0 - m).max(left + m));
        // Taskbar at the top → open below it; otherwise open above the bottom edge.
        let y = if cy < top { top + m } else { bottom - outer.1 - m };
        let _ = w.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
    }
    let _ = w.show();
    let _ = w.set_focus();
    update_metering(app);
}

pub fn on_window_event(window: &Window, event: &WindowEvent) {
    if let WindowEvent::Focused(false) = event {
        if window.label() == "flyout" {
            let app = window.app_handle().clone();
            std::thread::spawn(move || {
                std::thread::sleep(FOCUS_SETTLE);
                if let Some(w) = app.get_webview_window("flyout") {
                    if !w.is_focused().unwrap_or(false) {
                        if let Ok(mut t) = LAST_FLYOUT_CLOSE.lock() {
                            *t = Some(Instant::now());
                        }
                        let _ = w.close();
                    }
                }
            });
        }
    }
    if matches!(event, WindowEvent::Destroyed | WindowEvent::Focused(_) | WindowEvent::Resized(_)) {
        update_metering(window.app_handle());
    }
}
