use super::AppCore;
use crate::{backend::AudioBackend, engine::AudioEngine, error::Result};

impl<B: AudioBackend> AppCore<B> {
    /// Restart audio: stops the engine, swaps in a freshly built backend and engine, forgets per-run state, then
    /// re-reads everything. Saved app levels and routes are re-applied, and processing and call sends restart if on.
    /// Windows' default mics are left alone (the engine comes straight back), so calls don't see the mic flip.
    pub fn rebuild(&mut self, backend: B, engine: Box<dyn AudioEngine>) -> Result<()> {
        self.set_engine(engine);
        self.engine_state = Default::default();
        self.backend = backend;
        self.seen_sessions.clear();
        self.state.routing_available = self.backend.routing_available();
        // The new backend is in place; a failed read here is picked up by the next periodic resync.
        if let Err(e) = self.refresh() {
            tracing::warn!("refresh after restart failed: {e}");
        }
        Ok(())
    }
}
