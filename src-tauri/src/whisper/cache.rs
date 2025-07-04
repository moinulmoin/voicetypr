use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use super::transcriber::Transcriber;

/// Simple in-memory cache that keeps one `Transcriber` loaded per model path.
///
/// Loading a GGML model from disk can take hundreds of milliseconds and a lot
/// of RAM.  By keeping the model in memory we avoid doing that work every time
/// the user finishes a recording or drags a file onto the app.
#[derive(Default)]
pub struct TranscriberCache {
    /// Keyed by absolute path to the `.bin` model file.
    map: HashMap<String, Arc<Transcriber>>, //  String because PathBuf is not hash-stable across OSes
}

impl TranscriberCache {
    /// Create an empty cache.
    pub fn new() -> Self {
        Self { map: HashMap::new() }
    }

    /// Retrieve a cached transcriber, or load and cache it if it isn't present yet.
    pub fn get_or_create(&mut self, model_path: &Path) -> Result<Arc<Transcriber>, String> {
        // We store the path as a string key – this is fine because the path is
        // produced by the app itself and therefore always valid Unicode.
        let key = model_path.to_string_lossy().to_string();

        if let Some(t) = self.map.get(&key) {
            return Ok(t.clone());
        }

        // Not cached – load now.
        let transcriber = Arc::new(Transcriber::new(model_path)?);
        self.map.insert(key, transcriber.clone());
        Ok(transcriber)
    }

    /// Manually clear the cache (e.g. to free RAM or after a model upgrade).
    #[allow(dead_code)]
    pub fn clear(&mut self) {
        self.map.clear();
    }
}