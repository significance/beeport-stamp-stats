# Payment Channel Tracking - Architecture Overview

## Problem

Swarm's bandwidth incentive system uses per-node payment channels (ERC20SimpleSwap chequebooks) deployed via a factory contract. We need to:
1. Discover all deployed chequebook contracts
2. Track events from thousands of dynamically deployed contracts
3. Enable analytics on payment flows and channel states

## Solution: Two-Phase Architecture

### Phase 1: Factory Discovery
**Track the factory contract** that deploys chequebooks:
- Monitor `SimpleSwapDeployed(address)` events
- Store discovered chequebook addresses in `payment_channel_deployments` table
- One factory contract → many chequebook instances

### Phase 2: Chequebook Event Tracking
**Monitor events from discovered chequebooks**:
- ChequeCashed - payment settlements
- ChequeBounced - failed payments
- HardDeposit* - collateral changes
- Withdraw - balance withdrawals

## Key Design Decisions

### 1. Database Schema (3 tables)

```
payment_channel_factories
  ↓ (1:many)
payment_channel_deployments (discovered chequebooks)
  ↓ (1:many)
payment_channel_events (all chequebook events)
```

**Rationale**: Separate tables enable efficient queries and clear data relationships

### 2. Contract Abstraction

**New**: `PaymentChannelFactory` trait for factory contracts
**Reuse**: Existing `Contract` trait for individual chequebooks

**Rationale**: Reuse proven infrastructure (RPC caching, retry logic, chunk fetching)

### 3. Dynamic Contract Loading

Chequebooks loaded on-demand from database, not config file

**Rationale**: Thousands of contracts - can't preload all in memory

### 4. Caching Strategy

- Factory events cached per chunk (SHA256 hash)
- Each chequebook's events cached independently
- Track last scanned block per factory

**Rationale**: Reuse existing caching infrastructure, avoid redundant RPC calls

## Implementation Flow

### Discovery Command
```bash
beeport-stamp-stats discover-chequebooks --from-block 4752810 [--refresh]
```
1. Fetch `SimpleSwapDeployed` events from factory in chunks
2. **Per chunk**: Extract chequebook addresses → IMMEDIATELY store in DB
3. Incremental storage (not at end of run)
4. `--refresh`: Bypass cache, re-scan chunks

### Sync Command
```bash
beeport-stamp-stats sync-chequebooks --from-block 5000000 [--refresh]
```
1. Load discovered chequebooks from database
2. For each chequebook, fetch events in chunks
3. **Per chunk**: Parse events → IMMEDIATELY store in DB
4. Incremental storage using on_chunk_complete callback
5. `--refresh`: Re-fetch already cached chunks

## Factory Deployments

### Gnosis Chain (Mainnet)
- Factory: `0xc2d5a532cf69aa9a1378737d8ccdef884b6e7420`
- Token: `0xdbf3ea6f5bee45c02255b2c26a16f300502f68da` (BZZ)

### Sepolia (Testnet)
- Factory: `0x0fF044F6bB4F684a5A149B46D7eC03ea659F98A1`
- Token: `0x543dDb01Ba47acB11de34891cD86B675F04840db`
- Deployment Block: 4752810

## Analytics Commands

### 1. Cheque Summary (Per Chequebook)
```bash
beeport-stamp-stats cheque-summary --from-block 5000000 --to-block 6000000 --output table
```

Shows per chequebook over specified block range:
- Chequebook address (0x...)
- Overlay address
- Total cheques cashed (count)
- Total amount cashed (sum)

**Export formats**: `--output table` (ASCII with borders) | `json` | `csv`

### 2. Chequebook Balance Export
```bash
beeport-stamp-stats chequebook-balances --output json [--refresh]
```

Exports all discovered chequebooks with:
- Chequebook address
- Overlay address
- Current balance (from RPC)
- Last updated block

**Flags**:
- `--refresh`: Query RPC for current balances (slow but accurate)
- Without `--refresh`: Use cached balances from DB (fast)

**Export formats**: `--output table` | `json` | `csv`

### Additional Analytics
- Active chequebook count
- Settlement success/failure rates
- Collateral (HardDeposit) trends

## Architecture Principles

✅ **Separation of Concerns**: Factory discovery separate from event tracking
✅ **Incremental Storage**: Data saved per chunk, not at end of run
✅ **Reuse Infrastructure**: Leverage existing RPC caching, retry logic, on_chunk_complete
✅ **Type Safety**: Alloy sol! macro for compile-time ABI validation
✅ **Configuration-Driven**: Factory contracts in config.yaml
✅ **Scalable**: Handle thousands of dynamically deployed contracts
✅ **Refresh Capability**: `--refresh` flag bypasses cache for current data

## Files Modified

**New**:
- `migrations/YYYYMMDD_payment_channels.sql` - Database schema
- `src/contracts/payment_channel_registry.rs` - Dynamic registry
- `src/commands/chequebook_summary.rs` - Analytics

**Modified**:
- `src/contracts/mod.rs` - PaymentChannelFactory trait
- `src/contracts/abi.rs` - Factory + chequebook ABIs
- `src/contracts/impls.rs` - Contract implementations
- `src/contracts/parser.rs` - Event parsers
- `src/cache.rs` - Database methods
- `src/cli.rs` - New commands

## Trade-offs

**Pros**:
- Clean separation (discovery vs tracking)
- Reuses proven infrastructure
- Efficient (caching, incremental sync)
- Extensible (easy to add new event types)

**Cons**:
- Two-step process (discover, then sync)
- Additional database tables
- Need to periodically run discovery for new deployments

## Next Steps

1. **Phase 1**: Database migrations
2. **Phase 2**: ABIs and event parsers
3. **Phase 3**: Contract implementations
4. **Phase 4**: Database layer
5. **Phase 5-7**: CLI commands and analytics
6. **Phase 8**: Testing and validation
