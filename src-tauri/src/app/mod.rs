mod commands;
mod control;
mod hotkeys;
mod logging;
mod restart;
mod sink;
mod tray;
mod wnd;

use tauri::Manager;

use crate::{error::AudioError, paths, settings::Settings, worker};

pub fn run() {
    let _log_guard = logging::init(&paths::logs_dir());
    let settings_path = paths::settings_path();
    let initial = Settings::load(&settings_path);
    // Set by the Start with Windows entry: stay in the tray whatever "Start in the tray" says.
    let args: Vec<String> = std::env::args().collect();
    let launched_minimized = restart::wants_restart(&args) || args.iter().any(|a| a == crate::win::startup::MINIMIZED_ARG);
    tracing::info!("Resonance starting");

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            if restart::wants_restart(&args) {
                restart::spawn_restart(app);
            } else {
                wnd::show_main(app);
            }
        }))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(commands::IconCache::default())
        .manage(sink::Notices::default())
        .manage(wnd::VisibleWindows::default())
        .manage(control::MicStatus::default())
        .setup(move |app| {
            let handle = app.handle().clone();
            let worker = worker::spawn(
                crate::win::WasapiBackend::new,
                || Box::new(crate::win::WasapiEngine::new()) as Box<dyn crate::engine::AudioEngine>,
                settings_path.clone(),
                sink::TauriSink { app: handle.clone() },
            );
            let commands = worker.clone();
            app.manage(worker);
            // Lets companion apps (the Stream Deck plugin) show and toggle the mic's mute state.
            let mic_status = app.state::<control::MicStatus>().subscribe();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = control::serve(control::PIPE_NAME, mic_status, move |cmd| commands.send(cmd)).await {
                    tracing::warn!("companion control channel unavailable: {e}");
                }
            });
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
            // Keep Start with Windows pointing at this installed copy, off the main thread so the window
            // isn't delayed. Development builds leave it alone.
            #[cfg(not(debug_assertions))]
            std::thread::spawn(|| {
                if let Err(e) = crate::win::startup::refresh() {
                    tracing::warn!("couldn't refresh Start with Windows: {e}");
                }
            });
            if let Err(e) = hotkeys::apply(&handle, &initial.mic) {
                tracing::warn!("{e}");
                sink::push_notice(&handle, AudioError::Invalid(e));
            }
            if !initial.ui.start_minimized && !launched_minimized {
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
            commands::open_vb_cable,
            commands::save_profile,
            commands::update_profile,
            commands::apply_profile,
            commands::get_autostart,
            commands::set_autostart,
            commands::restart_audio,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Resonance")
        .run(|app, event| match event {
            // Windows are closed whenever they leave the screen; Resonance lives on in the tray.
            tauri::RunEvent::ExitRequested { code: None, api, .. } => api.prevent_exit(),
            tauri::RunEvent::Exit => {
                // Stop the engine and give Windows its default mics back before the process ends.
                if let Some(worker) = app.try_state::<worker::WorkerHandle>() {
                    worker.shutdown_and_wait(std::time::Duration::from_secs(3));
                }
            }
            _ => {}
        });
}
