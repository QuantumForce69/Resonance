mod backend;
pub mod boost;
mod com;
mod endpoints;
mod engine;
mod icon;
mod loopback;
pub mod meter;
mod policy;
mod procinfo;
mod shell;
pub mod startup;
pub mod routing;
mod watcher;
mod util;

pub use backend::WasapiBackend;
pub use engine::WasapiEngine;
pub use icon::exe_icon_png_base64;
pub use shell::open_url;

/// Whether this Windows can capture single apps (process loopback). Needs COM on the calling thread.
pub fn app_capture_supported() -> bool {
    loopback::supported()
}
