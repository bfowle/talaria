pub mod adaptive;
pub mod deadlock_detector;
pub mod lock_free_monitor;
/// Performance monitoring and optimization module
pub mod memory_monitor;
pub mod throughput;

pub use adaptive::{AdaptiveConfig, AdaptiveConfigBuilder, AdaptiveManager, PerformanceMetrics};
pub use lock_free_monitor::LockFreeThroughputMonitor;
pub use memory_monitor::{MemoryMonitor, MemoryStats};
pub use throughput::{Bottleneck, PerformanceReport, PerformanceSnapshot, ThroughputMonitor};

/// Get system information for performance tuning
pub fn get_system_info() -> SystemInfo {
    SystemInfo {
        cpu_cores: num_cpus::get(),
        total_memory_mb: get_total_memory_mb(),
        available_memory_mb: get_available_memory_mb(),
    }
}

/// System information
#[derive(Debug, Clone)]
pub struct SystemInfo {
    pub cpu_cores: usize,
    pub total_memory_mb: u64,
    pub available_memory_mb: u64,
}

#[cfg(target_os = "linux")]
fn get_total_memory_mb() -> u64 {
    use std::fs;
    use std::io::{BufRead, BufReader};

    if let Ok(file) = fs::File::open("/proc/meminfo") {
        let reader = BufReader::new(file);
        for line in reader.lines() {
            if let Ok(line) = line {
                if line.starts_with("MemTotal:") {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 {
                        if let Ok(kb) = parts[1].parse::<u64>() {
                            return kb / 1024; // KB to MB
                        }
                    }
                }
            }
        }
    }
    8192 // Default 8GB
}

#[cfg(not(target_os = "linux"))]
fn get_total_memory_mb() -> u64 {
    8192 // Default 8GB
}

#[cfg(target_os = "linux")]
fn get_available_memory_mb() -> u64 {
    use std::fs;
    use std::io::{BufRead, BufReader};

    if let Ok(file) = fs::File::open("/proc/meminfo") {
        let reader = BufReader::new(file);
        for line in reader.lines() {
            if let Ok(line) = line {
                if line.starts_with("MemAvailable:") {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 {
                        if let Ok(kb) = parts[1].parse::<u64>() {
                            return kb / 1024; // KB to MB
                        }
                    }
                }
            }
        }
    }
    4096 // Default 4GB
}

#[cfg(not(target_os = "linux"))]
fn get_available_memory_mb() -> u64 {
    4096 // Default 4GB
}
