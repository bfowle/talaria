use parking_lot::Mutex;
/// Regression tests for deadlock scenarios
///
/// These tests specifically target deadlock scenarios that have occurred in production,
/// particularly the ThroughputMonitor deadlock that caused WSL crashes.
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use tempfile::TempDir;

use talaria_sequoia::performance::{
    AdaptiveConfigBuilder, AdaptiveManager, LockFreeThroughputMonitor, ThroughputMonitor,
};

/// Test timeout - if a test takes longer than this, it's likely deadlocked
const TEST_TIMEOUT: Duration = Duration::from_secs(5);

/// Helper macro to run a test with timeout
macro_rules! run_with_timeout {
    ($test_name:expr, $test_fn:expr) => {{
        let start = Instant::now();
        let handle = thread::spawn(move || $test_fn());

        let mut elapsed = Duration::ZERO;
        while elapsed < TEST_TIMEOUT {
            if handle.is_finished() {
                return handle.join().expect("Test thread panicked");
            }
            thread::sleep(Duration::from_millis(100));
            elapsed = start.elapsed();
        }

        panic!(
            "Test '{}' timed out after {:?} - likely deadlocked",
            $test_name, TEST_TIMEOUT
        );
    }};
}

#[test]
fn test_throughput_monitor_no_deadlock_on_concurrent_access() {
    run_with_timeout!("concurrent_throughput_access", || {
        let monitor = Arc::new(ThroughputMonitor::new());
        let handles: Vec<_> = (0..10)
            .map(|i| {
                let mon = monitor.clone();
                thread::spawn(move || {
                    for j in 0..100 {
                        // Record sequences - this caused the original deadlock
                        mon.record_sequences(100 * i + j, 50000);

                        // This would trigger detect_bottlenecks internally
                        if j % 10 == 0 {
                            let _ = mon.generate_report();
                        }

                        thread::sleep(Duration::from_micros(10));
                    }
                })
            })
            .collect();

        for handle in handles {
            handle.join().expect("Thread panicked");
        }
    });
}

#[test]
fn test_adaptive_manager_no_deadlock_on_metrics_update() {
    run_with_timeout!("adaptive_manager_metrics", || {
        let config = AdaptiveConfigBuilder::new()
            .min_batch_size(100)
            .max_batch_size(10000)
            .build();

        let manager = Arc::new(AdaptiveManager::with_config(config).unwrap());

        let handles: Vec<_> = (0..5)
            .map(|i| {
                let mgr = manager.clone();
                thread::spawn(move || {
                    for j in 0..50 {
                        // Update metrics
                        mgr.update_metrics(100 * i + j, 10000);

                        // Adapt batch size - tests lock ordering
                        mgr.adapt_batch_size();

                        // Get performance report - tests multiple lock acquisition
                        let _ = mgr.get_performance_report();

                        // Check memory pressure
                        let _ = mgr.has_memory_pressure();

                        thread::sleep(Duration::from_micros(10));
                    }
                })
            })
            .collect();

        for handle in handles {
            handle.join().expect("Thread panicked");
        }
    });
}

#[test]
fn test_monitoring_with_aggressive_intervals() {
    run_with_timeout!("aggressive_monitoring", || {
        let monitor = Arc::new(ThroughputMonitor::new());

        // Simulate the scenario that crashed WSL
        let recorder = monitor.clone();
        let recorder_handle = thread::spawn(move || {
            for i in 0..1000 {
                recorder.record_sequences(100, 50000);
                if i % 10 == 0 {
                    recorder.record_chunks(10);
                }
                thread::sleep(Duration::from_micros(100));
            }
        });

        let reporter = monitor.clone();
        let reporter_handle = thread::spawn(move || {
            for _ in 0..100 {
                let _ = reporter.generate_report();
                thread::sleep(Duration::from_millis(10));
            }
        });

        recorder_handle.join().expect("Recorder thread panicked");
        reporter_handle.join().expect("Reporter thread panicked");
    });
}

#[test]
fn test_lock_free_monitor_performs_without_deadlock() {
    // The lock-free version should never deadlock
    let monitor = Arc::new(LockFreeThroughputMonitor::new());

    let handles: Vec<_> = (0..20)
        .map(|i| {
            let mon = monitor.clone();
            thread::spawn(move || {
                for j in 0..100 {
                    mon.record_sequences(100 * i + j, 50000);
                    mon.record_chunks(5);
                    mon.update_batch_size(1000 + i * 100);

                    if j % 10 == 0 {
                        let _ = mon.generate_report();
                        let _ = mon.detect_bottlenecks();
                    }
                }
            })
        })
        .collect();

    for handle in handles {
        handle.join().expect("Thread panicked");
    }
}

#[test]
fn test_nested_lock_prevention() {
    run_with_timeout!("nested_lock_prevention", || {
        // Test that we don't have nested lock acquisitions
        struct TestStruct {
            data: Arc<Mutex<Vec<i32>>>,
        }

        impl TestStruct {
            #[allow(dead_code)]
            fn safe_operation(&self) -> usize {
                // Extract data, release lock, then process
                let data_copy = {
                    let guard = self.data.lock();
                    guard.clone()
                };
                // Lock is released here
                data_copy.len()
            }
        }

        let test = TestStruct {
            data: Arc::new(Mutex::new(vec![1, 2, 3])),
        };

        // Test that the safe operation pattern works
        let _ = test.safe_operation();

        // Multiple threads calling safe_operation should not deadlock
        let handles: Vec<_> = (0..10)
            .map(|_| {
                let data = test.data.clone();
                thread::spawn(move || {
                    for _ in 0..100 {
                        let mut guard = data.lock();
                        guard.push(1);
                        drop(guard); // Explicitly drop to release lock

                        // Safe to acquire again
                        let guard = data.lock();
                        let _ = guard.len();
                    }
                })
            })
            .collect();

        for handle in handles {
            handle.join().expect("Thread panicked");
        }
    });
}

#[test]
fn test_performance_monitoring_under_load() {
    run_with_timeout!("monitoring_under_load", || {
        let temp_dir = TempDir::new().unwrap();
        std::env::set_var("TALARIA_DATA_DIR", temp_dir.path());

        // Create monitor with aggressive settings
        std::env::set_var("TALARIA_MONITOR", "1");
        std::env::set_var("TALARIA_MONITOR_INTERVAL", "1");

        let monitor = Arc::new(ThroughputMonitor::new());

        // Simulate heavy load
        let handles: Vec<_> = (0..10)
            .map(|i| {
                let mon = monitor.clone();
                thread::spawn(move || {
                    for j in 0..100 {
                        mon.record_sequences(1000 + i * 100 + j, 100000);
                        mon.update_batch_size(1000 + i * 10);

                        if j % 5 == 0 {
                            mon.record_chunks(10 + i);
                        }
                    }
                })
            })
            .collect();

        // Concurrent report generation
        let reporter = monitor.clone();
        let report_handle = thread::spawn(move || {
            for _ in 0..20 {
                let report = reporter.generate_report();
                assert!(report.total_sequences > 0);
                thread::sleep(Duration::from_millis(50));
            }
        });

        for handle in handles {
            handle.join().expect("Worker thread panicked");
        }
        report_handle.join().expect("Reporter thread panicked");

        // Clean up env vars
        std::env::remove_var("TALARIA_MONITOR");
        std::env::remove_var("TALARIA_MONITOR_INTERVAL");
        std::env::remove_var("TALARIA_DATA_DIR");
    });
}

#[cfg(feature = "deadlock_detection")]
#[test]
fn test_parking_lot_deadlock_detection() {
    use parking_lot::deadlock;

    // Start deadlock detector
    let detector_handle = thread::spawn(|| {
        for _ in 0..50 {
            // Check for 5 seconds
            thread::sleep(Duration::from_millis(100));
            let deadlocks = deadlock::check_deadlock();
            if !deadlocks.is_empty() {
                panic!("Deadlock detected in test!");
            }
        }
    });

    // Run some concurrent operations that could potentially deadlock
    let monitor = Arc::new(ThroughputMonitor::new());
    let handles: Vec<_> = (0..5)
        .map(|i| {
            let mon = monitor.clone();
            thread::spawn(move || {
                for j in 0..50 {
                    mon.record_sequences(100 * i + j, 50000);
                    let _ = mon.generate_report();
                }
            })
        })
        .collect();

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    detector_handle.join().expect("Detector thread panicked");
}

/// Test that demonstrates the original deadlock scenario is fixed
#[test]
fn test_original_deadlock_scenario_fixed() {
    run_with_timeout!("original_deadlock_fixed", || {
        // This test replicates the exact scenario that caused WSL to crash
        let monitor = ThroughputMonitor::new();

        // Phase 1: Initial sequences
        monitor.record_sequences(182281, 392_100_000);

        // Phase 2: This would trigger detect_bottlenecks which called estimate_cpu_usage
        // while holding the lock - this used to deadlock
        let report = monitor.generate_report();

        // If we get here, the deadlock is fixed
        assert!(report.total_sequences > 0);
        assert!(report.total_bytes > 0);
    });
}
