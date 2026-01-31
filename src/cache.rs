//! Element cache for `AXTerminator`
//!
//! LRU cache for accessibility elements to improve performance.

use lru::LruCache;
use std::num::NonZeroUsize;
use std::sync::Mutex;

use crate::element::AXElement;

/// Cache key for element lookup
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct CacheKey {
    /// Application PID
    pub pid: i32,
    /// Element query string
    pub query: String,
}

/// Element cache entry
pub struct CacheEntry {
    /// Cached element
    pub element: AXElement,
    /// Timestamp of cache entry
    pub timestamp: std::time::Instant,
}

/// Thread-safe element cache
pub struct ElementCache {
    cache: Mutex<LruCache<CacheKey, CacheEntry>>,
    /// Maximum age for cache entries (ms)
    max_age_ms: u64,
}

impl ElementCache {
    /// Create a new element cache
    #[must_use]
    pub fn new(capacity: usize, max_age_ms: u64) -> Self {
        Self {
            cache: Mutex::new(LruCache::new(
                NonZeroUsize::new(capacity).unwrap_or(NonZeroUsize::new(100).unwrap()),
            )),
            max_age_ms,
        }
    }

    /// Get an element from the cache
    pub fn get(&self, key: &CacheKey) -> Option<AXElement> {
        let mut cache = self.cache.lock().ok()?;

        if let Some(entry) = cache.get(key) {
            // Check if entry is still valid
            if entry.timestamp.elapsed().as_millis() < u128::from(self.max_age_ms) {
                // Clone the element (we can't move out of the cache)
                // TODO: Implement proper element cloning
                return None;
            }
            // Entry expired, remove it
            cache.pop(key);
        }
        None
    }

    /// Put an element in the cache
    pub fn put(&self, key: CacheKey, element: AXElement) {
        if let Ok(mut cache) = self.cache.lock() {
            cache.put(
                key,
                CacheEntry {
                    element,
                    timestamp: std::time::Instant::now(),
                },
            );
        }
    }

    /// Clear the cache
    pub fn clear(&self) {
        if let Ok(mut cache) = self.cache.lock() {
            cache.clear();
        }
    }

    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        if let Ok(cache) = self.cache.lock() {
            CacheStats {
                size: cache.len(),
                capacity: cache.cap().get(),
            }
        } else {
            CacheStats {
                size: 0,
                capacity: 0,
            }
        }
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    /// Current number of entries
    pub size: usize,
    /// Maximum capacity
    pub capacity: usize,
}

/// Global element cache
static GLOBAL_CACHE: std::sync::OnceLock<ElementCache> = std::sync::OnceLock::new();

/// Get the global element cache
pub fn global_cache() -> &'static ElementCache {
    GLOBAL_CACHE.get_or_init(|| ElementCache::new(500, 5000))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_key_equality() {
        let key1 = CacheKey {
            pid: 123,
            query: "Save".to_string(),
        };
        let key2 = CacheKey {
            pid: 123,
            query: "Save".to_string(),
        };
        assert_eq!(key1, key2);
    }
}
