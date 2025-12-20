/// Retry policy for operations prone to rate limiting
///
/// This module provides a generic retry mechanism with configurable exponential backoff
/// and extended retry phases. It's designed to be reusable across different RPC providers
/// and operation types.
use serde::{Deserialize, Serialize};
use std::future::Future;
use tokio::time::{sleep, Duration};

/// Configuration for retry behavior
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    /// Maximum number of retries in the fast exponential backoff phase
    pub max_retries: u32,

    /// Initial delay in milliseconds before the first retry
    pub initial_delay_ms: u64,

    /// Multiplier for exponential backoff (default: 4)
    /// Each retry delay = initial_delay_ms * backoff_multiplier^retry_count
    pub backoff_multiplier: u64,

    /// Wait time in seconds before entering extended retry mode
    /// After max_retries is exhausted, wait this long before resetting the counter
    pub extended_retry_wait_seconds: u64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 5,
            initial_delay_ms: 100,
            backoff_multiplier: 4,
            extended_retry_wait_seconds: 300, // 5 minutes
        }
    }
}

impl RetryConfig {
    /// Create a new retry configuration
    #[allow(dead_code)]
    pub fn new(
        max_retries: u32,
        initial_delay_ms: u64,
        backoff_multiplier: u64,
        extended_retry_wait_seconds: u64,
    ) -> Self {
        Self {
            max_retries,
            initial_delay_ms,
            backoff_multiplier,
            extended_retry_wait_seconds,
        }
    }

    /// Execute an operation with retry logic
    ///
    /// This method implements a two-phase retry strategy:
    ///
    /// **Phase 1: Fast exponential backoff**
    /// - Retries up to `max_retries` times
    /// - Delay increases exponentially: initial_delay * multiplier^retry_count
    /// - Example with 4x multiplier: 100ms → 400ms → 1600ms → 6400ms → 25600ms
    ///
    /// **Phase 2: Extended retry**
    /// - When Phase 1 is exhausted, wait `extended_retry_wait_seconds`
    /// - Resets retry counter and returns to Phase 1
    /// - Continues indefinitely until success
    ///
    /// # Arguments
    ///
    /// * `operation` - A closure that returns a Future with a Result
    ///
    /// # Returns
    ///
    /// Returns the successful result or propagates non-retryable errors immediately
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let config = RetryConfig::default();
    /// let result = config.execute(|| async {
    ///     make_rpc_call().await
    /// }).await?;
    /// ```
    pub async fn execute<F, Fut, T, E>(&self, mut operation: F) -> Result<T, String>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = std::result::Result<T, E>>,
        E: std::error::Error,
    {
        let mut extended_retry_count = 0;

        loop {
            let mut retries = 0;

            // Phase 1: Fast retry with exponential backoff
            loop {
                match operation().await {
                    Ok(result) => return Ok(result),
                    Err(e) => {
                        let error_msg = e.to_string();

                        // Check if this is a rate limit error
                        if error_msg.contains("429") || error_msg.contains("Too Many Requests") {
                            if retries < self.max_retries {
                                // Calculate exponential backoff delay
                                let delay_ms = self
                                    .initial_delay_ms
                                    .saturating_mul(self.backoff_multiplier.pow(retries));

                                let now = chrono::Local::now().format("%H:%M:%S");
                                tracing::debug!(
                                    "[{}] Rate limited (429), retrying after {}ms (attempt {}/{})",
                                    now,
                                    delay_ms,
                                    retries + 1,
                                    self.max_retries
                                );

                                sleep(Duration::from_millis(delay_ms)).await;
                                retries += 1;
                                continue;
                            } else {
                                // Phase 2: Extended retry
                                extended_retry_count += 1;
                                let now = chrono::Local::now().format("%H:%M:%S");
                                tracing::warn!(
                                    "[{}] Max retries ({}) exhausted. Waiting {} seconds before retry #{} (extended mode)",
                                    now,
                                    self.max_retries,
                                    self.extended_retry_wait_seconds,
                                    extended_retry_count
                                );

                                sleep(Duration::from_secs(self.extended_retry_wait_seconds)).await;

                                // Break inner loop to reset retry counter
                                break;
                            }
                        } else {
                            // Non-rate-limit error - fail immediately
                            return Err(format!("Operation failed: {e}"));
                        }
                    }
                }
            }
        }
    }

    /// Execute an operation with retry logic, using a custom error predicate
    ///
    /// This is similar to `execute` but allows custom logic to determine if an error
    /// should be retried.
    ///
    /// # Arguments
    ///
    /// * `operation` - A closure that returns a Future with a Result
    /// * `is_retryable` - A function that determines if an error should be retried
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let config = RetryConfig::default();
    /// let result = config.execute_with_predicate(
    ///     || async { make_rpc_call().await },
    ///     |e| e.to_string().contains("timeout")
    /// ).await?;
    /// ```
    #[allow(dead_code)]
    pub async fn execute_with_predicate<F, Fut, T, E, P>(
        &self,
        mut operation: F,
        is_retryable: P,
    ) -> Result<T, String>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = std::result::Result<T, E>>,
        E: std::error::Error,
        P: Fn(&E) -> bool,
    {
        let mut extended_retry_count = 0;

        loop {
            let mut retries = 0;

            // Phase 1: Fast retry with exponential backoff
            loop {
                match operation().await {
                    Ok(result) => return Ok(result),
                    Err(e) => {
                        if is_retryable(&e) {
                            if retries < self.max_retries {
                                // Calculate exponential backoff delay
                                let delay_ms = self
                                    .initial_delay_ms
                                    .saturating_mul(self.backoff_multiplier.pow(retries));

                                let now = chrono::Local::now().format("%H:%M:%S");
                                tracing::debug!(
                                    "[{}] Retryable error, retrying after {}ms (attempt {}/{}): {}",
                                    now,
                                    delay_ms,
                                    retries + 1,
                                    self.max_retries,
                                    e
                                );

                                sleep(Duration::from_millis(delay_ms)).await;
                                retries += 1;
                                continue;
                            } else {
                                // Phase 2: Extended retry
                                extended_retry_count += 1;
                                let now = chrono::Local::now().format("%H:%M:%S");
                                tracing::warn!(
                                    "[{}] Max retries ({}) exhausted. Waiting {} seconds before retry #{} (extended mode)",
                                    now,
                                    self.max_retries,
                                    self.extended_retry_wait_seconds,
                                    extended_retry_count
                                );

                                sleep(Duration::from_secs(self.extended_retry_wait_seconds)).await;

                                // Break inner loop to reset retry counter
                                break;
                            }
                        } else {
                            // Non-retryable error - fail immediately
                            return Err(format!("Operation failed: {e}"));
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[tokio::test]
    async fn test_retry_success_first_attempt() {
        let config = RetryConfig::default();
        let result = config
            .execute(|| async { Ok::<_, std::io::Error>(42) })
            .await;

        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn test_retry_on_rate_limit() {
        let config = RetryConfig::new(3, 10, 2, 30);
        let attempt = Arc::new(Mutex::new(0));
        let attempt_clone = attempt.clone();

        let result = config
            .execute(|| {
                let attempt = attempt_clone.clone();
                async move {
                    let mut count = attempt.lock().unwrap();
                    *count += 1;

                    if *count < 3 {
                        Err(std::io::Error::other(
                            "429 Too Many Requests",
                        ))
                    } else {
                        Ok(42)
                    }
                }
            })
            .await;

        assert_eq!(result.unwrap(), 42);
        assert_eq!(*attempt.lock().unwrap(), 3);
    }

    #[tokio::test]
    async fn test_non_retryable_error_fails_immediately() {
        let config = RetryConfig::default();
        let attempt = Arc::new(Mutex::new(0));
        let attempt_clone = attempt.clone();

        let result = config
            .execute(|| {
                let attempt = attempt_clone.clone();
                async move {
                    let mut count = attempt.lock().unwrap();
                    *count += 1;
                    Err::<i32, _>(std::io::Error::other(
                        "Some other error",
                    ))
                }
            })
            .await;

        assert!(result.is_err());
        assert_eq!(*attempt.lock().unwrap(), 1); // Should only try once
    }

    #[tokio::test]
    async fn test_custom_predicate() {
        let config = RetryConfig::new(2, 10, 2, 30);
        let attempt = Arc::new(Mutex::new(0));
        let attempt_clone = attempt.clone();

        let result = config
            .execute_with_predicate(
                || {
                    let attempt = attempt_clone.clone();
                    async move {
                        let mut count = attempt.lock().unwrap();
                        *count += 1;

                        if *count < 2 {
                            Err(std::io::Error::new(
                                std::io::ErrorKind::TimedOut,
                                "timeout",
                            ))
                        } else {
                            Ok(42)
                        }
                    }
                },
                |e| e.kind() == std::io::ErrorKind::TimedOut,
            )
            .await;

        assert_eq!(result.unwrap(), 42);
        assert_eq!(*attempt.lock().unwrap(), 2);
    }
}
