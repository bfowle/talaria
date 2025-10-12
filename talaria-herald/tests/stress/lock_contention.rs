/// Lock contention stress tests
///
/// These tests verify the system performs well under high lock contention
/// and doesn't deadlock even with many concurrent operations.

use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use parking_lot::Mutex;
use talaria_herald::performance::{
    ThroughputMonitor,
    AdaptiveManager,
    LockFreeThroughputMonitor,
};

const NUM_THREADS: usize = 100;
const OPERATIONS_PER_THREAD: usize = 1000;
const MAX_TEST_DURATION: Duration = Duration::from_secs(30);

#[test]
#[ignore] // Run with: cargo test --ignored lock_contention
fn stress_test_throughput_monitor_high_contention() {
    let monitor = Arc::new(ThroughputMonitor::new());
    let start = Instant::now();

    let handles: Vec<_> = (0..NUM_THREADS).map(|thread_id| {
        let mon = monitor.clone();
        thread::spawn(move || {
            for op_id in 0..OPERATIONS_PER_THREAD {
                // Simulate realistic workload
                mon.record_sequences(thread_id * 1000 + op_id, 10000);

                // Periodically generate reports (high contention operation)
                if op_id % 10 == 0 {
                    mon.record_chunks(5);
                    if op_id % 50 == 0 {
                        let _ = mon.generate_report();
                    }
                }

                // Yield occasionally to increase contention
                if op_id % 100 == 0 {
                    thread::yield_now();
                }

                // Ensure we don't run forever
                if start.elapsed() > MAX_TEST_DURATION {
                    break;
                }
            }
        })
    }).collect();

    for handle in handles {
        handle.join().expect("Thread panicked during stress test");
    }

    let duration = start.elapsed();
    let total_ops = NUM_THREADS * OPERATIONS_PER_THREAD;
    let ops_per_sec = total_ops as f64 / duration.as_secs_f64();

    println!("Stress test completed:");
    println!("  Threads: {}", NUM_THREADS);
    println!("  Total operations: {}", total_ops);
    println!("  Duration: {:?}", duration);
    println!("  Operations/sec: {:.2}", ops_per_sec);

    // Generate final report to ensure no corruption
    let report = monitor.generate_report();
    assert!(report.total_sequences > 0);
}

#[test]
#[ignore] // Run with: cargo test --ignored lock_free_comparison
fn stress_test_lock_free_vs_mutex_performance() {
    println!("\n=== Lock-Free vs Mutex Performance Comparison ===\n");

    // Test mutex-based version
    let mutex_monitor = Arc::new(ThroughputMonitor::new());
    let mutex_start = Instant::now();

    let mutex_handles: Vec<_> = (0..NUM_THREADS).map(|thread_id| {
        let mon = mutex_monitor.clone();
        thread::spawn(move || {
            for op_id in 0..OPERATIONS_PER_THREAD {
                mon.record_sequences(thread_id * 1000 + op_id, 10000);
                if op_id % 50 == 0 {
                    let _ = mon.generate_report();
                }
            }
        })
    }).collect();

    for handle in mutex_handles {
        handle.join().unwrap();
    }

    let mutex_duration = mutex_start.elapsed();

    // Test lock-free version
    let lockfree_monitor = Arc::new(LockFreeThroughputMonitor::new());
    let lockfree_start = Instant::now();

    let lockfree_handles: Vec<_> = (0..NUM_THREADS).map(|thread_id| {
        let mon = lockfree_monitor.clone();
        thread::spawn(move || {
            for op_id in 0..OPERATIONS_PER_THREAD {
                mon.record_sequences(thread_id * 1000 + op_id, 10000);
                if op_id % 50 == 0 {
                    let _ = mon.generate_report();
                }
            }
        })
    }).collect();

    for handle in lockfree_handles {
        handle.join().unwrap();
    }

    let lockfree_duration = lockfree_start.elapsed();

    // Compare results
    println!("Mutex-based implementation:");
    println!("  Duration: {:?}", mutex_duration);
    println!("  Ops/sec: {:.2}", (NUM_THREADS * OPERATIONS_PER_THREAD) as f64 / mutex_duration.as_secs_f64());

    println!("\nLock-free implementation:");
    println!("  Duration: {:?}", lockfree_duration);
    println!("  Ops/sec: {:.2}", (NUM_THREADS * OPERATIONS_PER_THREAD) as f64 / lockfree_duration.as_secs_f64());

    let speedup = mutex_duration.as_secs_f64() / lockfree_duration.as_secs_f64();
    println!("\nSpeedup: {:.2}x", speedup);

    // Lock-free should be at least as fast, ideally faster under high contention
    assert!(lockfree_duration <= mutex_duration.mul_f32(1.5),
            "Lock-free version should not be significantly slower");
}

#[test]
#[ignore] // Run with: cargo test --ignored adaptive_manager_stress
fn stress_test_adaptive_manager_concurrent_adaptation() {
    use talaria_herald::performance::AdaptiveConfigBuilder;

    let config = AdaptiveConfigBuilder::new()
        .min_batch_size(100)
        .max_batch_size(100_000)
        .build();

    let manager = Arc::new(AdaptiveManager::with_config(config).unwrap());
    let start = Instant::now();

    // Spawn threads that update metrics
    let updater_handles: Vec<_> = (0..NUM_THREADS/2).map(|thread_id| {
        let mgr = manager.clone();
        thread::spawn(move || {
            for op_id in 0..OPERATIONS_PER_THREAD {
                mgr.update_metrics(thread_id * 100 + op_id, 50000);

                if start.elapsed() > MAX_TEST_DURATION {
                    break;
                }
            }
        })
    }).collect();

    // Spawn threads that adapt parameters
    let adapter_handles: Vec<_> = (0..NUM_THREADS/2).map(|_| {
        let mgr = manager.clone();
        thread::spawn(move || {
            for _ in 0..OPERATIONS_PER_THREAD/10 {
                mgr.adapt_batch_size();
                mgr.auto_adapt();
                let _ = mgr.get_performance_report();

                thread::sleep(Duration::from_micros(100));

                if start.elapsed() > MAX_TEST_DURATION {
                    break;
                }
            }
        })
    }).collect();

    for handle in updater_handles {
        handle.join().expect("Updater thread panicked");
    }

    for handle in adapter_handles {
        handle.join().expect("Adapter thread panicked");
    }

    let report = manager.get_performance_report();
    println!("\nAdaptive Manager Stress Test Report:");
    println!("{}", report);

    // Verify no corruption
    assert!(manager.get_optimal_batch_size() > 0);
    assert!(manager.get_optimal_buffer_size() > 0);
}

/// Test lock acquisition patterns under extreme contention
#[test]
#[ignore] // Run with: cargo test --ignored extreme_lock_patterns
fn stress_test_extreme_lock_acquisition_patterns() {
    // Create multiple shared resources
    let resources: Vec<Arc<Mutex<u64>>> = (0..10)
        .map(|i| Arc::new(Mutex::new(i as u64)))
        .collect();

    let start = Instant::now();
    let handles: Vec<_> = (0..NUM_THREADS).map(|thread_id| {
        let res = resources.clone();
        thread::spawn(move || {
            let mut rng = thread_id;
            for _ in 0..OPERATIONS_PER_THREAD {
                // Pseudo-random resource selection
                rng = (rng * 1103515245 + 12345) & 0x7fffffff;
                let idx1 = (rng % 10) as usize;
                rng = (rng * 1103515245 + 12345) & 0x7fffffff;
                let idx2 = (rng % 10) as usize;

                if idx1 != idx2 {
                    // Always acquire in consistent order to prevent deadlock
                    let (first, second) = if idx1 < idx2 {
                        (idx1, idx2)
                    } else {
                        (idx2, idx1)
                    };

                    let mut guard1 = res[first].lock();
                    let mut guard2 = res[second].lock();

                    // Simulate work
                    *guard1 = guard1.wrapping_add(1);
                    *guard2 = guard2.wrapping_add(1);
                }

                if start.elapsed() > MAX_TEST_DURATION {
                    break;
                }
            }
        })
    }).collect();

    for handle in handles {
        handle.join().expect("Thread panicked in extreme lock test");
    }

    // Verify all operations completed without deadlock
    let total: u64 = resources.iter().map(|r| *r.lock()).sum();
    println!("Extreme lock pattern test completed. Total increments: {}", total);
    assert!(total > 0);
}

/// Benchmark lock contention with varying thread counts
#[test]
#[ignore] // Run with: cargo test --ignored contention_scaling
fn benchmark_lock_contention_scaling() {
    println!("\n=== Lock Contention Scaling Benchmark ===\n");

    for thread_count in [1, 2, 4, 8, 16, 32, 64, 128].iter() {
        let monitor = Arc::new(ThroughputMonitor::new());
        let start = Instant::now();

        let handles: Vec<_> = (0..*thread_count).map(|thread_id| {
            let mon = monitor.clone();
            thread::spawn(move || {
                for op_id in 0..1000 {
                    mon.record_sequences(thread_id * 1000 + op_id, 10000);
                }
            })
        }).collect();

        for handle in handles {
            handle.join().unwrap();
        }

        let duration = start.elapsed();
        let ops_per_sec = (*thread_count * 1000) as f64 / duration.as_secs_f64();

        println!("Threads: {:3} | Duration: {:?} | Ops/sec: {:.2}",
                 thread_count, duration, ops_per_sec);
    }
}