/// Alignment cache for performance optimization
// TODO: Update when talaria-bio alignment is properly exposed
// use talaria_bio::alignment::AlignmentResult;

// Cache-specific alignment result
#[derive(Debug, Clone)]
pub struct CachedAlignment {
    pub score: i32,
    pub alignment: Vec<u8>,
}
use dashmap::DashMap;
use std::sync::Arc;

pub struct AlignmentCache {
    cache: Arc<DashMap<(String, String), CachedAlignment>>,
    max_size: usize,
}

impl AlignmentCache {
    pub fn new(max_size: usize) -> Self {
        Self {
            cache: Arc::new(DashMap::new()),
            max_size,
        }
    }

    pub fn get(&self, ref_id: &str, query_id: &str) -> Option<CachedAlignment> {
        let key = (ref_id.to_string(), query_id.to_string());
        self.cache.get(&key).map(|entry| entry.clone())
    }

    pub fn insert(&self, ref_id: String, query_id: String, result: CachedAlignment) {
        if self.cache.len() < self.max_size {
            let key = (ref_id, query_id);
            self.cache.insert(key, result);
        }
    }

    pub fn clear(&self) {
        self.cache.clear();
    }

    pub fn len(&self) -> usize {
        self.cache.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_insert_and_retrieve() {
        let cache = AlignmentCache::new(100);

        // Insert alignment
        let alignment = CachedAlignment {
            score: 42,
            alignment: vec![1, 2, 3],
        };
        cache.insert(
            "ref1".to_string(),
            "query1".to_string(),
            alignment.clone(),
        );

        // Retrieve alignment
        let retrieved = cache.get("ref1", "query1");
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.score, 42);
        assert_eq!(retrieved.alignment, vec![1, 2, 3]);

        // Non-existent entry
        assert!(cache.get("ref2", "query2").is_none());
    }

    #[test]
    fn test_cache_size_limit() {
        let cache = AlignmentCache::new(2);

        // Insert up to limit
        cache.insert(
            "ref1".to_string(),
            "query1".to_string(),
            CachedAlignment {
                score: 1,
                alignment: vec![1],
            },
        );
        cache.insert(
            "ref2".to_string(),
            "query2".to_string(),
            CachedAlignment {
                score: 2,
                alignment: vec![2],
            },
        );
        assert_eq!(cache.len(), 2);

        // Try to insert beyond limit - should not insert
        cache.insert(
            "ref3".to_string(),
            "query3".to_string(),
            CachedAlignment {
                score: 3,
                alignment: vec![3],
            },
        );
        assert_eq!(cache.len(), 2); // Size should remain at limit
    }

    #[test]
    fn test_cache_clear() {
        let cache = AlignmentCache::new(100);

        // Add some entries
        cache.insert(
            "ref1".to_string(),
            "query1".to_string(),
            CachedAlignment {
                score: 1,
                alignment: vec![1],
            },
        );
        cache.insert(
            "ref2".to_string(),
            "query2".to_string(),
            CachedAlignment {
                score: 2,
                alignment: vec![2],
            },
        );
        assert_eq!(cache.len(), 2);

        // Clear cache
        cache.clear();
        assert_eq!(cache.len(), 0);
        assert!(cache.is_empty());
        assert!(cache.get("ref1", "query1").is_none());
    }

    #[test]
    fn test_cache_concurrent_access() {
        use std::sync::Arc;
        use std::thread;

        let cache = Arc::new(AlignmentCache::new(1000));
        let mut handles = vec![];

        // Spawn threads for concurrent writes
        for i in 0..10 {
            let cache_clone = Arc::clone(&cache);
            let handle = thread::spawn(move || {
                let ref_id = format!("ref{}", i);
                let query_id = format!("query{}", i);
                let alignment = CachedAlignment {
                    score: i as i32,
                    alignment: vec![i as u8],
                };
                cache_clone.insert(ref_id, query_id, alignment);
            });
            handles.push(handle);
        }

        // Wait for all threads
        for handle in handles {
            handle.join().unwrap();
        }

        // Verify all entries are present
        for i in 0..10 {
            let ref_id = format!("ref{}", i);
            let query_id = format!("query{}", i);
            let alignment = cache.get(&ref_id, &query_id);
            assert!(alignment.is_some());
            assert_eq!(alignment.unwrap().score, i as i32);
        }
    }

    #[test]
    fn test_cache_key_ordering_matters() {
        let cache = AlignmentCache::new(100);

        let alignment = CachedAlignment {
            score: 42,
            alignment: vec![1, 2, 3],
        };

        cache.insert("ref1".to_string(), "query1".to_string(), alignment);

        // Different key order should not retrieve the same entry
        assert!(cache.get("query1", "ref1").is_none());
        assert!(cache.get("ref1", "query1").is_some());
    }

    #[test]
    fn test_cache_len_and_is_empty() {
        let cache = AlignmentCache::new(100);

        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);

        cache.insert(
            "ref1".to_string(),
            "query1".to_string(),
            CachedAlignment {
                score: 1,
                alignment: vec![1],
            },
        );

        assert!(!cache.is_empty());
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn test_cache_same_key_overwrites() {
        let cache = AlignmentCache::new(100);

        // Insert first alignment
        cache.insert(
            "ref1".to_string(),
            "query1".to_string(),
            CachedAlignment {
                score: 10,
                alignment: vec![1, 2],
            },
        );

        // Insert with same key
        cache.insert(
            "ref1".to_string(),
            "query1".to_string(),
            CachedAlignment {
                score: 20,
                alignment: vec![3, 4],
            },
        );

        // Should have overwritten
        assert_eq!(cache.len(), 1);
        let retrieved = cache.get("ref1", "query1").unwrap();
        assert_eq!(retrieved.score, 20);
        assert_eq!(retrieved.alignment, vec![3, 4]);
    }
}
