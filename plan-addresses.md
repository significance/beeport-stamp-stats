# Address Investigation & Tracking - Implementation Plan

**Status:** Phase 1 + 2 + 3 Complete ✅ (Ready for Phase 4-7)
**Started:** 2026-01-09
**Last Updated:** 2026-01-11 13:35 UTC
**Branch:** feat/address-tracking-phase2
**Goal:** Track addresses, their stamp ownership, funding relationships, and interactions

**Primary Database:** `beeport4` (PostgreSQL)
**Database Status:**
- ✅ Phase 2 schema migrations deployed (11 total migrations)
- ✅ Phase 3 integration tested and verified with real blockchain data
- ✅ All functionality working correctly

**Current Progress:**
- ✅ Phase 1: Basic Address Tracking (from_address) - COMPLETE
- ✅ Quick Win: Address Analysis Query Command - COMPLETE
- ✅ Refactor: Incremental from_address population - COMPLETE
- ✅ Phase 2: Database Schema & Migrations - COMPLETE
- ✅ Phase 3: Core Data Collection - COMPLETE
- ⬜ Phase 4: Address Relationship Tracking - PENDING
- ⬜ Phase 5: Data Population & Backfill - PENDING
- ⬜ Phase 6: Query & Reporting Commands - PENDING
- ⬜ Phase 7: Testing & Validation - PENDING

**Completed Refactoring:**
Refactored fetch and sync commands to populate from_address incrementally during chunk processing. Events now have from_address populated immediately as they're stored, rather than in one batch at the end. This improves efficiency and makes data available sooner during long-running syncs.

**Testing Completed (All Passed ✅):**
- ✅ Test sync command with multiple small block ranges (37000000-37000100, 37000100-37000200)
- ✅ Verify from_address populated correctly during incremental fetch (4/4 events, then 6/6 total)
- ✅ Test various event types (BatchCreated, PriceUpdate, PotWithdrawn verified)
- ✅ Verify database updates work correctly with upsert logic (PostgreSQL ON CONFLICT)
- ✅ Test both PostgreSQL and SQLite backends (both working correctly)
- ✅ Verify blockchain data accuracy (validated against GnosisScan)
- ✅ Verify address-summary shows Sender roles correctly (4 addresses with Owner+Sender, Sender roles)
- ✅ Unit tests pass (cargo test)
- ✅ Clippy warnings: 0 (cargo clippy -D warnings)

---

## Overview

Build a comprehensive address tracking system that identifies:
1. Addresses that purchase/own stamps (owner, payer, transaction sender)
2. Funding relationships between addresses (who funds whom)
3. Address interaction patterns
4. Classification of addresses (stamp buyers vs funders)

This enables analysis of:
- Who is buying stamps and where their funds come from
- Funding patterns and address clustering
- Identification of "funder" addresses vs "buyer" addresses
- Top funders for any given stamp-buying address

---

## Current State Analysis

**What we currently capture:**
- `owner`: who owns the batch (from BatchCreated event data)
- `payer`: who paid for the batch (from StampsRegistry events only)
- `transaction_hash`: the transaction that emitted the event

**What we're missing:**
- Transaction `from` address (who actually signed and sent the transaction)
- Transaction `to` address (contract address being called)
- Transaction value (ETH/xDAI amount sent)
- Address relationships (who funds whom)
- Address classification (EOA vs contract, buyer vs funder)
- Historical funding sources

**Key insight:**
- `owner` = who will own the batch (recipient)
- `payer` = who pays (in StampsRegistry, can differ from owner)
- `from` = who signs the transaction (often same as payer, but not always)

All three addresses can be different! Example:
- Address A signs transaction (from)
- Address A calls StampsRegistry.createBatch() (payer)
- Batch owned by Address B (owner)

---

## Database Schema Design

### 1. Addresses Table
**Purpose:** Track all known addresses and their stamp activity

```sql
CREATE TABLE addresses (
    address TEXT PRIMARY KEY,                      -- Ethereum address (checksummed)

    -- Stamp activity
    stamp_ids TEXT[] NOT NULL DEFAULT '{}',        -- Array of batch IDs owned/purchased
    total_stamps_purchased INTEGER NOT NULL DEFAULT 0,
    total_amount_spent TEXT NOT NULL DEFAULT '0',  -- Total spent in wei

    -- Funding relationships
    top_funders JSONB,                             -- Top 10 funders: [{address, amount, tx_count}]
    is_funder BOOLEAN NOT NULL DEFAULT false,      -- True if funds other stamp buyers
    funded_addresses TEXT[] DEFAULT '{}',          -- Addresses this address has funded

    -- Activity metadata
    first_seen BIGINT NOT NULL,                    -- Block timestamp
    last_seen BIGINT NOT NULL,                     -- Block timestamp
    first_block BIGINT NOT NULL,
    last_block BIGINT NOT NULL,
    transaction_count INTEGER NOT NULL DEFAULT 0,

    -- Classification
    address_type TEXT,                             -- 'buyer', 'funder', 'both', 'contract'
    is_contract BOOLEAN NOT NULL DEFAULT false,

    -- Optional metadata
    label TEXT,                                    -- User-defined label
    notes TEXT                                     -- User-defined notes
);

CREATE INDEX idx_addresses_is_funder ON addresses(is_funder);
CREATE INDEX idx_addresses_stamp_count ON addresses(total_stamps_purchased);
CREATE INDEX idx_addresses_type ON addresses(address_type);
CREATE INDEX idx_addresses_first_seen ON addresses(first_seen);
```

### 2. Address Interactions Table
**Purpose:** Track direct funding transactions between addresses

```sql
CREATE TABLE address_interactions (
    id BIGSERIAL PRIMARY KEY,
    from_address TEXT NOT NULL,                    -- Sender (funder)
    to_address TEXT NOT NULL,                      -- Recipient
    transaction_hash TEXT NOT NULL,
    amount TEXT NOT NULL,                          -- Transfer amount in wei
    block_number BIGINT NOT NULL,
    block_timestamp BIGINT NOT NULL,

    -- Context: was this interaction related to stamp activity?
    related_to_stamp BOOLEAN NOT NULL DEFAULT false,
    stamp_batch_id TEXT,                           -- If related, which batch

    UNIQUE(transaction_hash, from_address, to_address)
);

CREATE INDEX idx_interactions_from ON address_interactions(from_address);
CREATE INDEX idx_interactions_to ON address_interactions(to_address);
CREATE INDEX idx_interactions_stamp_related ON address_interactions(related_to_stamp);
CREATE INDEX idx_interactions_block ON address_interactions(block_number);
```

### 3. Transaction Details Cache
**Purpose:** Cache full transaction details to avoid repeated RPC calls

```sql
CREATE TABLE transaction_details (
    transaction_hash TEXT PRIMARY KEY,
    from_address TEXT NOT NULL,
    to_address TEXT,                               -- NULL for contract creation
    value TEXT NOT NULL,                           -- ETH value in wei
    gas_price TEXT,
    gas_used BIGINT,
    block_number BIGINT NOT NULL,
    block_timestamp BIGINT NOT NULL,
    input_data TEXT,                               -- Contract call data
    is_contract_creation BOOLEAN NOT NULL DEFAULT false,
    fetched_at BIGINT NOT NULL
);

CREATE INDEX idx_tx_details_from ON transaction_details(from_address);
CREATE INDEX idx_tx_details_to ON transaction_details(to_address);
CREATE INDEX idx_tx_details_block ON transaction_details(block_number);
```

---

## Architecture Decisions

### 1. Top Funders Storage (JSONB)
Store as JSON array for flexibility and queryability:
```json
[
  {"address": "0x123...", "amount": "1000000000000000000", "tx_count": 5},
  {"address": "0x456...", "amount": "500000000000000000", "tx_count": 2}
]
```

**Benefits:**
- Easy to query with PostgreSQL JSONB operators
- Sortable and filterable
- Compact storage
- Can add metadata without schema changes

### 2. Funding Depth: Direct Only (Phase 1)
For initial implementation, track only **direct funders** (1 level deep).
- Transaction sender → stamp buyer
- No recursive tracing (can be added later)

**Why:**
- Simpler to implement and reason about
- Covers 90% of use cases
- Can extend later with recursive tracing if needed

### 3. Address Classification Strategy
```
is_contract: Detected via eth_getCode
address_type:
  - 'buyer': Has purchased stamps, not funded others
  - 'funder': Funds stamp buyers, hasn't purchased stamps
  - 'both': Does both
  - 'contract': Is a smart contract
```

---

## Implementation Phases

### ✅ Phase 1: Basic Address Tracking - COMPLETE
**Goal:** Capture transaction sender (from) address for all stamp events

**Completed:** 2026-01-10

**Tasks:**
- [x] Add `from_address` column to `stamp_events` table
  - [x] Create PostgreSQL migration (20260110000008)
  - [x] Create SQLite migration (20260110000008)
- [x] Fetch transaction details during event processing
  - [x] Add `eth_getTransactionByHash` to blockchain client
  - [x] Extract `from`, `to`, `value` from transaction
  - [x] Add `populate_from_addresses()` method
- [x] Update event processing to store `from_address`
- [x] Integrate into fetch command workflow
- [x] Tests updated and passing (132/132)
- [x] Clippy warnings resolved

**Success criteria achieved:**
- ✅ Every stamp event has `from_address` populated during fetch
- ✅ Verified against GnosisScan: `0x647942035bb69c8e4d7eb17c8313ebc50b0babfa`
- ✅ Database schema includes `from_address` column with index
- ✅ All tests passing, zero warnings

**What we can now do:**
- Track who actually signs/sends stamp purchase transactions
- Compare `from_address` vs `owner` vs `payer`
- Query addresses by transaction activity

**Foundation complete for Phase 2+**

---

### ✅ Quick Win: Address Analysis Query Command - COMPLETE

**Goal:** Create immediate value from Phase 1 data without new tables

**Completed:** 2026-01-10 15:50 UTC

**Implementation:**
- [x] Added `address-summary` command to CLI
- [x] Created `src/commands/address_summary.rs` module
- [x] Implemented SQL queries for both PostgreSQL and SQLite
  - [x] get_address_summary(): Aggregates owner/payer/sender roles
  - [x] get_delegation_cases(): Finds owner ≠ sender transactions
- [x] Added output format support (table/JSON/CSV)
- [x] Added filtering options (min_stamps, show_delegated_only)
- [x] All tests passing (83 unit tests)
- [x] Zero clippy warnings
- [x] Verified with real data

**Features:**
```bash
# Show all addresses with activity summary
beeport-stamp-stats address-summary

# Filter by minimum stamp count
beeport-stamp-stats address-summary --min-stamps 10

# Show only delegation cases (owner ≠ sender)
beeport-stamp-stats address-summary --show-delegated-only

# Export to JSON/CSV
beeport-stamp-stats address-summary --output json
beeport-stamp-stats address-summary --output csv
```

**Key Insights Enabled:**
- Role classification: Owner, Payer, Sender, or combinations
- Multi-role detection: Identifies addresses acting in multiple capacities
- Delegation detection: Finds transactions where owner ≠ transaction sender
- Activity timeline: First and last activity timestamps
- Stamp purchase patterns: Count of stamps per address

**Results from Testing:**

*Test Database (beeport_testing_2 - small dataset):*
- 117 unique addresses found (min 10 stamps filter)
- 40 delegation cases identified
- 1 multi-role address (Owner+Payer)
- Role breakdown: Owner (113), Payer (3), Sender (1), Owner+Payer (1)

*Production Database (beeport4 - 180k+ events):*
- 116 unique addresses with 10+ stamps
- Owner/Payer role analysis working correctly
- 1 multi-role address (Owner+Payer)
- Sender roles and delegation detection pending backfill

**Technical Implementation:**
- Uses CTE (Common Table Expression) for efficient aggregation
- PostgreSQL: JSONB operators with text casting (`data::jsonb->>'owner'`)
- SQLite: JSON extract functions (`json_extract(data, '$.BatchCreated.owner')`)
- Proper timestamp formatting for human readability
- Inline format strings for clippy compliance

**Success Criteria Achieved:**
- ✅ Query existing data without new tables
- ✅ Identify address roles and patterns
- ✅ Detect delegation cases
- ✅ Support multiple output formats
- ✅ Filter by activity level
- ✅ Fast query performance (< 1 second)

---

### ✅ Phase 2: Database Schema & Migrations - COMPLETE
**Goal:** Create comprehensive address tracking tables

**Completed:** 2026-01-11

- [x] Create PostgreSQL migration for addresses table (20260111000009)
- [x] Create PostgreSQL migration for address_interactions table (20260111000010)
- [x] Create PostgreSQL migration for transaction_details cache (20260111000011)
- [x] Create SQLite migrations (same schema - all 3 tables)
- [x] Test migrations on test database (PostgreSQL + SQLite)

**Database Tables Created:**
1. **addresses** - Tracks addresses, stamp activity, funding relationships
   - 16 columns including stamp_ids (array), top_funders (JSONB), is_contract
   - 6 indexes for efficient querying
2. **address_interactions** - Stores funding transactions between addresses
   - 9 columns including from/to addresses, amount, related_to_stamp
   - 5 indexes for querying funding relationships
3. **transaction_details** - Caches full transaction information
   - 11 columns including from, to, value, gas info, input_data
   - 4 indexes for efficient cache lookups

**Testing Results:**
- ✅ PostgreSQL migrations applied successfully
- ✅ SQLite migrations applied successfully
- ✅ All schemas verified correct
- ✅ Indexes created properly

---

### ✅ Phase 3: Core Data Collection - COMPLETE
**Goal:** Collect transaction details and build address records

**Completed:** 2026-01-11

- [x] Enhance transaction details fetching
  - [x] Implement `eth_getCode` for contract detection (blockchain.rs)
  - [x] Add transaction details caching (get/store in cache.rs)
  - [x] Handle all Ethereum transaction types (Legacy, EIP-2930, EIP-1559, etc.)

- [x] Extend event processing to populate addresses table
  - [x] Create/update address record for `from_address`
  - [x] Create/update address record for `owner`
  - [x] Create/update address record for `payer` (if present)
  - [x] Add batch_id to address's stamp_ids array
  - [x] Update statistics (total_amount_spent, transaction_count)

- [x] Integration into fetch/sync commands
  - [x] process_address_tracking() method (main integration point)
  - [x] Cache-first strategy (check cache before RPC calls)
  - [x] Contract detection for all addresses
  - [x] Address interaction tracking (sender → owner)
  - [x] Non-blocking (continues even if tracking fails)

**Testing Results (Block 31306385):**
- ✅ 40 events processed successfully
- ✅ 15 unique addresses tracked
- ✅ 1 transaction cached (efficient caching working)
- ✅ 14 funding relationships recorded
- ✅ All data verified correct:
  - Top buyer: 0x4466...2ff5 (15 stamps)
  - Second: 0x1354...4806 (13 stamps)
  - Transaction sender: 0x6479...babfa (signs all txs)
  - All addresses correctly marked as EOAs (not contracts)

**Code Locations:**
- `src/blockchain.rs`: get_transaction_details(), is_contract(), process_address_tracking()
- `src/cache.rs`: store_transaction_details(), get_transaction_details(), upsert_address(), store_address_interaction()
- `src/cli.rs`: Integration in fetch and sync command chunk callbacks

---

### Phase 4: Address Relationship Tracking ⬜
**Goal:** Build funding relationship graph

- [ ] Build address interaction tracking
  - [ ] Detect funding transactions
  - [ ] Calculate top funders per address
  - [ ] Mark funder addresses
  - [ ] Update funded_addresses arrays

- [ ] Implement address classification logic
  - [ ] Detect contracts
  - [ ] Classify as buyer/funder/both
  - [ ] Update address_type field

---

### Phase 5: Data Population & Backfill ⬜
**Goal:** Process existing data and build relationships

- [ ] Add command: `analyze-addresses`
  - [ ] Process existing events
  - [ ] Fetch missing transaction details
  - [ ] Build address relationships
  - [ ] Calculate statistics

- [ ] Integrate with existing commands
  - [ ] Update `fetch` to collect address data
  - [ ] Update `sync` to maintain address data
  - [ ] Update `follow` to track addresses in real-time

---

### Phase 6: Query & Reporting Commands ⬜
**Goal:** Make address data queryable and useful

- [ ] Add command: `address-info <address>`
  - [ ] Show stamp purchases
  - [ ] Show top funders
  - [ ] Show funded addresses
  - [ ] Show interaction history

- [ ] Add command: `address-list`
  - [ ] Filter by type (buyer/funder/both)
  - [ ] Sort options (stamp count, amount spent, etc.)
  - [ ] Output formats (table/json/csv)

- [ ] Add command: `funding-graph`
  - [ ] Show funding relationships
  - [ ] Export to graph format (GraphML, DOT)
  - [ ] Visualization-ready output

---

### Phase 7: Testing & Validation ⬜
**Goal:** Ensure correctness and performance

- [ ] Unit tests for address tracking logic
- [ ] Integration tests for data collection
- [ ] Validate against blockchain data (spot check)
- [ ] Performance testing with large datasets
- [ ] Test all new commands

---

## Technical Challenges & Solutions

### Challenge 1: Identifying Funding Relationships
**Problem:** How to determine if address A funded address B?

**Solution:**
1. When processing stamp event, get transaction details
2. Transaction `from` is the direct actor
3. To find funders: look at recipient's transaction history
   - Query `eth_getTransactionByHash` for recent incoming transactions
   - Store as address interactions
   - Calculate top 10 by amount

**Limitation:** This captures direct funding only, not multi-hop.

### Challenge 2: RPC Call Volume
**Problem:** Fetching transaction details adds many RPC calls.

**Solutions:**
- Cache transaction details aggressively
- Batch requests where possible
- Use existing retry logic
- Add rate limiting configuration
- Process incrementally (backfill gradually)

### Challenge 3: Top Funders Calculation
**Problem:** How to efficiently maintain top 10 funders?

**Solution:**
- Calculate during address analysis phase
- Store in JSONB for efficient updates
- Rebuild periodically (not per-transaction)
- Use PostgreSQL's JSONB aggregation functions

### Challenge 4: Contract Detection
**Problem:** Differentiating EOAs from contracts.

**Solution:**
- Use `eth_getCode` - returns non-empty for contracts
- Cache results (contracts don't change)
- Handle proxy patterns (detect proxy, mark as contract)

---

## RPC Methods Required

```
eth_getTransactionByHash    - Get transaction details (from, to, value)
eth_getCode                 - Detect if address is contract
eth_getTransactionReceipt   - Already used for logs/events
eth_getLogs                 - Already used for events
```

All methods supported by standard Ethereum RPC providers.

---

## Data Flow

```
Phase 1 (Basic):
1. Fetch stamp events (existing)
   ↓
2. For each event:
   - Fetch transaction details (eth_getTransactionByHash)
   - Extract from_address
   - Store in stamp_events.from_address
   ↓
3. Analyze:
   - Who sends transactions (from_address)
   - Who owns batches (owner)
   - Who pays (payer, if present)
   - When do they differ?

Later Phases (Comprehensive):
4. Update addresses table:
   - Add/update buyer address (from event owner/payer)
   - Add stamp_id to their array
   - Update statistics
   ↓
5. Analyze funding relationships:
   - For each buyer, find incoming transactions
   - Calculate top funders
   - Update top_funders JSONB
   ↓
6. Update funder addresses:
   - Mark funders with is_funder = true
   - Add to their funded_addresses array
   - Classify address_type
```

---

## Configuration Additions

```yaml
address_tracking:
  enabled: true
  max_funders_tracked: 10                  # Top N funders per address
  funding_lookback_blocks: 10000           # How far back to look for funding txs
  min_funding_amount: "1000000000000000"   # Minimum to count as funding (0.001 ETH)
  contract_detection: true
```

---

## Success Criteria

### Phase 1
- ✓ Every stamp event has from_address populated
- ✓ Can differentiate owner vs payer vs from_address
- ✓ Basic address analysis queries work

### Full Implementation
- ✓ Addresses table with all addresses that interact with stamps
- ✓ Address interactions captured for funding relationships
- ✓ Transaction details cached efficiently
- ✓ Every stamp event linked to owner/buyer/sender addresses
- ✓ Top funders accurately calculated (verified by manual check)
- ✓ is_funder flag correctly set
- ✓ Contract addresses properly detected
- ✓ Commands work (`address-info`, `address-list`, `analyze-addresses`)
- ✓ Real-time tracking in fetch/sync/follow
- ✓ Queries respond quickly (< 1 second for address lookups)

---

## Future Enhancements (Out of Scope)

1. **Multi-hop Funding Tracing**
   - Recursive funding analysis
   - Funding chains/trees
   - Ultimate source detection

2. **Address Labeling**
   - Integration with known address databases
   - Exchange detection
   - Known entity tagging

3. **Clustering & Pattern Detection**
   - Address clustering (same owner)
   - Sybil detection
   - Pattern analysis

4. **Graph Visualization**
   - Web interface for funding graphs
   - Interactive exploration
   - Community detection

5. **Token Transfers**
   - Track ERC20 transfers (BZZ, xDAI)
   - Include in funding analysis

---

## Dependencies

- Existing blockchain client infrastructure
- PostgreSQL JSONB support
- RPC provider with transaction detail methods
- SQLx for database operations

---

## Risk Mitigation

| Risk | Mitigation |
|------|------------|
| High RPC costs | Cache aggressively, batch requests, rate limit |
| Data volume growth | Indexes, partitioning, archival strategy |
| Complex queries slow | JSONB indexes, materialized views |
| Incomplete data | Validation queries, spot checks vs blockchain |

---

## Progress Tracking

**Current Phase:** Quick Win Complete, Ready for Phase 2
**Next Step:** Decide whether to proceed with comprehensive address tracking (Phase 2-7) or move to other features

**Timeline:**
- Phase 1: Basic Address Tracking - ✅ Complete (2026-01-10)
- Quick Win: Address Analysis Query - ✅ Complete (2026-01-10)
- Phase 2: Database Schema - ⬜ Not started
- Phase 3: Core Data Collection - ⬜ Not started
- Phase 4: Relationship Tracking - ⬜ Not started
- Phase 5: Data Population - ⬜ Not started
- Phase 6: Query Commands - ⬜ Not started
- Phase 7: Testing - ⬜ Not started

**Achievements:**
- ✅ Transaction sender tracking (from_address column)
- ✅ Address role analysis (owner/payer/sender)
- ✅ Delegation detection (owner ≠ sender)
- ✅ Multi-format output (table/JSON/CSV)
- ✅ Filtering and querying capabilities

---

*Last Updated: 2026-01-10 15:50 UTC*
