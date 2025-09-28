/// Retry logic with exponential backoff for resilient operations
use std::time::Duration;
use std::future::Future;
use anyhow::{Result, Context};
use rand::Rng;
use tracing::{debug, warn, error};

/// Configuration for retry behavior
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    /// Maximum number of retry attempts
    pub max_attempts: u32,
    /// Initial backoff duration
    pub initial_backoff: Duration,
    /// Maximum backoff duration
    pub max_backoff: Duration,
    /// Backoff multiplier (typically 2.0)
    pub multiplier: f32,
    /// Add jitter to prevent thundering herd
    pub jitter: bool,
    /// Which errors trigger retry (None = all errors)
    pub retryable_errors: Option<Vec<String>>,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_backoff: Duration::from_secs(1),
            max_backoff: Duration::from_secs(60),
            multiplier: 2.0,
            jitter: true,
            retryable_errors: None,
        }
    }
}

impl RetryPolicy {
    /// Create a policy for network operations
    pub fn for_network() -> Self {
        Self {
            max_attempts: 5,
            initial_backoff: Duration::from_secs(2),
            max_backoff: Duration::from_secs(120),
            multiplier: 2.0,
            jitter: true,
            retryable_errors: Some(vec![
                "connection".to_string(),
                "timeout".to_string(),
                "refused".to_string(),
                "reset".to_string(),
                "broken pipe".to_string(),
            ]),
        }
    }

    /// Create a policy for file operations
    pub fn for_file_io() -> Self {
        Self {
            max_attempts: 3,
            initial_backoff: Duration::from_millis(100),
            max_backoff: Duration::from_secs(5),
            multiplier: 2.0,
            jitter: true,
            retryable_errors: Some(vec![
                "permission".to_string(),
                "locked".to_string(),
                "busy".to_string(),
            ]),
        }
    }

    /// Create a policy for database operations
    pub fn for_database() -> Self {
        Self {
            max_attempts: 4,
            initial_backoff: Duration::from_millis(500),
            max_backoff: Duration::from_secs(30),
            multiplier: 2.0,
            jitter: true,
            retryable_errors: Some(vec![
                "locked".to_string(),
                "deadlock".to_string(),
                "concurrent".to_string(),
                "conflict".to_string(),
            ]),
        }
    }

    /// Check if an error is retryable based on policy
    fn is_retryable(&self, error: &anyhow::Error) -> bool {
        if let Some(ref retryable) = self.retryable_errors {
            let error_str = format!("{:?}", error).to_lowercase();
            retryable.iter().any(|pattern| error_str.contains(pattern))
        } else {
            true // Retry all errors if no specific list provided
        }
    }

    /// Calculate backoff duration for attempt number
    fn calculate_backoff(&self, attempt: u32) -> Duration {
        let mut backoff = self.initial_backoff.as_millis() as f32;

        // Apply exponential backoff
        for _ in 0..attempt {
            backoff *= self.multiplier;
        }

        // Cap at maximum
        let mut duration = Duration::from_millis(backoff.min(self.max_backoff.as_millis() as f32) as u64);

        // Add jitter if enabled
        if self.jitter {
            let mut rng = rand::thread_rng();
            let jitter_ms = rng.gen_range(0..=(duration.as_millis() / 4) as u32);
            duration = duration + Duration::from_millis(jitter_ms as u64);
        }

        duration
    }
}

/// Trait for operations that can be retried
pub trait RetryableOperation {
    type Output;

    /// Execute the operation
    fn execute(&mut self) -> Result<Self::Output>;

    /// Called before retry attempt
    fn before_retry(&mut self, attempt: u32, error: &anyhow::Error) {
        warn!("Retry attempt {} after error: {}", attempt, error);
    }

    /// Check if operation should continue retrying
    fn should_retry(&self, _error: &anyhow::Error) -> bool {
        true
    }
}

/// Execute an operation with retry logic
pub fn with_retry<F, T>(
    mut operation: F,
    policy: &RetryPolicy,
    context: &str,
) -> Result<T>
where
    F: FnMut() -> Result<T>,
{
    let mut last_error = None;

    for attempt in 0..policy.max_attempts {
        match operation() {
            Ok(result) => {
                if attempt > 0 {
                    debug!("Operation succeeded after {} retries", attempt);
                }
                return Ok(result);
            }
            Err(err) => {
                if !policy.is_retryable(&err) {
                    error!("Non-retryable error in {}: {}", context, err);
                    return Err(err);
                }

                if attempt < policy.max_attempts - 1 {
                    let backoff = policy.calculate_backoff(attempt);
                    warn!(
                        "Attempt {}/{} failed for {}: {}. Retrying in {:?}",
                        attempt + 1,
                        policy.max_attempts,
                        context,
                        err,
                        backoff
                    );
                    std::thread::sleep(backoff);
                } else {
                    error!(
                        "All {} attempts failed for {}: {}",
                        policy.max_attempts,
                        context,
                        err
                    );
                }

                last_error = Some(err);
            }
        }
    }

    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("Retry failed")))
        .context(format!("Failed after {} attempts: {}", policy.max_attempts, context))
}

/// Execute an async operation with retry logic
pub async fn with_retry_async<F, Fut, T>(
    mut operation: F,
    policy: &RetryPolicy,
    context: &str,
) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T>>,
{
    let mut last_error = None;

    for attempt in 0..policy.max_attempts {
        match operation().await {
            Ok(result) => {
                if attempt > 0 {
                    debug!("Async operation succeeded after {} retries", attempt);
                }
                return Ok(result);
            }
            Err(err) => {
                if !policy.is_retryable(&err) {
                    error!("Non-retryable error in {}: {}", context, err);
                    return Err(err);
                }

                if attempt < policy.max_attempts - 1 {
                    let backoff = policy.calculate_backoff(attempt);
                    warn!(
                        "Async attempt {}/{} failed for {}: {}. Retrying in {:?}",
                        attempt + 1,
                        policy.max_attempts,
                        context,
                        err,
                        backoff
                    );
                    tokio::time::sleep(backoff).await;
                } else {
                    error!(
                        "All {} async attempts failed for {}: {}",
                        policy.max_attempts,
                        context,
                        err
                    );
                }

                last_error = Some(err);
            }
        }
    }

    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("Async retry failed")))
        .context(format!("Failed after {} attempts: {}", policy.max_attempts, context))
}

/// Builder for retry policies
pub struct RetryPolicyBuilder {
    policy: RetryPolicy,
}

impl RetryPolicyBuilder {
    pub fn new() -> Self {
        Self {
            policy: RetryPolicy::default(),
        }
    }

    pub fn max_attempts(mut self, attempts: u32) -> Self {
        self.policy.max_attempts = attempts;
        self
    }

    pub fn initial_backoff(mut self, duration: Duration) -> Self {
        self.policy.initial_backoff = duration;
        self
    }

    pub fn max_backoff(mut self, duration: Duration) -> Self {
        self.policy.max_backoff = duration;
        self
    }

    pub fn multiplier(mut self, multiplier: f32) -> Self {
        self.policy.multiplier = multiplier;
        self
    }

    pub fn with_jitter(mut self, jitter: bool) -> Self {
        self.policy.jitter = jitter;
        self
    }

    pub fn retryable_errors(mut self, errors: Vec<String>) -> Self {
        self.policy.retryable_errors = Some(errors);
        self
    }

    pub fn build(self) -> RetryPolicy {
        self.policy
    }
}

/// Macro for easy retry wrapping
#[macro_export]
macro_rules! retry {
    ($operation:expr) => {
        retry!($operation, RetryPolicy::default())
    };

    ($operation:expr, $policy:expr) => {
        with_retry(
            || $operation,
            &$policy,
            &format!("{}:{}", file!(), line!()),
        )
    };

    ($operation:expr, $policy:expr, $context:expr) => {
        with_retry(|| $operation, &$policy, $context)
    };
}

/// Async version of retry macro
#[macro_export]
macro_rules! retry_async {
    ($operation:expr) => {
        retry_async!($operation, RetryPolicy::default())
    };

    ($operation:expr, $policy:expr) => {
        with_retry_async(
            || $operation,
            &$policy,
            &format!("{}:{}", file!(), line!()),
        ).await
    };

    ($operation:expr, $policy:expr, $context:expr) => {
        with_retry_async(|| $operation, &$policy, $context).await
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[test]
    fn test_retry_succeeds_first_attempt() {
        let result = with_retry(
            || Ok(42),
            &RetryPolicy::default(),
            "test operation"
        );

        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    fn test_retry_succeeds_after_failures() {
        let counter = AtomicU32::new(0);

        let result = with_retry(
            || {
                let count = counter.fetch_add(1, Ordering::SeqCst);
                if count < 2 {
                    Err(anyhow::anyhow!("Temporary failure"))
                } else {
                    Ok(42)
                }
            },
            &RetryPolicy::default(),
            "test with failures"
        );

        assert_eq!(result.unwrap(), 42);
        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn test_retry_exhausts_attempts() {
        let counter = AtomicU32::new(0);

        let result = with_retry(
            || {
                counter.fetch_add(1, Ordering::SeqCst);
                Err::<i32, _>(anyhow::anyhow!("Always fails"))
            },
            &RetryPolicy::default(),
            "always failing operation"
        );

        assert!(result.is_err());
        assert_eq!(counter.load(Ordering::SeqCst), 3); // Default max_attempts
    }

    #[test]
    fn test_non_retryable_error() {
        let policy = RetryPolicy {
            max_attempts: 5,
            retryable_errors: Some(vec!["network".to_string()]),
            ..Default::default()
        };

        let counter = AtomicU32::new(0);

        let result = with_retry(
            || {
                counter.fetch_add(1, Ordering::SeqCst);
                Err::<i32, _>(anyhow::anyhow!("File not found"))
            },
            &policy,
            "non-retryable error test"
        );

        assert!(result.is_err());
        assert_eq!(counter.load(Ordering::SeqCst), 1); // Should not retry
    }

    #[test]
    fn test_backoff_calculation() {
        let policy = RetryPolicy {
            initial_backoff: Duration::from_millis(100),
            multiplier: 2.0,
            max_backoff: Duration::from_secs(1),
            jitter: false,
            ..Default::default()
        };

        assert_eq!(policy.calculate_backoff(0), Duration::from_millis(100));
        assert_eq!(policy.calculate_backoff(1), Duration::from_millis(200));
        assert_eq!(policy.calculate_backoff(2), Duration::from_millis(400));
        assert_eq!(policy.calculate_backoff(3), Duration::from_millis(800));
        assert_eq!(policy.calculate_backoff(4), Duration::from_secs(1)); // Capped at max
    }

    #[tokio::test]
    async fn test_async_retry() {
        let counter = AtomicU32::new(0);

        let result = with_retry_async(
            || async {
                let count = counter.fetch_add(1, Ordering::SeqCst);
                if count < 1 {
                    Err(anyhow::anyhow!("Temporary async failure"))
                } else {
                    Ok(42)
                }
            },
            &RetryPolicy {
                initial_backoff: Duration::from_millis(10),
                ..Default::default()
            },
            "async retry test"
        ).await;

        assert_eq!(result.unwrap(), 42);
        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }
}