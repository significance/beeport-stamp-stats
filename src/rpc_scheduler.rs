/// RPC request scheduler with round-robin distribution and adaptive rate limiting
///
/// This module distributes RPC requests across multiple endpoints in a round-robin
/// fashion, with each endpoint having its own adaptive rate limiter.
use crate::cache::Cache;
use crate::config::{RateLimitingConfig, RpcEndpointConfig};
use crate::error::{Result, StampError};
use crate::rate_limiter_v2;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use alloy::providers::{ProviderBuilder, RootProvider};
use alloy::transports::http::{Client, Http};
use futures::future::join_all;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

/// RPC endpoint with provider and token bucket rate limiter
struct RpcEndpoint {
    url: String,
    provider: RootProvider<Http<Client>>,
    rate_limiter: Arc<RwLock<rate_limiter_v2::TokenBucket>>,
    _recovery_task: JoinHandle<()>,  // Background task for automatic recovery
    #[allow(dead_code)]
    priority: u8,
    #[allow(dead_code)]
    weight: u32,
}

/// RPC scheduler for distributing requests across multiple endpoints
#[derive(Clone)]
pub struct RpcScheduler {
    endpoints: Arc<Vec<RpcEndpoint>>,
    next_endpoint_index: Arc<AtomicUsize>,
    #[allow(dead_code)]
    config: Arc<RateLimitingConfig>,
}

impl RpcScheduler {
    /// Create a new RPC scheduler from endpoint configurations
    pub async fn new(
        endpoint_configs: Vec<RpcEndpointConfig>,
        rate_limiting_config: RateLimitingConfig,
        cache: Arc<Cache>,
    ) -> Result<Self> {
        if endpoint_configs.is_empty() {
            return Err(StampError::Config(
                "At least one RPC endpoint must be configured".into(),
            ));
        }

        let mut endpoints = Vec::new();

        for endpoint_config in endpoint_configs {
            // Create provider
            let provider = ProviderBuilder::new()
                .on_http(
                    endpoint_config
                        .url
                        .parse()
                        .map_err(|e| StampError::Rpc(format!("Invalid RPC URL: {e}")))?,
                );

            // Create recovery config
            let recovery_config = rate_limiter_v2::RecoveryConfig {
                cool_down_period: std::time::Duration::from_secs(
                    rate_limiting_config.recovery.cool_down_period_secs
                ),
                min_healthy_rate: rate_limiting_config.recovery.min_healthy_rate,
                gradual_recovery_factor: rate_limiting_config.recovery.gradual_recovery_factor,
                recovery_check_interval: std::time::Duration::from_secs(
                    rate_limiting_config.recovery.recovery_check_interval_secs
                ),
            };

            // Check if we have a cached rate limit
            let cached_rate = cache
                .get_rpc_rate_limit(&endpoint_config.url)
                .await?
                .unwrap_or(rate_limiting_config.adaptive.start_rps);

            // SAFETY: Ensure initial rate respects bounds
            let initial_rate = cached_rate
                .max(rate_limiting_config.adaptive.min_rps)
                .min(rate_limiting_config.adaptive.max_rps);

            // Create token bucket rate limiter
            let token_bucket = rate_limiter_v2::TokenBucket::new(
                initial_rate,
                5.0, // capacity (burst size)
                rate_limiting_config.adaptive.min_rps,
                rate_limiting_config.adaptive.max_rps,
                rate_limiting_config.adaptive.ramp_up_factor,
                rate_limiting_config.adaptive.ramp_up_threshold as u64,
                rate_limiting_config.adaptive.back_off_factor,
                recovery_config,
                endpoint_config.url.clone(),
                Some(cache.clone()),
            );

            let rate_limiter = Arc::new(RwLock::new(token_bucket));

            // Spawn background recovery task
            let recovery_task = rate_limiter_v2::spawn_recovery_task(
                rate_limiter.clone(),
                endpoint_config.url.clone(),
            );

            tracing::info!(
                "[{}] Token bucket initialized: rate={:.1} req/s (min={:.1}, max={:.1})",
                endpoint_config.url,
                initial_rate,
                rate_limiting_config.adaptive.min_rps,
                rate_limiting_config.adaptive.max_rps
            );

            endpoints.push(RpcEndpoint {
                url: endpoint_config.url,
                provider,
                rate_limiter,
                _recovery_task: recovery_task,
                priority: endpoint_config.priority,
                weight: endpoint_config.weight,
            });
        }

        tracing::info!(
            "RPC scheduler initialized with {} endpoint(s)",
            endpoints.len()
        );

        Ok(Self {
            endpoints: Arc::new(endpoints),
            next_endpoint_index: Arc::new(AtomicUsize::new(0)),
            config: Arc::new(rate_limiting_config),
        })
    }

    /// Execute a single request with round-robin distribution and rate limiting
    #[allow(dead_code)]
    pub async fn execute<F, T>(&self, operation: F) -> Result<T>
    where
        F: FnOnce(&RootProvider<Http<Client>>) -> Pin<Box<dyn Future<Output = Result<T>> + Send + 'static>>,
    {
        // Round-robin: get next endpoint
        let index = self
            .next_endpoint_index
            .fetch_add(1, Ordering::Relaxed);
        let endpoint = &self.endpoints[index % self.endpoints.len()];

        // Acquire rate limit permit (blocks if at capacity)
        endpoint.rate_limiter.write().await.acquire().await?;

        // Execute request and track timing
        let start = Instant::now();
        let result = operation(&endpoint.provider).await;
        let duration = start.elapsed();

        // Update rate limiter based on result
        match &result {
            Ok(_) => {
                endpoint.rate_limiter.write().await.record_success(duration).await;
            }
            Err(e) if is_rate_limit_error(e) => {
                endpoint.rate_limiter.write().await.record_rate_limit().await;
            }
            Err(_) => {
                endpoint.rate_limiter.write().await.record_error().await;
            }
        }

        result
    }

    /// Execute a request with retry across different endpoints on pruning/404 errors
    pub async fn execute_with_retry<F, T>(&self, create_operation: impl Fn(&RootProvider<Http<Client>>) -> F) -> Result<T>
    where
        F: Future<Output = Result<T>>,
        T: Send + 'static,
    {
        const MAX_RATE_LIMIT_RETRIES: usize = 5;
        const RATE_LIMIT_RETRY_DELAY_SECS: u64 = 3;

        let num_endpoints = self.endpoints.len();

        // Outer retry loop for when all endpoints hit rate limits
        for rate_limit_retry in 0..MAX_RATE_LIMIT_RETRIES {
            let start_index = self
                .next_endpoint_index
                .fetch_add(1, Ordering::Relaxed);

            let mut last_error: Option<String> = None;
            let mut pruning_error_count = 0;
            let mut rate_limit_error_count = 0;
            let mut network_error_count = 0;

            for attempt in 0..num_endpoints {
                let index = (start_index + attempt) % num_endpoints;
                let endpoint = &self.endpoints[index];

                // Acquire rate limit permit (blocks if at capacity)
                if let Err(e) = endpoint.rate_limiter.write().await.acquire().await {
                    last_error = Some(format!("{e}"));
                    continue;
                }

                // Execute request and track timing
                let start_time = Instant::now();
                let result = create_operation(&endpoint.provider).await;
                let duration = start_time.elapsed();

                // Update rate limiter based on result
                match &result {
                    Ok(_) => {
                        endpoint.rate_limiter.write().await.record_success(duration).await;
                        if attempt > 0 || rate_limit_retry > 0 {
                            tracing::debug!(
                                "[{}] Request succeeded after {} endpoint attempts and {} rate limit retries",
                                endpoint.url,
                                attempt,
                                rate_limit_retry
                            );
                        }
                        return result;
                    }
                    Err(e) if is_rate_limit_error(e) => {
                        endpoint.rate_limiter.write().await.record_rate_limit().await;
                        rate_limit_error_count += 1;
                        tracing::debug!("[{}] Rate limit error, trying next endpoint", endpoint.url);
                        last_error = Some(format!("{e}"));
                        continue;
                    }
                    Err(e) if is_pruning_error(e) => {
                        endpoint.rate_limiter.write().await.record_error().await;
                        pruning_error_count += 1;
                        tracing::debug!(
                            "[{}] Pruning/404 error (attempt {}/{}), trying next endpoint",
                            endpoint.url,
                            attempt + 1,
                            num_endpoints
                        );
                        last_error = Some(format!("{e}"));
                        continue;
                    }
                    Err(e) if is_network_error(e) => {
                        endpoint.rate_limiter.write().await.record_error().await;
                        network_error_count += 1;
                        tracing::debug!(
                            "[{}] Network error (attempt {}/{}), trying next endpoint: {}",
                            endpoint.url,
                            attempt + 1,
                            num_endpoints,
                            e
                        );
                        last_error = Some(format!("{e}"));
                        continue;
                    }
                    Err(e) => {
                        endpoint.rate_limiter.write().await.record_error().await;
                        tracing::debug!("[{}] Error: {}", endpoint.url, e);
                        return result;
                    }
                }
            }

            // Check what type of errors we got from all endpoints
            if pruning_error_count == num_endpoints {
                // All endpoints pruned - data not available
                return Err(StampError::DataUnavailable(
                    last_error.unwrap_or_else(|| "Data pruned on all RPC endpoints".to_string())
                ));
            } else if rate_limit_error_count == num_endpoints {
                // All endpoints hit rate limits - wait and retry
                if rate_limit_retry < MAX_RATE_LIMIT_RETRIES - 1 {
                    tracing::warn!(
                        "All {} RPC endpoints hit rate limits, waiting {}s before retry {}/{}",
                        num_endpoints,
                        RATE_LIMIT_RETRY_DELAY_SECS,
                        rate_limit_retry + 1,
                        MAX_RATE_LIMIT_RETRIES
                    );
                    tokio::time::sleep(tokio::time::Duration::from_secs(RATE_LIMIT_RETRY_DELAY_SECS)).await;
                    continue; // Retry all endpoints
                } else {
                    // Max retries exhausted
                    return Err(StampError::Rpc(
                        format!("All RPC endpoints exhausted rate limits after {MAX_RATE_LIMIT_RETRIES} retries")
                    ));
                }
            } else if network_error_count == num_endpoints {
                // All endpoints had network errors
                return Err(StampError::Rpc(
                    last_error.unwrap_or_else(|| "All RPC endpoints had network errors".to_string())
                ));
            } else {
                // Mixed errors or other failures
                return Err(StampError::Rpc(
                    last_error.unwrap_or_else(|| "All RPC endpoints failed".to_string())
                ));
            }
        }

        // Should never reach here due to MAX_RATE_LIMIT_RETRIES check above
        Err(StampError::Rpc("Rate limit retry loop exhausted unexpectedly".to_string()))
    }

    /// Execute many requests in parallel across all RPCs
    #[allow(dead_code)]
    pub async fn execute_many<F, T>(&self, operations: Vec<F>) -> Result<Vec<T>>
    where
        F: FnOnce(&RootProvider<Http<Client>>) -> Pin<Box<dyn Future<Output = Result<T>> + Send + 'static>>
            + Send
            + 'static,
        T: Send + 'static,
    {
        // Create futures that use the scheduler's execute method
        let futures: Vec<_> = operations
            .into_iter()
            .map(|op| {
                let scheduler = self.clone();
                async move { scheduler.execute(op).await }
            })
            .collect();

        // Execute all in parallel
        let results = join_all(futures).await;

        // Collect results
        results.into_iter().collect()
    }

    /// Get the primary provider (for backwards compatibility)
    pub fn primary_provider(&self) -> &RootProvider<Http<Client>> {
        &self.endpoints[0].provider
    }

    /// Get all providers
    #[allow(dead_code)]
    pub fn providers(&self) -> Vec<&RootProvider<Http<Client>>> {
        self.endpoints.iter().map(|e| &e.provider).collect()
    }

    /// Get statistics for all endpoints
    #[allow(dead_code)]
    pub async fn get_all_stats(&self) -> Vec<rate_limiter_v2::RateLimitStats> {
        let mut stats = Vec::new();
        for endpoint in self.endpoints.iter() {
            stats.push(endpoint.rate_limiter.read().await.stats());
        }
        stats
    }

    /// Print rate limit statistics
    pub async fn print_stats(&self) {
        tracing::info!("RPC Rate Limit Statistics:");
        for endpoint in self.endpoints.iter() {
            let stats = endpoint.rate_limiter.read().await.stats();
            tracing::info!(
                "  [{}] Rate: {:.1} req/s (min={:.1}, max={:.1}), State: {:?}, Total: {}, Errors: {}, Success: {:.1}%",
                stats.rpc_url,
                stats.current_limit,
                stats.min_rate,
                stats.max_rate,
                stats.state,
                stats.total_requests,
                stats.rate_limit_errors,
                stats.success_rate * 100.0
            );
        }
    }
}

/// Check if an error is a rate limit error
fn is_rate_limit_error(error: &StampError) -> bool {
    match error {
        StampError::Rpc(msg) => {
            let msg_lower = msg.to_lowercase();
            msg.contains("429") || msg_lower.contains("rate limit") || msg_lower.contains("too many requests")
        }
        _ => false,
    }
}

/// Check if an error is a pruning/404 error (data not available on this RPC)
fn is_pruning_error(error: &StampError) -> bool {
    match error {
        StampError::Rpc(msg) | StampError::DataUnavailable(msg) => {
            let msg_lower = msg.to_lowercase();
            msg.contains("404")
                || msg_lower.contains("not found")
                || msg_lower.contains("pruned")
                || msg_lower.contains("pruning")
                || msg_lower.contains("missing trie node")
                || msg_lower.contains("header not found")
                || msg_lower.contains("block not found")
        }
        _ => false,
    }
}

/// Check if an error is a network connectivity error (should try next endpoint)
fn is_network_error(error: &StampError) -> bool {
    match error {
        StampError::Rpc(msg) => {
            let msg_lower = msg.to_lowercase();
            msg_lower.contains("error sending request")
                || msg_lower.contains("connection reset")
                || msg_lower.contains("connection refused")
                || msg_lower.contains("connection closed")
                || msg_lower.contains("connection timeout")
                || msg_lower.contains("timed out")
                || msg_lower.contains("network")
                || msg_lower.contains("dns")
                || msg_lower.contains("tcp")
        }
        _ => false,
    }
}
