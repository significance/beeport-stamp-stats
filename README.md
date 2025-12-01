# Beeport Postage Stamp Statistics

A high-performance Rust CLI tool for tracking and analyzing Swarm postage stamp events on Gnosis Chain using direct Ethereum RPC calls.

## Overview

This tool provides real-time tracking and historical analysis of postage stamp batch events from the Swarm network's PostageStamp contract on Gnosis Chain. Built with modern Rust (2024 edition) and the Alloy framework, it offers efficient blockchain querying, SQLite caching, and beautiful markdown-formatted output.

## Features

### Core Capabilities
- **Direct RPC Access**: Uses Ethereum RPC directly instead of third-party APIs for reliable, rate-limit-free access
- **Multi-Contract Support**: Monitor multiple contracts simultaneously
  - PostageStamp contract (direct stamp purchases)
  - StampsRegistry contract (UI-based stamp purchases)
  - Easy to extend with additional contracts
- **Event Tracking**: Monitors all PostageStamp contract events:
  - `BatchCreated` - New postage batch creation
  - `BatchTopUp` - Batch balance increases
  - `BatchDepthIncrease` - Batch storage depth expansions
- **Contract-Specific Hooks**: Generic hook system for custom event handling per contract
- **SQLite Caching**: Persistent local database for fast historical queries
- **Beautiful Output**: Markdown-formatted tables for easy reading
- **Incremental Sync**: Fetch only new events since last run
- **Advanced Filtering**: Filter events by type, batch ID, and contract source
- **Data Export**: Export to CSV or JSON for external analysis
  - Export events, batches, or aggregated statistics
  - Apply filters during export
  - Time-range selection
- **Comprehensive Testing**: 28 unit tests with 100% coverage of core functionality

### Technology Stack
- **Rust 2024 Edition** - Latest Rust features and idioms
- **Alloy 0.8** - Modern Ethereum library (successor to ethers-rs)
- **SQLx 0.8** - Type-safe SQL with compile-time verification
- **Clap 4.5** - Elegant CLI with environment variable support
- **Tokio 1.43** - High-performance async runtime
- **Tabled 0.17** - Beautiful table formatting

## Installation

### Prerequisites
- Rust 1.89+ (2024 edition)
- SQLite 3.x
- Gnosis Chain RPC endpoint (default: https://rpc.gnosis.gateway.fm)

### Build from Source

```bash
# Clone the repository
cd beeport-tx-stats

# Build the release binary
cargo build --release

# The binary will be at target/release/beeport-stamp-stats
```

## Usage

### Commands

#### 1. Fetch Events

Retrieve postage stamp events from the blockchain and cache them:

```bash
# Fetch all events from contract deployment
beeport-stamp-stats fetch

# Fetch events from specific block range
beeport-stamp-stats fetch --from-block 19275989 --to-block 30000000

# Incremental sync (fetch only new events since last run)
beeport-stamp-stats fetch --incremental

# Use custom RPC endpoint
beeport-stamp-stats --rpc-url https://rpc.gnosischain.com fetch
```

#### 2. Summary Statistics

Display analytics from cached data with optional filtering:

```bash
# View summary grouped by week (last 12 months)
beeport-stamp-stats summary

# Group by day
beeport-stamp-stats summary --group-by day

# Group by month
beeport-stamp-stats summary --group-by month

# Analyze all-time data
beeport-stamp-stats summary --months 0

# Last 6 months
beeport-stamp-stats summary --months 6

# Filter by event type (only show BatchCreated events)
beeport-stamp-stats summary --event-type batch-created

# Filter by specific batch ID (partial match)
beeport-stamp-stats summary --batch-id 0x1234

# Filter by contract source
beeport-stamp-stats summary --contract postage-stamp
beeport-stamp-stats summary --contract stamps-registry

# Combine filters - BatchTopUp events for specific batch from PostageStamp
beeport-stamp-stats summary --event-type batch-top-up --batch-id 0xabcd --contract postage-stamp
```

#### 3. Follow Mode (Real-time)

Watch the blockchain for new postage stamp events in real-time:

```bash
# Follow with default 12s polling
beeport-stamp-stats follow

# Custom poll interval (in seconds)
beeport-stamp-stats follow --poll-interval 5

# Follow without displaying events (hooks only)
beeport-stamp-stats follow --display=false
```

**How it works:**
1. First ensures historical sync (fetches any missed events)
2. Then polls blockchain every N seconds for new events
3. Invokes event hooks for each new event
4. Caches and optionally displays new events
5. Runs indefinitely until Ctrl+C

**Event Hooks:**
The follow mode includes a generic hook system that triggers on each new event with contract-specific handlers:
- `on_event()` - Called for all events
- `on_postage_stamp_event()` - Called for PostageStamp contract events
- `on_stamps_registry_event()` - Called for StampsRegistry contract events

Currently implements a stub hook for demonstration, but can be extended to:
- Send notifications (email, Slack, Discord)
- Trigger webhooks
- Update external databases
- Execute contract-specific custom logic
- Route events to different systems based on contract source

#### 4. Export Data

Export cached data to CSV or JSON for further analysis:

```bash
# Export all events to JSON
beeport-stamp-stats export --output events.json

# Export events to CSV
beeport-stamp-stats export --output events.csv --format csv

# Export only batches
beeport-stamp-stats export --data-type batches --output batches.json

# Export period statistics
beeport-stamp-stats export --data-type stats --output stats.csv --format csv

# Export events from last 6 months only
beeport-stamp-stats export --output recent.json --months 6

# Export only BatchCreated events
beeport-stamp-stats export --output created.csv --format csv --event-type batch-created

# Export events for specific batch
beeport-stamp-stats export --output batch-history.json --batch-id 0x1234

# Export events from specific contract
beeport-stamp-stats export --output stamps-registry-events.json --contract stamps-registry

# Complex filter - BatchTopUp events from PostageStamp contract, last 3 months, specific batch
beeport-stamp-stats export \
  --output topups.csv \
  --format csv \
  --event-type batch-top-up \
  --batch-id 0xabcd \
  --contract postage-stamp \
  --months 3
```

### Environment Variables

```bash
# Set custom RPC URL
export RPC_URL=https://rpc.gnosischain.com

# Set custom cache location
export CACHE_DB=/path/to/stamp-cache.db
```

### Examples

**First-time setup - fetch all events:**
```bash
beeport-stamp-stats fetch --from-block 19275989
```

**Daily update - fetch new events:**
```bash
beeport-stamp-stats fetch --incremental
```

**View recent activity by week:**
```bash
beeport-stamp-stats summary --group-by week --months 3
```

**Analyze only batch creation events:**
```bash
beeport-stamp-stats summary --event-type batch-created --group-by month
```

**Track a specific batch:**
```bash
beeport-stamp-stats summary --batch-id 0xabcd1234
```

**Export all data for external analysis:**
```bash
beeport-stamp-stats export --output all-events.csv --format csv
```

**Create a report of batch top-ups:**
```bash
beeport-stamp-stats export \
  --output topups-report.json \
  --event-type batch-top-up \
  --months 6
```

**Monitor blockchain in real-time:**
```bash
# Start following for new events
beeport-stamp-stats follow

# Follow with custom interval
beeport-stamp-stats follow --poll-interval 6
```

## Output Examples

### Event Listing (Fetch Command)

```markdown
## Postage Stamp Events

| Block     | Type                 | Batch ID    | Details                                    | Timestamp        |
|-----------|---------------------|-------------|---------------------------------------------|------------------|
| 30123456  | BatchCreated        | 0x1234...ef | Owner: 0xabcd...89, Depth: 20, Bucket: 16 | 2025-01-15 14:30 |
| 30123789  | BatchTopUp          | 0x1234...ef | Top-up: 50.0000 PLUR                       | 2025-01-15 15:45 |
| 30124012  | BatchDepthIncrease  | 0x1234...ef | New Depth: 21                              | 2025-01-15 16:20 |

**Total events:** 3
```

### Summary Statistics

```markdown
## Postage Stamp Statistics Summary

### Overall Statistics

- **Total Events:** 156
- **Batch Created:** 42
- **Batch Top-ups:** 98
- **Batch Depth Increases:** 16
- **Unique Batches:** 42

### Time Range

- **From:** 2025-01-01 00:00
- **To:** 2025-11-30 23:59
- **Duration:** 333 days

### Activity by Week

| Period          | Created | Top-ups | Depth Inc. | Total Events | Unique Batches |
|-----------------|---------|---------|------------|--------------|----------------|
| Week 1 of 2025  | 3       | 5       | 1          | 9            | 3              |
| Week 2 of 2025  | 2       | 8       | 0          | 10           | 2              |
...

### Most Active Period

**Week 48 of 2025** with 23 events

### Recent Batches

| Batch ID    | Owner       | Depth | Bucket Depth | Immutable | Created          |
|-------------|-------------|-------|--------------|-----------|------------------|
| 0x1234...ef | 0xabcd...89 | 20    | 16           | No        | 2025-11-28 14:30 |
...
```

### Export Formats

**CSV Export (events.csv):**
```csv
block_number,timestamp,event_type,batch_id,transaction_hash,log_index,details
30123456,2025-01-15T14:30:00+00:00,BatchCreated,0x1234...ef,0xabcd...89,0,"{""type"":""BatchCreated"",""total_amount"":""1000000""...}"
30123789,2025-01-15T15:45:00+00:00,BatchTopUp,0x1234...ef,0xabcd...90,0,"{""type"":""BatchTopUp"",""topup_amount"":""500000""...}"
```

**JSON Export (events.json):**
```json
[
  {
    "event_type": "BatchCreated",
    "batch_id": "0x1234...ef",
    "block_number": 30123456,
    "block_timestamp": "2025-01-15T14:30:00+00:00",
    "transaction_hash": "0xabcd...89",
    "log_index": 0,
    "data": {
      "type": "BatchCreated",
      "total_amount": "1000000000000000000",
      "normalised_balance": "500000000000000000",
      "owner": "0x5678",
      "depth": 20,
      "bucket_depth": 16,
      "immutable_flag": false
    }
  }
]
```

## Contract Information

### PostageStamp Contract
- **Network:** Gnosis Chain
- **Contract Address:** `0x45a1502382541Cd610CC9068e88727426b696293`
- **Contract Type:** PostageStamp (Direct stamp purchases)
- **Deployment Block:** ~37,000,000
- **Explorer:** [View on GnosisScan](https://gnosisscan.io/address/0x45a1502382541Cd610CC9068e88727426b696293)

### StampsRegistry Contract
- **Network:** Gnosis Chain
- **Contract Address:** `0x5EBfBeFB1E88391eFb022d5d33302f50a46bF4f3`
- **Contract Type:** StampsRegistry (UI-based stamp purchases with payer tracking)
- **Deployment Block:** ~37,000,000
- **Explorer:** [View on GnosisScan](https://gnosisscan.io/address/0x5EBfBeFB1E88391eFb022d5d33302f50a46bF4f3)

### Understanding Contract Event Relationships

**Important:** When a batch is created through the StampsRegistry contract, **both contracts emit a BatchCreated event in the same transaction**. This means:

- Every StampsRegistry `BatchCreated` event also appears in PostageStamp events
- The StampsRegistry contract internally calls the PostageStamp contract
- Both events fire in sequence (PostageStamp first, then StampsRegistry)
- They share the same transaction hash and block number but have different log indices

**Event Counting:**
- **Total PostageStamp BatchCreated events:** 6,118
- **Total StampsRegistry BatchCreated events:** 303
- **Overlapping batch IDs:** 303 (100% of StampsRegistry events)
- **Direct PostageStamp purchases:** 5,815 (6,118 - 303)

**To get accurate counts:**
```bash
# Total batches created (count PostageStamp events only to avoid double-counting)
beeport-stamp-stats summary --contract postage-stamp --event-type batch-created

# Batches created via StampsRegistry (UI-based purchases with payer tracking)
beeport-stamp-stats summary --contract stamps-registry --event-type batch-created

# Direct batches (created directly on PostageStamp, not via StampsRegistry)
# This requires filtering: PostageStamp count minus StampsRegistry count = 5,815
```

**Why track both?**
- **PostageStamp events:** Show all batches created (complete dataset)
- **StampsRegistry events:** Identify which batches were created via the UI and include payer information
- **Difference:** Reveals batches created programmatically or via other interfaces

## Extending with New Contracts

The system is designed to be easily extensible. Here's how to add support for a new contract:

### Step 1: Add Contract Definition

In `src/contracts.rs`, add your contract to the `ContractType` enum:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContractType {
    PostageStamp,
    StampsRegistry,
    YourNewContract,  // Add your contract here
}
```

Add the contract address constant:

```rust
// Your contract address on Gnosis Chain
pub const YOUR_CONTRACT_ADDRESS: &str = "0x...";
```

Update the `ContractType` implementation:

```rust
impl ContractType {
    pub fn address(&self) -> &'static str {
        match self {
            ContractType::PostageStamp => POSTAGE_STAMP_ADDRESS,
            ContractType::StampsRegistry => STAMPS_REGISTRY_ADDRESS,
            ContractType::YourNewContract => YOUR_CONTRACT_ADDRESS,  // Add here
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            ContractType::PostageStamp => "PostageStamp",
            ContractType::StampsRegistry => "StampsRegistry",
            ContractType::YourNewContract => "YourContract",  // Add here
        }
    }

    pub fn all() -> Vec<ContractType> {
        vec![
            ContractType::PostageStamp,
            ContractType::StampsRegistry,
            ContractType::YourNewContract,  // Add here
        ]
    }
}
```

Define the contract ABI using Alloy's `sol!` macro:

```rust
sol! {
    #[allow(missing_docs)]
    #[sol(rpc)]
    YourContract,
    r#"[
        {
            "anonymous": false,
            "inputs": [
                {
                    "indexed": true,
                    "internalType": "bytes32",
                    "name": "eventField",
                    "type": "bytes32"
                }
            ],
            "name": "YourEvent",
            "type": "event"
        }
    ]"#
}
```

### Step 2: Add Event Parser

In `src/blockchain.rs`, add a parser method for your contract's events:

```rust
/// Parse YourContract events
fn parse_your_contract_log(
    &self,
    log: Log,
    block_number: u64,
    block_timestamp: DateTime<Utc>,
    transaction_hash: alloy::primitives::TxHash,
    log_index: u64,
    contract_source: String,
) -> Result<Option<StampEvent>> {
    // Try to parse as YourEvent
    if let Ok(event) = YourContract::YourEvent::decode_log(&log.inner, true) {
        return Ok(Some(StampEvent {
            event_type: EventType::BatchCreated,  // Or your event type
            batch_id: format!("{:?}", event.eventField),
            block_number,
            block_timestamp,
            transaction_hash: format!("{:?}", transaction_hash),
            log_index,
            contract_source,
            data: EventData::BatchCreated {  // Map to appropriate EventData
                // ... populate fields from event
            },
        }));
    }

    Ok(None)
}
```

Update the `parse_log` method to route to your parser:

```rust
match contract_type {
    ContractType::PostageStamp => self.parse_postage_stamp_log(...),
    ContractType::StampsRegistry => self.parse_stamps_registry_log(...),
    ContractType::YourNewContract => self.parse_your_contract_log(...),  // Add here
}
```

### Step 3: Add Custom Event Hook (Optional)

In `src/hooks.rs`, add a handler for your contract's events:

```rust
pub trait EventHook: Send + Sync {
    fn on_event(&self, event: &StampEvent);
    fn on_postage_stamp_event(&self, event: &StampEvent) { /* ... */ }
    fn on_stamps_registry_event(&self, event: &StampEvent) { /* ... */ }

    /// Called when a new event is detected from YourContract
    fn on_your_contract_event(&self, event: &StampEvent) {
        tracing::debug!(
            "YourContract event: {} at block {}",
            event.event_type,
            event.block_number
        );
    }
}
```

Update the `StubHook` implementation to route events:

```rust
impl EventHook for StubHook {
    fn on_event(&self, event: &StampEvent) {
        // Route to contract-specific handlers
        match event.contract_source.as_str() {
            "PostageStamp" => self.on_postage_stamp_event(event),
            "StampsRegistry" => self.on_stamps_registry_event(event),
            "YourContract" => self.on_your_contract_event(event),  // Add here
            _ => tracing::warn!("Unknown contract source: {}", event.contract_source),
        }
    }

    fn on_your_contract_event(&self, event: &StampEvent) {
        tracing::info!(
            "YourContract: {} event at block {}",
            event.event_type,
            event.block_number
        );
        // Add custom logic here:
        // - Send webhook notifications
        // - Update external systems
        // - Trigger alerts
        // - Custom business logic
    }
}
```

### Step 4: Add CLI Filter Option (Optional)

In `src/cli.rs`, update the `FilterContract` enum:

```rust
#[derive(Debug, Clone, clap::ValueEnum)]
pub enum FilterContract {
    PostageStamp,
    StampsRegistry,
    YourContract,  // Add here
}

impl FilterContract {
    fn matches(&self, contract_source: &str) -> bool {
        matches!(
            (self, contract_source),
            (FilterContract::PostageStamp, "PostageStamp")
                | (FilterContract::StampsRegistry, "StampsRegistry")
                | (FilterContract::YourContract, "YourContract")  // Add here
        )
    }
}
```

### Step 5: Update Display (Optional)

In `src/display.rs`, update the `truncate_contract_name` function:

```rust
fn truncate_contract_name(contract: &str) -> String {
    match contract {
        "PostageStamp" => "PostageStamp".to_string(),
        "StampsRegistry" => "StampsReg".to_string(),
        "YourContract" => "YourCont".to_string(),  // Add here
        _ => contract.to_string(),
    }
}
```

### Step 6: Test Your Integration

```bash
# Run tests
cargo test

# Fetch events from your new contract
cargo run --release -- fetch --from-block <start-block>

# View summary with contract breakdown
cargo run --release -- summary --months 0

# Filter by your contract
cargo run --release -- summary --contract your-contract

# Follow mode (monitors all contracts including yours)
cargo run --release -- follow
```

### Complete Example

For a complete working example, see how `StampsRegistry` was added alongside `PostageStamp`:
- Contract definition: `src/contracts.rs:39-235`
- Event parser: `src/blockchain.rs:261-329`
- Hook handler: `src/hooks.rs:61-70`
- CLI filter: `src/cli.rs:142-156`

## Development

### Running Tests

```bash
# Run all tests
cargo test

# Run tests with output
cargo test -- --nocapture

# Run specific test
cargo test test_cache_creation
```

### Project Structure

```
src/
├── main.rs          # Entry point and async runtime setup
├── cli.rs           # Command-line interface and argument parsing
├── contracts.rs     # Contract definitions and ABIs
├── blockchain.rs    # Multi-contract RPC client and event fetching
├── cache.rs         # SQLite database operations
├── events.rs        # Event types and data structures
├── hooks.rs         # Generic event hook system with contract-specific handlers
├── batch.rs         # Batch aggregation and statistics
├── display.rs       # Markdown table formatting
├── export.rs        # Data export functionality
└── error.rs         # Error types and handling
```

### Key Dependencies

- `alloy = 0.8` - Ethereum interactions (full feature set)
- `clap = 4.5` - CLI framework with derive macros
- `tokio = 1.43` - Async runtime
- `sqlx = 0.8` - SQLite with async support
- `tabled = 0.17` - Table formatting
- `chrono = 0.4` - Date/time handling
- `serde = 1.0` - Serialization
- `anyhow = 1.0` - Error handling
- `tracing = 0.1` - Structured logging

## Performance

- **RPC Efficiency**: Fetches events in 10,000-block chunks to optimize RPC calls
- **Caching**: All events stored locally - subsequent queries are instant
- **Memory**: Efficient streaming of large event sets
- **Concurrent**: Async/await for non-blocking I/O

## Troubleshooting

### RPC Connection Issues

```bash
# Test with different RPC endpoint
beeport-stamp-stats --rpc-url https://gnosis-pokt.nodies.app fetch
```

### Database Issues

```bash
# Remove cache and start fresh
rm stamp-cache.db
beeport-stamp-stats fetch
```

### Enable Debug Logging

```bash
# Set log level
RUST_LOG=debug beeport-stamp-stats fetch
```

## Legacy Scripts

The `gnosis-tx-stats-v2.js` script is still available for reference but is now superseded by this Rust implementation which offers:
- Direct RPC access (no API rate limits)
- Better performance
- Type safety
- Comprehensive testing
- More reliable event tracking

## Contributing

Contributions welcome! Please ensure:
- All tests pass: `cargo test`
- Code is formatted: `cargo fmt`
- No clippy warnings: `cargo clippy`

## License

MIT License - Use freely for blockchain data analysis.
