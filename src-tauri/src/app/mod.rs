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
                Box::new(crate::engine::NullEngine),
                settings_path.clone(),
                sink::TauriSink { app: handle.clone() },
            );
            app.manage(worker);
            let (headset, inbox) = crate::headset::HeadsetHandle::channel();
            app.manage(headset);
            let headset_sink = sink::HeadsetEvents { app: handle.clone() };
            let demo = std::env::var_os("RESONANCE_DEMO_HEADSET").is_some();
            tauri::async_runtime::spawn(async move {
                match crate::headset::LibConnector::open(paths::data_dir().join("openscq30.sqlite"), demo).await {
                    Ok(connector) => crate::headset::run(std::sync::Arc::new(connector), inbox, headset_sink).await,
                    Err(e) => {
                        tracing::error!("headphone support unavailable: {e}");
                        use crate::headset::{model::{Connection, HeadsetState}, HeadsetSink};
                        headset_sink.state(&HeadsetState::with_connection(Connection::NotFound));
                    }
                }
            });
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
            commands::headset_command,
            commands::get_headset,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Resonance");
}
