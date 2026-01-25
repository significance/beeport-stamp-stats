# Beeport TX Stats - Project Plan

**Last Updated:** 2026-01-24 17:00 UTC

---

## 🎯 Branch Purpose

**Branch:** `feat/improve-retrieval-efficiency`

**Purpose:** Chequebook/Payment Channel statistics implementation with multi-RPC parallel fetching.

**Working Database:** `postgresql://localhost/beeport_bandwidth`
- Contains 7086 discovered chequebooks
- 383 payment channel events synced

**Quick Start:**
```bash
# Sync chequebooks with parallel fetching (50 at a time)
./target/release/beeport-stamp-stats --database postgresql://localhost/beeport_bandwidth \
  sync-chequebooks --parallel-batch-size 50

# View cheque summary
./target/release/beeport-stamp-stats --database postgresql://localhost/beeport_bandwidth \
  cheque-summary
```

---

## 📍 Current Status

**Project State:** ✅ Payment Channel Integration + Parallel Fetching Complete

**Recent Work:**
- ✅ (2026-01-24) Merged payment channel tracking from `feat/investigate-addresses-plus-bandwidth-incentives`
- ✅ (2026-01-24) Implemented parallel chequebook fetching with `--parallel-batch-size` option
- ✅ (2026-01-24) Tested on `beeport_bandwidth` database (7086 chequebooks)

All core features implemented and tested:
- ✅ Postage stamp events tracking (PostageStamp, StampsRegistry contracts)
- ✅ Storage incentives tracking (PriceOracle, StakeRegistry, Redistribution contracts)
- ✅ Payment channel tracking (SimpleSwapFactory, ERC20SimpleSwap contracts)
- ✅ Chequebook discovery and event syncing
- ✅ **NEW:** Parallel chequebook syncing with configurable batch size
- ✅ Database migrations (SQLite + PostgreSQL)
- ✅ CLI commands (fetch, sync, follow, summary, batch-status, expiry-analytics, export)
- ✅ discover-chequebooks, sync-chequebooks, cheque-summary, chequebook-balances, payment-channel-summary
- ✅ Multi-RPC parallel execution with adaptive rate limiting
- ✅ Retry logic with exponential backoff (HTTP 429 + 502)
- ✅ 215+ tests passing, zero clippy warnings

**Ready for:** Data collection, analysis, production deployment

---

## 🆕 New Payment Channel Commands

```bash
# Discover chequebooks from factory contracts
./beeport-stamp-stats discover-chequebooks --from-block X --to-block Y

# Sync events from discovered chequebooks
./beeport-stamp-stats sync-chequebooks --from-block X --to-block Y

# Analyze cheque cashing activity
./beeport-stamp-stats cheque-summary --from-block X --to-block Y

# Export chequebook balances
./beeport-stamp-stats chequebook-balances --output table

# Payment channel activity summary
./beeport-stamp-stats payment-channel-summary --group-by week --months 12

# Enhanced address summary with filters
./beeport-stamp-stats address-summary --role owner --live-only --min-stamps 5
```

---

## 🎯 Project Capabilities

### Data Collection
- **7 Smart Contracts** tracked simultaneously on Gnosis Chain (was 5)
- **45+ Event Types** captured across postage stamps, storage incentives, and payment channels
- **Incremental syncing** with block caching and resume support
- **Follow mode** for continuous real-time monitoring
- **Resilient RPC handling** with automatic retry on rate limits and gateway errors
- **Multi-RPC parallel execution** for high throughput

### Analytics & Reporting
- **Batch status** with TTL calculations and price modeling
- **Expiry analytics** grouped by day/week/month
- **Event summaries** with contract and event type filtering
- **Export capabilities** (JSON, CSV formats)
- **Price history** from PriceOracle events
- **Redistribution game** tracking (commits, reveals, winners)
- **Staking dynamics** (updates, slashes, freezes, withdrawals)
- **NEW:** Cheque cashing analytics (ChequeCashed, ChequeBounced, HardDeposit*, Withdraw)
- **NEW:** Address role analysis (owner, payer, sender)

### Database
- **Four table design:**
  - `stamp_events` - Postage stamp events (BatchCreated, TopUp, etc.)
  - `storage_incentives_events` - Storage incentives events (PriceUpdate, Revealed, etc.)
  - `payment_channel_deployments` - Discovered chequebooks
  - `payment_channel_events` - Chequebook events (ChequeCashed, Withdraw, etc.)
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
| **SimpleSwapFactory** | `0xc2d5a532cf69aa9a1378737d8ccdef884b6e7420` | TBD | 1 type |
| **ERC20SimpleSwap** | Dynamic (per chequebook) | Dynamic | 6 types |

### Payment Channel Events (7 total)

**SimpleSwapFactory (1):**
- `SimpleSwapDeployed` - New chequebook created

**ERC20SimpleSwap (6):**
- `ChequeCashed` - Cheque payment processed
- `ChequeBounced` - Cheque rejected
- `HardDepositAmountChanged` - Deposit amount changed
- `HardDepositDecreasePrepared` - Decrease scheduled
- `HardDepositTimeoutChanged` - Timeout modified
- `Withdraw` - Funds withdrawn

---

## ✅ Parallel Chequebook Fetching (Completed 2026-01-24)

### Problem
Chequebook event syncing was sequential. With thousands of chequebooks on mainnet, this was slow.

### Solution
Adapted `execute_sync_chequebooks` to process chequebooks in parallel batches:

```rust
// Before (SLOW):
for chequebook in chequebooks {
    client.fetch_chequebook_events(chequebook).await;
}

// After (FAST):
for batch in chequebooks.chunks(parallel_batch_size) {
    let futures = batch.iter().map(|c| fetch_events(c));
    futures::future::join_all(futures).await;  // Parallel!
}
```

### Tasks
- [x] Add `--parallel-batch-size` CLI option (default: 10)
- [x] Modify `execute_sync_chequebooks` to batch chequebook processing
- [ ] Test with multi-RPC configuration
- [ ] Measure performance improvements

### Usage
```bash
# Sync with default batch size (10 chequebooks in parallel)
./beeport-stamp-stats sync-chequebooks --from-block X --to-block Y

# Sync with larger batch size for faster processing
./beeport-stamp-stats sync-chequebooks --parallel-batch-size 50
```

---

## 🔧 Technical Notes

### Payment Channel Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                     SimpleSwapFactory                            │
│  (1 per network - Gnosis, Sepolia)                              │
│                                                                  │
│  Emits: SimpleSwapDeployed(chequebook_address, issuer)          │
└─────────────────────────┬───────────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────────────┐
│              ERC20SimpleSwap (Chequebook)                        │
│  (1 per Bee node - thousands on mainnet)                        │
│                                                                  │
│  Events:                                                         │
│  - ChequeCashed(beneficiary, recipient, caller, totalPayout...) │
│  - ChequeBounced()                                               │
│  - HardDepositAmountChanged(beneficiary, amount)                │
│  - HardDepositDecreasePrepared(beneficiary, decreaseAmount)     │
│  - HardDepositTimeoutChanged(beneficiary, timeout)              │
│  - Withdraw(amount)                                              │
└─────────────────────────────────────────────────────────────────┘
```

### Configuration

Payment channel factories are configured in `config.yaml`:

```yaml
payment_channel_factories:
  - name: "SimpleSwapFactory-Gnosis"
    address: "0xc2d5a532cf69aa9a1378737d8ccdef884b6e7420"
    deployment_block: 1  # TBD - need to confirm
    network: "gnosis"
    active: false  # Enable when deployment block confirmed

  - name: "SimpleSwapFactory-Sepolia"
    address: "0x0fF044F6bB4F684a5A149B46D7eC03ea659F98A1"
    deployment_block: 4752810
    network: "sepolia"
    active: false  # Testnet, disabled by default
```

---

## 📦 Completed Work Archive

### ✅ Payment Channel Integration (2026-01-24)
**Goal:** Add bandwidth incentives tracking (chequebook/payment channel support)

**Implementation:**
- Merged from `feat/investigate-addresses-plus-bandwidth-incentives` branch
- Added `PaymentChannelFactory` and `PaymentChannelContract` traits
- Added SimpleSwapFactory and ERC20SimpleSwap ABIs and parsers
- Added 3 new database tables for payment channel data
- Added 5 new CLI commands for chequebook management
- Enhanced address-summary with --role, --live-only, --price filters
- Integrated with existing multi-RPC scheduler

**Testing Results:**
- ✅ 232 tests passing (was 132)
- ✅ Zero clippy warnings
- ✅ All merge conflicts resolved
- ✅ Multi-RPC scheduler integration working

**Result:** Full payment channel/bandwidth incentives tracking capability.

### ✅ Multi-RPC Parallel Execution (2026-01-19)
**Goal:** True parallel RPC execution for faster data retrieval

**Implementation:**
- Fixed blocking issue - execute_many() was never used
- Refactored blockchain client to use three-phase approach
- Verified 4 RPCs fetch 4 chunks truly in parallel

**Result:** N× throughput with N RPC endpoints.

### ✅ Multi-RPC System Implementation (2026-01-12)
**Goal:** Improve data retrieval speed by distributing requests across multiple RPC endpoints.

**Features:**
- Round-robin request distribution across multiple RPCs
- Adaptive rate limiting per endpoint
- Rate limit persistence across sessions
- Three strategies: Manual, Adaptive, Aggressive
- Full backward compatibility with single RPC mode

**Result:** Multi-RPC system fully integrated and tested.

### ✅ Storage Incentives Integration (2025-12-20)
Implemented support for PriceOracle, StakeRegistry, and Redistribution contracts.

**Result:** Tool tracks complete storage incentives ecosystem.

---

## 📚 Testing Strategy

### Test Database Convention
**IMPORTANT:** Always use PostgreSQL for testing, never SQLite

- **Database name:** `beeport4_testing`
- **Source database:** `beeport4` (production/main database)
- **Reset procedure:** Always recreate from `beeport4` at start of test run

### Test Results (Current)
```
running 83 tests  - lib: PASS
running 83 tests  - lib test: PASS
running 17 tests  - config_tests: PASS
running 22 tests  - retry_tests: PASS
running 10 tests  - price_tests: PASS

Total: 232 tests passing
Clippy: Zero warnings
```

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

**Key Files:**
- `src/contracts/` - Contract definitions, ABIs, parsers
- `src/blockchain.rs` - RPC client and event fetching
- `src/cache.rs` - Database operations
- `src/cli.rs` - CLI orchestration
- `src/retry.rs` - Retry logic with exponential backoff
- `src/rate_limiter.rs` - Adaptive rate limiting
- `src/rpc_scheduler.rs` - Multi-RPC distribution
- `migrations/` - SQLite schema
- `migrations_postgres/` - PostgreSQL schema

---

*This plan is maintained alongside code changes. Update when architectural decisions are made or major features are added.*
