use crate::performance::{Bottleneck, PerformanceReport, PerformanceSnapshot};
use crossbeam::queue::SegQueue;
/// Lock-free performance monitoring using atomic operations
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

/// Lock-free throughput monitor - no mutexes, no deadlocks
#[derive(Clone)]
pub struct LockFreeThroughputMonitor {
    /// Atomic counters for metrics
    metrics: Arc<AtomicMetrics>,
    /// Lock-free queue for performance snapshots
    history: Arc<SegQueue<PerformanceSnapshot>>,
    /// Start time (immutable after creation)
    start_time: Instant,
}

/// Atomic metrics storage - all lock-free
struct AtomicMetrics {
    /// Total sequences processed
    total_sequences: AtomicU64,
    /// Total bytes processed
    total_bytes: AtomicU64,
    /// Total chunks created
    total_chunks: AtomicU64,
    /// Current batch size
    current_batch_size: AtomicUsize,
    /// Last report timestamp (milliseconds since start)
    last_report_ms: AtomicU64,
    /// Current sequences per second (fixed-point: multiply by 1000)
    seq_per_sec_x1000: AtomicU64,
    /// Current MB per second (fixed-point: multiply by 1000)
    mb_per_sec_x1000: AtomicU64,
}

impl LockFreeThroughputMonitor {
    /// Create a new lock-free monitor
    pub fn new() -> Self {
        Self {
            metrics: Arc::new(AtomicMetrics {
                total_sequences: AtomicU64::new(0),
                total_bytes: AtomicU64::new(0),
                total_chunks: AtomicU64::new(0),
                current_batch_size: AtomicUsize::new(0),
                last_report_ms: AtomicU64::new(0),
                seq_per_sec_x1000: AtomicU64::new(0),
                mb_per_sec_x1000: AtomicU64::new(0),
            }),
            history: Arc::new(SegQueue::new()),
            start_time: Instant::now(),
        }
    }

    /// Record sequences processed - lock-free
    pub fn record_sequences(&self, count: usize, bytes: usize) {
        self.metrics
            .total_sequences
            .fetch_add(count as u64, Ordering::Relaxed);
        self.metrics
            .total_bytes
            .fetch_add(bytes as u64, Ordering::Relaxed);

        // Update throughput metrics if enough time has passed
        let elapsed = self.start_time.elapsed();
        let elapsed_ms = elapsed.as_millis() as u64;
        let last_report = self.metrics.last_report_ms.load(Ordering::Relaxed);

        // Report every 5 seconds
        if elapsed_ms > last_report + 5000 {
            // Try to claim the update slot
            if self
                .metrics
                .last_report_ms
                .compare_exchange(last_report, elapsed_ms, Ordering::SeqCst, Ordering::Relaxed)
                .is_ok()
            {
                // We won the race to update metrics
                let total_seq = self.metrics.total_sequences.load(Ordering::Relaxed);
                let total_bytes = self.metrics.total_bytes.load(Ordering::Relaxed);

                let seq_per_sec = (total_seq as f64 / elapsed.as_secs_f64() * 1000.0) as u64;
                let mb_per_sec =
                    ((total_bytes as f64 / 1_000_000.0) / elapsed.as_secs_f64() * 1000.0) as u64;

                self.metrics
                    .seq_per_sec_x1000
                    .store(seq_per_sec, Ordering::Relaxed);
                self.metrics
                    .mb_per_sec_x1000
                    .store(mb_per_sec, Ordering::Relaxed);

                // Store snapshot in history
                let snapshot = PerformanceSnapshot {
                    timestamp: elapsed,
                    sequences_per_sec: seq_per_sec as f64 / 1000.0,
                    mb_per_sec: mb_per_sec as f64 / 1000.0,
                    chunks_per_sec: self.metrics.total_chunks.load(Ordering::Relaxed) as f64
                        / elapsed.as_secs_f64(),
                    batch_size: self.metrics.current_batch_size.load(Ordering::Relaxed),
                    memory_mb: self.get_current_memory_mb(),
                    cpu_percent: self.estimate_cpu_usage(),
                };

                self.history.push(snapshot);

                // Keep history bounded (remove old entries)
                while self.history.len() > 100 {
                    self.history.pop();
                }
            }
        }
    }

    /// Record chunks created - lock-free
    pub fn record_chunks(&self, count: usize) {
        self.metrics
            .total_chunks
            .fetch_add(count as u64, Ordering::Relaxed);
    }

    /// Update batch size - lock-free
    pub fn update_batch_size(&self, size: usize) {
        self.metrics
            .current_batch_size
            .store(size, Ordering::Relaxed);
    }

    /// Get current throughput - lock-free
    pub fn current_throughput(&self) -> (f64, f64) {
        let seq_per_sec = self.metrics.seq_per_sec_x1000.load(Ordering::Relaxed) as f64 / 1000.0;
        let mb_per_sec = self.metrics.mb_per_sec_x1000.load(Ordering::Relaxed) as f64 / 1000.0;
        (seq_per_sec, mb_per_sec)
    }

    /// Detect bottlenecks - lock-free
    pub fn detect_bottlenecks(&self) -> Vec<Bottleneck> {
        let mut bottlenecks = Vec::new();

        let (_seq_per_sec, mb_per_sec) = self.current_throughput();
        let cpu_cores = num_cpus::get();
        let cpu_usage = self.estimate_cpu_usage();

        // CPU bound check
        if cpu_usage > 0.9 * (cpu_cores as f64) {
            bottlenecks.push(Bottleneck::CpuBound {
                utilization: cpu_usage / cpu_cores as f64,
            });
        }

        // Single-threaded check
        if cpu_usage < 1.5 && cpu_cores > 1 {
            bottlenecks.push(Bottleneck::SingleThreaded {
                cpu_cores,
                utilized: cpu_usage.ceil() as usize,
            });
        }

        // I/O bound check
        if mb_per_sec < 50.0 {
            bottlenecks.push(Bottleneck::IoBound {
                read_mb_sec: mb_per_sec,
                write_mb_sec: mb_per_sec / 2.0,
            });
        }

        // Memory pressure check
        if let Some(memory_bottleneck) = self.check_memory_pressure() {
            bottlenecks.push(memory_bottleneck);
        }

        bottlenecks
    }

    /// Generate performance report - lock-free
    pub fn generate_report(&self) -> PerformanceReport {
        let elapsed = self.start_time.elapsed();
        let total_sequences = self.metrics.total_sequences.load(Ordering::Relaxed);
        let total_bytes = self.metrics.total_bytes.load(Ordering::Relaxed);
        let total_chunks = self.metrics.total_chunks.load(Ordering::Relaxed);

        let (avg_seq_per_sec, avg_mb_per_sec) = self.current_throughput();

        // Collect history snapshots
        let mut metrics_history = Vec::new();
        // Note: This is not perfectly thread-safe but good enough for reporting
        while let Some(snapshot) = self.history.pop() {
            metrics_history.push(snapshot);
        }
        // Put them back
        for snapshot in &metrics_history {
            self.history.push(snapshot.clone());
        }

        let peak_seq_per_sec = metrics_history
            .iter()
            .map(|s| s.sequences_per_sec)
            .fold(0.0, f64::max);
        let peak_mb_per_sec = metrics_history
            .iter()
            .map(|s| s.mb_per_sec)
            .fold(0.0, f64::max);

        PerformanceReport {
            duration: elapsed,
            total_sequences,
            total_bytes,
            total_chunks,
            avg_sequences_per_sec: avg_seq_per_sec,
            avg_mb_per_sec,
            peak_sequences_per_sec: peak_seq_per_sec,
            peak_mb_per_sec,
            bottlenecks: self.detect_bottlenecks(),
            recommendations: self.generate_recommendations(),
            metrics_history,
        }
    }

    // Private helper methods

    fn estimate_cpu_usage(&self) -> f64 {
        let seq_per_sec = self.metrics.seq_per_sec_x1000.load(Ordering::Relaxed) as f64 / 1000.0;
        let max_theoretical = 200_000.0;
        (seq_per_sec / max_theoretical) * num_cpus::get() as f64
    }

    fn get_current_memory_mb(&self) -> u64 {
        use crate::performance::MemoryMonitor;
        let monitor = MemoryMonitor::new();
        monitor.get_stats().used_mb()
    }

    fn check_memory_pressure(&self) -> Option<Bottleneck> {
        use crate::performance::MemoryMonitor;
        let monitor = MemoryMonitor::new();
        let stats = monitor.get_stats();

        let available = stats.available_mb();
        let total = stats.total_mb();
        let pressure = 1.0 - (available as f64 / total as f64);

        if pressure > 0.8 {
            Some(Bottleneck::MemoryBound {
                available_mb: available,
                pressure,
            })
        } else {
            None
        }
    }

    fn generate_recommendations(&self) -> Vec<String> {
        let mut recommendations = Vec::new();
        let bottlenecks = self.detect_bottlenecks();

        for bottleneck in bottlenecks {
            match bottleneck {
                Bottleneck::CpuBound { .. } => {
                    recommendations
                        .push("CPU bound: Consider using larger batch sizes".to_string());
                }
                Bottleneck::MemoryBound { .. } => {
                    recommendations
                        .push("Memory pressure: Reduce batch size or enable swap".to_string());
                }
                Bottleneck::IoBound { .. } => {
                    recommendations
                        .push("I/O bound: Consider faster storage or compression".to_string());
                }
                Bottleneck::SingleThreaded { .. } => {
                    recommendations.push("Underutilized: Increase parallelism".to_string());
                }
                _ => {}
            }
        }

        recommendations
    }
}

impl Default for LockFreeThroughputMonitor {
    fn default() -> Self {
        Self::new()
    }
}
