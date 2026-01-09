# Beeport TX Stats - Project Plan

**Last Updated:** 2026-01-02

---

## 📍 Current Status

**Project State:** ✅ Production Ready

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

### Test Database Convention
**IMPORTANT:** Always use PostgreSQL for testing, never SQLite

- **Database name:** `beeport2_testing`
- **Source database:** `beeport2` (production/main database)
- **Reset procedure:** Always recreate from `beeport2` at start of test run

**User Confirmation Required Before:**
1. Copying `beeport2` to `beeport2_testing` (ask first!)
2. Creating fresh empty database if `beeport2` doesn't exist

**Standard setup:**
```bash
# Drop and recreate testing database from production data
psql -c "DROP DATABASE IF EXISTS beeport2_testing;"
psql -c "CREATE DATABASE beeport2_testing TEMPLATE beeport2;"

# Or create fresh empty if source doesn't exist
psql -c "DROP DATABASE IF EXISTS beeport2_testing;"
psql -c "CREATE DATABASE beeport2_testing;"
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

## 📦 Completed Work Archive

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
  --database-url "postgresql://localhost/beeport2" \
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
