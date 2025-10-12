/// Deadlock detection guard for integration tests
///
/// This module provides utilities to automatically detect deadlocks
/// in integration tests and provide better error messages.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;
use std::panic;

/// Guard that monitors for deadlocks during test execution
pub struct DeadlockGuard {
    detector_thread: Option<thread::JoinHandle<()>>,
    should_stop: Arc<AtomicBool>,
    test_name: String,
}

impl DeadlockGuard {
    /// Create a new deadlock guard for a test
    pub fn new(test_name: impl Into<String>) -> Self {
        let test_name = test_name.into();
        let should_stop = Arc::new(AtomicBool::new(false));
        let should_stop_clone = should_stop.clone();
        let test_name_clone = test_name.clone();

        #[cfg(feature = "deadlock_detection")]
        let detector_thread = {
            Some(thread::spawn(move || {
                use parking_lot::deadlock;

                while !should_stop_clone.load(Ordering::Relaxed) {
                    thread::sleep(Duration::from_millis(100));

                    let deadlocks = deadlock::check_deadlock();
                    if !deadlocks.is_empty() {
                        eprintln!("\n!!! DEADLOCK DETECTED IN TEST: {} !!!", test_name_clone);
                        eprintln!("Number of deadlock cycles: {}", deadlocks.len());

                        for (i, threads) in deadlocks.iter().enumerate() {
                            eprintln!("\nDeadlock cycle #{}:", i + 1);
                            for t in threads {
                                eprintln!("  Thread {:?}", t.thread_id());
                                // Print first few frames of backtrace for context
                                let backtrace = format!("{:?}", t.backtrace());
                                for line in backtrace.lines().take(10) {
                                    eprintln!("    {}", line);
                                }
                            }
                        }

                        panic!("Deadlock detected in test: {}", test_name_clone);
                    }
                }
            }))
        };

        #[cfg(not(feature = "deadlock_detection"))]
        let detector_thread = None;

        Self {
            detector_thread,
            should_stop,
            test_name,
        }
    }

    /// Stop monitoring and return whether a deadlock was detected
    pub fn stop(self) -> bool {
        self.should_stop.store(true, Ordering::Relaxed);

        if let Some(handle) = self.detector_thread {
            // Use panic::catch_unwind to see if the detector thread panicked
            match panic::catch_unwind(panic::AssertUnwindSafe(|| handle.join())) {
                Ok(Ok(())) => false, // No deadlock detected
                _ => true, // Deadlock was detected (thread panicked)
            }
        } else {
            false
        }
    }
}

impl Drop for DeadlockGuard {
    fn drop(&mut self) {
        self.should_stop.store(true, Ordering::Relaxed);
        // Don't join in drop to avoid blocking
    }
}

/// Macro to wrap a test with deadlock detection
#[macro_export]
macro_rules! with_deadlock_detection {
    ($test_name:expr, $test_body:block) => {{
        use $crate::common::deadlock_guard::DeadlockGuard;

        let _guard = DeadlockGuard::new($test_name);
        let result = $test_body;

        if _guard.stop() {
            panic!("Test failed due to deadlock");
        }

        result
    }};
}

/// Test timeout guard that automatically fails if test takes too long
pub struct TimeoutGuard {
    start_time: std::time::Instant,
    timeout: Duration,
    test_name: String,
    checker_thread: Option<thread::JoinHandle<()>>,
    should_stop: Arc<AtomicBool>,
}

impl TimeoutGuard {
    pub fn new(test_name: impl Into<String>, timeout: Duration) -> Self {
        let test_name = test_name.into();
        let should_stop = Arc::new(AtomicBool::new(false));
        let should_stop_clone = should_stop.clone();
        let test_name_clone = test_name.clone();

        let checker_thread = Some(thread::spawn(move || {
            thread::sleep(timeout);
            if !should_stop_clone.load(Ordering::Relaxed) {
                eprintln!("\n!!! TEST TIMEOUT: {} !!!", test_name_clone);
                eprintln!("Test exceeded timeout of {:?}", timeout);
                eprintln!("This may indicate a deadlock or infinite loop");
                panic!("Test '{}' timed out after {:?}", test_name_clone, timeout);
            }
        }));

        Self {
            start_time: std::time::Instant::now(),
            timeout,
            test_name,
            checker_thread,
            should_stop,
        }
    }

    pub fn stop(self) -> Duration {
        self.should_stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.checker_thread {
            // Don't wait for the thread if it's still sleeping
            let _ = handle.join();
        }
        self.start_time.elapsed()
    }
}

impl Drop for TimeoutGuard {
    fn drop(&mut self) {
        self.should_stop.store(true, Ordering::Relaxed);
    }
}

/// Combined guard for both deadlock detection and timeout
pub struct TestGuard {
    deadlock_guard: DeadlockGuard,
    timeout_guard: TimeoutGuard,
}

impl TestGuard {
    pub fn new(test_name: impl Into<String> + Clone, timeout: Duration) -> Self {
        let test_name = test_name.into();
        Self {
            deadlock_guard: DeadlockGuard::new(test_name.clone()),
            timeout_guard: TimeoutGuard::new(test_name, timeout),
        }
    }

    pub fn complete(self) -> Duration {
        let duration = self.timeout_guard.stop();
        if self.deadlock_guard.stop() {
            panic!("Test failed due to deadlock");
        }
        duration
    }
}

/// Helper macro to run a test with both deadlock detection and timeout
#[macro_export]
macro_rules! guarded_test {
    ($test_name:expr, $timeout_secs:expr, $test_body:block) => {{
        use $crate::common::deadlock_guard::TestGuard;
        use std::time::Duration;

        let guard = TestGuard::new($test_name, Duration::from_secs($timeout_secs));
        let result = $test_body;
        let duration = guard.complete();

        println!("Test '{}' completed in {:?}", $test_name, duration);
        result
    }};
}

#[cfg(test)]
mod tests {
    use super::*;
    use parking_lot::Mutex;

    #[test]
    fn test_deadlock_guard_no_deadlock() {
        let guard = DeadlockGuard::new("test_no_deadlock");

        // Do some work that doesn't deadlock
        let data = Arc::new(Mutex::new(0));
        for _ in 0..10 {
            let mut d = data.lock();
            *d += 1;
        }

        assert!(!guard.stop());
    }

    #[test]
    fn test_timeout_guard() {
        let guard = TimeoutGuard::new("test_timeout", Duration::from_secs(1));

        thread::sleep(Duration::from_millis(100));

        let elapsed = guard.stop();
        assert!(elapsed < Duration::from_secs(1));
    }

    #[test]
    fn test_combined_guard() {
        let guard = TestGuard::new("test_combined", Duration::from_secs(5));

        // Do some work
        thread::sleep(Duration::from_millis(100));

        let duration = guard.complete();
        assert!(duration < Duration::from_secs(1));
    }
}