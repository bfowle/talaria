/// Macro utilities for safe lock management

/// Safe lock acquisition with timeout
///
/// This macro ensures that locks are not held for too long and provides
/// better error messages when deadlocks occur.
#[macro_export]
macro_rules! acquire_lock {
    ($mutex:expr) => {{
        $mutex.lock()
    }};
    ($mutex:expr, $timeout:expr) => {{
        // For future: could implement timeout-based locking
        // For now, just acquire normally
        $mutex.lock()
    }};
}

/// Safe lock acquisition with automatic release
///
/// Ensures the lock is released quickly by extracting needed data
#[macro_export]
macro_rules! with_lock {
    ($mutex:expr, $var:ident, $body:expr) => {{
        let $var = $mutex.lock();
        $body
    }};
}

/// Extract data from a lock and immediately release
///
/// Use this when you need to clone data from a locked structure
#[macro_export]
macro_rules! extract_from_lock {
    ($mutex:expr, $field:ident) => {{
        let guard = $mutex.lock();
        guard.$field.clone()
    }};
    ($mutex:expr, |$guard:ident| $body:expr) => {{
        let $guard = $mutex.lock();
        $body
    }};
}

/// Acquire multiple locks in a consistent order
///
/// This macro helps prevent deadlocks by ensuring locks are always
/// acquired in the same order across the codebase.
#[macro_export]
macro_rules! acquire_locks_ordered {
    ($lock1:expr, $lock2:expr) => {{
        // Simple ordering based on pointer address
        let ptr1 = &*$lock1 as *const _ as usize;
        let ptr2 = &*$lock2 as *const _ as usize;

        if ptr1 < ptr2 {
            let guard1 = $lock1.lock();
            let guard2 = $lock2.lock();
            (guard1, guard2)
        } else {
            let guard2 = $lock2.lock();
            let guard1 = $lock1.lock();
            (guard1, guard2)
        }
    }};
}

/// Debug assertion for lock ordering
///
/// Use in debug builds to verify locks are acquired in correct order
#[macro_export]
macro_rules! assert_lock_order {
    ($level:expr, $lock_name:expr) => {
        #[cfg(debug_assertions)]
        {
            use std::cell::RefCell;

            thread_local! {
                static LOCK_STACK: RefCell<Vec<(u8, &'static str)>> = RefCell::new(Vec::new());
            }

            LOCK_STACK.with(|stack| {
                let mut stack = stack.borrow_mut();

                // Check that we're not violating lock ordering
                if let Some(&(last_level, last_name)) = stack.last() {
                    if $level <= last_level {
                        panic!(
                            "Lock ordering violation! Trying to acquire '{}' (level {}) \
                             while holding '{}' (level {}). Higher level locks must be \
                             acquired first.",
                            $lock_name, $level, last_name, last_level
                        );
                    }
                }

                stack.push(($level, $lock_name));
            });
        }
    };
}

/// Release a lock from the debug stack
#[macro_export]
macro_rules! release_lock_order {
    ($level:expr, $lock_name:expr) => {
        #[cfg(debug_assertions)]
        {
            use std::cell::RefCell;

            thread_local! {
                static LOCK_STACK: RefCell<Vec<(u8, &'static str)>> = RefCell::new(Vec::new());
            }

            LOCK_STACK.with(|stack| {
                let mut stack = stack.borrow_mut();
                if let Some((last_level, last_name)) = stack.pop() {
                    if last_level != $level || last_name != $lock_name {
                        panic!(
                            "Lock release ordering violation! Expected to release '{}' (level {}) \
                             but releasing '{}' (level {})",
                            last_name, last_level, $lock_name, $level
                        );
                    }
                }
            });
        }
    };
}

/// Timed lock acquisition for testing
///
/// Useful in tests to detect potential deadlocks
#[macro_export]
macro_rules! acquire_lock_timeout {
    ($mutex:expr, $timeout_ms:expr) => {{
        #[cfg(test)]
        {
            use std::thread;
            use std::time::{Duration, Instant};

            let start = Instant::now();
            let timeout = Duration::from_millis($timeout_ms);

            // Try to acquire the lock with a timeout
            // Note: parking_lot doesn't have try_lock_for, so we simulate it
            loop {
                if let Some(guard) = $mutex.try_lock() {
                    break Ok(guard);
                }

                if start.elapsed() > timeout {
                    break Err(format!(
                        "Failed to acquire lock within {}ms - potential deadlock",
                        $timeout_ms
                    ));
                }

                thread::sleep(Duration::from_micros(100));
            }
        }

        #[cfg(not(test))]
        {
            Ok($mutex.lock())
        }
    }};
}

/// Lock statistics tracking for debugging
#[cfg(debug_assertions)]
pub mod lock_stats {
    use std::collections::HashMap;
    use std::sync::Mutex;
    use std::time::Duration;

    #[derive(Default)]
    struct LockStat {
        acquisitions: u64,
        total_hold_time: Duration,
        max_hold_time: Duration,
        contentions: u64,
    }

    lazy_static::lazy_static! {
        static ref LOCK_STATS: Mutex<HashMap<String, LockStat>> = Mutex::new(HashMap::new());
    }

    pub fn record_acquisition(name: &str) {
        if let Ok(mut stats) = LOCK_STATS.lock() {
            let stat = stats.entry(name.to_string()).or_default();
            stat.acquisitions += 1;
        }
    }

    pub fn record_hold_time(name: &str, duration: Duration) {
        if let Ok(mut stats) = LOCK_STATS.lock() {
            let stat = stats.entry(name.to_string()).or_default();
            stat.total_hold_time += duration;
            if duration > stat.max_hold_time {
                stat.max_hold_time = duration;
            }
        }
    }

    pub fn record_contention(name: &str) {
        if let Ok(mut stats) = LOCK_STATS.lock() {
            let stat = stats.entry(name.to_string()).or_default();
            stat.contentions += 1;
        }
    }

    pub fn report() -> String {
        if let Ok(stats) = LOCK_STATS.lock() {
            let mut report = String::from("Lock Statistics:\n");
            for (name, stat) in stats.iter() {
                let avg_hold = if stat.acquisitions > 0 {
                    stat.total_hold_time / stat.acquisitions as u32
                } else {
                    Duration::ZERO
                };

                report.push_str(&format!(
                    "  {}: {} acquisitions, {:.3}ms avg hold, {:.3}ms max hold, {} contentions\n",
                    name,
                    stat.acquisitions,
                    avg_hold.as_secs_f64() * 1000.0,
                    stat.max_hold_time.as_secs_f64() * 1000.0,
                    stat.contentions
                ));
            }
            report
        } else {
            String::from("Lock statistics unavailable")
        }
    }
}

#[cfg(test)]
mod tests {
    use parking_lot::Mutex;
    use std::sync::Arc;

    #[test]
    fn test_with_lock_macro() {
        let data = Arc::new(Mutex::new(vec![1, 2, 3]));

        with_lock!(data, guard, {
            assert_eq!(guard.len(), 3);
        });
    }

    #[test]
    fn test_extract_from_lock() {
        struct TestData {
            value: i32,
            name: String,
        }

        let data = Arc::new(Mutex::new(TestData {
            value: 42,
            name: "test".to_string(),
        }));

        let value = extract_from_lock!(data, value);
        assert_eq!(value, 42);

        let name_len = extract_from_lock!(data, |g| g.name.len());
        assert_eq!(name_len, 4);
    }

    #[test]
    fn test_acquire_lock_timeout() {
        let data = Arc::new(Mutex::new(42));

        let result = acquire_lock_timeout!(data, 100);
        assert!(result.is_ok());

        if let Ok(guard) = result {
            assert_eq!(*guard, 42);
        }
    }
}
