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
                return Some(entry.element.clone());
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
    use std::time::{Duration, Instant};

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

    fn test_element(role: &str, title: &str) -> AXElement {
        AXElement {
            element: std::ptr::null(),
            role: Some(role.to_string()),
            title: Some(title.to_string()),
        }
    }

    #[test]
    fn test_cache_returns_cloned_element_before_expiry() {
        let cache = ElementCache::new(10, 5_000);
        let key = CacheKey {
            pid: 123,
            query: "button:Save".to_string(),
        };
        let element = test_element("AXButton", "Save");

        cache.put(key.clone(), element);

        let cached = cache.get(&key).expect("expected cached element");
        assert_eq!(cached.role, Some("AXButton".to_string()));
        assert_eq!(cached.title, Some("Save".to_string()));
    }

    #[test]
    fn test_cache_removes_expired_entries() {
        let cache = ElementCache::new(10, 5_000);
        let key = CacheKey {
            pid: 123,
            query: "button:Save".to_string(),
        };

        cache.put(key.clone(), test_element("AXButton", "Save"));

        {
            let mut inner = cache
                .cache
                .lock()
                .expect("cache lock should not be poisoned");
            let entry = inner
                .get_mut(&key)
                .expect("expected inserted cache entry to exist");
            entry.timestamp = Instant::now() - Duration::from_secs(10);
        }

        assert!(cache.get(&key).is_none());
        assert_eq!(cache.stats().size, 0);
    }
}
