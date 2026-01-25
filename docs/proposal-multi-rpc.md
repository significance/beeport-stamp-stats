# Multi-RPC Parallel Request Distribution - Proposal

**Branch**: feat/improve-retrieval-efficiency
**Date**: 2026-01-11
**Status**: Proposal - Awaiting Review

## ✅ Completed Work

- [x] Created PostgreSQL migration for `rpc_rate_limits` table (20260111000008)
- [x] Created SQLite migration for `rpc_rate_limits` table (20260111000008)
- [x] Updated proposal with persistence architecture and cache methods
- [ ] Ready to begin Phase 1 implementation

## Problem Statement

### Current Limitations
- **Single RPC bottleneck**: All requests serialized through one endpoint
- **Underutilized capacity**: Multiple free RPCs available but unused
- **Conservative rate limiting**: We don't know actual limits until we hit them
- **Slow retrieval**: Can't parallelize requests across providers
- **Manual tuning**: No automatic rate limit discovery

### Target Performance
- **Parallel request distribution**: Request #1 → RPC1, Request #2 → RPC2, etc.
- **Adaptive throttling**: Automatically discover and respect each RPC's rate limit
- **Maximum throughput**: Keep ALL RPCs running at their individual maximums
- **Manual override**: Optionally specify known rate limits in YAML

---

## Proposed Architecture

### High-Level Design

```
┌──────────────────────────────────────────────────────────────┐
│                  BlockchainClient                            │
│  - Submits requests to RpcScheduler                          │
└────────────────────┬─────────────────────────────────────────┘
                     │
                     ▼
┌──────────────────────────────────────────────────────────────┐
│                    RpcScheduler                              │
│  - Round-robin request distribution                          │
│  - Per-endpoint request queues                               │
│  - Adaptive rate limit tracking                              │
│  - Concurrent execution with throttling                      │
└────────────┬─────────────────────────────────────────────────┘
             │
    ┌────────┼────────┬────────┬────────┐
    ▼        ▼        ▼        ▼        ▼
┌────────┐┌────────┐┌────────┐┌────────┐┌────────┐
│RPC #1  ││RPC #2  ││RPC #3  ││RPC #4  ││RPC #5  │
│Queue   ││Queue   ││Queue   ││Queue   ││Queue   │
│───────│││───────│││───────│││───────│││───────││
│Req 1  │││Req 2  │││Req 3  │││Req 4  │││Req 5  ││
│Req 6  │││Req 7  │││Req 8  │││Req 9  │││Req 10 ││
│Req 11 │││Req 12 │││Req 13 │││...    │││...    ││
│───────│││───────│││───────│││───────│││───────││
│Limit: ││ Limit: ││ Limit: ││ Limit: ││ Limit: ││
│50 r/s │││30 r/s │││?? r/s │││100 r/s│││?? r/s ││
│Auto   │││Auto   │││Auto   │││Manual │││Auto   ││
└────────┘└────────┘└────────┘└────────┘└────────┘
```

### Request Flow

```
1. Client: "Fetch events for blocks 1000-2000"
   ↓
2. Chunk into requests: [getLogs(1000-1100), getLogs(1100-1200), ...]
   ↓
3. RpcScheduler distributes:
   - Request 1 → RPC1 queue
   - Request 2 → RPC2 queue
   - Request 3 → RPC3 queue
   - Request 4 → RPC1 queue (wrap around)
   - ...
   ↓
4. Each RPC queue executes with adaptive throttling:
   - RPC1: Max 50 req/s (detected)
   - RPC2: Max 30 req/s (detected)
   - RPC3: Max unknown → start conservative, ramp up
   - RPC4: Max 100 req/s (manual config)
   ↓
5. Results collected and returned in order
```

---

## Core Components

### 1. RpcScheduler - Request Distribution & Throttling

```rust
pub struct RpcScheduler {
    endpoints: Vec<RpcEndpoint>,
    next_endpoint_index: Arc<AtomicUsize>,
    rate_limiters: Arc<RwLock<HashMap<String, AdaptiveRateLimiter>>>,
}

impl RpcScheduler {
    /// Submit a request - it will be distributed to next available RPC
    pub async fn execute<F, T>(&self, operation: F) -> Result<T>
    where
        F: FnOnce(&RootProvider<Http<Client>>) -> BoxFuture<'_, Result<T>> + Send + 'static,
        T: Send + 'static,
    {
        // Round-robin: get next endpoint
        let index = self.next_endpoint_index.fetch_add(1, Ordering::Relaxed);
        let endpoint = &self.endpoints[index % self.endpoints.len()];

        // Acquire rate limit permit (blocks if at capacity)
        let limiter = self.rate_limiters.read().await
            .get(&endpoint.url).unwrap().clone();

        let _permit = limiter.acquire().await?;

        // Execute request and track timing
        let start = Instant::now();
        let result = operation(&endpoint.provider).await;
        let duration = start.elapsed();

        // Update rate limiter based on result
        match &result {
            Ok(_) => {
                limiter.record_success(duration).await;
            }
            Err(e) if is_rate_limit_error(e) => {
                limiter.record_rate_limit().await;
            }
            Err(e) => {
                limiter.record_error(e).await;
            }
        }

        result
    }

    /// Execute many requests in parallel across all RPCs
    pub async fn execute_many<F, T>(&self, operations: Vec<F>) -> Result<Vec<T>>
    where
        F: FnOnce(&RootProvider<Http<Client>>) -> BoxFuture<'_, Result<T>> + Send + 'static,
        T: Send + 'static,
    {
        let tasks: Vec<_> = operations.into_iter()
            .map(|op| self.execute(op))
            .collect();

        // Execute all in parallel with tokio::spawn
        let handles: Vec<_> = tasks.into_iter()
            .map(|task| tokio::spawn(task))
            .collect();

        // Collect results (maintains order)
        let mut results = Vec::new();
        for handle in handles {
            results.push(handle.await??);
        }

        Ok(results)
    }
}
```

### 2. AdaptiveRateLimiter - Per-Endpoint Throttling

```rust
pub struct AdaptiveRateLimiter {
    config: Arc<RwLock<RateLimitConfig>>,
    semaphore: Arc<Semaphore>,
    state: Arc<RwLock<RateLimitState>>,
    cache: Arc<Cache>, // For persisting discovered limits
}

pub struct RateLimitConfig {
    /// Manual rate limit (if specified in config)
    manual_limit: Option<f64>,

    /// Current effective rate limit (requests per second)
    current_limit: f64,

    /// Strategy for rate limit discovery
    strategy: RateLimitStrategy,
}

pub struct RateLimitState {
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

pub enum RateLimitStrategy {
    /// Manual: User-specified, never changes
    Manual(f64),

    /// Adaptive: Start conservative, ramp up until we hit limit
    Adaptive {
        start_rps: f64,      // Default: 10 req/s
        max_rps: f64,        // Default: 1000 req/s (safety cap)
        ramp_up_factor: f64, // Default: 1.2 (20% increase)
        back_off_factor: f64, // Default: 0.5 (50% decrease)
    },

    /// Aggressive: Start high, back off when hit
    Aggressive {
        start_rps: f64,      // Default: 100 req/s
        min_rps: f64,        // Default: 1 req/s
        back_off_factor: f64, // Default: 0.7 (30% decrease)
    },
}

impl AdaptiveRateLimiter {
    pub async fn acquire(&self) -> Result<RateLimitPermit> {
        // Calculate delay based on current rate limit
        let config = self.config.read().await;
        let delay = Duration::from_secs_f64(1.0 / config.current_limit);
        drop(config);

        // Wait for next available slot
        let mut state = self.state.write().await;

        // Clean up old entries (older than 1 second)
        let now = Instant::now();
        while let Some(&front) = state.recent_requests.front() {
            if now.duration_since(front) > Duration::from_secs(1) {
                state.recent_requests.pop_front();
            } else {
                break;
            }
        }

        // Check if we're at capacity
        let config = self.config.read().await;
        while state.recent_requests.len() >= config.current_limit.ceil() as usize {
            // Wait for oldest request to age out
            if let Some(&oldest) = state.recent_requests.front() {
                let age = now.duration_since(oldest);
                if age < Duration::from_secs(1) {
                    let wait = Duration::from_secs(1) - age + Duration::from_millis(10);
                    drop(state);
                    drop(config);
                    tokio::time::sleep(wait).await;
                    state = self.state.write().await;
                    config = self.config.read().await;
                    continue;
                }
            }
            state.recent_requests.pop_front();
        }

        // Acquire permit
        state.recent_requests.push_back(Instant::now());
        state.total_requests += 1;
        drop(state);
        drop(config);

        Ok(RateLimitPermit { _private: () })
    }

    pub async fn record_success(&self, duration: Duration) {
        let mut state = self.state.write().await;
        state.consecutive_successes += 1;

        // Calculate current measured RPS
        let now = Instant::now();
        let recent_count = state.recent_requests.iter()
            .filter(|&&t| now.duration_since(t) < Duration::from_secs(1))
            .count();
        state.measured_rps = recent_count as f64;

        // Adaptive ramp-up: If we're consistently succeeding, increase limit
        let config = self.config.read().await;
        if let RateLimitStrategy::Adaptive { ramp_up_factor, max_rps, .. } = config.strategy {
            drop(config);

            // Ramp up every 100 successful requests
            if state.consecutive_successes >= 100 {
                let mut config = self.config.write().await;
                let new_limit = (config.current_limit * ramp_up_factor).min(max_rps);

                if new_limit != config.current_limit {
                    tracing::info!(
                        "Rate limit increased: {:.1} → {:.1} req/s (measured: {:.1} req/s)",
                        config.current_limit,
                        new_limit,
                        state.measured_rps
                    );
                    config.current_limit = new_limit;

                    // Persist to database
                    self.persist_to_cache().await;
                }

                state.consecutive_successes = 0;
            }
        }
    }

    pub async fn record_rate_limit(&self) {
        let mut state = self.state.write().await;
        state.last_rate_limit = Some(Instant::now());
        state.rate_limit_errors += 1;
        state.consecutive_successes = 0;

        // Back off immediately
        let mut config = self.config.write().await;

        let back_off_factor = match config.strategy {
            RateLimitStrategy::Manual(_) => return, // Don't adjust manual limits
            RateLimitStrategy::Adaptive { back_off_factor, .. } => back_off_factor,
            RateLimitStrategy::Aggressive { back_off_factor, min_rps, .. } => {
                let new_limit = (config.current_limit * back_off_factor).max(min_rps);
                config.current_limit = new_limit;

                tracing::warn!(
                    "Rate limit hit! Backing off: {:.1} → {:.1} req/s",
                    config.current_limit / back_off_factor,
                    new_limit
                );
                return;
            }
        };

        let new_limit = config.current_limit * back_off_factor;

        tracing::warn!(
            "Rate limit hit! Backing off: {:.1} → {:.1} req/s (measured: {:.1} req/s)",
            config.current_limit,
            new_limit,
            state.measured_rps
        );

        config.current_limit = new_limit;
        drop(config);
        drop(state);

        // Persist to database
        self.persist_to_cache().await;
    }

    pub async fn record_error(&self, error: &StampError) {
        let mut state = self.state.write().await;
        state.consecutive_successes = 0;

        // Different errors may warrant different handling
        match error {
            StampError::Rpc(msg) if msg.contains("502") || msg.contains("503") => {
                // Temporary server error - reduce rate temporarily
                let mut config = self.config.write().await;
                config.current_limit = (config.current_limit * 0.8).max(1.0);

                tracing::warn!(
                    "Server error (502/503), reducing rate to {:.1} req/s",
                    config.current_limit
                );
            }
            _ => {
                // Other errors don't affect rate limiting
            }
        }
    }

    /// Get current statistics
    pub async fn stats(&self) -> RateLimitStats {
        let config = self.config.read().await;
        let state = self.state.read().await;

        RateLimitStats {
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

        if let Err(e) = self.cache.upsert_rpc_rate_limit(
            &self.rpc_url,
            config.current_limit,
            &format!("{:?}", config.strategy),
            state.total_requests,
            state.rate_limit_errors,
            state.measured_rps,
            state.consecutive_successes,
        ).await {
            tracing::warn!("Failed to persist rate limit to cache: {}", e);
        }
    }

    /// Load previously discovered rate limit from database
    pub async fn load_from_cache(cache: &Cache, rpc_url: &str) -> Option<f64> {
        cache.get_rpc_rate_limit(rpc_url).await.ok().flatten()
    }
}

pub struct RateLimitPermit {
    _private: (), // Marker to ensure Drop is called
}
```

### 3. Rate Limit Persistence (Cache Integration)

**Database Schema:**

```sql
-- PostgreSQL
CREATE TABLE rpc_rate_limits (
    id SERIAL PRIMARY KEY,
    rpc_url TEXT NOT NULL UNIQUE,
    discovered_rate_limit REAL NOT NULL,
    rate_limit_strategy TEXT NOT NULL,
    total_requests BIGINT NOT NULL DEFAULT 0,
    rate_limit_errors BIGINT NOT NULL DEFAULT 0,
    success_rate REAL NOT NULL DEFAULT 1.0,
    measured_rps REAL,
    avg_response_time_ms REAL,
    first_seen_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_rate_limit_at TIMESTAMPTZ,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    consecutive_successes INTEGER NOT NULL DEFAULT 0,
    consecutive_failures INTEGER NOT NULL DEFAULT 0
);
```

**Cache Methods:**

```rust
impl Cache {
    /// Store or update RPC rate limit information
    pub async fn upsert_rpc_rate_limit(
        &self,
        rpc_url: &str,
        discovered_rate_limit: f64,
        strategy: &str,
        total_requests: u64,
        rate_limit_errors: u64,
        measured_rps: f64,
        consecutive_successes: u32,
    ) -> Result<()> {
        let success_rate = if total_requests > 0 {
            1.0 - (rate_limit_errors as f64 / total_requests as f64)
        } else {
            1.0
        };

        match &self.pool {
            DatabasePool::Postgres(pool) => {
                sqlx::query(
                    r#"
                    INSERT INTO rpc_rate_limits (
                        rpc_url, discovered_rate_limit, rate_limit_strategy,
                        total_requests, rate_limit_errors, success_rate,
                        measured_rps, consecutive_successes, last_updated_at
                    ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, NOW())
                    ON CONFLICT (rpc_url) DO UPDATE SET
                        discovered_rate_limit = $2,
                        rate_limit_strategy = $3,
                        total_requests = $4,
                        rate_limit_errors = $5,
                        success_rate = $6,
                        measured_rps = $7,
                        consecutive_successes = $8,
                        last_updated_at = NOW()
                    "#,
                )
                .bind(rpc_url)
                .bind(discovered_rate_limit)
                .bind(strategy)
                .bind(total_requests as i64)
                .bind(rate_limit_errors as i64)
                .bind(success_rate)
                .bind(measured_rps)
                .bind(consecutive_successes as i32)
                .execute(pool)
                .await?;
            }
            DatabasePool::Sqlite(pool) => {
                sqlx::query(
                    r#"
                    INSERT INTO rpc_rate_limits (
                        rpc_url, discovered_rate_limit, rate_limit_strategy,
                        total_requests, rate_limit_errors, success_rate,
                        measured_rps, consecutive_successes, last_updated_at
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, datetime('now'))
                    ON CONFLICT (rpc_url) DO UPDATE SET
                        discovered_rate_limit = ?2,
                        rate_limit_strategy = ?3,
                        total_requests = ?4,
                        rate_limit_errors = ?5,
                        success_rate = ?6,
                        measured_rps = ?7,
                        consecutive_successes = ?8,
                        last_updated_at = datetime('now')
                    "#,
                )
                .bind(rpc_url)
                .bind(discovered_rate_limit)
                .bind(strategy)
                .bind(total_requests as i64)
                .bind(rate_limit_errors as i64)
                .bind(success_rate)
                .bind(measured_rps)
                .bind(consecutive_successes as i32)
                .execute(pool)
                .await?;
            }
        }

        Ok(())
    }

    /// Get previously discovered rate limit for an RPC
    pub async fn get_rpc_rate_limit(&self, rpc_url: &str) -> Result<Option<f64>> {
        let rate_limit = match &self.pool {
            DatabasePool::Postgres(pool) => {
                sqlx::query_scalar::<_, f64>(
                    "SELECT discovered_rate_limit FROM rpc_rate_limits WHERE rpc_url = $1 AND is_active = TRUE"
                )
                .bind(rpc_url)
                .fetch_optional(pool)
                .await?
            }
            DatabasePool::Sqlite(pool) => {
                sqlx::query_scalar::<_, f64>(
                    "SELECT discovered_rate_limit FROM rpc_rate_limits WHERE rpc_url = ?1 AND is_active = 1"
                )
                .bind(rpc_url)
                .fetch_optional(pool)
                .await?
            }
        };

        Ok(rate_limit)
    }

    /// Get statistics for all tracked RPCs
    pub async fn get_all_rpc_stats(&self) -> Result<Vec<RpcRateLimitStats>> {
        // Implementation for listing all RPC stats
        // Useful for monitoring and debugging
    }
}
```

**Benefits of Persistence:**

1. **Resume on restart**: Don't re-discover limits every time the tool starts
2. **Historical tracking**: See how rate limits change over time
3. **Smart initialization**: Start with last known good value instead of conservative default
4. **Monitoring**: Query database to see current rate limits for all RPCs
5. **Analytics**: Track which RPCs are most reliable, fastest, etc.

**Example Flow:**

```
Session 1:
  - RPC1 starts at 10 req/s (adaptive default)
  - Discovers actual limit is 50 req/s after 2 minutes
  - Persists to database: rpc_rate_limits(rpc1, 50.0)

Session 2 (restart):
  - Load from database: RPC1 = 50 req/s
  - Start immediately at 50 req/s (no re-discovery needed!)
  - Continue adaptive adjustment from there
```

### 4. Integration with BlockchainClient

```rust
impl BlockchainClient {
    /// Create from RPC scheduler (replaces single provider)
    pub async fn new_with_scheduler(scheduler: Arc<RpcScheduler>) -> Result<Self> {
        Ok(Self { scheduler })
    }

    /// Fetch events from multiple blocks in parallel
    pub async fn fetch_logs_parallel(
        &self,
        chunks: Vec<(u64, u64)>, // Vec of (from_block, to_block)
        filter_template: Filter,
    ) -> Result<Vec<Vec<Log>>> {
        // Create operations for each chunk
        let operations: Vec<_> = chunks.into_iter()
            .map(|(from, to)| {
                let filter = filter_template.clone()
                    .from_block(from)
                    .to_block(to);

                Box::new(move |provider: &RootProvider<Http<Client>>| {
                    Box::pin(async move {
                        provider.get_logs(&filter).await
                            .map_err(|e| StampError::Rpc(format!("getLogs failed: {e}")))
                    }) as Pin<Box<dyn Future<Output = Result<Vec<Log>>> + Send>>
                })
            })
            .collect();

        // Execute all chunks in parallel across all RPCs
        self.scheduler.execute_many(operations).await
    }

    /// Fetch single block timestamp (uses scheduler)
    pub async fn get_block_timestamp(&self, block_number: u64) -> Result<DateTime<Utc>> {
        self.scheduler.execute(|provider| {
            Box::pin(async move {
                let block = provider
                    .get_block_by_number(block_number.into(), BlockTransactionsKind::Hashes)
                    .await
                    .map_err(|e| StampError::Rpc(format!("getBlockByNumber failed: {e}")))?
                    .ok_or_else(|| StampError::Rpc(format!("Block {block_number} not found")))?;

                Ok(DateTime::from_timestamp(block.header.timestamp as i64, 0)
                    .ok_or_else(|| StampError::Parse("Invalid timestamp".into()))?)
            })
        }).await
    }
}
```

---

## Configuration Schema

### YAML Configuration

```yaml
rpc:
  endpoints:
    # Endpoint 1: Manual rate limit (known stable RPC)
    - url: "https://rpc.gnosis.gateway.fm"
      rate_limit: 50.0  # 50 requests per second (manual)
      priority: 0       # Highest priority

    # Endpoint 2: Adaptive rate limiting (unknown capacity)
    - url: "https://rpc.gnosischain.com"
      rate_limit: "adaptive"
      adaptive_config:
        start_rps: 10.0       # Start conservative
        max_rps: 200.0        # Safety cap
        ramp_up_factor: 1.2   # 20% increase on success
        back_off_factor: 0.5  # 50% decrease on rate limit
      priority: 1

    # Endpoint 3: Aggressive (fast discovery)
    - url: "https://gnosis-pokt.nodies.app"
      rate_limit: "aggressive"
      aggressive_config:
        start_rps: 100.0      # Start high
        min_rps: 1.0          # Floor
        back_off_factor: 0.7  # 30% decrease
      priority: 2

    # Endpoint 4: Default adaptive (no config needed)
    - url: "https://rpc.ankr.com/gnosis"
      # rate_limit omitted = adaptive with defaults

    # Endpoint 5: Very conservative (untrusted RPC)
    - url: "https://new-untested-rpc.com"
      rate_limit: 5.0  # Manual: 5 req/s max
      priority: 3

# Global rate limiting settings (optional)
rate_limiting:
  # Maximum concurrent requests across ALL RPCs
  max_concurrent: 100

  # Health check interval (optional)
  health_check_interval_seconds: 60

  # Log rate limit stats every N seconds
  stats_interval_seconds: 30

# Backward compatibility: single URL (converted to single endpoint)
# rpc:
#   url: "https://rpc.gnosis.gateway.fm"
```

### Environment Variables

```bash
# Endpoint 1
BEEPORT__RPC__ENDPOINTS__0__URL="https://rpc1.com"
BEEPORT__RPC__ENDPOINTS__0__RATE_LIMIT=50.0

# Endpoint 2 (adaptive)
BEEPORT__RPC__ENDPOINTS__1__URL="https://rpc2.com"
BEEPORT__RPC__ENDPOINTS__1__RATE_LIMIT="adaptive"
BEEPORT__RPC__ENDPOINTS__1__ADAPTIVE_CONFIG__START_RPS=10.0
BEEPORT__RPC__ENDPOINTS__1__ADAPTIVE_CONFIG__MAX_RPS=200.0

# Global settings
BEEPORT__RATE_LIMITING__MAX_CONCURRENT=100
```

### CLI Arguments

```bash
# Multiple endpoints with automatic detection
beeport-stamp-stats \
  --rpc-url "https://rpc1.com" \
  --rpc-url "https://rpc2.com" \
  --rpc-url "https://rpc3.com" \
  fetch

# With manual rate limits
beeport-stamp-stats \
  --rpc-url "https://rpc1.com" --rpc-rate-limit 50 \
  --rpc-url "https://rpc2.com" --rpc-rate-limit adaptive \
  --rpc-url "https://rpc3.com" --rpc-rate-limit 100 \
  fetch
```

---

## Implementation Plan

### Phase 1: Core Scheduler (2-3 sessions)

#### 1.1 Database Migrations
- [x] Create PostgreSQL migration for `rpc_rate_limits` table
- [x] Create SQLite migration for `rpc_rate_limits` table
- [ ] Test migrations on both database types

**Files**: `migrations_postgres/20260111000008_*.sql`, `migrations_sqlite/20260111000008_*.sql`

#### 1.2 Configuration Updates
- [ ] Add `RpcEndpointConfig` with rate_limit field
- [ ] Add `AdaptiveConfig` and `AggressiveConfig` structs
- [ ] Add `RateLimitingConfig` for global settings
- [ ] Implement config parsing and validation
- [ ] Test backward compatibility

**Files**: `src/config.rs`

#### 1.3 Cache Persistence Methods
- [ ] Add `upsert_rpc_rate_limit()` method to Cache
- [ ] Add `get_rpc_rate_limit()` method to Cache
- [ ] Add `get_all_rpc_stats()` method for monitoring
- [ ] Handle both PostgreSQL and SQLite properly
- [ ] Unit tests for persistence

**Files**: `src/cache.rs`

#### 1.4 AdaptiveRateLimiter Implementation
- [ ] Create `src/rate_limiter.rs`
- [ ] Implement sliding window rate limiting
- [ ] Implement adaptive ramp-up logic
- [ ] Implement back-off on rate limit errors
- [ ] Add statistics tracking
- [ ] Integrate cache persistence (load on init, save on changes)
- [ ] Unit tests for all strategies

**New file**: `src/rate_limiter.rs`

#### 1.5 RpcScheduler Implementation
- [ ] Create `src/rpc_scheduler.rs`
- [ ] Implement round-robin distribution
- [ ] Integrate AdaptiveRateLimiter per endpoint
- [ ] Load persisted rate limits on startup
- [ ] Implement `execute()` and `execute_many()`
- [ ] Add request ordering/result collection
- [ ] Unit tests for distribution

**New file**: `src/rpc_scheduler.rs`

#### 1.6 BlockchainClient Integration
- [ ] Replace single provider with Arc<RpcScheduler>
- [ ] Update all RPC calls to use scheduler
- [ ] Implement parallel chunk fetching
- [ ] Maintain method signatures (no breaking changes)
- [ ] Integration tests

**Files**: `src/blockchain.rs`

---

### Phase 2: Observability & Optimization (1-2 sessions)

#### 2.1 Metrics & Logging
- [ ] Log rate limit adjustments (ramp-up/back-off)
- [ ] Periodic stats logging (RPS, success rate, errors)
- [ ] Request distribution visualization
- [ ] Per-endpoint health dashboard

#### 2.2 Performance Tuning
- [ ] Benchmark single vs multi-RPC
- [ ] Tune adaptive parameters (ramp-up factor, etc.)
- [ ] Add concurrency limits
- [ ] Optimize memory usage

#### 2.3 Error Handling
- [ ] Graceful degradation (endpoint failure)
- [ ] Circuit breaker integration
- [ ] Retry logic for non-rate-limit errors
- [ ] Request timeout handling

---

### Phase 3: Advanced Features (1 session)

#### 3.1 Request Prioritization
- [ ] Priority queue per endpoint
- [ ] Critical requests bypass rate limiting
- [ ] Batch optimization

#### 3.2 Smart Chunk Distribution
- [ ] Chunk size optimization per RPC speed
- [ ] Work stealing (fast RPC takes work from slow)
- [ ] Dynamic rebalancing

---

## Rate Limit Discovery Examples

### Scenario 1: Unknown RPC (Adaptive)

```
Time    Action                          Current Limit    Measured RPS
────────────────────────────────────────────────────────────────────
0:00    Start with conservative limit   10 req/s         -
0:10    100 successes → ramp up         12 req/s         10 req/s
0:20    100 successes → ramp up         14.4 req/s       12 req/s
0:30    100 successes → ramp up         17.3 req/s       14.4 req/s
0:40    100 successes → ramp up         20.8 req/s       17.3 req/s
0:50    Hit rate limit (429) → back off 10.4 req/s       20.8 req/s
1:00    Stabilize at safe level         10.4 req/s       10.4 req/s
```

**Result**: Discovered actual limit is ~20 req/s, now running at 10.4 req/s (safe margin)

### Scenario 2: Fast RPC (Aggressive)

```
Time    Action                          Current Limit    Measured RPS
────────────────────────────────────────────────────────────────────
0:00    Start aggressive               100 req/s         -
0:02    Hit rate limit (429) → back off 70 req/s         100 req/s
0:05    100 successes → ramp up         84 req/s         70 req/s
0:07    Hit rate limit → back off       58.8 req/s       84 req/s
0:10    Stabilize                       58.8 req/s       58.8 req/s
```

**Result**: Discovered actual limit is ~84 req/s, now running at 58.8 req/s (safe margin)

---

## Performance Projections

### Baseline (Single RPC)

```
Single RPC at 50 req/s:
- Fetch 1000 blocks (100 chunks of 10 blocks)
- Time: 100 chunks / 50 req/s = 2.0 seconds
```

### Multi-RPC (5 endpoints)

```
5 RPCs with adaptive limits (assume discovered limits):
- RPC1: 50 req/s
- RPC2: 30 req/s
- RPC3: 40 req/s
- RPC4: 60 req/s
- RPC5: 20 req/s

Combined throughput: 200 req/s

- Fetch 1000 blocks (100 chunks)
- Time: 100 chunks / 200 req/s = 0.5 seconds

Speedup: 4x faster
```

### Real-World Fetch (31M → 36M blocks)

```
Block range: 31,000,000 → 36,000,000 (5 million blocks)
Chunk size: 10,000 blocks
Total chunks: 500 chunks

Single RPC (50 req/s):
- Time: 500 / 50 = 10 seconds (fetch only)

Multi-RPC (5 endpoints, 200 req/s combined):
- Time: 500 / 200 = 2.5 seconds (fetch only)

Speedup: 4x faster on fetch operations
```

**Note**: Actual time includes block timestamp fetching and database writes, but RPC parallelization provides significant speedup.

---

## Testing Strategy

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_round_robin_distribution() {
        // Create scheduler with 3 endpoints
        // Execute 10 requests
        // Verify: req1→rpc1, req2→rpc2, req3→rpc3, req4→rpc1, ...
    }

    #[tokio::test]
    async fn test_adaptive_ramp_up() {
        let limiter = AdaptiveRateLimiter::new(/* adaptive config */);

        // Start at 10 req/s
        assert_eq!(limiter.current_limit(), 10.0);

        // 100 successes → should ramp up
        for _ in 0..100 {
            let _permit = limiter.acquire().await.unwrap();
            limiter.record_success(Duration::from_millis(50)).await;
        }

        assert!(limiter.current_limit() > 10.0);
    }

    #[tokio::test]
    async fn test_rate_limit_backoff() {
        let limiter = AdaptiveRateLimiter::new(/* adaptive config */);

        let initial = limiter.current_limit();
        limiter.record_rate_limit().await;

        // Should back off by 50%
        assert_eq!(limiter.current_limit(), initial * 0.5);
    }

    #[tokio::test]
    async fn test_sliding_window() {
        let limiter = AdaptiveRateLimiter::new_manual(10.0); // 10 req/s

        // Should allow 10 requests immediately
        for _ in 0..10 {
            assert!(limiter.acquire().await.is_ok());
        }

        // 11th request should block
        let start = Instant::now();
        limiter.acquire().await.unwrap();
        assert!(start.elapsed() > Duration::from_millis(100));
    }
}
```

### Integration Tests

```bash
# Test with multiple real RPCs
beeport-stamp-stats --config multi-rpc-test.yaml fetch \
  --from-block 31300000 --to-block 31310000

# Expected output:
# [INFO] RPC distribution:
# [INFO]   - rpc.gnosis.gateway.fm: 34 requests, 50.0 req/s
# [INFO]   - rpc.gnosischain.com: 33 requests, 30.0 req/s
# [INFO]   - gnosis-pokt.nodies.app: 33 requests, 40.0 req/s
# [INFO] Total: 100 chunks fetched in 0.5s (200 req/s effective)
```

### Load Testing

```bash
# Stress test: Large block range with adaptive discovery
time beeport-stamp-stats \
  --rpc-url https://rpc1.com \
  --rpc-url https://rpc2.com \
  --rpc-url https://rpc3.com \
  --rpc-url https://rpc4.com \
  --rpc-url https://rpc5.com \
  fetch --from-block 31000000 --to-block 36000000

# Monitor rate limit discovery:
# [INFO] [rpc1] Rate limit: 10.0 → 12.0 req/s (ramp up)
# [INFO] [rpc2] Rate limit: 10.0 → 12.0 req/s (ramp up)
# [WARN] [rpc3] Rate limit hit! 17.3 → 8.6 req/s (back off)
# [INFO] [rpc4] Rate limit: 20.8 → 25.0 req/s (ramp up)
# [INFO] [rpc5] Rate limit: 50.0 → 60.0 req/s (ramp up)
```

---

## Success Criteria

### Functional Requirements
- ✅ Distribute requests round-robin across all RPCs
- ✅ Automatically discover rate limits for each endpoint
- ✅ Maintain each RPC at its maximum safe throughput
- ✅ Manual override for known rate limits
- ✅ Graceful degradation on endpoint failure
- ✅ Backward compatibility (single RPC configs work)

### Performance Requirements
- 🎯 **3-5x faster** fetch operations with 5 RPCs
- 🎯 Adaptive discovery within 60 seconds
- 🎯 <5% overhead for rate limiting logic
- 🎯 Zero requests lost during rate limit adjustment

### Reliability Requirements
- 🎯 Never exceed RPC rate limits after discovery
- 🎯 Automatic recovery from transient 429 errors
- 🎯 Handle endpoint failures gracefully
- 🎯 Maintain request ordering

---

## Recommended RPC Providers for Gnosis Chain

### Free Tier (Add all for maximum throughput)
1. **Gnosis Gateway** - https://rpc.gnosis.gateway.fm (~50 req/s)
2. **Gnosis Official** - https://rpc.gnosischain.com (~30 req/s)
3. **Ankr** - https://rpc.ankr.com/gnosis (~30 req/s)
4. **Nodies** - https://gnosis-pokt.nodies.app (~40 req/s)
5. **1RPC** - https://1rpc.io/gnosis (~20 req/s)

**Combined potential: ~170 req/s** (3.4x faster than single RPC)

---

## Next Steps

1. **Review architecture** - Confirm round-robin + adaptive approach
2. **Approve implementation plan** - Begin Phase 1 development
3. **Identify test RPCs** - Which endpoints to use for testing?
4. **Set adaptive defaults** - Start conservative (10 req/s) or aggressive (50 req/s)?

---

*Last Updated: 2026-01-11 (Revised proposal - parallel distribution)*
