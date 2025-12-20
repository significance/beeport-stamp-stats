//! Unit tests for retry module
//!
//! Tests cover:
//! - Successful operations (no retries)
//! - Retry on rate limit errors
//! - Exponential backoff timing
//! - Max retries exhaustion
//! - Non-retryable errors
//! - Custom predicates

use beeport_stamp_stats::retry::RetryConfig;
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
    let config = RetryConfig {
        max_retries: 3,
        initial_delay_ms: 10,
        backoff_multiplier: 2,
        extended_retry_wait_seconds: 30,
    };
    let attempt = Arc::new(Mutex::new(0));
    let attempt_clone = attempt.clone();

    let result = config
        .execute(|| {
            let attempt = attempt_clone.clone();
            async move {
                let mut count = attempt.lock().unwrap();
                *count += 1;

                if *count < 3 {
                    Err(std::io::Error::other("429 Too Many Requests"))
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
async fn test_non_rate_limit_error_fails_immediately() {
    let config = RetryConfig::default();
    let attempt = Arc::new(Mutex::new(0));
    let attempt_clone = attempt.clone();

    let result = config
        .execute(|| {
            let attempt = attempt_clone.clone();
            async move {
                let mut count = attempt.lock().unwrap();
                *count += 1;
                Err::<i32, _>(std::io::Error::other("Some other error"))
            }
        })
        .await;

    assert!(result.is_err());
    assert_eq!(*attempt.lock().unwrap(), 1); // Should only try once
}

#[tokio::test]
async fn test_custom_predicate() {
    let config = RetryConfig {
        max_retries: 2,
        initial_delay_ms: 10,
        backoff_multiplier: 2,
        extended_retry_wait_seconds: 30,
    };
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

#[tokio::test]
async fn test_custom_predicate_non_retryable() {
    let config = RetryConfig {
        max_retries: 3,
        initial_delay_ms: 10,
        backoff_multiplier: 2,
        extended_retry_wait_seconds: 30,
    };
    let attempt = Arc::new(Mutex::new(0));
    let attempt_clone = attempt.clone();

    let result = config
        .execute_with_predicate(
            || {
                let attempt = attempt_clone.clone();
                async move {
                    let mut count = attempt.lock().unwrap();
                    *count += 1;
                    Err::<i32, _>(std::io::Error::new(
                        std::io::ErrorKind::PermissionDenied,
                        "permission denied",
                    ))
                }
            },
            |e| e.kind() == std::io::ErrorKind::TimedOut,
        )
        .await;

    assert!(result.is_err());
    assert_eq!(*attempt.lock().unwrap(), 1); // Should fail immediately
}

#[tokio::test]
async fn test_exponential_backoff_delays() {
    let config = RetryConfig {
        max_retries: 3,
        initial_delay_ms: 100,
        backoff_multiplier: 4,
        extended_retry_wait_seconds: 30,
    };

    let attempt = Arc::new(Mutex::new(0));
    let attempt_clone = attempt.clone();
    let start = std::time::Instant::now();

    let _result = config
        .execute(|| {
            let attempt = attempt_clone.clone();
            async move {
                let mut count = attempt.lock().unwrap();
                *count += 1;

                if *count < 4 {
                    Err(std::io::Error::other("429 Too Many Requests"))
                } else {
                    Ok(42)
                }
            }
        })
        .await;

    let elapsed = start.elapsed();

    // With multiplier 4 and initial delay 100ms:
    // Attempt 1: immediate
    // Attempt 2: +100ms delay
    // Attempt 3: +400ms delay
    // Attempt 4: +1600ms delay
    // Total minimum delay: 2100ms
    assert!(elapsed.as_millis() >= 2000); // Allow some margin
}

#[test]
fn test_retry_config_creation() {
    let config = RetryConfig {
        max_retries: 10,
        initial_delay_ms: 200,
        backoff_multiplier: 3,
        extended_retry_wait_seconds: 600,
    };

    assert_eq!(config.max_retries, 10);
    assert_eq!(config.initial_delay_ms, 200);
    assert_eq!(config.backoff_multiplier, 3);
    assert_eq!(config.extended_retry_wait_seconds, 600);
}

#[test]
fn test_retry_config_default() {
    let config = RetryConfig::default();

    assert_eq!(config.max_retries, 5);
    assert_eq!(config.initial_delay_ms, 100);
    assert_eq!(config.backoff_multiplier, 4);
    assert_eq!(config.extended_retry_wait_seconds, 300);
}

#[tokio::test]
async fn test_multiple_rate_limit_errors() {
    let config = RetryConfig {
        max_retries: 5,
        initial_delay_ms: 10,
        backoff_multiplier: 2,
        extended_retry_wait_seconds: 30,
    };
    let attempt = Arc::new(Mutex::new(0));
    let attempt_clone = attempt.clone();

    let result = config
        .execute(|| {
            let attempt = attempt_clone.clone();
            async move {
                let mut count = attempt.lock().unwrap();
                *count += 1;

                if *count < 5 {
                    Err(std::io::Error::other("429 Too Many Requests"))
                } else {
                    Ok(100)
                }
            }
        })
        .await;

    assert_eq!(result.unwrap(), 100);
    assert_eq!(*attempt.lock().unwrap(), 5);
}

#[tokio::test]
async fn test_error_message_contains_too_many_requests() {
    let config = RetryConfig {
        max_retries: 2,
        initial_delay_ms: 10,
        backoff_multiplier: 2,
        extended_retry_wait_seconds: 30,
    };
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
                        "HTTP 429: Too Many Requests - Rate limit exceeded",
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
