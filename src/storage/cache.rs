/// Alignment cache for performance optimization

use crate::bio::alignment::AlignmentResult;
use dashmap::DashMap;
use std::sync::Arc;

pub struct AlignmentCache {
    cache: Arc<DashMap<(String, String), AlignmentResult>>,
    max_size: usize,
}

impl AlignmentCache {
    pub fn new(max_size: usize) -> Self {
        Self {
            cache: Arc::new(DashMap::new()),
            max_size,
        }
    }
    
    pub fn get(&self, ref_id: &str, query_id: &str) -> Option<AlignmentResult> {
        let key = (ref_id.to_string(), query_id.to_string());
        self.cache.get(&key).map(|entry| entry.clone())
    }
    
    pub fn insert(&self, ref_id: String, query_id: String, result: AlignmentResult) {
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