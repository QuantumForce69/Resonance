use tauri::{
    image::Image,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager,
};

use super::wnd;
use crate::{model::Command, worker::WorkerHandle};

pub const TRAY_ID: &str = "resonance";
const ICON: &[u8] = include_bytes!("../../icons/tray.png");
const ICON_MUTED: &[u8] = include_bytes!("../../icons/tray-muted.png");

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open Resonance", true, None::<&str>)?;
    let mute = MenuItem::with_id(app, "mute", "Toggle microphone mute", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Resonance", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&open, &mute, &sep, &quit])?;
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(Image::from_bytes(ICON)?)
        .tooltip("Resonance")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => wnd::show_main(app),
            "mute" => app.state::<WorkerHandle>().send(Command::ToggleMicMute),
            "quit" => {
                app.state::<WorkerHandle>().shutdown_and_wait(std::time::Duration::from_secs(3));
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, rect, .. } = event {
                wnd::toggle_flyout(tray.app_handle(), rect);
            }
        })
        .build(app)?;
    Ok(())
}

pub fn set_mic_muted(app: &AppHandle, muted: bool) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else { return };
    if let Ok(img) = Image::from_bytes(if muted { ICON_MUTED } else { ICON }) {
        let _ = tray.set_icon(Some(img));
    }
    let _ = tray.set_tooltip(Some(if muted { "Resonance — microphone muted" } else { "Resonance" }));
}
