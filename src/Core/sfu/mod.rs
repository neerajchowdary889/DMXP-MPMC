use sfb::lifecycle::BlobStoreLifecycleExt;
use sfb::{Config, PinnedBlobStore};
use std::sync::Arc;
use std::time::Duration;

pub struct BlobStoreBuilder {
    config: Config,
    cleanup_interval: Option<Duration>,
}

impl BlobStoreBuilder {
    /// Create a new builder with the default configuration
    pub fn new() -> Self {
        Self {
            config: Config::default(),
            cleanup_interval: Some(Duration::from_millis(100)), // Default background cleanup interval
        }
    }

    /// Create a builder starting from a performance-oriented configuration
    pub fn performance() -> Self {
        Self {
            config: Config::performance(),
            cleanup_interval: Some(Duration::from_millis(100)),
        }
    }

    /// Create a builder starting from a memory-efficient configuration
    pub fn memory_efficient() -> Self {
        Self {
            config: Config::memory_efficient(),
            cleanup_interval: Some(Duration::from_millis(500)),
        }
    }

    /// Set the page size in bytes
    pub fn with_page_size(mut self, size: usize) -> Self {
        self.config.page_size = size;
        self
    }

    /// Set the threshold for prefetching the next page (0.0 to 1.0)
    pub fn with_prefetch_threshold(mut self, threshold: f32) -> Self {
        self.config.prefetch_threshold = threshold;
        self
    }

    /// Set how long to keep empty pages before freeing (in milliseconds)
    pub fn with_decay_timeout(mut self, timeout_ms: u64) -> Self {
        self.config.decay_timeout_ms = timeout_ms;
        self
    }

    /// Set the default TTL for stored data (in milliseconds)
    pub fn with_ttl(mut self, ttl_ms: u64) -> Self {
        self.config.default_ttl_ms = ttl_ms;
        self
    }

    /// Set the background cleanup interval. Set to None to disable background cleanup.
    pub fn with_cleanup_interval(mut self, interval: Option<Duration>) -> Self {
        self.cleanup_interval = interval;
        self
    }

    /// Consume the builder and return an Arc<PinnedBlobStore>,
    /// automatically starting background cleanup if an interval is configured.
    pub fn build(self) -> Result<Arc<PinnedBlobStore>, sfb::BlobError> {
        let store = Arc::new(PinnedBlobStore::new(self.config)?);

        if let Some(interval) = self.cleanup_interval {
            store.start_cleanup(interval);
        }

        Ok(store)
    }
}

impl Default for BlobStoreBuilder {
    fn default() -> Self {
        Self::memory_efficient()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder_defaults() {
        let store = BlobStoreBuilder::default().build().unwrap();
        let handle = store.append(b"test data").unwrap();
        let data = store.get(&handle).unwrap();
        println!("Data: {:?}", String::from_utf8_lossy(&data));
        assert_eq!(data, b"test data");
        store.acknowledge(&handle);
    }
}