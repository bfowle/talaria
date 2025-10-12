/// Adaptive performance tuning for optimal throughput
use super::memory_monitor::MemoryMonitor;
use anyhow::Result;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Configuration for adaptive performance management
#[derive(Debug, Clone)]
pub struct AdaptiveConfig {
    /// Minimum batch size (sequences)
    pub min_batch_size: usize,
    /// Maximum batch size (sequences)
    pub max_batch_size: usize,
    /// Minimum channel buffer size
    pub min_channel_buffer: usize,
    /// Maximum channel buffer size
    pub max_channel_buffer: usize,
    /// Memory limit in MB (0 for auto-detect)
    pub memory_limit_mb: usize,
    /// Target memory usage ratio (0.0 to 1.0)
    pub target_memory_usage: f32,
    /// Average sequence size estimate (bytes)
    pub avg_sequence_size: usize,
}

impl Default for AdaptiveConfig {
    fn default() -> Self {
        Self {
            min_batch_size: 1_000,
            max_batch_size: 100_000,
            min_channel_buffer: 5,
            max_channel_buffer: 100,
            memory_limit_mb: 0,        // Auto-detect
            target_memory_usage: 0.75, // Use 75% of available memory
            avg_sequence_size: 500,    // Typical protein sequence
        }
    }
}

/// Performance metrics for adaptive tuning
#[derive(Debug, Clone)]
pub struct PerformanceMetrics {
    /// Sequences processed per second
    pub sequences_per_second: f64,
    /// Bytes processed per second
    pub bytes_per_second: f64,
    /// Channel utilization (0.0 to 1.0)
    pub channel_utilization: f64,
    /// Time spent waiting for channel space
    pub channel_wait_time: Duration,
    /// Last measurement time
    pub last_update: Instant,
}

impl Default for PerformanceMetrics {
    fn default() -> Self {
        Self {
            sequences_per_second: 0.0,
            bytes_per_second: 0.0,
            channel_utilization: 0.0,
            channel_wait_time: Duration::ZERO,
            last_update: Instant::now(),
        }
    }
}

/// Adaptive performance manager that dynamically adjusts batch and buffer sizes
pub struct AdaptiveManager {
    /// Configuration
    config: AdaptiveConfig,
    /// Current batch size
    current_batch_size: Arc<AtomicUsize>,
    /// Current channel buffer size
    current_buffer_size: Arc<AtomicUsize>,
    /// Memory monitor
    memory_monitor: Arc<Mutex<MemoryMonitor>>,
    /// Performance metrics
    metrics: Arc<Mutex<PerformanceMetrics>>,
    /// Number of sequences processed
    sequences_processed: Arc<AtomicUsize>,
    /// Bytes processed
    bytes_processed: Arc<AtomicUsize>,
    /// Start time
    start_time: Instant,
}

impl AdaptiveManager {
    /// Create new adaptive manager with default config
    pub fn new() -> Result<Self> {
        Self::with_config(AdaptiveConfig::default())
    }

    /// Create new adaptive manager with custom config
    pub fn with_config(config: AdaptiveConfig) -> Result<Self> {
        let mut memory_monitor = MemoryMonitor::new();
        memory_monitor.start(Duration::from_secs(1));

        // Start with conservative sizes
        let initial_batch_size = config.min_batch_size * 2;
        let initial_buffer_size = config.min_channel_buffer * 2;

        Ok(Self {
            config,
            current_batch_size: Arc::new(AtomicUsize::new(initial_batch_size)),
            current_buffer_size: Arc::new(AtomicUsize::new(initial_buffer_size)),
            memory_monitor: Arc::new(Mutex::new(memory_monitor)),
            metrics: Arc::new(Mutex::new(PerformanceMetrics::default())),
            sequences_processed: Arc::new(AtomicUsize::new(0)),
            bytes_processed: Arc::new(AtomicUsize::new(0)),
            start_time: Instant::now(),
        })
    }

    /// Get current optimal batch size
    pub fn get_optimal_batch_size(&self) -> usize {
        self.current_batch_size.load(Ordering::Relaxed)
    }

    /// Get current optimal channel buffer size
    pub fn get_optimal_buffer_size(&self) -> usize {
        self.current_buffer_size.load(Ordering::Relaxed)
    }

    /// Check if system has memory pressure
    pub fn has_memory_pressure(&self) -> bool {
        let monitor = self.memory_monitor.lock();
        monitor.has_memory_pressure(self.config.target_memory_usage as f64)
    }

    /// Update performance metrics
    pub fn update_metrics(&self, sequences: usize, bytes: usize) {
        self.sequences_processed
            .fetch_add(sequences, Ordering::Relaxed);
        self.bytes_processed.fetch_add(bytes, Ordering::Relaxed);

        let total_sequences = self.sequences_processed.load(Ordering::Relaxed);
        let total_bytes = self.bytes_processed.load(Ordering::Relaxed);
        let elapsed = self.start_time.elapsed().as_secs_f64();

        if elapsed > 0.0 {
            let mut metrics = self.metrics.lock();
            metrics.sequences_per_second = total_sequences as f64 / elapsed;
            metrics.bytes_per_second = total_bytes as f64 / elapsed;
            metrics.last_update = Instant::now();
        }
    }

    /// Adapt batch size based on current performance
    pub fn adapt_batch_size(&self) {
        // Extract metrics data quickly and release lock
        let sequences_per_second = {
            let metrics = self.metrics.lock();
            metrics.sequences_per_second
        };

        // Extract memory stats quickly and release lock
        let memory_stats = {
            let monitor = self.memory_monitor.lock();
            monitor.get_stats()
        };

        let current_size = self.current_batch_size.load(Ordering::Relaxed);
        let mut new_size = current_size;

        // Check memory pressure
        if memory_stats.has_pressure(self.config.target_memory_usage as f64) {
            // Reduce batch size if memory pressure
            new_size = (current_size * 3 / 4).max(self.config.min_batch_size);
        } else if memory_stats.available_mb() > 1000 {
            // Increase batch size if plenty of memory and good performance
            if sequences_per_second > 1000.0 {
                new_size = (current_size * 5 / 4).min(self.config.max_batch_size);
            }
        }

        // Apply new size if different
        if new_size != current_size {
            self.current_batch_size.store(new_size, Ordering::Relaxed);
            tracing::debug!("Adapted batch size: {} -> {}", current_size, new_size);
        }
    }

    /// Adapt channel buffer size based on utilization
    pub fn adapt_buffer_size(&self, channel_fill_ratio: f64) {
        let current_size = self.current_buffer_size.load(Ordering::Relaxed);
        let mut new_size = current_size;

        if channel_fill_ratio > 0.9 {
            // Channel is nearly full, increase buffer
            new_size = (current_size * 2).min(self.config.max_channel_buffer);
        } else if channel_fill_ratio < 0.3 && current_size > self.config.min_channel_buffer {
            // Channel is underutilized, decrease buffer
            new_size = (current_size * 3 / 4).max(self.config.min_channel_buffer);
        }

        // Apply new size if different
        if new_size != current_size {
            self.current_buffer_size.store(new_size, Ordering::Relaxed);
            tracing::debug!("Adapted buffer size: {} -> {}", current_size, new_size);
        }
    }

    /// Get memory-aware batch size recommendation
    pub fn get_memory_aware_batch_size(&self) -> usize {
        let monitor = self.memory_monitor.lock();

        // Use configured limit or auto-detect
        let memory_limit_mb = if self.config.memory_limit_mb > 0 {
            self.config.memory_limit_mb as u64
        } else {
            // Use 75% of available memory
            let stats = monitor.get_stats();
            (stats.available_mb() as f64 * self.config.target_memory_usage as f64) as u64
        };

        monitor.calculate_optimal_batch_size(
            memory_limit_mb,
            self.config.avg_sequence_size,
            self.config.min_batch_size,
            self.config.max_batch_size,
        )
    }

    /// Perform automatic adaptation based on current metrics
    pub fn auto_adapt(&self) {
        // Adapt batch size based on memory and performance
        self.adapt_batch_size();

        // Note: Buffer size adaptation needs channel fill ratio
        // which should be provided by the caller
    }

    /// Get current performance report
    pub fn get_performance_report(&self) -> String {
        // Extract metrics data quickly and release lock
        let (sequences_per_second, bytes_per_second) = {
            let metrics = self.metrics.lock();
            (metrics.sequences_per_second, metrics.bytes_per_second)
        };

        // Extract memory stats quickly and release lock
        let memory = {
            let monitor = self.memory_monitor.lock();
            monitor.get_stats()
        };

        format!(
            "Performance Report:\n\
             - Batch Size: {}\n\
             - Buffer Size: {}\n\
             - Sequences/sec: {:.1}\n\
             - Throughput: {:.1} MB/s\n\
             - Memory Usage: {:.1}% ({} MB / {} MB)\n\
             - Process RSS: {} MB",
            self.current_batch_size.load(Ordering::Relaxed),
            self.current_buffer_size.load(Ordering::Relaxed),
            sequences_per_second,
            bytes_per_second / (1024.0 * 1024.0),
            memory.usage_ratio * 100.0,
            (memory.total - memory.available) / (1024 * 1024),
            memory.total / (1024 * 1024),
            memory.process_rss_mb()
        )
    }

    /// Reset metrics
    pub fn reset_metrics(&self) {
        self.sequences_processed.store(0, Ordering::Relaxed);
        self.bytes_processed.store(0, Ordering::Relaxed);
        *self.metrics.lock() = PerformanceMetrics::default();
    }

    /// Get current memory statistics
    pub fn get_memory_stats(&self) -> super::memory_monitor::MemoryStats {
        let monitor = self.memory_monitor.lock();
        monitor.get_stats()
    }

    /// Force a specific batch size (used when critical memory pressure detected)
    pub fn force_batch_size(&self, size: usize) {
        let clamped = size.clamp(self.config.min_batch_size, self.config.max_batch_size);
        self.current_batch_size.store(clamped, Ordering::Relaxed);
        tracing::info!("Batch size forced to: {}", clamped);
    }

    /// Increase batch size gradually (when memory is available)
    pub fn increase_batch_size(&self) {
        let current = self.current_batch_size.load(Ordering::Relaxed);
        let new_size = ((current as f64 * 1.1) as usize).min(self.config.max_batch_size);
        if new_size > current {
            self.current_batch_size.store(new_size, Ordering::Relaxed);
            tracing::debug!("Increased batch size: {} -> {}", current, new_size);
        }
    }
}

/// Builder for adaptive configuration
pub struct AdaptiveConfigBuilder {
    config: AdaptiveConfig,
}

impl AdaptiveConfigBuilder {
    pub fn new() -> Self {
        Self {
            config: AdaptiveConfig::default(),
        }
    }

    pub fn min_batch_size(mut self, size: usize) -> Self {
        self.config.min_batch_size = size;
        self
    }

    pub fn max_batch_size(mut self, size: usize) -> Self {
        self.config.max_batch_size = size;
        self
    }

    pub fn min_channel_buffer(mut self, size: usize) -> Self {
        self.config.min_channel_buffer = size;
        self
    }

    pub fn max_channel_buffer(mut self, size: usize) -> Self {
        self.config.max_channel_buffer = size;
        self
    }

    pub fn memory_limit_mb(mut self, limit: usize) -> Self {
        self.config.memory_limit_mb = limit;
        self
    }

    pub fn target_memory_usage(mut self, ratio: f32) -> Self {
        self.config.target_memory_usage = ratio.max(0.0).min(1.0);
        self
    }

    pub fn avg_sequence_size(mut self, size: usize) -> Self {
        self.config.avg_sequence_size = size;
        self
    }

    pub fn build(self) -> AdaptiveConfig {
        self.config
    }
}

impl Default for AdaptiveConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adaptive_manager_creation() {
        let manager = AdaptiveManager::new().unwrap();
        assert!(manager.get_optimal_batch_size() > 0);
        assert!(manager.get_optimal_buffer_size() > 0);
    }

    #[test]
    fn test_config_builder() {
        let config = AdaptiveConfigBuilder::new()
            .min_batch_size(500)
            .max_batch_size(50000)
            .memory_limit_mb(1024)
            .target_memory_usage(0.8)
            .build();

        assert_eq!(config.min_batch_size, 500);
        assert_eq!(config.max_batch_size, 50000);
        assert_eq!(config.memory_limit_mb, 1024);
        assert_eq!(config.target_memory_usage, 0.8);
    }

    #[test]
    fn test_metrics_update() {
        let manager = AdaptiveManager::new().unwrap();
        manager.update_metrics(1000, 500000);

        // Sleep a bit to get non-zero elapsed time
        std::thread::sleep(Duration::from_millis(10));
        manager.update_metrics(1000, 500000);

        let report = manager.get_performance_report();
        assert!(report.contains("Performance Report"));
    }
}
