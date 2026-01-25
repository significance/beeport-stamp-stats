# Rate Limiter Redesign - Token Bucket with Automatic Recovery

## Problem Statement

Current adaptive rate limiting implementation has critical flaws:

1. **No minimum floor** - Adaptive strategy can drop to zero through repeated 429 errors, causing deadlock
2. **No recovery mechanism** - Rates don't automatically increase after rate limit windows expire
3. **Inefficient fractional rates** - `ceil()` operation causes rates < 1.0 to behave incorrectly (0.1 req/s becomes 1 req/s)
4. **Slow recovery** - Requires 50 consecutive successful requests to trigger ramp-up (can take 500+ seconds at low rates)

**Recent crashes:**
```
15:27: Rate 50.0 → 429 error → backs off to 25.0 → crashes
15:46: Rate 50.0 → 429 error → backs off to 25.0 → crashes
```

Crashes were due to block timestamp fetching not using `execute_with_retry()` (now fixed), but underlying rate limiting issues remain.

## User Requirements

- **Full redesign** using token bucket algorithm
- **Hybrid recovery**: Reset to floor after cool-down period + gradual increase
- Maintain adaptive discovery of actual rate limits
- Persist discovered limits to database

## Proposed Solution

### Architecture: Token Bucket Rate Limiter

**Why Token Bucket over Sliding Window:**
- Handles fractional rates properly (0.1 req/s = 1 token every 10 seconds)
- Allows burst traffic within limit (accumulate tokens up to bucket size)
- Simpler recovery logic (just adjust refill rate)
- Industry standard for rate limiting

**Token Bucket Parameters:**
```rust
struct TokenBucket {
    tokens: f64,              // Current token count
    capacity: f64,            // Max tokens (bucket size)
    refill_rate: f64,         // Tokens per second
    last_refill: Instant,     // Last refill timestamp
    min_rate: f64,            // Minimum refill rate (floor)
    max_rate: f64,            // Maximum refill rate (ceiling)
}
```

### Rate Adjustment Strategy

**1. Back-off on 429 errors:**
```
new_rate = max(current_rate * back_off_factor, min_rate)
```
- Default: `back_off_factor = 0.5`, `min_rate = 0.5 req/s`
- Never drops below floor

**2. Ramp-up on success:**
```
After N consecutive successes:
  new_rate = min(current_rate * ramp_up_factor, max_rate)
```
- Default: `ramp_up_factor = 1.1`, `ramp_up_threshold = 50`, `max_rate = 50.0 req/s`

**3. Automatic recovery (NEW):**
```
If no 429 errors for cool_down_period (60s):
  - If current_rate < min_healthy_rate (5.0 req/s):
      Reset to min_healthy_rate
  - Else:
      Increase by gradual_recovery_factor (1.05)
```

### Recovery Mechanism Design

**Hybrid approach:**

```rust
struct RecoveryConfig {
    cool_down_period: Duration,        // 60 seconds
    min_healthy_rate: f64,             // 5.0 req/s (reset target)
    gradual_recovery_factor: f64,      // 1.05 (5% increase)
    recovery_check_interval: Duration, // 30 seconds
}
```

**Recovery state machine:**
```
State: NORMAL (rate >= min_healthy_rate)
  - On 429 error → BACKING_OFF
  - On recovery check → no action

State: BACKING_OFF (rate < min_healthy_rate, recent 429)
  - On 429 error → continue back-off
  - After cool_down_period with no 429 → RECOVERING

State: RECOVERING (rate < min_healthy_rate, no recent 429)
  - On recovery check → reset to min_healthy_rate → NORMAL
  - On 429 error → BACKING_OFF

State: GRADUAL_RECOVERY (rate >= min_healthy_rate, no recent 429)
  - On recovery check → increase by gradual_recovery_factor
  - On 429 error → BACKING_OFF
```

**Timeline example:**
```
0s:    Rate = 10 req/s (NORMAL)
10s:   429 error → Rate = 5.0 req/s (BACKING_OFF)
20s:   429 error → Rate = 2.5 req/s (BACKING_OFF)
30s:   429 error → Rate = 1.25 req/s (BACKING_OFF)
40s:   429 error → Rate = 0.625 req/s (floor applied) (BACKING_OFF)
100s:  No 429 for 60s → Rate = 5.0 req/s (RECOVERING → NORMAL)
130s:  Recovery check → Rate = 5.25 req/s (GRADUAL_RECOVERY)
160s:  Recovery check → Rate = 5.51 req/s (GRADUAL_RECOVERY)
...
```

## Implementation Plan

### Phase 1: Create New Token Bucket Rate Limiter

**File:** `src/rate_limiter_v2.rs` (new file)

**Components:**
1. `TokenBucket` struct with proper fractional rate handling
2. `acquire()` method - waits for available tokens
3. `refill()` method - adds tokens based on elapsed time
4. Recovery state machine
5. Background task for periodic recovery checks

**Key methods:**
```rust
impl TokenBucket {
    async fn acquire(&mut self) -> Result<()> {
        loop {
            self.refill();
            if self.tokens >= 1.0 {
                self.tokens -= 1.0;
                return Ok(());
            }
            // Calculate wait time until next token
            let wait = Duration::from_secs_f64(1.0 / self.refill_rate);
            tokio::time::sleep(wait).await;
        }
    }

    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        let new_tokens = elapsed * self.refill_rate;
        self.tokens = (self.tokens + new_tokens).min(self.capacity);
        self.last_refill = now;
    }

    async fn record_rate_limit(&mut self) {
        self.last_rate_limit = Some(Instant::now());
        self.state = RecoveryState::BackingOff;
        let new_rate = (self.refill_rate * self.back_off_factor).max(self.min_rate);
        self.refill_rate = new_rate;
        // Log warning
    }

    async fn record_success(&mut self) {
        self.consecutive_successes += 1;
        if self.consecutive_successes >= self.ramp_up_threshold {
            let new_rate = (self.refill_rate * self.ramp_up_factor).min(self.max_rate);
            self.refill_rate = new_rate;
            self.consecutive_successes = 0;
        }
    }

    async fn check_recovery(&mut self) {
        let time_since_last_429 = self.last_rate_limit
            .map(|t| Instant::now().duration_since(t))
            .unwrap_or(Duration::MAX);

        if time_since_last_429 < self.recovery_config.cool_down_period {
            return; // Still in cool-down
        }

        match self.state {
            RecoveryState::BackingOff if self.refill_rate < self.min_healthy_rate => {
                // Transition to recovering
                self.state = RecoveryState::Recovering;
                self.refill_rate = self.min_healthy_rate;
                // Log recovery
            }
            RecoveryState::Recovering | RecoveryState::Normal => {
                // Gradual increase
                let new_rate = (self.refill_rate * self.gradual_recovery_factor).min(self.max_rate);
                if new_rate > self.refill_rate {
                    self.refill_rate = new_rate;
                    self.state = RecoveryState::GradualRecovery;
                }
            }
            _ => {}
        }
    }
}
```

### Phase 2: Integrate Recovery Background Task

**File:** `src/rate_limiter_v2.rs`

**Background task:**
```rust
pub async fn spawn_recovery_task(rate_limiter: Arc<RwLock<TokenBucket>>) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(
            Duration::from_secs(30) // recovery_check_interval
        );

        loop {
            interval.tick().await;
            let mut limiter = rate_limiter.write().await;
            limiter.check_recovery().await;
        }
    })
}
```

**Lifecycle:**
- Spawn recovery task when RpcEndpoint is created
- Task runs every 30 seconds
- Checks if recovery conditions are met
- Adjusts rates accordingly

### Phase 3: Update RPC Scheduler

**File:** `src/rpc_scheduler.rs`

**Changes:**
1. Replace `AdaptiveRateLimiter` with `TokenBucket` in `RpcEndpoint`
2. Update configuration loading
3. Spawn recovery tasks for each endpoint
4. Store task handles for cleanup

**Modified struct:**
```rust
struct RpcEndpoint {
    provider: RootProvider<Http<Client>>,
    rate_limiter: Arc<RwLock<TokenBucket>>,  // Changed type
    recovery_task: JoinHandle<()>,            // New field
    url: String,
    priority: u8,
    weight: u32,
}
```

### Phase 4: Update Configuration

**File:** `src/config.rs`

**Add recovery configuration:**
```rust
#[derive(Debug, Clone, Deserialize)]
pub struct RecoveryConfig {
    #[serde(default = "default_cool_down_period")]
    pub cool_down_period_secs: u64,  // 60

    #[serde(default = "default_min_healthy_rate")]
    pub min_healthy_rate: f64,  // 5.0

    #[serde(default = "default_gradual_recovery_factor")]
    pub gradual_recovery_factor: f64,  // 1.05

    #[serde(default = "default_recovery_check_interval")]
    pub recovery_check_interval_secs: u64,  // 30
}
```

**File:** `config.yaml`

**Add to rate_limiting section:**
```yaml
rate_limiting:
  adaptive:
    start_rps: 2.0
    max_rps: 50.0
    min_rps: 0.5          # NEW: minimum floor
    ramp_up_factor: 1.1
    back_off_factor: 0.5
    ramp_up_threshold: 50

  recovery:              # NEW section
    cool_down_period_secs: 60
    min_healthy_rate: 5.0
    gradual_recovery_factor: 1.05
    recovery_check_interval_secs: 30
```

### Phase 5: Migration Strategy

**Backward compatibility:**
1. Keep old `AdaptiveRateLimiter` in `src/rate_limiter.rs` (deprecated)
2. New `TokenBucket` in `src/rate_limiter_v2.rs`
3. Update `RpcScheduler` to use new implementation
4. Database schema unchanged (still stores `discovered_rate_limit`)

**Testing strategy:**
1. Unit tests for token bucket refill logic
2. Unit tests for recovery state machine
3. Integration test: simulate 429 errors and verify recovery
4. Live test: run fetch command with new implementation

### Phase 6: Database Persistence

**No schema changes needed** - continue using existing `rpc_rate_limits` table.

**Persistence points:**
1. After successful recovery (rate increased)
2. After discovering new limit (rate decreased)
3. Periodically (every 5 minutes)

## Critical Files

| File | Changes | Lines (est) |
|------|---------|-------------|
| `src/rate_limiter_v2.rs` | NEW - Token bucket implementation | ~500 |
| `src/rpc_scheduler.rs` | Update to use TokenBucket | ~50 |
| `src/config.rs` | Add RecoveryConfig | ~30 |
| `config.yaml` | Add min_rps and recovery section | ~10 |
| `src/rate_limiter.rs` | Add deprecation notice (optional) | ~5 |

## Success Criteria

1. **Minimum floor enforced** - Rate never drops below `min_rps` (0.5 req/s)
2. **Automatic recovery** - After 60s with no 429s, rate resets to `min_healthy_rate` (5.0 req/s)
3. **Gradual increase** - Rates increase by 5% every 30s during recovery phase
4. **Proper fractional rates** - 0.1 req/s behaves as 1 request per 10 seconds (not 1 req/s)
5. **No deadlocks** - Process never blocks indefinitely
6. **Database persistence** - Discovered limits saved and restored across restarts

## Testing Plan

### Unit Tests
```rust
#[tokio::test]
async fn test_token_bucket_fractional_rate() {
    let mut bucket = TokenBucket::new(0.1, 1.0, 0.1, 50.0);
    // Should allow 1 request, then wait ~10 seconds
}

#[tokio::test]
async fn test_recovery_after_cool_down() {
    let mut bucket = TokenBucket::new(0.5, 1.0, 0.5, 50.0);
    bucket.record_rate_limit().await;
    assert_eq!(bucket.refill_rate, 0.5); // Hit floor

    // Simulate 60s passing
    tokio::time::sleep(Duration::from_secs(60)).await;
    bucket.check_recovery().await;
    assert_eq!(bucket.refill_rate, 5.0); // Reset to min_healthy_rate
}

#[tokio::test]
async fn test_gradual_recovery() {
    let mut bucket = TokenBucket::new(5.0, 1.0, 0.5, 50.0);
    bucket.check_recovery().await;
    assert_eq!(bucket.refill_rate, 5.25); // 5% increase
}
```

### Integration Test
```bash
# Test with actual RPC endpoints
cargo test --test recovery_integration -- --nocapture

# Live test with small block range
./target/release/beeport-stamp-stats fetch --from-block 31306385 --to-block 31307385
```

## Rollout Plan

1. **Develop and test locally** - Unit tests pass
2. **Integration test** - Small block range fetch completes without crashes
3. **Deploy with monitoring** - Full fetch with 60-minute monitoring
4. **Verify recovery** - Check logs for automatic rate increases after 429s
5. **Monitor database** - Verify discovered limits are persisted

## Risks and Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| Token bucket has bugs | HIGH - Could deadlock or crash | Thorough unit tests, fallback to old implementation |
| Recovery too aggressive | MEDIUM - Hit rate limits again | Conservative defaults (60s cool-down, 5% increases) |
| Recovery too slow | LOW - Slow performance | Configurable via config.yaml |
| Background tasks leak memory | MEDIUM - Process grows over time | Proper cleanup on shutdown, monitoring |
| Breaking change for users | LOW - Internal implementation | No API changes, backward compatible config |

## Alternative Considered: Quick Fix

**If full redesign is too complex, quick fix option:**
1. Add `min_rps: 0.5` to Adaptive strategy (5 lines)
2. Add recovery check in `record_success()` (10 lines)
3. Total: ~15 lines changed

**Pros:** Fast, low risk
**Cons:** Doesn't fix fractional rate issues, less robust recovery

**Decision:** User chose full redesign for long-term robustness.

---

*Plan created: 2026-01-21*
*Status: Ready for implementation*
