use anyhow::Result;
/// Memory monitoring and management for adaptive performance tuning
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

/// Memory statistics
#[derive(Debug, Clone, Copy)]
pub struct MemoryStats {
    /// Total system memory in bytes
    pub total: u64,
    /// Available memory in bytes
    pub available: u64,
    /// Current process RSS in bytes
    pub process_rss: u64,
    /// Memory usage percentage (0.0 to 1.0)
    pub usage_ratio: f64,
}

impl MemoryStats {
    /// Check if memory pressure is high
    pub fn has_pressure(&self, threshold: f64) -> bool {
        self.usage_ratio > threshold
    }

    /// Get available memory in MB
    pub fn available_mb(&self) -> u64 {
        self.available / (1024 * 1024)
    }

    /// Get total memory in MB
    pub fn total_mb(&self) -> u64 {
        self.total / (1024 * 1024)
    }

    /// Get used memory in MB
    pub fn used_mb(&self) -> u64 {
        (self.total - self.available) / (1024 * 1024)
    }

    /// Get process RSS in MB
    pub fn process_rss_mb(&self) -> u64 {
        self.process_rss / (1024 * 1024)
    }
}

/// Memory monitor that tracks system and process memory usage
pub struct MemoryMonitor {
    /// Current memory stats (atomic for thread safety)
    current_stats: Arc<AtomicMemoryStats>,
    /// Whether monitoring is active
    active: Arc<AtomicBool>,
    /// Monitor thread handle
    monitor_thread: Option<thread::JoinHandle<()>>,
}

/// Atomic version of memory stats for thread-safe access
struct AtomicMemoryStats {
    total: AtomicU64,
    available: AtomicU64,
    process_rss: AtomicU64,
}

impl AtomicMemoryStats {
    fn new() -> Self {
        Self {
            total: AtomicU64::new(0),
            available: AtomicU64::new(0),
            process_rss: AtomicU64::new(0),
        }
    }

    fn update(&self, stats: MemoryStats) {
        self.total.store(stats.total, Ordering::Relaxed);
        self.available.store(stats.available, Ordering::Relaxed);
        self.process_rss.store(stats.process_rss, Ordering::Relaxed);
    }

    fn get(&self) -> MemoryStats {
        let total = self.total.load(Ordering::Relaxed);
        let available = self.available.load(Ordering::Relaxed);
        let process_rss = self.process_rss.load(Ordering::Relaxed);

        MemoryStats {
            total,
            available,
            process_rss,
            usage_ratio: if total > 0 {
                1.0 - (available as f64 / total as f64)
            } else {
                0.0
            },
        }
    }
}

impl MemoryMonitor {
    /// Create a new memory monitor
    pub fn new() -> Self {
        Self {
            current_stats: Arc::new(AtomicMemoryStats::new()),
            active: Arc::new(AtomicBool::new(false)),
            monitor_thread: None,
        }
    }

    /// Start monitoring memory usage
    pub fn start(&mut self, update_interval: Duration) {
        if self.active.load(Ordering::Relaxed) {
            return; // Already monitoring
        }

        self.active.store(true, Ordering::Relaxed);

        let stats = Arc::clone(&self.current_stats);
        let active = Arc::clone(&self.active);

        let handle = thread::spawn(move || {
            while active.load(Ordering::Relaxed) {
                if let Ok(memory_stats) = Self::get_system_memory() {
                    stats.update(memory_stats);
                }
                thread::sleep(update_interval);
            }
        });

        self.monitor_thread = Some(handle);
    }

    /// Stop monitoring
    pub fn stop(&mut self) {
        self.active.store(false, Ordering::Relaxed);
        if let Some(handle) = self.monitor_thread.take() {
            let _ = handle.join();
        }
    }

    /// Get current memory statistics
    pub fn get_stats(&self) -> MemoryStats {
        self.current_stats.get()
    }

    /// Check if memory pressure exists
    pub fn has_memory_pressure(&self, threshold: f64) -> bool {
        self.get_stats().has_pressure(threshold)
    }

    /// Get system memory statistics
    #[cfg(target_os = "linux")]
    fn get_system_memory() -> Result<MemoryStats> {
        use std::fs;
        use std::io::{BufRead, BufReader};

        // Parse /proc/meminfo
        let file = fs::File::open("/proc/meminfo")?;
        let reader = BufReader::new(file);

        let mut total = 0u64;
        let mut available = 0u64;

        for line in reader.lines() {
            let line = line?;
            let parts: Vec<&str> = line.split_whitespace().collect();

            if parts.len() >= 2 {
                match parts[0] {
                    "MemTotal:" => {
                        total = parts[1].parse::<u64>().unwrap_or(0) * 1024; // Convert KB to bytes
                    }
                    "MemAvailable:" => {
                        available = parts[1].parse::<u64>().unwrap_or(0) * 1024;
                    }
                    _ => {}
                }
            }
        }

        // Get process RSS from /proc/self/status
        let process_rss = Self::get_process_rss()?;

        Ok(MemoryStats {
            total,
            available,
            process_rss,
            usage_ratio: if total > 0 {
                1.0 - (available as f64 / total as f64)
            } else {
                0.0
            },
        })
    }

    /// Get system memory statistics (fallback for non-Linux)
    #[cfg(not(target_os = "linux"))]
    fn get_system_memory() -> Result<MemoryStats> {
        use sysinfo::{PidExt, ProcessExt, System, SystemExt};

        let mut sys = System::new_all();
        sys.refresh_memory();
        sys.refresh_processes();

        let total = sys.total_memory() * 1024; // KB to bytes
        let available = sys.available_memory() * 1024; // KB to bytes

        // Get current process RSS
        let pid = sysinfo::get_current_pid().unwrap_or(sysinfo::Pid::from(0));
        let process_rss = sys
            .process(pid)
            .map(|p| p.memory() * 1024) // KB to bytes
            .unwrap_or(0);

        let usage_ratio = if total > 0 {
            (total - available) as f64 / total as f64
        } else {
            0.0
        };

        Ok(MemoryStats {
            total,
            available,
            process_rss,
            usage_ratio,
        })
    }

    /// Get process RSS
    #[cfg(target_os = "linux")]
    fn get_process_rss() -> Result<u64> {
        use std::fs;
        use std::io::{BufRead, BufReader};

        let file = fs::File::open("/proc/self/status")?;
        let reader = BufReader::new(file);

        for line in reader.lines() {
            let line = line?;
            if line.starts_with("VmRSS:") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    return Ok(parts[1].parse::<u64>().unwrap_or(0) * 1024); // KB to bytes
                }
            }
        }

        Ok(0)
    }

    /// Get process RSS (fallback)
    #[cfg(not(target_os = "linux"))]
    fn get_process_rss() -> Result<u64> {
        use sysinfo::{PidExt, ProcessExt, System, SystemExt};

        let mut sys = System::new_all();
        sys.refresh_processes();

        let pid = sysinfo::get_current_pid().unwrap_or(sysinfo::Pid::from(0));
        let process_rss = sys
            .process(pid)
            .map(|p| p.memory() * 1024) // KB to bytes
            .unwrap_or(0);

        Ok(process_rss)
    }

    /// Estimate memory needed for batch processing
    pub fn estimate_batch_memory(batch_size: usize, avg_sequence_size: usize) -> u64 {
        // Rough estimate: sequence data + overhead (metadata, indices, etc.)
        let sequence_memory = (batch_size * avg_sequence_size) as u64;
        let overhead_factor = 2; // Account for processing overhead
        sequence_memory * overhead_factor
    }

    /// Calculate optimal batch size based on available memory
    pub fn calculate_optimal_batch_size(
        &self,
        target_memory_mb: u64,
        avg_sequence_size: usize,
        min_batch: usize,
        max_batch: usize,
    ) -> usize {
        let stats = self.get_stats();
        let available_mb = stats.available_mb();

        // Use at most target_memory_mb or 50% of available memory
        let usable_memory_mb = target_memory_mb.min(available_mb / 2);
        let usable_bytes = usable_memory_mb * 1024 * 1024;

        // Calculate how many sequences fit in usable memory
        let overhead_factor = 2; // Account for processing overhead
        let bytes_per_sequence = (avg_sequence_size * overhead_factor) as u64;

        let optimal_size = if bytes_per_sequence > 0 {
            (usable_bytes / bytes_per_sequence) as usize
        } else {
            max_batch
        };

        // Clamp to min/max bounds
        optimal_size.max(min_batch).min(max_batch)
    }
}

impl Drop for MemoryMonitor {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_stats() {
        let stats = MemoryStats {
            total: 8_589_934_592,       // 8GB
            available: 4_294_967_296,   // 4GB
            process_rss: 1_073_741_824, // 1GB
            usage_ratio: 0.5,
        };

        assert_eq!(stats.available_mb(), 4096);
        assert_eq!(stats.process_rss_mb(), 1024);
        assert!(!stats.has_pressure(0.7));
        assert!(stats.has_pressure(0.3));
    }

    #[test]
    fn test_memory_monitor_creation() {
        let monitor = MemoryMonitor::new();
        let stats = monitor.get_stats();

        // Initial stats should be zero
        assert_eq!(stats.total, 0);
        assert_eq!(stats.available, 0);
    }

    #[test]
    fn test_batch_memory_estimation() {
        let estimated = MemoryMonitor::estimate_batch_memory(1000, 500);
        assert_eq!(estimated, 1_000_000); // 1000 * 500 * 2
    }

    #[test]
    fn test_optimal_batch_calculation() {
        let monitor = MemoryMonitor::new();

        // Manually set some stats for testing
        let optimal = monitor.calculate_optimal_batch_size(
            100,   // 100MB target
            500,   // 500 bytes avg sequence
            100,   // min batch
            10000, // max batch
        );

        // Should be within bounds
        assert!(optimal >= 100);
        assert!(optimal <= 10000);
    }
}
