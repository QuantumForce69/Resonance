pub mod app_state;
pub mod backend;
pub mod error;
#[cfg(test)]
mod fake;
pub mod model;
pub mod paths;
pub mod settings;
#[cfg(windows)]
pub mod win;
pub mod worker;

pub fn run() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("error while running Resonance");
}
