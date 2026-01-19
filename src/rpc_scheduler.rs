/// RPC request scheduler with round-robin distribution and adaptive rate limiting
///
/// This module distributes RPC requests across multiple endpoints in a round-robin
/// fashion, with each endpoint having its own adaptive rate limiter.
use crate::cache::Cache;
use crate::config::{RateLimitingConfig, RpcEndpointConfig};
use crate::error::{Result, StampError};
use crate::rate_limiter::AdaptiveRateLimiter;
use alloy::providers::{ProviderBuilder, RootProvider};
use alloy::transports::http::{Client, Http};
use futures::future::join_all;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

/// RPC endpoint with provider and rate limiter
struct RpcEndpoint {
    url: String,
    provider: RootProvider<Http<Client>>,
    rate_limiter: AdaptiveRateLimiter,
    priority: u8,
    weight: u32,
}

/// RPC scheduler for distributing requests across multiple endpoints
#[derive(Clone)]
pub struct RpcScheduler {
    endpoints: Arc<Vec<RpcEndpoint>>,
    next_endpoint_index: Arc<AtomicUsize>,
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

            // Create rate limiter
            let rate_limiter = AdaptiveRateLimiter::new(
                endpoint_config.url.clone(),
                endpoint_config.rate_limit,
                &rate_limiting_config.adaptive,
                &rate_limiting_config.aggressive,
                cache.clone(),
            )
            .await?;

            endpoints.push(RpcEndpoint {
                url: endpoint_config.url,
                provider,
                rate_limiter,
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
        endpoint.rate_limiter.acquire().await?;

        // Execute request and track timing
        let start = Instant::now();
        let result = operation(&endpoint.provider).await;
        let duration = start.elapsed();

        // Update rate limiter based on result
        match &result {
            Ok(_) => {
                endpoint.rate_limiter.record_success(duration).await;
            }
            Err(e) if is_rate_limit_error(e) => {
                endpoint.rate_limiter.record_rate_limit().await;
            }
            Err(_) => {
                endpoint.rate_limiter.record_error().await;
            }
        }

        result
    }

    /// Execute many requests in parallel across all RPCs
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
    pub fn providers(&self) -> Vec<&RootProvider<Http<Client>>> {
        self.endpoints.iter().map(|e| &e.provider).collect()
    }

    /// Get statistics for all endpoints
    pub async fn get_all_stats(&self) -> Vec<crate::rate_limiter::RateLimitStats> {
        let mut stats = Vec::new();
        for endpoint in self.endpoints.iter() {
            stats.push(endpoint.rate_limiter.stats().await);
        }
        stats
    }

    /// Print rate limit statistics
    pub async fn print_stats(&self) {
        tracing::info!("RPC Rate Limit Statistics:");
        for endpoint in self.endpoints.iter() {
            let stats = endpoint.rate_limiter.stats().await;
            tracing::info!(
                "  [{}] Current: {:.1} req/s, Measured: {:.1} req/s, Total: {}, Errors: {}, Success: {:.1}%",
                stats.rpc_url,
                stats.current_limit,
                stats.measured_rps,
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
            msg.contains("429") || msg.contains("rate limit") || msg.contains("too many requests")
        }
        _ => false,
    }
}
