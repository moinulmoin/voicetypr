use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::sync::Arc;

use super::transcriber::Transcriber;

/// Maximum number of models to keep in cache
/// Only cache the current model to minimize RAM usage (1-3GB per model)
const MAX_CACHE_SIZE: usize = 1;

/// Simple LRU cache that keeps loaded `Transcriber` models with size limits.
///
/// Loading a GGML model from disk can take hundreds of milliseconds and a lot
/// of RAM (1-3GB per model). By keeping a limited number of models in memory
/// we balance performance with memory usage.
pub struct TranscriberCache {
    /// Keyed by absolute path to the `.bin` model file.
    map: HashMap<String, Arc<Transcriber>>,
    /// Track access order for LRU eviction
    lru_order: VecDeque<String>,
    /// Maximum number of models to cache
    max_size: usize,
}

impl Default for TranscriberCache {
    fn default() -> Self {
        Self::new()
    }
}

impl TranscriberCache {
    /// Create an empty cache with default size limit.
    pub fn new() -> Self {
        Self::with_capacity(MAX_CACHE_SIZE)
    }

    /// Create a cache with a specific capacity.
    pub fn with_capacity(max_size: usize) -> Self {
        Self {
            map: HashMap::new(),
            lru_order: VecDeque::new(),
            max_size: max_size.max(1), // At least 1
        }
    }

    /// Retrieve a cached transcriber, or load and cache it if it isn't present yet.
    pub fn get_or_create(&mut self, model_path: &Path) -> Result<Arc<Transcriber>, String> {
        log::info!(
            "[TRANSCRIPTION_DEBUG] get_or_create called with path: {:?}",
            model_path
        );

        // Check if the model file exists
        if !model_path.exists() {
            let error = format!("Model file does not exist: {:?}", model_path);
            log::error!("[TRANSCRIPTION_DEBUG] {}", error);
            return Err(error);
        }

        // We store the path as a string key – this is fine because the path is
        // produced by the app itself and therefore always valid Unicode.
        let key = model_path.to_string_lossy().to_string();

        // Check if already cached
        if self.map.contains_key(&key) {
            log::info!("[TRANSCRIPTION_DEBUG] Model found in cache: {}", key);
            // Clone the transcriber before updating LRU
            let transcriber = self.map.get(&key).cloned();
            // Move to end of LRU order
            self.update_lru(&key);
            if let Some(t) = transcriber {
                return Ok(t);
            }
        }

        // Not cached – check if we need to evict
        if self.map.len() >= self.max_size {
            log::info!("[TRANSCRIPTION_DEBUG] Cache full, evicting LRU model");
            self.evict_lru();
        }

        // Load the model
        log::info!(
            "[TRANSCRIPTION_DEBUG] Loading new model into cache: {}",
            key
        );
        let start = std::time::Instant::now();

        let transcriber = match Transcriber::new(model_path) {
            Ok(t) => {
                let elapsed = start.elapsed();
                log::info!(
                    "[TRANSCRIPTION_DEBUG] Model loaded successfully in {:?}",
                    elapsed
                );
                Arc::new(t)
            }
            Err(e) => {
                log::error!("[TRANSCRIPTION_DEBUG] Failed to load model: {}", e);
                return Err(e);
            }
        };

        // Insert into cache
        self.map.insert(key.clone(), transcriber.clone());
        self.lru_order.push_back(key.clone());
        log::info!(
            "[TRANSCRIPTION_DEBUG] Model cached successfully. Cache size: {}/{}",
            self.map.len(),
            self.max_size
        );

        Ok(transcriber)
    }

    /// Update LRU order when a model is accessed
    fn update_lru(&mut self, key: &str) {
        // Remove from current position
        self.lru_order.retain(|k| k != key);
        // Add to end (most recently used)
        self.lru_order.push_back(key.to_string());
    }

    /// Evict the least recently used model
    fn evict_lru(&mut self) {
        if let Some(key) = self.lru_order.pop_front() {
            log::info!("Evicting model from cache: {}", key);
            self.map.remove(&key);
        }
    }

    /// Manually clear the cache (e.g. to free RAM or after a model upgrade).
    #[cfg(test)]
    pub fn clear(&mut self) {
        self.map.clear();
        self.lru_order.clear();
    }

    /// Get the current number of cached models
    #[cfg(test)]
    pub fn size(&self) -> usize {
        self.map.len()
    }

    /// Get the maximum cache size
    #[cfg(test)]
    pub fn capacity(&self) -> usize {
        self.max_size
    }
}
