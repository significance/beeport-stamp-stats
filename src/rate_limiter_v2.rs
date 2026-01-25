use crate::cache::Cache;
use crate::error::{Result, StampError};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tokio::task::JoinHandle;

/// Recovery state for the rate limiter
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryState {
    /// Operating normally at healthy rate
    Normal,
    /// Recently hit rate limit, backing off
    BackingOff,
    /// In recovery phase after cool-down period
    Recovering,
    /// Gradually increasing rate after recovery
    GradualRecovery,
}

/// Configuration for automatic recovery after rate limits
#[derive(Debug, Clone)]
pub struct RecoveryConfig {
    /// How long to wait after last 429 before attempting recovery
    pub cool_down_period: Duration,
    /// Target rate to reset to after cool-down (e.g., 5.0 req/s)
    pub min_healthy_rate: f64,
    /// Factor to multiply rate by during gradual recovery (e.g., 1.05 = 5% increase)
    pub gradual_recovery_factor: f64,
    /// How often to check for recovery opportunities
    pub recovery_check_interval: Duration,
}

impl Default for RecoveryConfig {
    fn default() -> Self {
        Self {
            cool_down_period: Duration::from_secs(60),
            min_healthy_rate: 5.0,
            gradual_recovery_factor: 1.05,
            recovery_check_interval: Duration::from_secs(30),
        }
    }
}

/// Token bucket rate limiter with automatic recovery
///
/// This rate limiter uses a token bucket algorithm that:
/// - Properly handles fractional rates (0.1 req/s = 1 token every 10 seconds)
/// - Enforces a minimum rate floor to prevent deadlocks
/// - Automatically recovers after rate limit periods expire
/// - Persists discovered limits to database
pub struct TokenBucket {
    /// Current number of tokens available
    tokens: f64,
    /// Maximum tokens that can accumulate (burst capacity)
    capacity: f64,
    /// Rate at which tokens are added (tokens per second)
    refill_rate: f64,
    /// Last time tokens were refilled
    last_refill: Instant,

    /// Minimum refill rate (floor) - PREVENTS DEADLOCK
    min_rate: f64,
    /// Maximum refill rate (ceiling)
    max_rate: f64,

    /// Factor to multiply rate by when ramping up (e.g., 1.1 = 10% increase)
    ramp_up_factor: f64,
    /// Number of consecutive successes needed before ramping up
    ramp_up_threshold: u64,
    /// Factor to multiply rate by when backing off (e.g., 0.5 = 50% decrease)
    back_off_factor: f64,

    /// Recovery configuration
    recovery_config: RecoveryConfig,
    /// Current recovery state
    state: RecoveryState,
    /// Last time a rate limit (429) was encountered
    last_rate_limit: Option<Instant>,
    /// Number of consecutive successful requests
    consecutive_successes: u64,

    /// RPC URL for logging
    rpc_url: String,
    /// Optional cache for persistence
    cache: Option<Arc<Cache>>,

    /// Statistics
    total_requests: u64,
    rate_limit_errors: u64,
    successful_requests: u64,
}

impl TokenBucket {
    /// Create a new token bucket rate limiter
    ///
    /// # Arguments
    /// * `initial_rate` - Starting refill rate (tokens per second)
    /// * `capacity` - Maximum tokens that can accumulate
    /// * `min_rate` - Minimum rate floor (CRITICAL: prevents deadlock)
    /// * `max_rate` - Maximum rate ceiling
    /// * `ramp_up_factor` - Multiplier for rate increases (e.g., 1.1)
    /// * `ramp_up_threshold` - Successes needed before increase
    /// * `back_off_factor` - Multiplier for rate decreases (e.g., 0.5)
    /// * `recovery_config` - Configuration for automatic recovery
    /// * `rpc_url` - URL for logging
    /// * `cache` - Optional database cache for persistence
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        initial_rate: f64,
        capacity: f64,
        min_rate: f64,
        max_rate: f64,
        ramp_up_factor: f64,
        ramp_up_threshold: u64,
        back_off_factor: f64,
        recovery_config: RecoveryConfig,
        rpc_url: String,
        cache: Option<Arc<Cache>>,
    ) -> Self {
        // SAFETY CHECK: Ensure min_rate is positive to prevent deadlock
        let min_rate = min_rate.max(0.1);

        // SAFETY CHECK: Ensure initial_rate is at least min_rate
        let initial_rate = initial_rate.max(min_rate).min(max_rate);

        // Determine initial state based on rate
        let state = if initial_rate >= recovery_config.min_healthy_rate {
            RecoveryState::Normal
        } else {
            RecoveryState::BackingOff
        };

        Self {
            tokens: capacity, // Start with full bucket
            capacity,
            refill_rate: initial_rate,
            last_refill: Instant::now(),
            min_rate,
            max_rate,
            ramp_up_factor,
            ramp_up_threshold,
            back_off_factor,
            recovery_config,
            state,
            last_rate_limit: None,
            consecutive_successes: 0,
            rpc_url,
            cache,
            total_requests: 0,
            rate_limit_errors: 0,
            successful_requests: 0,
        }
    }

    /// Refill tokens based on elapsed time
    ///
    /// SAFETY: This method always makes progress - tokens increase over time
    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();

        // Calculate new tokens to add
        let new_tokens = elapsed * self.refill_rate;

        // Add tokens up to capacity
        self.tokens = (self.tokens + new_tokens).min(self.capacity);

        // Update refill time
        self.last_refill = now;
    }

    /// Acquire a token (permit to make a request)
    ///
    /// SAFETY GUARANTEES:
    /// 1. Always returns eventually (no infinite loops)
    /// 2. Minimum rate floor ensures tokens accumulate
    /// 3. Wait time is bounded and calculable
    /// 4. Refills before checking, ensuring progress
    pub async fn acquire(&mut self) -> Result<()> {
        // SAFETY: Track attempts to detect potential issues
        let max_attempts = 1000;
        let mut attempts = 0;

        loop {
            attempts += 1;

            // SAFETY CHECK: Detect infinite loop (should never happen)
            if attempts > max_attempts {
                tracing::error!(
                    "[{}] SAFETY: acquire() exceeded {} attempts (rate={}, tokens={})",
                    self.rpc_url,
                    max_attempts,
                    self.refill_rate,
                    self.tokens
                );
                return Err(StampError::Rpc(
                    "Rate limiter deadlock detected - safety check failed".to_string()
                ));
            }

            // Refill tokens based on elapsed time
            self.refill();

            // Check if we have enough tokens
            if self.tokens >= 1.0 {
                self.tokens -= 1.0;
                self.total_requests += 1;
                return Ok(());
            }

            // SAFETY: Calculate exact wait time until next token
            // This ensures we always make progress
            let wait_secs = 1.0 / self.refill_rate;

            // SAFETY CHECK: Ensure wait time is reasonable
            if wait_secs > 60.0 {
                tracing::warn!(
                    "[{}] SAFETY: Very slow rate ({} req/s) means waiting {:.1}s per token",
                    self.rpc_url,
                    self.refill_rate,
                    wait_secs
                );
            }

            // Wait for next token (bounded by refill_rate, never infinite)
            let wait_duration = Duration::from_secs_f64(wait_secs);
            tokio::time::sleep(wait_duration).await;
        }
    }

    /// Record a rate limit error (429)
    ///
    /// SAFETY: Always enforces minimum floor to prevent zero rate
    pub async fn record_rate_limit(&mut self) {
        self.last_rate_limit = Some(Instant::now());
        self.rate_limit_errors += 1;
        self.consecutive_successes = 0;

        // Transition to backing off state
        self.state = RecoveryState::BackingOff;

        let old_limit = self.refill_rate;

        // SAFETY: Apply back-off with floor enforcement
        let new_rate = (self.refill_rate * self.back_off_factor).max(self.min_rate);

        tracing::warn!(
            "[{}] Rate limit hit! Backing off: {:.1} → {:.1} req/s (state: {:?})",
            self.rpc_url,
            old_limit,
            new_rate,
            self.state
        );

        self.refill_rate = new_rate;

        // Persist to database
        if let Some(ref cache) = self.cache
            && let Err(e) = cache.upsert_rpc_rate_limit(
                &self.rpc_url,
                new_rate,
                "token_bucket",
                self.total_requests,
                self.rate_limit_errors,
                new_rate, // Use current rate as measured_rps
                self.consecutive_successes as u32,
            ).await {
                tracing::warn!("[{}] Failed to persist rate limit: {e}", self.rpc_url);
        }
    }

    /// Record a successful request
    ///
    /// SAFETY: Rate increases are bounded by max_rate
    pub async fn record_success(&mut self, _duration: Duration) {
        self.successful_requests += 1;
        self.consecutive_successes += 1;

        // Check if we should ramp up
        if self.consecutive_successes >= self.ramp_up_threshold {
            let old_limit = self.refill_rate;

            // SAFETY: Apply ramp-up with ceiling enforcement
            let new_rate = (self.refill_rate * self.ramp_up_factor).min(self.max_rate);

            if new_rate > old_limit {
                tracing::info!(
                    "[{}] Rate limit increased: {:.1} → {:.1} req/s (state: {:?})",
                    self.rpc_url,
                    old_limit,
                    new_rate,
                    self.state
                );

                self.refill_rate = new_rate;

                // Update state if we've reached healthy rate
                if new_rate >= self.recovery_config.min_healthy_rate {
                    self.state = RecoveryState::Normal;
                }

                // Persist to database
                if let Some(ref cache) = self.cache
                    && let Err(e) = cache.upsert_rpc_rate_limit(
                        &self.rpc_url,
                        new_rate,
                        "token_bucket",
                        self.total_requests,
                        self.rate_limit_errors,
                        new_rate, // Use current rate as measured_rps
                        self.consecutive_successes as u32,
                    ).await {
                        tracing::warn!("[{}] Failed to persist rate limit: {e}", self.rpc_url);
                }
            }

            self.consecutive_successes = 0;
        }
    }

    /// Record an error (non-rate-limit)
    pub async fn record_error(&mut self) {
        self.consecutive_successes = 0;
    }

    /// Check for recovery opportunities
    ///
    /// SAFETY: This method is called periodically by the background task
    /// It ensures the rate limiter can recover even if no requests are being made
    pub async fn check_recovery(&mut self) {
        let time_since_last_429 = self.last_rate_limit
            .map(|t| Instant::now().duration_since(t))
            .unwrap_or(Duration::MAX);

        // SAFETY: Only attempt recovery if cool-down period has passed
        if time_since_last_429 < self.recovery_config.cool_down_period {
            return; // Still in cool-down
        }

        let old_rate = self.refill_rate;
        let mut rate_changed = false;

        match self.state {
            RecoveryState::BackingOff if self.refill_rate < self.recovery_config.min_healthy_rate => {
                // Transition to recovering - reset to healthy rate
                self.state = RecoveryState::Recovering;
                self.refill_rate = self.recovery_config.min_healthy_rate;
                rate_changed = true;

                tracing::info!(
                    "[{}] Recovery: Reset rate {:.1} → {:.1} req/s after {}s cool-down",
                    self.rpc_url,
                    old_rate,
                    self.refill_rate,
                    time_since_last_429.as_secs()
                );
            }
            RecoveryState::Recovering | RecoveryState::Normal => {
                // Gradual increase
                let new_rate = (self.refill_rate * self.recovery_config.gradual_recovery_factor).min(self.max_rate);

                if new_rate > self.refill_rate {
                    self.refill_rate = new_rate;
                    self.state = RecoveryState::GradualRecovery;
                    rate_changed = true;

                    tracing::info!(
                        "[{}] Gradual recovery: {:.1} → {:.1} req/s (+{:.1}%)",
                        self.rpc_url,
                        old_rate,
                        self.refill_rate,
                        (self.recovery_config.gradual_recovery_factor - 1.0) * 100.0
                    );
                }
            }
            _ => {}
        }

        // Persist if rate changed
        if rate_changed
            && let Some(ref cache) = self.cache
            && let Err(e) = cache.upsert_rpc_rate_limit(
                    &self.rpc_url,
                    self.refill_rate,
                    "token_bucket",
                    self.total_requests,
                    self.rate_limit_errors,
                    self.refill_rate, // Use current rate as measured_rps
                    self.consecutive_successes as u32,
                ).await {
                    tracing::warn!("[{}] Failed to persist rate limit: {e}", self.rpc_url);
        }
    }

    /// Get current statistics
    pub fn stats(&self) -> RateLimitStats {
        RateLimitStats {
            rpc_url: self.rpc_url.clone(),
            current_limit: self.refill_rate,
            min_rate: self.min_rate,
            max_rate: self.max_rate,
            total_requests: self.total_requests,
            successful_requests: self.successful_requests,
            rate_limit_errors: self.rate_limit_errors,
            success_rate: if self.total_requests > 0 {
                self.successful_requests as f64 / self.total_requests as f64
            } else {
                0.0
            },
            state: self.state,
        }
    }
}

/// Statistics for a rate-limited endpoint
#[derive(Debug, Clone)]
pub struct RateLimitStats {
    pub rpc_url: String,
    pub current_limit: f64,
    pub min_rate: f64,
    pub max_rate: f64,
    pub total_requests: u64,
    #[allow(dead_code)]
    pub successful_requests: u64,
    pub rate_limit_errors: u64,
    pub success_rate: f64,
    pub state: RecoveryState,
}

/// Spawn a background task that periodically checks for recovery opportunities
///
/// SAFETY: This task ensures the rate limiter can always recover, even if no
/// requests are being made. The task cannot panic or stop due to:
/// 1. Bounded interval timing
/// 2. Error handling in check_recovery()
/// 3. No panicking operations
pub fn spawn_recovery_task(
    rate_limiter: Arc<RwLock<TokenBucket>>,
    url: String,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        // SAFETY: Read recovery config once at start
        let check_interval = {
            let limiter = rate_limiter.read().await;
            limiter.recovery_config.recovery_check_interval
        };

        let mut interval = tokio::time::interval(check_interval);

        loop {
            // SAFETY: This will always tick after the interval
            interval.tick().await;

            // SAFETY: Acquire write lock and check recovery
            // Even if this fails, the loop continues
            match rate_limiter.try_write() {
                Ok(mut limiter) => {
                    limiter.check_recovery().await;
                }
                Err(_) => {
                    // Lock contention - skip this check, will retry next interval
                    tracing::debug!(
                        "[{}] Recovery check skipped (lock contention)",
                        url
                    );
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_minimum_floor_prevents_deadlock() {
        let config = RecoveryConfig::default();
        let mut bucket = TokenBucket::new(
            10.0, // initial_rate
            5.0,  // capacity
            0.5,  // min_rate (FLOOR)
            50.0, // max_rate
            1.1,  // ramp_up_factor
            50,   // ramp_up_threshold
            0.5,  // back_off_factor
            config,
            "test".to_string(),
            None,
        );

        // Hit rate limit multiple times
        for _ in 0..10 {
            bucket.record_rate_limit().await;
        }

        // Rate should never go below min_rate
        assert!(bucket.refill_rate >= 0.5, "Rate dropped below minimum: {}", bucket.refill_rate);

        // Should still be able to acquire (won't deadlock)
        let result = tokio::time::timeout(
            Duration::from_secs(5),
            bucket.acquire()
        ).await;

        assert!(result.is_ok(), "acquire() timed out - deadlock detected");
    }

    #[tokio::test]
    async fn test_fractional_rates() {
        let config = RecoveryConfig::default();
        let mut bucket = TokenBucket::new(
            0.5, // 0.5 req/s = 1 request every 2 seconds
            2.0,
            0.1,
            50.0,
            1.1,
            50,
            0.5,
            config,
            "test".to_string(),
            None,
        );

        // First request should succeed immediately
        bucket.acquire().await.unwrap();

        // Second request should wait ~2 seconds
        let start = Instant::now();
        bucket.acquire().await.unwrap();
        let elapsed = start.elapsed();

        assert!(elapsed.as_secs_f64() >= 1.5, "Fractional rate not working correctly");
    }

    #[tokio::test]
    async fn test_recovery_after_cool_down() {
        let config = RecoveryConfig {
            cool_down_period: Duration::from_millis(100), // Short for testing
            min_healthy_rate: 5.0,
            gradual_recovery_factor: 1.05,
            recovery_check_interval: Duration::from_secs(30),
        };

        let mut bucket = TokenBucket::new(
            10.0,
            5.0,
            0.5,
            50.0,
            1.1,
            50,
            0.5,
            config,
            "test".to_string(),
            None,
        );

        // Hit rate limit - should drop to 5.0 then to 2.5, then to 1.25, then to 0.625 (floor)
        bucket.record_rate_limit().await;
        assert_eq!(bucket.refill_rate, 5.0);
        bucket.record_rate_limit().await;
        assert_eq!(bucket.refill_rate, 2.5);
        bucket.record_rate_limit().await;
        assert_eq!(bucket.refill_rate, 1.25);
        bucket.record_rate_limit().await;
        assert_eq!(bucket.refill_rate, 0.625);

        assert_eq!(bucket.state, RecoveryState::BackingOff);

        // Wait for cool-down
        tokio::time::sleep(Duration::from_millis(150)).await;

        // Check recovery - should reset to min_healthy_rate
        bucket.check_recovery().await;

        assert_eq!(bucket.refill_rate, 5.0, "Recovery did not reset to min_healthy_rate");
        assert_eq!(bucket.state, RecoveryState::Recovering);
    }
}
