# Beeport TX Stats - Project Plan

**Last Updated:** 2026-10-15 UTC
**See also:** `state.md` for a verified snapshot of git, build, config, and database state. Read it first in a new session.

---

## 📍 Current Status

**Branch:** `feat/improve-retrieval-efficiency-with-bandwidth`
**Goal:** Fix expiry-analytics command to show stamp expiry predictions
**Database:** `beeport_bandwidth` (PostgreSQL)

**Build Status:** ✅ All systems go
- 87 unit tests passing (added Hour period test)
- Zero clippy warnings
- Release binary built

**Project State:** ✅ Phases 1 & 2 Complete - Expiry Analytics Fixed

**2026-10-12 11:30 UTC — session resumed after ~8 months (Claude transcripts for this project were deleted; repo notes are the only history):**
- Working tree: Phase 1 + 2 expiry-analytics changes (`src/cli.rs`, `src/commands/expiry_analytics.rs`, this file) are still UNCOMMITTED on `feat/improve-retrieval-efficiency-with-bandwidth`, 25 commits ahead of `main`, nothing behind.
- `cargo test` / `cargo clippy` could not run: "No space left on device". Root volume ~3 GB free; `target/` here is 1.6 GB, `~/Library/Caches` 20 GB, duplicate `.sql` dumps ~2.5 GB across beeport-tx-stats and -multi2. Nothing deleted — user's call.
- Postgres is up with `beeport3m`, `beeport5`, `beeport6`, `beeport_bandwidth`, `beeport_bandwidth_testing`, `beeport_tx_prod`. `config.yaml` points at `beeport3m`; bandwidth work uses `bandwidth-test-config.yaml` → `beeport_bandwidth`.
- Next: free disk, rebuild, confirm 87 tests + clippy clean, then commit Phase 1/2 and decide on Phase 3 (parallel balance refresh) and the ARCHITECTURE_REVIEW.md cleanup list.

**2026-10-12 12:05 UTC — build green again:**
- User freed disk (35 GB free). The ENOSPC failure had corrupted `target/debug/incremental` ("cached cgu … should have an object file"); fix was `rm -rf target/debug/incremental`, then rebuild. Debug note: an out-of-disk mid-compile leaves a broken incremental cache; clear that directory before anything else.
- Toolchain is now rustc 1.96.0-nightly (2026-03-15); newer clippy lints flagged 11 pre-existing sites. Fixed: 4× `% 3 == 0` → `is_multiple_of(3)` (thousands-separator helpers in cache.rs, cli.rs, batch_status.rs, expiry_analytics.rs), 6× `sort_by` → `sort_by_key` (cli.rs, batch_status.rs, expiry_analytics.rs), 1× `is_some()`+`unwrap()` → `if let` (cli.rs factory filter). Behaviour unchanged.
- `cargo test`: 87 passed. `cargo clippy -- -D warnings`: clean. `cargo build --release`: OK.
- Known, not fixed: `cargo clippy --all-targets` flags "items after a test module" in `src/contracts/impls.rs:185` (pre-existing; move the `mod tests` block to the end of the file when touching that file).
- All of the above plus the Phase 1/2 expiry-analytics changes remain UNCOMMITTED, awaiting the user's go-ahead.

**2026-10-15 09:00 UTC — first real run in 8 months; sync bug found and fixed:**
- `sync` with no `--to-block` walked chunks to block 9.2e18 ("Chunk n/4611686018405256"). Cause: commit
  195ca6a (2026-01-25) changed the default from `u64::MAX` to `i64::MAX as u64` to keep SQL i64 bindings
  happy, but `blockchain.rs` only treats `u64::MAX` as "resolve to chain head", so the new value was taken
  literally. Same bug in factory discovery and chequebook sync. Fix: all three now call
  `client.get_current_block()` once up front and pass a real block number (no sentinel at all).
  The aborted run wrote nothing to the database (verified: max block and rpc_cache unchanged).
- Added `config-prod.yaml`: same as `config.yaml` but database `beeport_tx_prod` (the one with price
  history), no `localhost:8545` endpoint (no local node), chunk_size 2000. Running `migrations` against
  prod applied 20260119000009 and 20260121000009 (new empty tables only).
- Sync of `beeport_tx_prod` from 44,264,608 to chain head 48,766,631 started 08:45 UTC over three
  public RPCs; log in the session scratchpad `sync-prod.log`.
- 09:16 restarted with a 4th endpoint (gateway.fm) from 45,002,566. 09:40 restarted again from
  45,408,558 after finding `config.yaml` had a dead StampsRegistry address (`0xCfC2…`, no code on
  chain); corrected to `0x5EBf…` / 42,390,510 in `config.yaml` and `config-prod.yaml`.
  **TODO after the sync finishes:** backfill StampsRegistry for 44,264,608–45,408,557 with
  `fetch --from-block 44264608 --to-block 45408557` (PostageStamp chunks are in `rpc_cache` and skip).
- 10:08 sync died at block 45,780,551: one endpoint returned HTTP 503 "connection termination" for
  ~30 s. `rpc_scheduler::is_network_error` matched none of those strings, so the error was treated
  as permanent, and even an all-endpoints-transient outcome returned immediately. Fixed: both
  `retry::is_transient_error` (new, single-RPC path) and the scheduler classifier now cover
  502/503/504, "connection termination/failure", "upstream connect error", "disconnect/reset";
  the scheduler's outer loop retries any all-transient round with backoff 3s→60s, up to 10 times.
  Unit test added. 10:40 restarted from 45,780,552.
  **TODO after sync:** 17 txs in blocks 45,780,202–45,780,551 were stored with `from_address` NULL
  during the outage; backfill them (`SELECT transaction_hash FROM events WHERE from_address IS NULL
  AND block_number > 44264607`).
- 15:16 stamp sync COMPLETE: `events` at 48,768,672, 84,438 new events, price cached 187,439.
  Then found `sync` never fetched the storage-incentives contracts (only `fetch` did), so
  `storage_incentives_events` was still at 44,267,041. Fixed: `execute_sync` now runs the SI pass
  too, resuming from `get_last_storage_incentives_block()` (new cache helper). `follow` still
  stamps-only (documented as a limitation). 88 tests, clippy clean, release rebuilt.
- 16:04 started step 1 of the catch-up: `fetch --from-block 44264608 --to-block 45408557`
  (StampsRegistry backfill + SI for that range). Step 2 after it finishes: plain `sync`, which
  will carry SI from 45,408,558 to head. Then re-render the graph and backfill the 17 NULL senders.
- 16:19 backfill fetch finished: StampsRegistry gap filled (274 rows now) but "0 storage
  incentives events" in 30 s. ROOT CAUSE of the frozen price history: commit 460c9dd (2026-01-25)
  rewrote `config.yaml` and moved PriceOracle/StakeRegistry/Redistribution under a new
  `storage_incentives_contracts:` key that `AppConfig` never had, so serde dropped it and the
  storage-incentives registry was empty from that day on. Fixed: `AppConfig` now has the field
  (`#[serde(default)]`), `all_contracts()` chains both lists, the SI registry and validation use
  it. Test added (89 pass). The same commit also dropped the historical contract versions
  (Redistribution v0.8.6–v0.9.3, StakeRegistry v0.9.2, PostageStamp-Phase4, PriceOracle
  0x86de…) from the config; their data is still in the database, nothing fetches them now,
  which is fine because they are dead.
- 16:25 `sync` running with the fixed binary: stamps topped up (+262), storage incentives from
  44,267,042 → 48,772,053 (PriceOracle first). Several hours. Uncommitted: config.rs,
  contracts/mod.rs, this file, state.md.
- 16:33 that sync died: Postgres "No space left on device" (disk briefly hit 0; 20 GB free an
  hour later, so macOS purgeable space). It had finished PriceOracle to 47.97M only. 16:35
  restarted; 17:17 COMPLETE at head (91,436 SI events, price 186,952) — but because the resume
  point was the table-wide max, StakeRegistry and Redistribution were fetched only from
  47,969,000, leaving a 3.7M-block hole (44.27M–47.97M) for those two.
- Fixed: resume point is now per contract (`get_last_storage_incentives_block_for(address)`),
  `execute_sync` loops contracts individually via `fetch_storage_incentives_contract_events`
  (now `pub(crate)`). Falls back to the contract's deployment block when it has no rows.
  89 tests, clippy clean, release rebuilt. ~17:25 sync started to fill the hole.
  Note: `target/` is 5 GB (4.1 GB debug); the first SI pass over 3.7M blocks took ~8 min at
  4×50 req/s with zero errors, so the earlier "hours" estimate was far too pessimistic for the
  SI contracts; Redistribution is the dense one.
- 19:21 `sync --contract` implemented (was parsed and ignored): a storage-incentives name syncs
  that contract alone, a stamp name runs the stamp pass, unknown names are rejected. Used it to
  fill the hole: StakeRegistry 44,251,993–47,968,999 done 19:28 (1,152 events, 7 min);
  Redistribution 44,264,608–47,968,999 running (~27k blocks/min, ~2 h).
- 19:35 the 17 txs with NULL `from_address` from the 503 outage were backfilled with a one-off
  script (`eth_getTransactionByHash` → `UPDATE events`). Zero NULL senders remain above 44.26M.
  Debug note: publicnode returns 403 to Python's default user-agent; send a curl-like one.
- README rewritten to match this branch (commands, options, config schema, contract table, tables).
- `plots/plot.py` verified working (headless render against `beeport_tx_prod`, ~15 s). It was the
  source of the ".env" memory: it reads `DB_NAME`/`DB_USER`/… via python-dotenv and defaulted to the
  vanished `beeport2`; default now `beeport_tx_prod`. Added `plots/README.md`, `.env.example`,
  `requirements.txt`, and a Plotting section in the main README. TODO: re-render the checked-in
  `incentives_hero_dashboard.png` after the sync completes. Debug tip: the "Chunk n/N" burst at start
  is the plan phase, not fetches; progress is best read from `SELECT max(block_number) FROM events`.

**2026-10-12 12:30 UTC — env + data inventory (verified against Postgres):**
- There is NO `.env` file and never was one in git. Config is YAML (`config.yaml` → `beeport3m`; `bandwidth-config.yaml` → `beeport_bandwidth`; `bandwidth-test-config.yaml` → `beeport_bandwidth_testing`), overridable with `BEEPORT__SECTION__KEY` env vars. `rpc-api-keys.txt` contains no keys, only a pasted Claude message listing providers. No secrets anywhere in the tree.
- Real table names are `events` and `storage_incentives_events` (this file's older sections say `stamp_events`; that is wrong).
- Databases (all data ends late Jan 2026, ~4.4M blocks behind chain head):
  - `beeport_tx_prod` 861 MB — the ONLY db with price history: 1.23M storage-incentives events incl. 80,396 PriceUpdate, blocks 31,307,171–44,267,041 (2023-12-06 → 2026-01-21); 184k stamp events.
  - `beeport3m` 166 MB — freshest stamp events (→ block 44,412,633, 2026-01-29), 0 storage-incentives events. This is what `config.yaml` points at.
  - `beeport_bandwidth` 165 MB — stamp events → 2026-01-25, 713 payment_channel_deployments, 0 si events.
  - `beeport_bandwidth_testing` 1 GB — payment channel data only (35k deployments, 6k events) + 2.25M-row rpc_cache; no stamp/si events.
  - `beeport5` 12 MB, `beeport6` 8 MB — early stubs, ignorable.
  - `batch_balances` is empty in every db (root cause of the expiry-analytics bug).
- Git (checked 2026-10-15): remote `origin` = github.com/significance/beeport-stamp-stats. Current branch is in sync with origin (0 ahead/0 behind) apart from the uncommitted work above; it has NO pull request. `main` is in sync but last moved 2026-01-10. Open PRs: #4 investigate-addresses, #5 address-tracking-phase2, #6 improve-retrieval-efficiency (that branch has diverged: 8 local commits unpushed, 3 remote commits unpulled). Everything in PRs #4–#6 is already contained in this branch's history.
- RPC keys: there are none. Every config, doc, and the shell environment use only free public Gnosis endpoints; the multi-RPC scheduler exists precisely because there were no keys.
- Loose files (untracked, gitignored): `price-updates.csv` 16 MB, `events.csv`, `balances.csv`, `stamp-cache.db`, `beeport_test_2.db` (all 24–25 Jan 2026); 2.5 GB of `.sql` dumps duplicated in `../beeport-tx-stats`.

---

## 🚧 Current Task: Fix Expiry Analytics Command

**Problem:** The `expiry-analytics` command shows nothing because:
1. The `batch_balances` cache table is empty (0 rows)
2. Without cached balances and without `--refresh`, the code sets balance to "0"
3. All batches are skipped as "expired" → no output
4. No `Hour` period option (only Day/Week/Month)

**Root Cause Analysis:**
- `batches` table: 17,551 rows (has `normalised_balance` from creation/last event)
- `batch_balances` table: 0 rows (stores current on-chain balance)
- Code logic: If no cached balance AND not refresh mode → balance = "0" → skip

**Solution Plan:**

### Phase 1: Add fallback to stored balance (quick fix)
- [x] When no cached balance exists, use `normalised_balance` from batches table
- [x] Add warning that this may not reflect current on-chain state
- [x] Batches created long ago will show inaccurate expiry (balance depleted)

### Phase 2: Add Hour time period
- [x] Add `Hour` variant to `TimePeriod` enum in `src/cli.rs`
- [x] Add hour formatting in `ExpiryPeriod::format_period()` in `src/commands/expiry_analytics.rs`
- [x] Add unit test for Hour period formatting

### Phase 3: Efficient balance refresh (optional, for accurate data)
- [ ] Consider batch RPC calls (multicall) for fetching many balances
- [ ] Use parallel fetching across multiple RPCs
- [ ] Or: Add a new command `refresh-balances` that efficiently updates the cache

**Files to modify:**
- `src/cli.rs` - Add `Hour` to `TimePeriod` enum
- `src/commands/expiry_analytics.rs` - Add fallback logic + hour formatting

---

**Last Verified (2026-02-12):**
- ✅ 86 unit tests passing
- ✅ Zero clippy warnings
- ✅ Release binary built

**Previous Testing Session (2026-01-25 20:41 UTC):**
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
| **PostageStamp** | `0x45a1502382541Cd610CC9068e88727426b696293` | 31,306,385 | 6 types |
| **StampsRegistry** | `0x5EBfBeFB1E88391eFb022d5d33302f50a46bF4f3` | 42,390,510 | 6 types (+ payer) |

(Corrected 2026-10-15. This table previously listed `0x6a1A…` and `0xCfC2…`; the latter has no code
on Gnosis and had crept into `config.yaml`, so StampsRegistry fetched nothing. `0x6a1A…` is a real
older PostageStamp deployment but is not tracked by any config.)
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
