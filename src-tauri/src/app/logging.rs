use std::path::Path;

use tracing_appender::{non_blocking::WorkerGuard, rolling};
use tracing_subscriber::EnvFilter;

/// Daily-rotated log files in `dir`, keeping 7. Keep the guard alive for the app's lifetime.
pub fn init(dir: &Path) -> Option<WorkerGuard> {
    std::fs::create_dir_all(dir).ok()?;
    let appender = rolling::Builder::new()
        .rotation(rolling::Rotation::DAILY)
        .filename_prefix("resonance")
        .filename_suffix("log")
        .max_log_files(7)
        .build(dir)
        .ok()?;
    let (writer, guard) = tracing_appender::non_blocking(appender);
    tracing_subscriber::fmt()
        .with_writer(writer)
        .with_ansi(false)
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .try_init()
        .ok()?;
    Some(guard)
}
