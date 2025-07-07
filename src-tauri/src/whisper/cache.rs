use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::sync::Arc;

use super::transcriber::Transcriber;

/// Maximum number of models to keep in cache
const MAX_CACHE_SIZE: usize = 2;

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
        // We store the path as a string key – this is fine because the path is
        // produced by the app itself and therefore always valid Unicode.
        let key = model_path.to_string_lossy().to_string();

        // Check if already cached
        if self.map.contains_key(&key) {
            // Clone the transcriber before updating LRU
            let transcriber = self.map.get(&key).map(|t| t.clone());
            // Move to end of LRU order
            self.update_lru(&key);
            if let Some(t) = transcriber {
                return Ok(t);
            }
        }

        // Not cached – check if we need to evict
        if self.map.len() >= self.max_size {
            self.evict_lru();
        }

        // Load the model
        log::info!("Loading model into cache: {}", key);
        let transcriber = Arc::new(Transcriber::new(model_path)?);
        
        // Insert into cache
        self.map.insert(key.clone(), transcriber.clone());
        self.lru_order.push_back(key);
        
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
    pub fn clear(&mut self) {
        self.map.clear();
        self.lru_order.clear();
    }

    /// Get the current number of cached models
    #[allow(dead_code)]
    pub fn size(&self) -> usize {
        self.map.len()
    }

    /// Get the maximum cache size
    #[allow(dead_code)]
    pub fn capacity(&self) -> usize {
        self.max_size
    }
}