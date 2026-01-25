/// Adaptive rate limiting for RPC endpoints
///
/// This module implements a sliding window rate limiter with adaptive discovery
/// of each endpoint's rate limits. It tracks request patterns and automatically
/// adjusts the rate limit based on successes and failures.
///
/// DEPRECATED: This module is replaced by rate_limiter_v2 with token bucket algorithm
use crate::cache::Cache;
use crate::config::{AdaptiveStrategyConfig, AggressiveStrategyConfig, RateLimitMode, RateLimitStrategyName};
use crate::error::Result;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// Adaptive rate limiter for a single RPC endpoint
#[allow(dead_code)]
#[derive(Clone)]
pub struct AdaptiveRateLimiter {
    rpc_url: String,
    config: Arc<RwLock<RateLimitConfig>>,
    state: Arc<RwLock<RateLimitState>>,
    cache: Arc<Cache>,
}

/// Rate limit configuration
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct RateLimitConfig {
    /// Manual rate limit (if specified in config)
    manual_limit: Option<f64>,

    /// Current effective rate limit (requests per second)
    current_limit: f64,

    /// Strategy for rate limit discovery
    strategy: RateLimitStrategy,
}

/// Rate limit state tracking
#[derive(Debug)]
#[allow(dead_code)]
struct RateLimitState {
    /// Sliding window of recent requests
    recent_requests: VecDeque<Instant>,

    /// Last rate limit error time
    last_rate_limit: Option<Instant>,

    /// Consecutive successes since last rate limit
    consecutive_successes: u32,

    /// Total requests sent
    total_requests: u64,

    /// Rate limit errors encountered
    rate_limit_errors: u64,

    /// Current requests per second (measured)
    measured_rps: f64,
}

/// Rate limiting strategy
#[allow(dead_code)]
#[derive(Debug, Clone)]
enum RateLimitStrategy {
    /// Manual: User-specified, never changes
    Manual,

    /// Adaptive: Start conservative, ramp up until we hit limit
    Adaptive {
        start_rps: f64,
        max_rps: f64,
        ramp_up_factor: f64,
        back_off_factor: f64,
        ramp_up_threshold: u32,
    },

    /// Aggressive: Start high, back off when hit
    Aggressive {
        start_rps: f64,
        min_rps: f64,
        back_off_factor: f64,
    },
}

#[allow(dead_code)]
impl AdaptiveRateLimiter {
    /// Create a new rate limiter for an endpoint
    pub async fn new(
        rpc_url: String,
        rate_limit_mode: Option<RateLimitMode>,
        adaptive_config: &AdaptiveStrategyConfig,
        aggressive_config: &AggressiveStrategyConfig,
        cache: Arc<Cache>,
    ) -> Result<Self> {
        // Determine strategy and initial limit
        let (strategy, initial_limit) = match rate_limit_mode {
            Some(RateLimitMode::Manual(limit)) => (RateLimitStrategy::Manual, limit),
            Some(RateLimitMode::Strategy(RateLimitStrategyName::Adaptive)) => (
                RateLimitStrategy::Adaptive {
                    start_rps: adaptive_config.start_rps,
                    max_rps: adaptive_config.max_rps,
                    ramp_up_factor: adaptive_config.ramp_up_factor,
                    back_off_factor: adaptive_config.back_off_factor,
                    ramp_up_threshold: adaptive_config.ramp_up_threshold,
                },
                adaptive_config.start_rps,
            ),
            Some(RateLimitMode::Strategy(RateLimitStrategyName::Aggressive)) => (
                RateLimitStrategy::Aggressive {
                    start_rps: aggressive_config.start_rps,
                    min_rps: aggressive_config.min_rps,
                    back_off_factor: aggressive_config.back_off_factor,
                },
                aggressive_config.start_rps,
            ),
            None => (
                // Default to adaptive
                RateLimitStrategy::Adaptive {
                    start_rps: adaptive_config.start_rps,
                    max_rps: adaptive_config.max_rps,
                    ramp_up_factor: adaptive_config.ramp_up_factor,
                    back_off_factor: adaptive_config.back_off_factor,
                    ramp_up_threshold: adaptive_config.ramp_up_threshold,
                },
                adaptive_config.start_rps,
            ),
        };

        // Try to load previously discovered limit from cache (for non-manual strategies)
        let current_limit = if matches!(strategy, RateLimitStrategy::Manual) {
            initial_limit
        } else {
            match cache.get_rpc_rate_limit(&rpc_url).await {
                Ok(Some(cached_limit)) => {
                    tracing::info!(
                        "[{}] Loaded cached rate limit: {:.1} req/s",
                        rpc_url,
                        cached_limit
                    );
                    cached_limit
                }
                _ => {
                    tracing::info!(
                        "[{}] Starting with default rate limit: {:.1} req/s",
                        rpc_url,
                        initial_limit
                    );
                    initial_limit
                }
            }
        };

        let config = Arc::new(RwLock::new(RateLimitConfig {
            manual_limit: if matches!(strategy, RateLimitStrategy::Manual) {
                Some(initial_limit)
            } else {
                None
            },
            current_limit,
            strategy,
        }));

        let state = Arc::new(RwLock::new(RateLimitState {
            recent_requests: VecDeque::new(),
            last_rate_limit: None,
            consecutive_successes: 0,
            total_requests: 0,
            rate_limit_errors: 0,
            measured_rps: 0.0,
        }));

        Ok(Self {
            rpc_url,
            config,
            state,
            cache,
        })
    }

    /// Acquire a rate limit permit (blocks if at capacity)
    pub async fn acquire(&self) -> Result<()> {
        loop {
            let mut state = self.state.write().await;
            let config = self.config.read().await;
            let now = Instant::now();

            // Clean up old entries (older than 1 second)
            while let Some(&front) = state.recent_requests.front() {
                if now.duration_since(front) > Duration::from_secs(1) {
                    state.recent_requests.pop_front();
                } else {
                    break;
                }
            }

            // Check if we're at capacity
            let capacity = config.current_limit.ceil() as usize;
            if state.recent_requests.len() < capacity {
                // Acquire permit
                state.recent_requests.push_back(now);
                state.total_requests += 1;
                drop(state);
                drop(config);
                return Ok(());
            }

            // Need to wait - find when oldest request ages out
            if let Some(&oldest) = state.recent_requests.front() {
                let age = now.duration_since(oldest);
                if age < Duration::from_secs(1) {
                    let wait = Duration::from_secs(1) - age + Duration::from_millis(10);
                    drop(state);
                    drop(config);
                    tokio::time::sleep(wait).await;
                    continue;
                }
            }

            // Shouldn't reach here, but safety fallback
            drop(state);
            drop(config);
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    /// Record a successful request
    pub async fn record_success(&self, _duration: Duration) {
        let mut state = self.state.write().await;
        state.consecutive_successes += 1;

        // Calculate current measured RPS
        let now = Instant::now();
        let recent_count = state
            .recent_requests
            .iter()
            .filter(|&&t| now.duration_since(t) < Duration::from_secs(1))
            .count();
        state.measured_rps = recent_count as f64;

        // Adaptive ramp-up: If we're consistently succeeding, increase limit
        let config = self.config.read().await;
        if let RateLimitStrategy::Adaptive {
            ramp_up_factor,
            max_rps,
            ramp_up_threshold,
            ..
        } = config.strategy
        {
            drop(config);

            // Ramp up after threshold successful requests
            if state.consecutive_successes >= ramp_up_threshold {
                let mut config = self.config.write().await;
                let new_limit = (config.current_limit * ramp_up_factor).min(max_rps);

                if new_limit != config.current_limit {
                    tracing::info!(
                        "[{}] Rate limit increased: {:.1} → {:.1} req/s (measured: {:.1} req/s)",
                        self.rpc_url,
                        config.current_limit,
                        new_limit,
                        state.measured_rps
                    );
                    config.current_limit = new_limit;

                    // Persist to database
                    drop(config);
                    drop(state);
                    self.persist_to_cache().await;
                    return;
                }

                state.consecutive_successes = 0;
            }
        }
    }

    /// Record a rate limit error
    pub async fn record_rate_limit(&self) {
        let mut state = self.state.write().await;
        state.last_rate_limit = Some(Instant::now());
        state.rate_limit_errors += 1;
        state.consecutive_successes = 0;

        // Back off immediately
        let mut config = self.config.write().await;

        match config.strategy {
            RateLimitStrategy::Manual => {
                // Don't adjust manual limits
                return;
            }
            RateLimitStrategy::Adaptive { back_off_factor, .. } => {
                let old_limit = config.current_limit;
                let new_limit = config.current_limit * back_off_factor;

                tracing::warn!(
                    "[{}] Rate limit hit! Backing off: {:.1} → {:.1} req/s (measured: {:.1} req/s)",
                    self.rpc_url,
                    old_limit,
                    new_limit,
                    state.measured_rps
                );

                config.current_limit = new_limit;
            }
            RateLimitStrategy::Aggressive {
                back_off_factor,
                min_rps,
                ..
            } => {
                let old_limit = config.current_limit;
                let new_limit = (config.current_limit * back_off_factor).max(min_rps);

                tracing::warn!(
                    "[{}] Rate limit hit! Backing off: {:.1} → {:.1} req/s",
                    self.rpc_url,
                    old_limit,
                    new_limit
                );

                config.current_limit = new_limit;
            }
        }

        drop(config);
        drop(state);

        // Persist to database
        self.persist_to_cache().await;
    }

    /// Record a general error (not rate limit)
    pub async fn record_error(&self) {
        let mut state = self.state.write().await;
        state.consecutive_successes = 0;
        // Don't adjust rate limit for non-rate-limit errors
    }

    /// Get current rate limit
    pub async fn current_limit(&self) -> f64 {
        self.config.read().await.current_limit
    }

    /// Get current statistics
    pub async fn stats(&self) -> RateLimitStats {
        let config = self.config.read().await;
        let state = self.state.read().await;

        RateLimitStats {
            rpc_url: self.rpc_url.clone(),
            configured_limit: config.manual_limit,
            current_limit: config.current_limit,
            measured_rps: state.measured_rps,
            total_requests: state.total_requests,
            rate_limit_errors: state.rate_limit_errors,
            success_rate: if state.total_requests > 0 {
                1.0 - (state.rate_limit_errors as f64 / state.total_requests as f64)
            } else {
                1.0
            },
        }
    }

    /// Persist current rate limit to database
    async fn persist_to_cache(&self) {
        let config = self.config.read().await;
        let state = self.state.read().await;

        let strategy_name = match &config.strategy {
            RateLimitStrategy::Manual => "manual",
            RateLimitStrategy::Adaptive { .. } => "adaptive",
            RateLimitStrategy::Aggressive { .. } => "aggressive",
        };

        if let Err(e) = self
            .cache
            .upsert_rpc_rate_limit(
                &self.rpc_url,
                config.current_limit,
                strategy_name,
                state.total_requests,
                state.rate_limit_errors,
                state.measured_rps,
                state.consecutive_successes,
            )
            .await
        {
            tracing::warn!(
                "[{}] Failed to persist rate limit to cache: {}",
                self.rpc_url,
                e
            );
        }
    }
}

/// Rate limit statistics
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct RateLimitStats {
    pub rpc_url: String,
    pub configured_limit: Option<f64>,
    pub current_limit: f64,
    pub measured_rps: f64,
    pub total_requests: u64,
    pub rate_limit_errors: u64,
    pub success_rate: f64,
}
