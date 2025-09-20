/// Memory estimation and management for clustering and alignment operations
use crate::bio::sequence::Sequence;
use std::collections::HashMap;
use sysinfo::System;

/// Memory usage estimator for bioinformatics operations
#[derive(Debug, Clone)]
pub struct MemoryEstimator {
    /// Available system memory in bytes
    pub available_memory: u64,

    /// Safety margin (fraction of memory to keep free)
    pub safety_margin: f64,

    /// Estimated bytes per sequence for storage
    pub bytes_per_sequence: usize,

    /// Estimated bytes per sequence for LAMBDA index
    pub bytes_per_index_entry: usize,

    /// Overhead multiplier for alignment operations
    pub alignment_overhead: f64,
}

impl Default for MemoryEstimator {
    fn default() -> Self {
        let system = System::new_all();

        Self {
            available_memory: system.total_memory().saturating_sub(system.used_memory()),
            safety_margin: 0.3,         // Keep 30% free
            bytes_per_sequence: 1024,   // Average protein sequence ~1KB
            bytes_per_index_entry: 256, // Index overhead per sequence
            alignment_overhead: 2.5,    // LAMBDA needs ~2.5x sequence memory
        }
    }
}

impl MemoryEstimator {
    /// Create a new memory estimator with current system state
    pub fn new() -> Self {
        Self::default()
    }

    /// Update available memory from system
    pub fn refresh(&mut self) {
        let system = System::new_all();
        self.available_memory = system.total_memory().saturating_sub(system.used_memory());
    }

    /// Get usable memory after safety margin
    pub fn usable_memory(&self) -> u64 {
        (self.available_memory as f64 * (1.0 - self.safety_margin)) as u64
    }

    /// Estimate memory needed for a set of sequences
    pub fn estimate_sequence_memory(&self, sequences: &[Sequence]) -> u64 {
        sequences
            .iter()
            .map(|seq| {
                // Base sequence data
                let seq_size = seq.sequence.len()
                    + seq.id.len()
                    + seq.description.as_ref().map_or(0, |d| d.len());

                // Add overhead for structure and metadata
                seq_size + self.bytes_per_sequence
            })
            .sum::<usize>() as u64
    }

    /// Estimate memory needed for LAMBDA alignment
    pub fn estimate_alignment_memory(&self, num_sequences: usize, avg_length: usize) -> u64 {
        let base_memory = (num_sequences * (avg_length + self.bytes_per_index_entry)) as u64;
        (base_memory as f64 * self.alignment_overhead) as u64
    }

    /// Calculate maximum sequences that can fit in memory
    pub fn max_sequences_in_memory(&self, avg_sequence_length: usize) -> usize {
        let per_sequence = avg_sequence_length + self.bytes_per_sequence;
        let with_alignment = (per_sequence as f64 * self.alignment_overhead) as usize;

        (self.usable_memory() as usize / with_alignment).max(1)
    }

    /// Suggest optimal batch size for processing
    pub fn suggest_batch_size(&self, sequences: &[Sequence]) -> usize {
        if sequences.is_empty() {
            return 0;
        }

        let avg_length =
            sequences.iter().map(|s| s.sequence.len()).sum::<usize>() / sequences.len();

        let max_batch = self.max_sequences_in_memory(avg_length);

        // Use smaller of calculated max or reasonable batch size
        max_batch.min(50000)
    }

    /// Check if a cluster will fit in memory for alignment
    pub fn can_process_cluster(&self, sequences: &[Sequence]) -> bool {
        let required = self.estimate_sequence_memory(sequences);
        let with_alignment = (required as f64 * self.alignment_overhead) as u64;

        with_alignment <= self.usable_memory()
    }

    /// Split sequences into memory-appropriate batches
    pub fn split_into_batches(&self, sequences: Vec<Sequence>) -> Vec<Vec<Sequence>> {
        if sequences.is_empty() {
            return vec![];
        }

        let batch_size = self.suggest_batch_size(&sequences);
        let mut batches = Vec::new();

        for chunk in sequences.chunks(batch_size) {
            batches.push(chunk.to_vec());
        }

        batches
    }
}

/// Memory tracking for cluster operations
#[derive(Debug)]
pub struct ClusterMemoryTracker {
    /// Memory usage per cluster
    cluster_memory: HashMap<String, u64>,

    /// Total allocated memory
    total_allocated: u64,

    /// Memory limit
    memory_limit: u64,
}

impl ClusterMemoryTracker {
    pub fn new(memory_limit: u64) -> Self {
        Self {
            cluster_memory: HashMap::new(),
            total_allocated: 0,
            memory_limit,
        }
    }

    /// Track memory allocation for a cluster
    pub fn allocate(&mut self, cluster_id: String, size: u64) -> Result<(), String> {
        if self.total_allocated + size > self.memory_limit {
            return Err(format!(
                "Memory limit exceeded: {} + {} > {}",
                self.total_allocated, size, self.memory_limit
            ));
        }

        self.cluster_memory.insert(cluster_id, size);
        self.total_allocated += size;
        Ok(())
    }

    /// Release memory for a cluster
    pub fn release(&mut self, cluster_id: &str) {
        if let Some(size) = self.cluster_memory.remove(cluster_id) {
            self.total_allocated = self.total_allocated.saturating_sub(size);
        }
    }

    /// Get remaining available memory
    pub fn available(&self) -> u64 {
        self.memory_limit.saturating_sub(self.total_allocated)
    }

    /// Check if we can allocate more memory
    pub fn can_allocate(&self, size: u64) -> bool {
        self.total_allocated + size <= self.memory_limit
    }

    /// Get memory usage statistics
    pub fn stats(&self) -> MemoryStats {
        MemoryStats {
            total_allocated: self.total_allocated,
            memory_limit: self.memory_limit,
            cluster_count: self.cluster_memory.len(),
            available: self.available(),
            utilization: (self.total_allocated as f64 / self.memory_limit as f64) * 100.0,
        }
    }
}

#[derive(Debug)]
pub struct MemoryStats {
    pub total_allocated: u64,
    pub memory_limit: u64,
    pub cluster_count: usize,
    pub available: u64,
    pub utilization: f64,
}

/// Adaptive memory manager that adjusts batch sizes based on performance
#[derive(Debug)]
pub struct AdaptiveMemoryManager {
    estimator: MemoryEstimator,
    performance_history: Vec<PerformanceMetric>,
    optimal_batch_size: Option<usize>,
}

#[derive(Debug)]
struct PerformanceMetric {
    batch_size: usize,
    processing_time: f64,
    memory_used: u64,
}

impl AdaptiveMemoryManager {
    pub fn new() -> Self {
        Self {
            estimator: MemoryEstimator::new(),
            performance_history: Vec::new(),
            optimal_batch_size: None,
        }
    }

    /// Record performance metric
    pub fn record_performance(
        &mut self,
        batch_size: usize,
        processing_time: f64,
        memory_used: u64,
    ) {
        self.performance_history.push(PerformanceMetric {
            batch_size,
            processing_time,
            memory_used,
        });

        // Update optimal batch size based on performance
        self.update_optimal_batch_size();
    }

    /// Update optimal batch size based on performance history
    fn update_optimal_batch_size(&mut self) {
        if self.performance_history.len() < 3 {
            return;
        }

        // Find batch size with best throughput (sequences/second)
        let best = self
            .performance_history
            .iter()
            .map(|m| (m.batch_size, m.batch_size as f64 / m.processing_time))
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
            .map(|(size, _)| size);

        self.optimal_batch_size = best;
    }

    /// Get adaptive batch size
    pub fn get_batch_size(&self, sequences: &[Sequence]) -> usize {
        self.optimal_batch_size
            .unwrap_or_else(|| self.estimator.suggest_batch_size(sequences))
    }

    /// Check if we should increase batch size
    pub fn should_increase_batch_size(&self) -> bool {
        if self.performance_history.len() < 2 {
            return false;
        }

        // Check if recent performance is improving
        let recent = &self.performance_history[self.performance_history.len() - 2..];
        recent[1].processing_time < recent[0].processing_time * 0.9
    }

    /// Check if we should decrease batch size
    pub fn should_decrease_batch_size(&self) -> bool {
        if self.performance_history.is_empty() {
            return false;
        }

        // Check if we're using too much memory
        let last = self.performance_history.last().unwrap();
        last.memory_used as f64 > self.estimator.usable_memory() as f64 * 0.8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_estimator() {
        let estimator = MemoryEstimator::new();

        assert!(estimator.available_memory > 0);
        assert!(estimator.usable_memory() < estimator.available_memory);
    }

    #[test]
    fn test_batch_size_suggestion() {
        let estimator = MemoryEstimator::new();

        let sequences = vec![
            Sequence {
                id: "test".to_string(),
                description: None,
                sequence: vec![b'A'; 300],
                taxon_id: Some(562),
                taxonomy_sources: Default::default(),
            };
            100
        ];

        let batch_size = estimator.suggest_batch_size(&sequences);
        assert!(batch_size > 0);
        assert!(batch_size <= 50000);
    }

    #[test]
    fn test_cluster_memory_tracker() {
        let mut tracker = ClusterMemoryTracker::new(1_000_000);

        assert!(tracker.allocate("cluster1".to_string(), 100_000).is_ok());
        assert_eq!(tracker.total_allocated, 100_000);

        assert!(tracker.allocate("cluster2".to_string(), 2_000_000).is_err());

        tracker.release("cluster1");
        assert_eq!(tracker.total_allocated, 0);
    }

    #[test]
    fn test_adaptive_manager() {
        let mut manager = AdaptiveMemoryManager::new();

        manager.record_performance(1000, 10.0, 100_000);
        manager.record_performance(2000, 18.0, 200_000);
        manager.record_performance(1500, 12.0, 150_000);

        // Should find 1500 as optimal (125 sequences/second)
        assert_eq!(manager.optimal_batch_size, Some(1500));
    }
}
