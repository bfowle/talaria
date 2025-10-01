/// RocksDB performance monitoring and metrics collection
use anyhow::Result;
use parking_lot::Mutex;
use rocksdb::{DBWithThreadMode, MultiThreaded};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Performance metrics for RocksDB operations
#[derive(Debug, Clone)]
pub struct RocksDBMetrics {
    // Operation counts
    pub total_reads: u64,
    pub total_writes: u64,
    pub total_deletes: u64,
    pub batch_writes: u64,
    pub multi_gets: u64,

    // Timing metrics (in microseconds)
    pub read_latency_sum: u64,
    pub write_latency_sum: u64,
    pub batch_latency_sum: u64,

    // Cache metrics
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub bloom_filter_hits: u64,
    pub bloom_filter_misses: u64,

    // Storage metrics
    pub total_keys: u64,
    pub total_size_bytes: u64,
    pub num_files: u64,
    pub compaction_time_ms: u64,

    // Error tracking
    pub read_errors: u64,
    pub write_errors: u64,

    // Timestamp of last reset
    pub last_reset: Instant,
}

impl Default for RocksDBMetrics {
    fn default() -> Self {
        Self {
            total_reads: 0,
            total_writes: 0,
            total_deletes: 0,
            batch_writes: 0,
            multi_gets: 0,
            read_latency_sum: 0,
            write_latency_sum: 0,
            batch_latency_sum: 0,
            cache_hits: 0,
            cache_misses: 0,
            bloom_filter_hits: 0,
            bloom_filter_misses: 0,
            total_keys: 0,
            total_size_bytes: 0,
            num_files: 0,
            compaction_time_ms: 0,
            read_errors: 0,
            write_errors: 0,
            last_reset: Instant::now(),
        }
    }
}

impl RocksDBMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    /// Calculate average read latency in microseconds
    pub fn avg_read_latency_us(&self) -> f64 {
        if self.total_reads == 0 {
            0.0
        } else {
            self.read_latency_sum as f64 / self.total_reads as f64
        }
    }

    /// Calculate average write latency in microseconds
    pub fn avg_write_latency_us(&self) -> f64 {
        if self.total_writes == 0 {
            0.0
        } else {
            self.write_latency_sum as f64 / self.total_writes as f64
        }
    }

    /// Calculate cache hit rate
    pub fn cache_hit_rate(&self) -> f64 {
        let total = self.cache_hits + self.cache_misses;
        if total == 0 {
            0.0
        } else {
            self.cache_hits as f64 / total as f64
        }
    }

    /// Calculate bloom filter effectiveness
    pub fn bloom_filter_positive_rate(&self) -> f64 {
        let total = self.bloom_filter_hits + self.bloom_filter_misses;
        if total == 0 {
            0.0
        } else {
            self.bloom_filter_hits as f64 / total as f64
        }
    }

    /// Get operations per second since last reset
    pub fn ops_per_second(&self) -> f64 {
        let elapsed = self.last_reset.elapsed().as_secs_f64();
        if elapsed == 0.0 {
            0.0
        } else {
            (self.total_reads + self.total_writes) as f64 / elapsed
        }
    }

    /// Reset all metrics
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// Generate a summary report
    pub fn summary(&self) -> String {
        let elapsed = self.last_reset.elapsed();
        format!(
            r#"RocksDB Performance Metrics ({}s)
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
Operations:
  • Total Reads:     {:>10} ({:.1} µs avg)
  • Total Writes:    {:>10} ({:.1} µs avg)
  • Batch Writes:    {:>10}
  • Multi-Gets:      {:>10}
  • OPS:             {:>10.1}/s

Cache Performance:
  • Cache Hit Rate:  {:>10.1}%
  • Bloom Filter:    {:>10.1}% positive

Storage:
  • Total Keys:      {:>10}
  • Total Size:      {:>10} MB
  • Files:           {:>10}
  • Compaction Time: {:>10} ms

Errors:
  • Read Errors:     {:>10}
  • Write Errors:    {:>10}
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"#,
            elapsed.as_secs(),
            self.total_reads,
            self.avg_read_latency_us(),
            self.total_writes,
            self.avg_write_latency_us(),
            self.batch_writes,
            self.multi_gets,
            self.ops_per_second(),
            self.cache_hit_rate() * 100.0,
            self.bloom_filter_positive_rate() * 100.0,
            self.total_keys,
            self.total_size_bytes / 1_024 / 1_024,
            self.num_files,
            self.compaction_time_ms,
            self.read_errors,
            self.write_errors,
        )
    }
}

/// Metrics collector that wraps RocksDB operations
#[allow(dead_code)]
pub struct MetricsCollector {
    metrics: Arc<Mutex<RocksDBMetrics>>,
}

#[allow(dead_code)]
impl MetricsCollector {
    pub fn new() -> Self {
        Self {
            metrics: Arc::new(Mutex::new(RocksDBMetrics::new())),
        }
    }

    /// Record a read operation
    pub fn record_read(&self, duration: Duration, success: bool) {
        let mut metrics = self.metrics.lock();
        metrics.total_reads += 1;
        metrics.read_latency_sum += duration.as_micros() as u64;
        if !success {
            metrics.read_errors += 1;
        }
    }

    /// Record a write operation
    pub fn record_write(&self, duration: Duration, success: bool) {
        let mut metrics = self.metrics.lock();
        metrics.total_writes += 1;
        metrics.write_latency_sum += duration.as_micros() as u64;
        if !success {
            metrics.write_errors += 1;
        }
    }

    /// Record a batch write operation
    pub fn record_batch_write(&self, duration: Duration, count: usize) {
        let mut metrics = self.metrics.lock();
        metrics.batch_writes += 1;
        metrics.total_writes += count as u64;
        metrics.batch_latency_sum += duration.as_micros() as u64;
    }

    /// Record a multi-get operation
    pub fn record_multi_get(&self, count: usize, hits: usize) {
        let mut metrics = self.metrics.lock();
        metrics.multi_gets += 1;
        metrics.total_reads += count as u64;
        metrics.cache_hits += hits as u64;
        metrics.cache_misses += (count - hits) as u64;
    }

    /// Update storage statistics from RocksDB
    pub fn update_db_stats(&self, db: &Arc<DBWithThreadMode<MultiThreaded>>) -> Result<()> {
        let mut metrics = self.metrics.lock();

        // Get property values
        if let Ok(Some(val)) = db.property_int_value("rocksdb.estimate-num-keys") {
            metrics.total_keys = val;
        }

        if let Ok(Some(val)) = db.property_int_value("rocksdb.estimate-live-data-size") {
            metrics.total_size_bytes = val;
        }

        if let Ok(Some(val)) = db.property_int_value("rocksdb.num-files-at-level0") {
            metrics.num_files = val;
        }

        Ok(())
    }

    /// Get current metrics
    pub fn get_metrics(&self) -> RocksDBMetrics {
        self.metrics.lock().clone()
    }

    /// Get metrics summary
    pub fn get_summary(&self) -> String {
        self.metrics.lock().summary()
    }

    /// Reset metrics
    pub fn reset(&self) {
        self.metrics.lock().reset();
    }
}

/// Real-time monitoring with periodic reporting
#[allow(dead_code)]
pub struct RocksDBMonitor {
    collector: Arc<MetricsCollector>,
    interval: Duration,
    running: Arc<Mutex<bool>>,
}

#[allow(dead_code)]
impl RocksDBMonitor {
    pub fn new(collector: Arc<MetricsCollector>, interval: Duration) -> Self {
        Self {
            collector,
            interval,
            running: Arc::new(Mutex::new(false)),
        }
    }

    /// Start monitoring in background thread
    pub fn start(&self, db: Arc<DBWithThreadMode<MultiThreaded>>) {
        let collector = Arc::clone(&self.collector);
        let interval = self.interval;
        let running = Arc::clone(&self.running);

        *running.lock() = true;

        std::thread::spawn(move || {
            while *running.lock() {
                // Update DB stats
                let _ = collector.update_db_stats(&db);

                // Log metrics periodically
                if tracing::enabled!(tracing::Level::INFO) {
                    let summary = collector.get_summary();
                    tracing::info!("\n{}", summary);
                }

                std::thread::sleep(interval);
            }
        });
    }

    /// Stop monitoring
    pub fn stop(&self) {
        *self.running.lock() = false;
    }
}

/// Performance profiler for specific operations
#[allow(dead_code)]
pub struct OperationProfiler {
    name: String,
    start: Instant,
    collector: Option<Arc<MetricsCollector>>,
}

#[allow(dead_code)]
impl OperationProfiler {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            start: Instant::now(),
            collector: None,
        }
    }

    pub fn with_collector(mut self, collector: Arc<MetricsCollector>) -> Self {
        self.collector = Some(collector);
        self
    }

    pub fn finish(self) -> Duration {
        let duration = self.start.elapsed();
        tracing::debug!(
            "Operation '{}' completed in {:.2}ms",
            self.name,
            duration.as_secs_f64() * 1000.0
        );
        duration
    }

    pub fn finish_read(self, success: bool) -> Duration {
        let duration = self.start.elapsed();
        if let Some(collector) = &self.collector {
            collector.record_read(duration, success);
        }
        duration
    }

    pub fn finish_write(self, success: bool) -> Duration {
        let duration = self.start.elapsed();
        if let Some(collector) = &self.collector {
            collector.record_write(duration, success);
        }
        duration
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_metrics_calculation() {
        let mut metrics = RocksDBMetrics::new();

        metrics.total_reads = 1000;
        metrics.read_latency_sum = 50_000; // 50ms total
        assert_eq!(metrics.avg_read_latency_us(), 50.0);

        metrics.cache_hits = 800;
        metrics.cache_misses = 200;
        assert_eq!(metrics.cache_hit_rate(), 0.8);
    }

    #[test]
    fn test_metrics_collector() {
        let collector = MetricsCollector::new();

        // Record some operations
        collector.record_read(Duration::from_micros(100), true);
        collector.record_write(Duration::from_micros(200), true);
        collector.record_batch_write(Duration::from_millis(10), 100);

        let metrics = collector.get_metrics();
        assert_eq!(metrics.total_reads, 1);
        assert_eq!(metrics.total_writes, 101); // 1 + 100 from batch
        assert_eq!(metrics.batch_writes, 1);
    }

    #[test]
    fn test_operation_profiler() {
        let collector = Arc::new(MetricsCollector::new());

        let profiler = OperationProfiler::new("test_read").with_collector(Arc::clone(&collector));

        thread::sleep(Duration::from_millis(10));
        let duration = profiler.finish_read(true);

        assert!(duration.as_millis() >= 10);

        let metrics = collector.get_metrics();
        assert_eq!(metrics.total_reads, 1);
        assert!(metrics.read_latency_sum > 0);
    }

    #[test]
    fn test_metrics_summary() {
        let metrics = RocksDBMetrics {
            total_reads: 10000,
            total_writes: 5000,
            read_latency_sum: 500_000,
            write_latency_sum: 300_000,
            cache_hits: 8000,
            cache_misses: 2000,
            total_keys: 15000,
            total_size_bytes: 100_000_000,
            ..Default::default()
        };

        let summary = metrics.summary();
        assert!(summary.contains("Total Reads"));
        assert!(summary.contains("10000"));
        assert!(summary.contains("Cache Hit Rate"));
    }
}
