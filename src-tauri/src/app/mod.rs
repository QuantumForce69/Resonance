mod commands;
mod hotkeys;
mod logging;
mod sink;
mod tray;
mod wnd;

use tauri::Manager;

use crate::{error::AudioError, paths, settings::Settings, worker};

pub fn run() {
    let _log_guard = logging::init(&paths::logs_dir());
    let settings_path = paths::settings_path();
    let initial = Settings::load(&settings_path);
    tracing::info!("Resonance starting");

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| wnd::show_main(app)))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(commands::IconCache::default())
        .manage(sink::Notices::default())
        .setup(move |app| {
            let handle = app.handle().clone();
            let worker = worker::spawn(
                crate::win::WasapiBackend::new,
                settings_path.clone(),
                crate::win::accent_hex(),
                sink::TauriSink { app: handle.clone() },
            );
            app.manage(worker);
            tray::create(&handle)?;
            if let Err(e) = hotkeys::apply(&handle, &initial.mic) {
                tracing::warn!("{e}");
                sink::push_notice(&handle, AudioError::Invalid(e));
            }
            if !initial.ui.start_minimized {
                wnd::show_main(&handle);
            }
            Ok(())
        })
        .on_window_event(wnd::on_window_event)
        .invoke_handler(tauri::generate_handler![
            commands::command,
            commands::get_state,
            commands::get_app_icon,
            commands::set_mic_hotkey,
            commands::take_notices,
            commands::show_main,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Resonance");
}
