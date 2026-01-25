# Beeport TX Stats - Project Plan

**Last Updated:** 2026-01-25 20:41 UTC (comprehensive testing completed)

---

## 📍 Current Status

**Branch:** `feat/improve-retrieval-efficiency-with-bandwidth`
**Goal:** Improve retrieval efficiency for postage batch data collection
**Database:** `beeport_bandwidth_testing` (PostgreSQL, for testing)

**Project State:** ✅ Comprehensive Testing Complete

**Testing Session (2026-01-25 20:41 UTC):**
- ✅ 86 unit tests passing
- ✅ Zero clippy warnings
- ✅ Fixed 3 failing tests (config defaults, constant values, fractional rate test)
- ✅ Fixed `u64::MAX` overflow bug in SQL queries (caused empty results)
- ✅ Fixed duplicate `#[allow(dead_code)]` attributes in rate_limiter.rs
- ✅ Payment channel commands tested and working
- ✅ Parallel chequebook syncing operational

**Bug Fixes This Session:**
- Fixed `unwrap_or(u64::MAX)` to `unwrap_or(i64::MAX as u64)` - prevents SQL query failures due to i64 overflow
- Fixed test constants to match actual deployment block values
- Fixed fractional rate test (capacity was 2.0, should be 1.0)
- Removed duplicate `#[allow(dead_code)]` attributes

**Recent Work:** Implemented token bucket rate limiter with automatic recovery (2026-01-25)
- ✅ New `rate_limiter_v2.rs` with token bucket algorithm
- ✅ Automatic recovery after cool-down period (no more deadlocks at 0 req/s)
- ✅ Cached rate limits persist to database (`rpc_rate_limits` table)
- ✅ Background recovery tasks for gradual rate increase
- ✅ Fixed PostgreSQL migration to use DOUBLE PRECISION (f64 compatible)
- ✅ Tested with local RPC and parallel chequebook syncing

**Previous Work:** Parallel RPC execution (2026-01-19)
- ✅ Identified problem: execute_many() existed but was never used
- ✅ Refactored blockchain client to use three-phase approach (collect, fetch parallel, process)
- ✅ Verified 4 RPCs fetch 4 chunks truly in parallel (not sequentially)
- ✅ 100% backward compatibility with single RPC mode
- ✅ Organized documentation into docs/ folder

**Key Files Changed:**
- `src/rate_limiter_v2.rs` - New token bucket implementation
- `src/rpc_scheduler.rs` - Uses new rate limiter
- `migrations_postgres/20260119000009_add_rpc_rate_limits_table.sql` - Rate limit persistence

**Next Steps:**
1. Run extended fetch operation to verify stability
2. Monitor rate limiter behavior under 429 errors
3. Verify recovery mechanism works as expected

All core features implemented and tested:
- ✅ Postage stamp events tracking (PostageStamp, StampsRegistry contracts)
- ✅ Storage incentives tracking (PriceOracle, StakeRegistry, Redistribution contracts)
- ✅ Database migrations (SQLite + PostgreSQL)
- ✅ CLI commands (fetch, sync, follow, summary, batch-status, expiry-analytics, export)
- ✅ Retry logic with exponential backoff (HTTP 429 + 502)
- ✅ 132 tests passing, zero clippy warnings
- ✅ 100% blockchain data accuracy verified against GnosisScan
- ✅ Visualization: plots/plot.py includes PotWithdrawn events

**Ready for:** Data collection, analysis, production deployment

---

## 🎯 Project Capabilities

### Data Collection
- **5 Smart Contracts** tracked simultaneously on Gnosis Chain
- **35+ Event Types** captured across postage stamps and storage incentives
- **Incremental syncing** with block caching and resume support
- **Follow mode** for continuous real-time monitoring
- **Resilient RPC handling** with automatic retry on rate limits and gateway errors

### Analytics & Reporting
- **Batch status** with TTL calculations and price modeling
- **Expiry analytics** grouped by day/week/month
- **Event summaries** with contract and event type filtering
- **Export capabilities** (JSON, CSV formats)
- **Price history** from PriceOracle events
- **Redistribution game** tracking (commits, reveals, winners)
- **Staking dynamics** (updates, slashes, freezes, withdrawals)

### Database
- **Two table design:**
  - `stamp_events` - Postage stamp events (BatchCreated, TopUp, etc.)
  - `storage_incentives_events` - Storage incentives events (PriceUpdate, Revealed, etc.)
- **Multi-database support:** SQLite (local), PostgreSQL (production)
- **Optimized indexes** for common query patterns
- **Deduplication** via unique constraints on (transaction_hash, log_index)

---

## 📋 Contract Reference

### Tracked Contracts (Gnosis Chain Mainnet)

| Contract | Address | Deployment Block | Events |
|----------|---------|------------------|--------|
| **PostageStamp** (v1) | `0x6a1A21ECA3aB28BE85C7Ba22b2d6eAE5907c900e` | 20,685,974 | 9 types |
| **StampsRegistry** (v2) | `0xCfC2FfF779E572990304bc5Da857087a3e576dd0` | 28,165,570 | 10 types |
| **PriceOracle** | `0x47EeF336e7fE5bED98499A4696bce8f28c1B0a8b` | 37,339,168 | 2 types |
| **StakeRegistry** | `0xda2a16EE889E7f04980A8d597b48c8D51B9518F4` | 40,430,237 | 5 types |
| **Redistribution** | `0x5069cdfB3D9E56d23B1cAeE83CE6109A7E4fd62d` | 41,105,199 | 11 types |

### Postage Stamp Events (19 total)
- BatchCreated, BatchTopUp, BatchDepthIncrease
- PriceUpdate (contract-level price changes)
- PotWithdrawn (admin withdrawal)
- CopyBatchFailed (batch copy errors)

### Storage Incentives Events (18 total)

**PriceOracle (2):**
- `PriceUpdate` - Storage price adjustments (every 152 blocks = 1 round)
- `StampPriceUpdateFailed` - Failed price update attempts

**StakeRegistry (5):**
- `StakeUpdated` - Node stake changes (committed/potential amounts)
- `StakeSlashed` - Penalty for misbehavior
- `StakeFrozen` - Temporary freeze after freeze event
- `OverlayChanged` - Node overlay address updates
- `StakeWithdrawn` - Stake removal

**Redistribution (11):**
- `Committed` - Round participation commitment
- `Revealed` - Reveal phase submission
- `WinnerSelected` - Round winner announcement (nested Reveal struct)
- `TruthSelected` - Consensus truth hash
- `CurrentRevealAnchor` - Current round anchor
- `CountCommits` / `CountReveals` / `ChunkCount` - Round statistics
- `PriceAdjustmentSkipped` - Redundancy-based skip
- `WithdrawFailed` - Failed reward withdrawal
- `transformedChunkAddressFromInclusionProof` - Proof verification

---

## 🔧 Technical Notes

### Key Implementation Patterns

**1. Dual Contract Registry System**
- `ContractRegistry` - Handles postage stamp contracts (PostageStamp, StampsRegistry)
- `StorageIncentivesContractRegistry` - Handles storage incentives (PriceOracle, StakeRegistry, Redistribution)
- Allows different event structures and parsing logic per domain

**2. Retry Strategy (Two-Phase)**
```
Phase 1: Exponential backoff (fast retry)
  delay = initial_delay_ms * backoff_multiplier^retry_count
  Example: 100ms → 400ms → 1600ms → 6400ms → 25600ms
  Retries: up to max_retries (default: 5)
  Triggers: HTTP 429, HTTP 502, "Too Many Requests", "Bad Gateway"

Phase 2: Extended retry (when Phase 1 exhausted)
  delay = extended_retry_wait_seconds (default: 300s / 5 min)
  Resets Phase 1 counter
  Continues indefinitely until success
```

**3. Round & Phase Calculations**
```rust
// Round number (152 blocks = 1 round, ~12.6 minutes)
round_number = block_number / 152

// Phase within round (redistribution game timing)
position = block_number % 152
phase = if position < 38 { "commit" }
        else if position < 76 { "reveal" }
        else { "claim" }
```

**4. WinnerSelected Event Handling**
The `WinnerSelected` event emits a nested `Reveal` struct:
```rust
struct Reveal {
    bytes32 overlay;
    address owner;
    uint8 depth;
    uint256 stake;
    uint256 stakeDensity;
    bytes32 hash;
}
```
Decoded using Alloy's tuple support, fields extracted to database columns.

**5. Nullable Field Pattern**
Single `storage_incentives_events` table supports 18 event types using `Option<T>`:
- Core fields (block_number, contract_source, event_type) always present
- Event-specific fields nullable (price, stake, overlay, winner_*, etc.)
- Database queries filter by `event_type` to get relevant fields

---

## 💡 Example Queries

### Price History Over Time
```sql
SELECT round_number, price, block_timestamp
FROM storage_incentives_events
WHERE event_type = 'PriceUpdate'
ORDER BY block_number;
```

### Top Redistribution Winners
```sql
SELECT winner_owner, COUNT(*) as wins,
       AVG(CAST(winner_stake AS REAL)) as avg_stake
FROM storage_incentives_events
WHERE event_type = 'WinnerSelected'
GROUP BY winner_owner
ORDER BY wins DESC
LIMIT 10;
```

### Staking Activity by Node
```sql
SELECT overlay, owner_address,
       SUM(CASE WHEN event_type = 'StakeUpdated' THEN 1 ELSE 0 END) as updates,
       SUM(CASE WHEN event_type = 'StakeFrozen' THEN 1 ELSE 0 END) as freezes,
       SUM(CASE WHEN event_type = 'StakeSlashed' THEN 1 ELSE 0 END) as slashes
FROM storage_incentives_events
WHERE contract_source = 'StakeRegistry'
GROUP BY overlay, owner_address;
```

### Redistribution Round Statistics
```sql
SELECT round_number,
       MAX(CASE WHEN event_type = 'CountCommits' THEN commit_count END) as commits,
       MAX(CASE WHEN event_type = 'CountReveals' THEN reveal_count END) as reveals,
       MAX(CASE WHEN event_type = 'ChunkCount' THEN chunk_count END) as chunks
FROM storage_incentives_events
WHERE contract_source = 'Redistribution'
GROUP BY round_number
ORDER BY round_number DESC;
```

### Active Batches with TTL
```sql
SELECT batch_id,
       owner,
       depth,
       normalised_balance / (storage_price * POW(2, depth + 16)) as ttl_blocks,
       (normalised_balance / (storage_price * POW(2, depth + 16)) * 5.0) / 86400.0 as ttl_days
FROM stamp_events
WHERE event_type = 'BatchCreated'
  AND normalised_balance > 0;
```

---

## 🚀 Future Enhancements

### Analytics Commands (Future Work)
- `price-history` - Chart price changes with visualization
- `redistribution-rounds` - Round-by-round game analysis
- `staking-activity` - Comprehensive staking report
- `node-performance` - Track specific node's participation
- `cross-contract-analysis` - Correlate price, staking, and redistribution

### Features
- **Contract filtering** - `--contract-filter` CLI flag for selective fetching
- **Event hooks** - Custom callbacks for specific events
- **GraphQL API** - Query interface for external tools
- **Web dashboard** - Real-time monitoring UI
- **Alert system** - Notifications for critical events (slashes, freezes, etc.)

---

## 📚 Testing Strategy

### Bandwidth Testing Configuration

**Use `bandwidth-test-config.yaml` for all bandwidth-related testing:**

```bash
# Run commands with bandwidth test config
./target/release/beeport-stamp-stats --config bandwidth-test-config.yaml <command>

# Example: Sync chequebooks
./target/release/beeport-stamp-stats --config bandwidth-test-config.yaml sync-chequebooks
```

**Config Details:**
- **Database:** `postgresql://localhost/beeport_bandwidth`
- **Payment Channel Factory:** SimpleSwapFactory-Gnosis at `0xc2d5a532cf69aa9a1378737d8ccdef884b6e7420`
- **RPC:** `http://localhost:8545` (use local node or update for testing)

**When to use:**
- Testing payment channel / chequebook syncing
- Testing multi-RPC bandwidth optimizations
- Testing rate limiter behavior under load
- Any work on the `feat/improve-retrieval-efficiency-with-bandwidth` branch

### Database Convention
**IMPORTANT:** Always use PostgreSQL database `beeport_bandwidth_testing`

- **Database name:** `beeport_bandwidth_testing` (ONLY database to use)
- **Never delete:** Always ASK user for confirmation before any DROP DATABASE operations
- **Backup first:** If user confirms deletion, suggest backing up first

**User Confirmation Required Before:**
1. **ANY DROP DATABASE operation** - ALWAYS ask first, suggest backup
2. Deleting or truncating data from `beeport_bandwidth_testing`
3. Any destructive operations on the database

**Standard usage:**
```bash
# Normal operation - always use beeport_bandwidth_testing
./target/release/beeport-stamp-stats fetch
./target/release/beeport-stamp-stats sync

# Database is configured in config.yaml:
database:
  path: "postgresql://localhost/beeport_bandwidth_testing"
```

### Verification Checklist
When making significant changes:
1. ✅ Unit tests (`cargo test`)
2. ✅ Clippy warnings (`cargo clippy -- -D warnings`)
3. ✅ Fetch command (small block range)
4. ✅ Blockchain verification (compare with GnosisScan)
5. ✅ Sync command (incremental updates)
6. ✅ Summary command with filters
7. ✅ Batch status (all output formats)
8. ✅ Expiry analytics (all periods)
9. ✅ Export (JSON + CSV)
10. ✅ Follow mode (brief background run)
11. ✅ Configuration system (file, env vars, CLI args priority)
12. ✅ Price calculations (manual verification)

---

## 🚧 In Progress: Multi-RPC System (2026-01-11)

**Goal:** Improve data retrieval speed by distributing requests across multiple RPC endpoints in parallel.

### Architecture

**Round-robin distribution:** Request #1 → RPC1, Request #2 → RPC2, Request #3 → RPC3, etc.

**Adaptive rate limiting:** Each RPC has its own rate limiter that:
- Starts with conservative default (10 req/s) or loads cached limit from database
- Ramps up on consecutive successes (1.2x multiplier after 100 successes)
- Backs off immediately on rate limit errors (0.5x multiplier)
- Persists discovered limits to database for next session

**Three strategies:**
1. **Manual** - Fixed rate, never changes
2. **Adaptive** - Start conservative (10 req/s), ramp to max (1000 req/s)
3. **Aggressive** - Start high (100 req/s), back off to min (1 req/s)

### Implementation Summary

**New Files:**
- `src/rate_limiter.rs` (350+ lines) - Sliding window rate limiter
- `src/rpc_scheduler.rs` (200+ lines) - Round-robin scheduler

**Database Migrations:**
- `migrations_postgres/20260111000008_add_rpc_rate_limits_table.sql`
- `migrations_sqlite/20260111000008_add_rpc_rate_limits_table.sql`
- Table stores: discovered_rate_limit, strategy, statistics, timestamps

**Modified Files:**
- `src/config.rs` - Added multi-RPC configuration support (RpcConfig enum)
- `src/cache.rs` - Added rate limit persistence methods
- `src/cli.rs` - Integrated scheduler, auto-detects multi-RPC mode
- `src/main.rs` + `src/lib.rs` - Module declarations

**Configuration Example:**
```yaml
rpc:
  endpoints:
    - url: "https://gnosis.example1.com"
      rate_limit: adaptive  # or aggressive, or manual: 50
      priority: 1
      weight: 1
    - url: "https://gnosis.example2.com"
      rate_limit: adaptive
      priority: 1
      weight: 1

rate_limiting:
  max_concurrent_requests: 100
  adaptive:
    start_rps: 10
    max_rps: 1000
    ramp_up_factor: 1.2
    back_off_factor: 0.5
    ramp_up_threshold: 100
  aggressive:
    start_rps: 100
    min_rps: 1
    back_off_factor: 0.5
```

**Expected Benefits:**
- 4-5x faster fetching with 5 RPCs (estimated 200+ req/s vs 50 req/s)
- Instant failover on rate limit errors
- Smart initialization from cached limits
- Zero re-discovery time on restart

### Testing Plan (Phase 3)

- [ ] Create multi-RPC test configuration
- [ ] Test with 2-3 Gnosis Chain public RPCs
- [ ] Verify rate limit discovery works
- [ ] Measure actual performance improvements
- [ ] Test database persistence across restarts
- [ ] Verify backward compatibility (single RPC still works)
- [ ] Update documentation with examples

---

## 📦 Completed Work Archive

### ✅ Multi-RPC System Implementation (2026-01-12)
**Goal:** Improve data retrieval speed by distributing requests across multiple RPC endpoints in parallel.

**Implementation:**
- Created `src/rate_limiter.rs` (350+ lines) - Sliding window rate limiter with adaptive discovery
- Created `src/rpc_scheduler.rs` (200+ lines) - Round-robin scheduler with per-endpoint rate limiting
- Added database migrations for rate limit persistence (PostgreSQL + SQLite)
- Modified `BlockchainClient` to optionally use scheduler for get_logs requests
- Modified `src/config.rs` - RpcConfig changed to struct with Option fields for config merging
- Created `RPC.md` with 15+ public Gnosis Chain RPC endpoints

**Features:**
- Round-robin request distribution across multiple RPCs
- Adaptive rate limiting per endpoint (starts at 10 req/s, ramps up to discovered limit)
- Rate limit persistence across sessions (stored in database)
- Three strategies: Manual (fixed), Adaptive (ramp up), Aggressive (start high)
- Per-endpoint statistics tracking (requests, errors, measured throughput)
- Automatic multi-RPC mode detection when >1 endpoint configured
- Full backward compatibility with single RPC mode

**Testing Results:**
- ✅ Perfect round-robin distribution verified (4 RPCs, 1 request each)
- ✅ Fetched 17 postage stamp events + 7 storage incentives events
- ✅ Per-endpoint statistics working correctly
- ✅ Backward compatibility confirmed (single RPC still works)
- ✅ Auto-detection working (switches modes based on config)

**Result:** Multi-RPC system fully integrated and tested. Provides N× potential throughput with N endpoints.

### ✅ Storage Incentives Integration (2025-12-20)
Implemented support for PriceOracle, StakeRegistry, and Redistribution contracts:
- Database schema with `storage_incentives_events` table
- Contract ABIs using Alloy's `sol!` macro (420 lines)
- Event parsers for 18 event types (1000+ lines)
- StorageIncentivesContract trait with 3 implementations
- CLI integration for simultaneous fetching of all 5 contracts
- 100% blockchain data accuracy verified against GnosisScan

**Result:** Tool now tracks complete storage incentives ecosystem.

### ✅ HTTP 502 Retry Support (2026-01-02)
Added retry logic for HTTP 502 Bad Gateway errors:
- Updated `src/retry.rs` to handle both 429 and 502 errors
- Fixed test compilation errors (batch_id: String → Option<String>)
- Updated 5 test files to wrap batch_id in Some()
- 132 tests passing, zero clippy warnings

**Result:** More resilient RPC operations during gateway issues.

### ✅ PotWithdrawn, PriceUpdate, CopyBatchFailed Events
Added support for additional postage stamp events:
- PotWithdrawn (admin pot withdrawal)
- PriceUpdate (contract-level price changes)
- CopyBatchFailed (batch copy errors)
- Database columns: pot_withdrawn_amount, price_update_value, copy_batch_failed_batch_id
- batch_id changed to Option<String> (some events don't have batch IDs)

**Result:** Complete coverage of PostageStamp and StampsRegistry contract events.

---

## 🔗 Resources

- **Contract Source Code:** `/Users/sig32/Code/swarm2/storage-incentives/src/`
- **Deployment Info:** `/Users/sig32/Code/swarm2/storage-incentives/deployments/mainnet/`
- **Alloy Documentation:** https://alloy.rs/
- **GnosisScan Explorer:** https://gnosisscan.io/
- **Architecture Guide:** See `CLAUDE.md` in project root

---

## 📝 Notes for New Sessions

If starting a new session:

1. **Check git status** - See what's modified
2. **Read this plan** - Understand current state
3. **Review CLAUDE.md** - Architecture and development philosophy
4. **Run tests** - Verify everything still works (`cargo test`)
5. **Check database** - Know which database you're working with

**Common Operations:**
```bash
# Fetch events for a block range
./target/release/beeport-stamp-stats \
  fetch --from-block 41105199 --to-block 41106199

# Follow mode (real-time monitoring)
./target/release/beeport-stamp-stats follow --poll-interval 10

# Export all events
./target/release/beeport-stamp-stats export \
  --output events.json --format json
```

**Key Files:**
- `src/contracts/` - Contract definitions, ABIs, parsers
- `src/blockchain.rs` - RPC client and event fetching
- `src/cache.rs` - Database operations
- `src/cli.rs` - CLI orchestration
- `src/retry.rs` - Retry logic with exponential backoff
- `migrations/` - SQLite schema
- `migrations_postgres/` - PostgreSQL schema

---

*This plan is maintained alongside code changes. Update when architectural decisions are made or major features are added.*
