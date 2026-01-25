# Payment Channel Discovery and Tracking - Implementation Plan

## Overview

Add support for tracking bandwidth incentive payment channel contracts (ERC20SimpleSwap chequebooks) deployed via the SimpleSwapFactory contract. These per-node instances are used for Swarm's bandwidth incentive mechanism.

**Goal**: Track factory deployments and all chequebook events to enable bandwidth incentive analytics.

## Requirements (User Confirmed)

1. ✅ Track BOTH factory deployment events AND individual chequebook events
2. ✅ Track ALL chequebook events (ChequeCashed, ChequeBounced, HardDeposit*, Withdraw)
3. ✅ Scan from factory deployment block (historical + ongoing)
4. ✅ Support Gnosis Chain mainnet and Sepolia testnet

## Factory Deployment Info

### Gnosis Chain (Mainnet, Chain ID 100)
- **Factory Address**: `0xc2d5a532cf69aa9a1378737d8ccdef884b6e7420`
- **Token (BZZ)**: `0xdbf3ea6f5bee45c02255b2c26a16f300502f68da` (bridged)
- **Deployment Block**: TBD (need to query)

### Sepolia (Testnet, Chain ID 11155111)
- **Factory Address**: `0x0fF044F6bB4F684a5A149B46D7eC03ea659F98A1`
- **Token**: `0x543dDb01Ba47acB11de34891cD86B675F04840db`
- **Deployment Block**: 4752810

## Architecture Design

### Recommended Approach: Two-Phase Tracking

**Phase 1: Factory Event Discovery**
- Track `SimpleSwapDeployed(address contractAddress)` events from factory
- Store discovered chequebook addresses in database
- Enables efficient discovery without scanning entire chain

**Phase 2: Chequebook Event Tracking**
- For each discovered chequebook, track all events
- Events: ChequeCashed, ChequeBounced, HardDeposit*, Withdraw
- Reuse existing RPC chunk caching infrastructure

### Database Schema

#### New Tables

**`payment_channel_factories`** (factory registry)
```sql
CREATE TABLE payment_channel_factories (
    id SERIAL PRIMARY KEY,
    name VARCHAR NOT NULL,
    factory_address VARCHAR NOT NULL UNIQUE,
    deployment_block BIGINT NOT NULL,
    network VARCHAR NOT NULL,  -- "gnosis" or "sepolia"
    active BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX idx_factories_address ON payment_channel_factories(factory_address);
```

**`payment_channel_deployments`** (discovered chequebooks)
```sql
CREATE TABLE payment_channel_deployments (
    id SERIAL PRIMARY KEY,
    chequebook_address VARCHAR NOT NULL UNIQUE,
    factory_address VARCHAR NOT NULL,
    deployed_at_block BIGINT NOT NULL,
    deployed_at_timestamp TIMESTAMP NOT NULL,
    transaction_hash VARCHAR NOT NULL,
    issuer_address VARCHAR,  -- Ethereum address (extracted from init event/transaction)
    overlay_address VARCHAR,  -- Swarm overlay address (if available)
    discovered_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (factory_address) REFERENCES payment_channel_factories(factory_address)
);
CREATE INDEX idx_deployments_address ON payment_channel_deployments(chequebook_address);
CREATE INDEX idx_deployments_factory ON payment_channel_deployments(factory_address);
CREATE INDEX idx_deployments_block ON payment_channel_deployments(deployed_at_block);
CREATE INDEX idx_deployments_issuer ON payment_channel_deployments(issuer_address);
```

**`payment_channel_events`** (all chequebook events)
```sql
CREATE TABLE payment_channel_events (
    id SERIAL PRIMARY KEY,
    event_type VARCHAR NOT NULL,  -- 'ChequeCashed', 'ChequeBounced', etc.
    chequebook_address VARCHAR NOT NULL,
    block_number BIGINT NOT NULL,
    block_timestamp TIMESTAMP NOT NULL,
    transaction_hash VARCHAR NOT NULL,
    log_index BIGINT NOT NULL,

    -- ChequeCashed fields
    beneficiary VARCHAR,
    recipient VARCHAR,
    caller VARCHAR,
    total_payout VARCHAR,  -- uint256 as string
    cumulative_payout VARCHAR,
    caller_payout VARCHAR,

    -- HardDeposit fields
    deposit_beneficiary VARCHAR,
    deposit_amount VARCHAR,
    deposit_decrease_amount VARCHAR,
    deposit_timeout BIGINT,

    -- Withdraw fields
    withdraw_amount VARCHAR,

    -- Raw data for debugging
    data JSONB,

    UNIQUE (transaction_hash, log_index),
    FOREIGN KEY (chequebook_address) REFERENCES payment_channel_deployments(chequebook_address)
);
CREATE INDEX idx_pc_events_chequebook ON payment_channel_events(chequebook_address);
CREATE INDEX idx_pc_events_block ON payment_channel_events(block_number);
CREATE INDEX idx_pc_events_type ON payment_channel_events(event_type);
CREATE INDEX idx_pc_events_timestamp ON payment_channel_events(block_timestamp);
CREATE INDEX idx_pc_events_beneficiary ON payment_channel_events(beneficiary);
```

### Contract Abstraction Strategy

**New Trait: `PaymentChannelFactory`**
```rust
pub trait PaymentChannelFactory: Send + Sync {
    fn name(&self) -> &str;
    fn address(&self) -> &str;
    fn deployment_block(&self) -> u64;
    fn parse_deployment_event(&self, log: Log, ...) -> Result<Option<ChequebookDeployment>>;
}
```

**Reuse Existing `Contract` Trait** for chequebooks
- Each discovered chequebook becomes a `Contract` implementation
- Parse events using standard `parse_log()` method
- Return `PaymentChannelEvent` instead of `StampEvent`

**Registry: `PaymentChannelRegistry`**
```rust
pub struct PaymentChannelRegistry {
    factory: Box<dyn PaymentChannelFactory>,
    known_chequebooks: HashMap<String, Box<dyn Contract>>,  // address -> contract
}
```

### Event Fetching Flow

**Existing**: `fetch_batch_events()` → iterate all contracts → fetch events

**New**: Two separate flows

**Flow 1: Factory Discovery** (new command: `discover-chequebooks`)
```
1. Load factory from config
2. Fetch SimpleSwapDeployed events in chunks (reuse existing infrastructure)
3. For each chunk:
   a. Parse events to extract chequebook addresses
   b. IMMEDIATELY store in payment_channel_deployments table (incremental)
   c. Update chunk cache to avoid re-scanning
4. Continue until all chunks processed
```

**Key**: Incremental storage per chunk (same pattern as existing fetch commands)

**Flow 2: Chequebook Event Tracking** (integrated into existing commands)
```
1. Load discovered chequebooks from database
2. For each chequebook, create Contract instance
3. Add to PaymentChannelRegistry
4. Use existing fetch_batch_events() infrastructure with on_chunk_complete callback
5. For each chunk:
   a. Parse chequebook events
   b. IMMEDIATELY store in payment_channel_events table (incremental)
   c. Update chunk cache
6. Continue until all chequebooks processed
```

**Key**: Reuse existing on_chunk_complete callback pattern for incremental storage

### Configuration

**config.yaml additions:**
```yaml
payment_channels:
  enabled: true
  factories:
    - name: "SimpleSwapFactory-Gnosis"
      address: "0xc2d5a532cf69aa9a1378737d8ccdef884b6e7420"
      deployment_block: TBD  # need to query
      network: "gnosis"
      active: true

    - name: "SimpleSwapFactory-Sepolia"
      address: "0x0fF044F6bB4F684a5A149B46D7eC03ea659F98A1"
      deployment_block: 4752810
      network: "sepolia"
      active: true
```

### CLI Commands

**New Commands:**

1. **`discover-chequebooks`** - Scan factory for deployments
   ```bash
   beeport-stamp-stats discover-chequebooks --from-block 4752810 --to-block latest [--refresh]
   ```
   - `--refresh`: Re-scan already cached chunks (force update)

2. **`sync-chequebooks`** - Sync events from discovered chequebooks
   ```bash
   beeport-stamp-stats sync-chequebooks --from-block 5000000 [--refresh]
   ```
   - `--refresh`: Re-fetch already cached chunks (force update)

3. **`cheque-summary`** - Analytics on cheque cashing
   ```bash
   beeport-stamp-stats cheque-summary --from-block 5000000 --to-block 6000000 [--output table|json|csv]
   ```

4. **`chequebook-balances`** - Export chequebook balances
   ```bash
   beeport-stamp-stats chequebook-balances [--output table|json|csv] [--refresh]
   ```
   - `--refresh`: Query RPC for current balances (bypass cache)

**Integration with Existing Commands:**

- **`follow`** mode - Add payment channel tracking
- **`sync`** - Option to include chequebooks

### Caching Strategy

**Reuse Existing Infrastructure:**

1. **RPC Chunk Caching** - Same SHA256(address, from, to) pattern
   - Factory chunks cached separately
   - Each chequebook chunks cached separately

2. **Block Timestamp Caching** - No changes needed

3. **New: Chequebook Discovery Cache**
   - Track last scanned block for factory
   - Avoid re-discovering known chequebooks

## Implementation Phases

### Phase 1: Database Schema & Migrations
**Files to create/modify:**
- `migrations/YYYYMMDD_payment_channels.sql`
- Test migration on beeport2_testing database

**Deliverables:**
- ✅ Three new tables created
- ✅ Indexes for performance
- ✅ Foreign key relationships

### Phase 2: Contract ABIs & Parsing
**Files to create/modify:**
- `src/contracts/abi.rs` - Add Factory and ERC20SimpleSwap ABIs
- `src/contracts/parser.rs` - Add parsing functions
- `src/events.rs` - Add PaymentChannelEvent types

**Deliverables:**
- ✅ Type-safe ABIs using sol! macro
- ✅ Parser for SimpleSwapDeployed events
- ✅ Parsers for all chequebook events

### Phase 3: Contract Implementations & Registry
**Files to create/modify:**
- `src/contracts/mod.rs` - Add PaymentChannelFactory trait
- `src/contracts/impls.rs` - Implement factory and chequebook contracts
- `src/contracts/payment_channel_registry.rs` (new file)

**Deliverables:**
- ✅ Factory trait and implementation
- ✅ Chequebook contract implementation
- ✅ PaymentChannelRegistry with dynamic loading

### Phase 4: Database Layer
**Files to modify:**
- `src/cache.rs` - Add methods for payment channel tables

**New methods:**
- `store_factory()`
- `store_chequebook_deployment()`
- `store_payment_channel_events()`
- `get_discovered_chequebooks()`
- `get_last_factory_scan_block()`

**Deliverables:**
- ✅ CRUD operations for all three tables
- ✅ Efficient queries for discovery

### Phase 5: CLI Commands - Discovery
**Files to modify:**
- `src/cli.rs` - Add discover-chequebooks command

**Deliverables:**
- ✅ New command to scan factory
- ✅ Progress reporting
- ✅ Integration with existing config system
- ✅ `--refresh` flag support (bypass cache)
- ✅ Incremental storage via on_chunk_complete callback

### Phase 6: CLI Commands - Sync
**Files to modify:**
- `src/cli.rs` - Add sync-chequebooks command
- `src/blockchain.rs` - Support dynamic contract loading

**Deliverables:**
- ✅ Sync discovered chequebooks
- ✅ Reuse existing fetch infrastructure with on_chunk_complete
- ✅ Incremental storage (events saved per chunk)
- ✅ `--refresh` flag support (re-fetch cached chunks)

### Phase 7: Analytics & Summary
**Files to create:**
- `src/commands/cheque_summary.rs` (new)
- `src/commands/chequebook_balances.rs` (new)

**Deliverables:**
- ✅ `cheque-summary`: Aggregate ChequeCashed events per chequebook
- ✅ `chequebook-balances`: Export all chequebooks with balances
- ✅ `--refresh` flag for balance queries (bypass cache)
- ✅ Export formats: ASCII table (solid borders), JSON, CSV
- ✅ Show overlay + eth addresses

### Phase 8: Testing & Validation
**Tasks:**
- Unit tests for parsers
- Integration tests with testnet data
- Verify against GnosisScan
- Performance testing with many chequebooks

## Critical Files to Modify

1. `src/contracts/mod.rs` - Add PaymentChannelFactory trait
2. `src/contracts/abi.rs` - Add factory and chequebook ABIs
3. `src/contracts/impls.rs` - Implement factory and chequebook
4. `src/contracts/parser.rs` - Add parsing functions
5. `src/events.rs` - Add PaymentChannelEvent types
6. `src/cache.rs` - Add database methods
7. `src/cli.rs` - Add new commands
8. `migrations/` - Create new migration
9. `config.yaml` - Add factory configuration

## Implementation Decisions (User Confirmed)

1. ✅ **Chequebook caching**: Cache in memory (loaded from DB at command start)
2. ✅ **Batch size**: Use same approach as existing codebase (chunk-based fetching)
3. ✅ **Issuer address**: Extract from event data (easiest approach)
4. ✅ **Incremental storage**: Store data as it's retrieved (per chunk), NOT at end of run
5. ✅ **Refresh functionality**: Implement `--refresh` flag where relevant (bypass cache)
6. ✅ **Analytics requirements**:
   - **Cheque summary**: Total cheques cashed per chequebook over specified block range
     - Show overlay address and eth address for each chequebook
     - New CLI command for this
   - **Balance export**: Bulk export of all chequebook addresses with current balance
   - **Export formats**: All data exportable as CSV, JSON, or ASCII table (solid borders)

## Analytics Commands (Detailed)

### 1. Cheque Summary Command
```bash
beeport-stamp-stats cheque-summary --from-block 5000000 --to-block 6000000 [--output json|csv|table]
```

**Output columns**:
- Chequebook Address (0x...)
- Overlay Address (if available)
- Total Cheques Cashed (count)
- Total Amount Cashed (sum of totalPayout)
- Block Range

**Formats**:
- `--output table` (default): ASCII table with solid borders
- `--output json`: JSON array of objects
- `--output csv`: CSV with headers

**Note**: Uses cached event data from database (fast)

### 2. Chequebook Balance Export
```bash
beeport-stamp-stats chequebook-balances [--output json|csv|table] [--refresh]
```

**Output columns**:
- Chequebook Address
- Overlay Address
- Current Balance (from RPC call)
- Last Updated Block

**Flags**:
- `--refresh`: Force RPC balance query (bypass cache)
- Without `--refresh`: Use cached balances from database

**Note**: Without `--refresh`, uses cached balances (fast). With `--refresh`, queries RPC for all chequebooks (slow but current)

**Status**: Plan approved, ready for implementation
**Last Updated**: 2026-01-21
