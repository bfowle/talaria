use std::sync::atomic::{AtomicBool, Ordering};
/// Runtime deadlock detection for development and testing
///
/// This module provides automatic deadlock detection using parking_lot's
/// deadlock detection API. It should be enabled in debug builds and tests.
use std::sync::Arc;
use std::thread;
use std::time::Duration;

/// Deadlock detector that runs in a background thread
pub struct DeadlockDetector {
    running: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl DeadlockDetector {
    /// Create and start a new deadlock detector
    ///
    /// The detector will check for deadlocks at the specified interval
    /// and panic if any are found (in debug mode) or log them (in release mode).
    #[cfg(feature = "deadlock_detection")]
    pub fn new(check_interval: Duration) -> Self {
        let running = Arc::new(AtomicBool::new(true));
        let running_clone = running.clone();

        let handle = thread::spawn(move || {
            use parking_lot::deadlock;

            tracing::info!(
                "Deadlock detector started with interval {:?}",
                check_interval
            );

            while running_clone.load(Ordering::Relaxed) {
                thread::sleep(check_interval);

                let deadlocks = deadlock::check_deadlock();
                if deadlocks.is_empty() {
                    continue;
                }

                // Deadlock detected!
                tracing::warn!("\n═══════════════════════════════════════════════════");
                tracing::warn!("                DEADLOCK DETECTED!");
                tracing::warn!("═══════════════════════════════════════════════════");

                for (i, threads) in deadlocks.iter().enumerate() {
                    tracing::warn!("\nDeadlock #{}", i + 1);
                    tracing::warn!("───────────────────────────────────────────────────");

                    for t in threads {
                        tracing::warn!("Thread {:?}:", t.thread_id());
                        tracing::warn!("{:#?}", t.backtrace());
                    }
                }

                tracing::warn!("═══════════════════════════════════════════════════\n");

                // Log to tracing system as well
                for (i, threads) in deadlocks.iter().enumerate() {
                    tracing::error!(
                        deadlock_id = i + 1,
                        thread_count = threads.len(),
                        "Deadlock detected!"
                    );

                    for t in threads {
                        tracing::error!(
                            thread_id = ?t.thread_id(),
                            backtrace = ?t.backtrace(),
                            "Deadlocked thread"
                        );
                    }
                }

                #[cfg(debug_assertions)]
                {
                    // In debug builds, panic to catch issues early
                    panic!("Deadlock detected! See backtrace above for details.");
                }

                #[cfg(not(debug_assertions))]
                {
                    // In release builds, just log and continue
                    // Could also trigger alerts or recovery mechanisms here
                    tracing::error!("Deadlock detected in release build - system may be unstable");
                }
            }

            tracing::info!("Deadlock detector stopped");
        });

        Self {
            running,
            handle: Some(handle),
        }
    }

    /// Create a stub detector when deadlock detection is not available
    #[cfg(not(feature = "deadlock_detection"))]
    pub fn new(_check_interval: Duration) -> Self {
        tracing::debug!("Deadlock detection not available - enable 'deadlock_detection' feature");
        Self {
            running: Arc::new(AtomicBool::new(false)),
            handle: None,
        }
    }

    /// Stop the deadlock detector
    pub fn stop(mut self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }

    /// Check if the detector is running
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }
}

impl Drop for DeadlockDetector {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        // Don't join here to avoid blocking in drop
    }
}

/// Initialize global deadlock detection for the application
///
/// This should be called once at startup in debug builds or when
/// explicitly enabled via environment variable.
pub fn init_global_deadlock_detection() {
    #[cfg(feature = "deadlock_detection")]
    {
        use std::sync::Once;
        static INIT: Once = Once::new();

        INIT.call_once(|| {
            // Check if explicitly enabled or in debug mode
            let enabled = std::env::var("TALARIA_DEADLOCK_DETECTION")
                .map(|v| v == "1" || v.to_lowercase() == "true")
                .unwrap_or(cfg!(debug_assertions));

            if enabled {
                let interval = std::env::var("TALARIA_DEADLOCK_INTERVAL_MS")
                    .ok()
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(5000); // Default 5 seconds

                let detector = DeadlockDetector::new(Duration::from_millis(interval));

                // Keep the detector alive for the lifetime of the program
                std::mem::forget(detector);

                tracing::info!(
                    "Global deadlock detection initialized with {}ms interval",
                    interval
                );
            }
        });
    }

    #[cfg(not(feature = "deadlock_detection"))]
    {
        tracing::debug!("Deadlock detection not compiled in - enable 'deadlock_detection' feature");
    }
}

/// Macro to enable deadlock detection in tests
#[macro_export]
macro_rules! enable_test_deadlock_detection {
    () => {
        #[cfg(all(test, feature = "deadlock_detection"))]
        {
            use std::time::Duration;
            use $crate::performance::deadlock_detector::DeadlockDetector;

            // Create a detector for this test
            let _detector = DeadlockDetector::new(Duration::from_millis(100));
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use parking_lot::Mutex;
    use std::sync::Arc;

    #[test]
    #[cfg(feature = "deadlock_detection")]
    fn test_detector_starts_and_stops() {
        let detector = DeadlockDetector::new(Duration::from_millis(100));
        assert!(detector.is_running());

        thread::sleep(Duration::from_millis(200));

        detector.stop();
    }

    #[test]
    #[cfg(not(feature = "deadlock_detection"))]
    fn test_detector_stub_when_disabled() {
        let detector = DeadlockDetector::new(Duration::from_millis(100));
        assert!(!detector.is_running());
    }

    #[test]
    fn test_no_deadlock_with_proper_locking() {
        enable_test_deadlock_detection!();

        let data = Arc::new(Mutex::new(vec![1, 2, 3]));

        let handles: Vec<_> = (0..5)
            .map(|_| {
                let data = data.clone();
                thread::spawn(move || {
                    for _ in 0..100 {
                        let mut guard = data.lock();
                        guard.push(1);
                        // Lock is properly released here
                    }
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }
    }
}
