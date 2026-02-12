# Architecture Review - 2026-02-12

## Current State

**Branch:** `feat/improve-retrieval-efficiency-with-bandwidth`
**Tests:** 86 passing
**Lines changed from main:** 14,164 across 55 files

---

## Codebase Structure

```
src/
├── main.rs              (46 lines)   - Entry point
├── lib.rs               (31 lines)   - Module declarations
├── cli.rs             (2271 lines)   - CLI orchestration
├── blockchain.rs      (1888 lines)   - RPC client, event fetching
├── cache.rs           (2631 lines)   - Database operations (SQLite/PostgreSQL)
├── config.rs           (897 lines)   - Configuration loading/merging
├── rate_limiter.rs     (412 lines)   - Rate limiter v1 (sliding window)
├── rate_limiter_v2.rs  (580 lines)   - Rate limiter v2 (token bucket)
├── rpc_scheduler.rs    (422 lines)   - Multi-RPC round-robin scheduler
├── retry.rs            (338 lines)   - Exponential backoff retry logic
├── price.rs            (312 lines)   - TTL/price calculations
├── events.rs           (294 lines)   - Event type definitions
├── types.rs            (229 lines)   - Core type definitions
├── batch.rs            (193 lines)   - Batch operations
├── display.rs          (321 lines)   - Output formatting
├── export.rs           (311 lines)   - JSON/CSV export
├── hooks.rs            (134 lines)   - Event hooks
├── error.rs             (66 lines)   - Error types
├── commands/
│   ├── mod.rs
│   ├── batch_status.rs    (14KB)    - Batch status with TTL
│   ├── expiry_analytics.rs (13KB)   - Expiry predictions
│   └── address_summary.rs  (5KB)    - Address activity summary
└── contracts/
    ├── mod.rs           (24KB)      - Contract registry, traits
    ├── abi.rs           (29KB)      - Solidity ABIs (sol! macro)
    ├── parser.rs        (46KB)      - Event parsing logic
    ├── impls.rs         (15KB)      - Contract implementations
    └── metadata.rs      (7.5KB)     - Contract metadata

Total: ~11,400 lines (src/)
```

---

## Issues Identified

### 1. Duplicate Rate Limiters

Two implementations exist:

| File | Algorithm | Lines | Status |
|------|-----------|-------|--------|
| `rate_limiter.rs` | Sliding window | 412 | v1 - older |
| `rate_limiter_v2.rs` | Token bucket | 580 | v2 - newer, has recovery |

**v2 advantages:**
- Automatic recovery after cool-down period
- Prevents deadlock at 0 req/s (minimum floor)
- Background recovery tasks
- Database persistence of rate limits

**Recommendation:** Remove v1, keep v2 only.

---

### 2. Branch Proliferation

```
Local branches (9):
  backup/wip-changes-20260125
  feat/add-pot-withdrawn-event
  feat/address-tracking-phase2
  feat/github-ci-workflows
  feat/improve-retrieval-efficiency           ← similar
  feat/improve-retrieval-efficiency-with-bandwidth  ← current
  feat/improve-retrieval-efficiency-with-bandwidth-refactor  ← similar
  feat/investigate-addresses
  main
```

**Recommendation:** Consolidate or delete stale branches after merging needed changes.

---

### 3. Temporary Files in Repo

Files that should be gitignored:

```
log.txt                    - Debug output
out.md                     - Temporary output
prompt2.md                 - Conversation prompt
plan_recovery.md           - Recovery plan
plan-addresses.md          - Feature plan
stamp-stats.md             - Notes
*.txt conversation files   - Session transcripts
Reports Dashboard.xlsx     - Excel file (untracked)
history.txt                - History (untracked)
note.txt                   - Notes (untracked)
rpc-api-keys.txt           - SENSITIVE (untracked)
```

---

### 4. Configuration Drift

Multiple configs reference different databases:

| Config File | Database |
|-------------|----------|
| `config.yaml` | `postgresql://localhost/beeport3m` |
| `bandwidth-test-config.yaml` | `postgresql://localhost/beeport_bandwidth` |
| `plan.md` references | `beeport4`, `beeport_bandwidth_testing` |

**Recommendation:** Standardize on one naming convention.

---

### 5. Feature Status

| Feature | Status | Notes |
|---------|--------|-------|
| Postage stamp tracking | ✅ Complete | PostageStamp + StampsRegistry |
| Storage incentives | ✅ Complete | PriceOracle, StakeRegistry, Redistribution |
| Multi-RPC system | ✅ Complete | Round-robin with adaptive rate limiting |
| Rate limiter v2 | ✅ Complete | Token bucket with recovery |
| Payment channels | ⚠️ Merged | Needs verification |
| Address summary | ⚠️ Added | Needs verification |
| Expiry analytics | ❌ Broken | batch_balances table empty |

---

## Contracts Tracked

| Contract | Address | Events |
|----------|---------|--------|
| PostageStamp | `0x45a1502...` | 9 types |
| StampsRegistry | `0xCfC2FfF...` | 10 types |
| PriceOracle | `0x47EeF33...` | 2 types |
| StakeRegistry | `0xda2a16E...` | 5 types |
| Redistribution | `0x5069cdf...` | 11 types |

---

## Recommended Actions

### Immediate (cleanup)

1. **Add to `.gitignore`:**
   ```
   log.txt
   out.md
   *.xlsx
   *-api-keys.txt
   ```

2. **Remove v1 rate limiter** if v2 is stable

3. **Update `config.yaml`** to use consistent database name

### Short-term

4. **Fix expiry-analytics** - use `normalised_balance` fallback when `batch_balances` empty

5. **Verify payment channel commands** work correctly

6. **Create PR to main** with clean, tested features

### Medium-term

7. **Delete stale branches** after confirming no needed changes

8. **Consolidate plan files** into single `plan.md`

9. **Add CI/CD** (branch `feat/github-ci-workflows` exists)

---

## Database Schema Summary

**Main tables:**
- `stamp_events` - Postage stamp events (BatchCreated, TopUp, etc.)
- `storage_incentives_events` - Storage incentives (PriceUpdate, Revealed, etc.)
- `batches` - Batch state cache
- `batch_balances` - On-chain balance cache (currently empty)
- `rpc_rate_limits` - Rate limit persistence
- `chequebooks` - Payment channel tracking
- `cheques` - Individual cheque records

**Supported backends:** SQLite, PostgreSQL

---

## Commands Available

```bash
# Data collection
fetch --from-block N --to-block M    # Fetch events
sync                                  # Incremental sync
follow --poll-interval N             # Real-time monitoring

# Analysis
summary                              # Event summary with filters
batch-status                         # Batch TTL calculations
expiry-analytics                     # Expiry predictions (broken)
address-summary                      # Address activity

# Export
export --format json|csv             # Export events

# Payment channels
sync-chequebooks                     # Sync chequebook data
payment-channel-summary              # Payment activity
```

---

*Generated: 2026-02-12*
