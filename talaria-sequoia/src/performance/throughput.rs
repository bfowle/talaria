use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
/// Real-time throughput monitoring and performance tracking
///
/// This module provides live monitoring of processing throughput,
/// identifies bottlenecks, and provides optimization recommendations.
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Moving window for calculating throughput averages
const WINDOW_SIZE: usize = 100;

/// Interval for reporting metrics (seconds)
const REPORT_INTERVAL_SECS: u64 = 10;

/// Throughput monitor for tracking processing performance
#[derive(Clone)]
pub struct ThroughputMonitor {
    inner: Arc<Mutex<MonitorInner>>,
}

struct MonitorInner {
    /// Start time of monitoring
    start_time: Instant,
    /// Last report time
    last_report_time: Instant,
    /// Total sequences processed
    total_sequences: u64,
    /// Total bytes processed
    total_bytes: u64,
    /// Total chunks created
    total_chunks: u64,
    /// Moving window of throughput samples
    sequence_throughput: VecDeque<f64>,
    byte_throughput: VecDeque<f64>,
    chunk_throughput: VecDeque<f64>,
    /// Current batch size
    current_batch_size: usize,
    /// Performance bottlenecks detected
    bottlenecks: Vec<Bottleneck>,
    /// Performance metrics history
    metrics_history: Vec<PerformanceSnapshot>,
}

/// Performance bottleneck types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Bottleneck {
    /// CPU bound - all cores at high utilization
    CpuBound { utilization: f64 },
    /// Memory bound - high memory pressure
    MemoryBound { available_mb: u64, pressure: f64 },
    /// I/O bound - disk throughput limiting
    IoBound { read_mb_sec: f64, write_mb_sec: f64 },
    /// Network bound - for remote operations
    NetworkBound { bandwidth_mb_sec: f64 },
    /// Single-threaded - not utilizing available cores
    SingleThreaded { cpu_cores: usize, utilized: usize },
}

/// Snapshot of performance at a point in time
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceSnapshot {
    pub timestamp: Duration,
    pub sequences_per_sec: f64,
    pub mb_per_sec: f64,
    pub chunks_per_sec: f64,
    pub batch_size: usize,
    pub memory_mb: u64,
    pub cpu_percent: f64,
}

/// Performance report with analysis and recommendations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceReport {
    pub duration: Duration,
    pub total_sequences: u64,
    pub total_bytes: u64,
    pub total_chunks: u64,
    pub avg_sequences_per_sec: f64,
    pub avg_mb_per_sec: f64,
    pub peak_sequences_per_sec: f64,
    pub peak_mb_per_sec: f64,
    pub bottlenecks: Vec<Bottleneck>,
    pub recommendations: Vec<String>,
    pub metrics_history: Vec<PerformanceSnapshot>,
}

impl ThroughputMonitor {
    /// Create a new throughput monitor
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(MonitorInner {
                start_time: Instant::now(),
                last_report_time: Instant::now(),
                total_sequences: 0,
                total_bytes: 0,
                total_chunks: 0,
                sequence_throughput: VecDeque::with_capacity(WINDOW_SIZE),
                byte_throughput: VecDeque::with_capacity(WINDOW_SIZE),
                chunk_throughput: VecDeque::with_capacity(WINDOW_SIZE),
                current_batch_size: 1000,
                bottlenecks: Vec::new(),
                metrics_history: Vec::new(),
            })),
        }
    }

    /// Record sequences processed
    pub fn record_sequences(&self, count: usize, bytes: usize) {
        let mut inner = self.inner.lock();
        inner.total_sequences += count as u64;
        inner.total_bytes += bytes as u64;

        // Calculate instantaneous throughput
        let elapsed = inner.start_time.elapsed().as_secs_f64();
        if elapsed > 0.0 {
            let seq_per_sec = inner.total_sequences as f64 / elapsed;
            let mb_per_sec = (inner.total_bytes as f64 / 1_000_000.0) / elapsed;
            let chunks_per_sec = inner.total_chunks as f64 / elapsed;

            // Update moving averages
            if inner.sequence_throughput.len() >= WINDOW_SIZE {
                inner.sequence_throughput.pop_front();
            }
            inner.sequence_throughput.push_back(seq_per_sec);

            if inner.byte_throughput.len() >= WINDOW_SIZE {
                inner.byte_throughput.pop_front();
            }
            inner.byte_throughput.push_back(mb_per_sec);

            // Check if we should report
            if inner.last_report_time.elapsed() >= Duration::from_secs(REPORT_INTERVAL_SECS) {
                // Extract data needed for metrics calculation before calling methods that might lock
                let current_throughput = inner.sequence_throughput.back().cloned().unwrap_or(0.0);
                let elapsed = inner.start_time.elapsed();
                let batch_size = inner.current_batch_size;

                // Store throughput in history
                inner.sequence_throughput.push_back(seq_per_sec);
                if inner.sequence_throughput.len() > WINDOW_SIZE {
                    inner.sequence_throughput.pop_front();
                }

                inner.byte_throughput.push_back(mb_per_sec);
                if inner.byte_throughput.len() > WINDOW_SIZE {
                    inner.byte_throughput.pop_front();
                }

                inner.chunk_throughput.push_back(chunks_per_sec);
                if inner.chunk_throughput.len() > WINDOW_SIZE {
                    inner.chunk_throughput.pop_front();
                }

                inner.last_report_time = Instant::now();

                // Drop the lock before calling methods
                drop(inner);

                // Now we can safely calculate memory and CPU usage
                let memory_mb = self.get_current_memory_mb();
                let cpu_percent = self.estimate_cpu_usage_from_throughput(current_throughput);

                // Create and log performance snapshot
                let snapshot = PerformanceSnapshot {
                    timestamp: elapsed,
                    sequences_per_sec: seq_per_sec,
                    mb_per_sec,
                    chunks_per_sec,
                    batch_size,
                    memory_mb,
                    cpu_percent,
                };

                // Log the metrics
                tracing::info!(
                    "Performance: {:.0} seq/s, {:.1} MB/s, {:.0} chunks/s | Batch: {} | Memory: {} MB | CPU: {:.0}%",
                    snapshot.sequences_per_sec,
                    snapshot.mb_per_sec,
                    snapshot.chunks_per_sec,
                    snapshot.batch_size,
                    snapshot.memory_mb,
                    snapshot.cpu_percent
                );

                // Store snapshot and detect bottlenecks
                {
                    let mut inner = self.inner.lock();
                    inner.metrics_history.push(snapshot);
                    inner.bottlenecks = self.detect_bottlenecks();
                    for bottleneck in &inner.bottlenecks {
                        tracing::warn!("Bottleneck detected: {:?}", bottleneck);
                    }
                }
            } else {
                // Just update the throughput windows
                inner.sequence_throughput.push_back(seq_per_sec);
                if inner.sequence_throughput.len() > WINDOW_SIZE {
                    inner.sequence_throughput.pop_front();
                }

                inner.byte_throughput.push_back(mb_per_sec);
                if inner.byte_throughput.len() > WINDOW_SIZE {
                    inner.byte_throughput.pop_front();
                }

                inner.chunk_throughput.push_back(chunks_per_sec);
                if inner.chunk_throughput.len() > WINDOW_SIZE {
                    inner.chunk_throughput.pop_front();
                }
            }
        }
    }

    /// Record chunks created
    pub fn record_chunks(&self, count: usize) {
        let mut inner = self.inner.lock();
        inner.total_chunks += count as u64;
    }

    /// Update current batch size
    pub fn update_batch_size(&self, size: usize) {
        let mut inner = self.inner.lock();
        inner.current_batch_size = size;
    }

    /// Detect performance bottlenecks
    pub fn detect_bottlenecks(&self) -> Vec<Bottleneck> {
        let mut bottlenecks = Vec::new();

        // Extract data we need from inner to avoid holding lock
        let (current_throughput, sequence_throughput, byte_throughput) = {
            let inner = self.inner.lock();
            (
                inner.sequence_throughput.back().cloned().unwrap_or(0.0),
                inner.sequence_throughput.clone(),
                inner.byte_throughput.clone(),
            )
        };

        // Check CPU utilization
        let cpu_cores = num_cpus::get();
        let cpu_usage = self.estimate_cpu_usage_from_throughput(current_throughput);

        if cpu_usage > 0.9 * (cpu_cores as f64) {
            bottlenecks.push(Bottleneck::CpuBound {
                utilization: cpu_usage / cpu_cores as f64,
            });
        }

        // Check if single-threaded
        if cpu_usage < 1.5 && cpu_cores > 1 {
            bottlenecks.push(Bottleneck::SingleThreaded {
                cpu_cores,
                utilized: cpu_usage.ceil() as usize,
            });
        }

        // Check memory pressure
        if let Some(memory_pressure) = self.check_memory_pressure() {
            bottlenecks.push(memory_pressure);
        }

        // Check I/O bottlenecks
        if let Some(io_bottleneck) =
            self.check_io_bottleneck_from_data(&sequence_throughput, &byte_throughput)
        {
            bottlenecks.push(io_bottleneck);
        }

        bottlenecks
    }

    /// Generate performance report
    pub fn generate_report(&self) -> PerformanceReport {
        let inner = self.inner.lock();
        let duration = inner.start_time.elapsed();

        let avg_seq_per_sec = if !inner.sequence_throughput.is_empty() {
            inner.sequence_throughput.iter().sum::<f64>() / inner.sequence_throughput.len() as f64
        } else {
            0.0
        };

        let avg_mb_per_sec = if !inner.byte_throughput.is_empty() {
            inner.byte_throughput.iter().sum::<f64>() / inner.byte_throughput.len() as f64
        } else {
            0.0
        };

        let peak_seq_per_sec = inner
            .sequence_throughput
            .iter()
            .cloned()
            .fold(0.0, f64::max);
        let peak_mb_per_sec = inner.byte_throughput.iter().cloned().fold(0.0, f64::max);

        let recommendations = self.generate_recommendations(&inner);

        PerformanceReport {
            duration,
            total_sequences: inner.total_sequences,
            total_bytes: inner.total_bytes,
            total_chunks: inner.total_chunks,
            avg_sequences_per_sec: avg_seq_per_sec,
            avg_mb_per_sec: avg_mb_per_sec,
            peak_sequences_per_sec: peak_seq_per_sec,
            peak_mb_per_sec: peak_mb_per_sec,
            bottlenecks: inner.bottlenecks.clone(),
            recommendations,
            metrics_history: inner.metrics_history.clone(),
        }
    }

    /// Get current throughput
    pub fn current_throughput(&self) -> (f64, f64) {
        let inner = self.inner.lock();

        let seq_per_sec = inner.sequence_throughput.back().cloned().unwrap_or(0.0);
        let mb_per_sec = inner.byte_throughput.back().cloned().unwrap_or(0.0);

        (seq_per_sec, mb_per_sec)
    }

    /// Private helper methods

    fn estimate_cpu_usage_from_throughput(&self, current_throughput: f64) -> f64 {
        // Simplified CPU estimation based on throughput
        // In production, use proper CPU monitoring
        let max_theoretical = 200_000.0; // Theoretical max sequences/sec
        (current_throughput / max_theoretical) * num_cpus::get() as f64
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

    fn check_io_bottleneck_from_data(
        &self,
        _seq_throughput: &VecDeque<f64>,
        byte_throughput: &VecDeque<f64>,
    ) -> Option<Bottleneck> {
        // Check if throughput is limited by I/O
        let mb_per_sec = byte_throughput.back().cloned().unwrap_or(0.0);

        // Typical SSD can do 500+ MB/s
        // If we're processing less than 50 MB/s, might be I/O bound
        // Check if we have processed significant data
        let total_bytes = {
            let inner = self.inner.lock();
            inner.total_bytes
        };

        if mb_per_sec < 50.0 && total_bytes > 100_000_000 {
            Some(Bottleneck::IoBound {
                read_mb_sec: mb_per_sec,
                write_mb_sec: mb_per_sec / 2.0, // Assume write is half
            })
        } else {
            None
        }
    }

    fn generate_recommendations(&self, inner: &MonitorInner) -> Vec<String> {
        let mut recommendations = Vec::new();

        for bottleneck in &inner.bottlenecks {
            match bottleneck {
                Bottleneck::CpuBound { .. } => {
                    recommendations
                        .push("Consider using release build for better performance".to_string());
                    recommendations.push("Increase batch size to reduce overhead".to_string());
                }
                Bottleneck::MemoryBound { available_mb, .. } => {
                    recommendations.push(format!(
                        "Reduce batch size - only {} MB available",
                        available_mb
                    ));
                    recommendations.push("Close other applications to free memory".to_string());
                }
                Bottleneck::IoBound { read_mb_sec, .. } => {
                    recommendations.push(format!(
                        "I/O limited to {:.1} MB/s - consider faster storage",
                        read_mb_sec
                    ));
                    recommendations
                        .push("Use SSD instead of HDD for better performance".to_string());
                }
                Bottleneck::SingleThreaded {
                    cpu_cores,
                    utilized,
                } => {
                    recommendations.push(format!(
                        "Using only {} of {} cores - enable parallel processing",
                        utilized, cpu_cores
                    ));
                    recommendations.push("Increase batch size to utilize more cores".to_string());
                }
                _ => {}
            }
        }

        // General recommendations based on metrics
        if inner.current_batch_size < 1000 {
            recommendations
                .push("Batch size is very small - consider increasing to 5000+".to_string());
        }

        let avg_seq_per_sec = if !inner.sequence_throughput.is_empty() {
            inner.sequence_throughput.iter().sum::<f64>() / inner.sequence_throughput.len() as f64
        } else {
            0.0
        };

        if avg_seq_per_sec < 10_000.0 {
            recommendations
                .push("Performance is below expected - ensure using release build".to_string());
        }

        recommendations
    }
}

/// Format performance report as a string
impl PerformanceReport {
    pub fn format(&self) -> String {
        let mut output = String::new();

        output.push_str(&format!("=== Performance Report ===\n"));
        output.push_str(&format!("Duration: {:.1}s\n", self.duration.as_secs_f64()));
        output.push_str(&format!("Total Sequences: {}\n", self.total_sequences));
        output.push_str(&format!(
            "Total Data: {:.1} MB\n",
            self.total_bytes as f64 / 1_000_000.0
        ));
        output.push_str(&format!("Total Chunks: {}\n", self.total_chunks));
        output.push_str(&format!("\nThroughput:\n"));
        output.push_str(&format!(
            "  Average: {:.0} seq/s, {:.1} MB/s\n",
            self.avg_sequences_per_sec, self.avg_mb_per_sec
        ));
        output.push_str(&format!(
            "  Peak: {:.0} seq/s, {:.1} MB/s\n",
            self.peak_sequences_per_sec, self.peak_mb_per_sec
        ));

        if !self.bottlenecks.is_empty() {
            output.push_str(&format!("\nBottlenecks Detected:\n"));
            for bottleneck in &self.bottlenecks {
                output.push_str(&format!("  - {:?}\n", bottleneck));
            }
        }

        if !self.recommendations.is_empty() {
            output.push_str(&format!("\nRecommendations:\n"));
            for rec in &self.recommendations {
                output.push_str(&format!("  • {}\n", rec));
            }
        }

        output
    }

    /// Export report as JSON
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Export metrics as CSV
    pub fn metrics_to_csv(&self) -> String {
        let mut csv = String::from("timestamp_sec,sequences_per_sec,mb_per_sec,chunks_per_sec,batch_size,memory_mb,cpu_percent\n");

        for snapshot in &self.metrics_history {
            csv.push_str(&format!(
                "{:.1},{:.0},{:.1},{:.0},{},{},{:.1}\n",
                snapshot.timestamp.as_secs_f64(),
                snapshot.sequences_per_sec,
                snapshot.mb_per_sec,
                snapshot.chunks_per_sec,
                snapshot.batch_size,
                snapshot.memory_mb,
                snapshot.cpu_percent,
            ));
        }

        csv
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_throughput_monitoring() {
        let monitor = ThroughputMonitor::new();

        // Record some sequences
        for _ in 0..10 {
            monitor.record_sequences(1000, 500_000);
            std::thread::sleep(Duration::from_millis(10));
        }

        let (seq_per_sec, mb_per_sec) = monitor.current_throughput();
        assert!(seq_per_sec > 0.0);
        assert!(mb_per_sec > 0.0);

        let report = monitor.generate_report();
        assert_eq!(report.total_sequences, 10_000);
        assert_eq!(report.total_bytes, 5_000_000);
    }

    #[test]
    fn test_bottleneck_detection() {
        let monitor = ThroughputMonitor::new();

        // Simulate low throughput (potential I/O bottleneck)
        monitor.record_sequences(100, 50_000);

        let bottlenecks = monitor.detect_bottlenecks();
        // Bottlenecks might be detected based on system state
        println!("Detected bottlenecks: {:?}", bottlenecks);
    }

    #[test]
    fn test_report_generation() {
        let monitor = ThroughputMonitor::new();

        monitor.record_sequences(10_000, 5_000_000);
        monitor.record_chunks(100);
        monitor.update_batch_size(5000);

        let report = monitor.generate_report();
        let formatted = report.format();

        assert!(formatted.contains("Performance Report"));
        assert!(formatted.contains("Total Sequences: 10000"));

        // Test JSON export
        let json = report.to_json().unwrap();
        assert!(json.contains("\"total_sequences\": 10000"));

        // Test CSV export
        let csv = report.metrics_to_csv();
        assert!(csv.starts_with("timestamp_sec,sequences_per_sec"));
    }
}
