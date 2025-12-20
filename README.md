# Beeport Postage Stamp Statistics

A high-performance Rust CLI tool for tracking and analyzing Swarm postage stamp events on Gnosis Chain using direct Ethereum RPC calls.

## Overview

This tool provides real-time tracking and historical analysis of postage stamp batch events from the Swarm network's PostageStamp contract on Gnosis Chain. Built with modern Rust (2024 edition) and the Alloy framework, it offers efficient blockchain querying, SQLite caching, and beautiful markdown-formatted output.

## Quick Start

Get up and running in 3 steps:

```bash
# 1. Build the release binary
cargo build --release

# 2. Fetch events from all 5 contracts (PostageStamp, StampsRegistry, PriceOracle, StakeRegistry, Redistribution)
./target/release/beeport-stamp-stats fetch --from-block 41105000 --to-block 41106000

# 3. View batch status with TTL and expiry dates
./target/release/beeport-stamp-stats batch-status --sort-by ttl
```

**What you'll get:**
- ✅ All postage stamp events (BatchCreated, BatchTopUp, BatchDepthIncrease)
- ✅ Storage incentives events (PriceUpdate, StakeUpdated, Redistribution game events)
- ✅ Batch TTL and expiry predictions
- ✅ SQLite cache for instant future queries

**Common workflows:**

```bash
# Initial historical sync (fetches all events from contract deployment)
./target/release/beeport-stamp-stats fetch

# Daily updates (only fetch new events)
./target/release/beeport-stamp-stats sync

# Monitor blockchain in real-time
./target/release/beeport-stamp-stats follow

# View activity summary
./target/release/beeport-stamp-stats summary --group-by week --months 3

# Analyze batch expirations
./target/release/beeport-stamp-stats expiry-analytics --period week

# Export data for analysis
./target/release/beeport-stamp-stats export --output events.csv --format csv
```

**Storage Incentives Analytics:**

The tool now tracks storage incentives contracts for comprehensive Swarm network analysis:

```bash
# Price history analysis (PriceOracle events)
sqlite3 stamp-cache.db "SELECT round_number, price, block_timestamp FROM storage_incentives_events WHERE event_type='PriceUpdate' ORDER BY block_number DESC LIMIT 20"

# Redistribution game participation (Redistribution events)
sqlite3 stamp-cache.db "SELECT event_type, COUNT(*) FROM storage_incentives_events WHERE contract_source='Redistribution' GROUP BY event_type"

# Node staking activity (StakeRegistry events)
sqlite3 stamp-cache.db "SELECT owner_address, COUNT(*) as updates FROM storage_incentives_events WHERE event_type='StakeUpdated' GROUP BY owner_address ORDER BY updates DESC LIMIT 10"
```

## Features

### Core Capabilities
- **Direct RPC Access**: Uses Ethereum RPC directly instead of third-party APIs for reliable, rate-limit-free access
- **Multi-Contract Support**: Monitor 5 contracts simultaneously
  - **PostageStamp** - Direct stamp purchases
  - **StampsRegistry** - UI-based stamp purchases with payer tracking
  - **PriceOracle** - Dynamic price adjustments (storage cost tracking)
  - **StakeRegistry** - Node staking for redistribution game
  - **Redistribution** - Schelling coordination game for storage incentives
- **Comprehensive Event Tracking**:
  - **Postage Stamps**: BatchCreated, BatchTopUp, BatchDepthIncrease
  - **Storage Incentives**: PriceUpdate, StakeUpdated, StakeSlashed, StakeFrozen, Committed, Revealed, WinnerSelected, and more (18 event types total)
- **Contract-Specific Hooks**: Generic hook system for custom event handling per contract
- **SQLite Caching**: Persistent local database for fast historical queries
- **Beautiful Output**: Markdown-formatted tables for easy reading
- **Incremental Sync**: Fetch only new events since last run
- **Advanced Filtering**: Filter events by type, batch ID, and contract source
- **Data Export**: Export to CSV or JSON for external analysis
  - Export events, batches, or aggregated statistics
  - Apply filters during export
  - Time-range selection
- **Batch Analysis Tools**:
  - **Batch Status**: View TTL, expiry dates, and size for all batches
  - **Expiry Analytics**: Aggregate batch expirations by time period
  - **Price Modeling**: Model price changes to predict batch lifetimes
  - **Multiple Export Formats**: Table, CSV, and JSON output
- **Comprehensive Testing**: 41 unit tests with 100% coverage of core functionality

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

## Configuration

Beeport Stamp Stats supports flexible configuration through multiple sources with clear priority:

**Priority Order:** CLI arguments > Environment variables > Config file > Built-in defaults

### Configuration File Formats

The tool supports **YAML**, **TOML**, and **JSON** configuration files. Choose the format you prefer:

#### YAML Format (config.yaml)

```yaml
# Beeport Stamp Stats Configuration
# ==================================
# All settings are optional - the tool has sensible defaults for everything.
# Priority: CLI arguments > Environment variables > Config file > Built-in defaults

# RPC Configuration
rpc:
  url: "https://rpc.gnosis.gateway.fm"
  # Alternative endpoints:
  # - https://rpc.gnosischain.com
  # - https://gnosis-pokt.nodies.app
  # - https://gnosis.drpc.org

# Database Configuration
database:
  path: "./stamp-cache.db"
  # For PostgreSQL: "postgres://user:pass@localhost/stamps"
  # For PostgreSQL with SSL: "postgres://user:pass@db.example.com:5432/stamps?sslmode=require"

# Blockchain Configuration
blockchain:
  chunk_size: 10000          # Blocks per RPC chunk (larger = fewer calls, may hit limits)
  block_time_seconds: 5.0    # Gnosis Chain block time (used for TTL calculations)

# Contract Configuration (all 5 contracts)
contracts:
  # Postage Stamp Contracts
  - name: "PostageStamp"
    contract_type: "PostageStamp"
    address: "0x45a1502382541Cd610CC9068e88727426b696293"
    deployment_block: 31305656

  - name: "StampsRegistry"
    contract_type: "StampsRegistry"
    address: "0x5EBfBeFB1E88391eFb022d5d33302f50a46bF4f3"
    deployment_block: 42390510

  # Storage Incentives Contracts
  - name: "PriceOracle"
    contract_type: "PriceOracle"
    address: "0x47EeF336e7fE5bED98499A4696bce8f28c1B0a8b"
    deployment_block: 37339168

  - name: "StakeRegistry"
    contract_type: "StakeRegistry"
    address: "0xda2a16EE889E7f04980A8d597b48c8D51B9518F4"
    deployment_block: 40430237

  - name: "Redistribution"
    contract_type: "Redistribution"
    address: "0x5069cdfB3D9E56d23B1cAeE83CE6109A7E4fd62d"
    deployment_block: 41105199

# Retry Configuration (for rate-limited RPC calls)
retry:
  max_retries: 5                      # Fast retries before extended retry
  initial_delay_ms: 100               # Initial delay before first retry
  backoff_multiplier: 4               # Exponential backoff multiplier
  extended_retry_wait_seconds: 300    # Wait time for extended retry (5 minutes)
```

#### TOML Format (config.toml)

```toml
# Beeport Stamp Stats Configuration
# ==================================
# All settings are optional - the tool has sensible defaults for everything.
# Priority: CLI arguments > Environment variables > Config file > Built-in defaults

# RPC Configuration
[rpc]
url = "https://rpc.gnosis.gateway.fm"
# Alternative endpoints:
# - https://rpc.gnosischain.com
# - https://gnosis-pokt.nodies.app
# - https://gnosis.drpc.org

# Database Configuration
[database]
path = "./stamp-cache.db"
# For PostgreSQL: "postgres://user:pass@localhost/stamps"
# For PostgreSQL with SSL: "postgres://user:pass@db.example.com:5432/stamps?sslmode=require"

# Blockchain Configuration
[blockchain]
chunk_size = 10000              # Blocks per RPC chunk (larger = fewer calls, may hit limits)
block_time_seconds = 5.0        # Gnosis Chain block time (used for TTL calculations)

# Retry Configuration (for rate-limited RPC calls)
[retry]
max_retries = 5                      # Fast retries before extended retry
initial_delay_ms = 100               # Initial delay before first retry
backoff_multiplier = 4               # Exponential backoff multiplier
extended_retry_wait_seconds = 300    # Wait time for extended retry (5 minutes)

# Contract Configuration
# Postage Stamp Contracts
[[contracts]]
name = "PostageStamp"
contract_type = "PostageStamp"
address = "0x45a1502382541Cd610CC9068e88727426b696293"
deployment_block = 31305656

[[contracts]]
name = "StampsRegistry"
contract_type = "StampsRegistry"
address = "0x5EBfBeFB1E88391eFb022d5d33302f50a46bF4f3"
deployment_block = 42390510

# Storage Incentives Contracts
[[contracts]]
name = "PriceOracle"
contract_type = "PriceOracle"
address = "0x47EeF336e7fE5bED98499A4696bce8f28c1B0a8b"
deployment_block = 37339168

[[contracts]]
name = "StakeRegistry"
contract_type = "StakeRegistry"
address = "0xda2a16EE889E7f04980A8d597b48c8D51B9518F4"
deployment_block = 40430237

[[contracts]]
name = "Redistribution"
contract_type = "Redistribution"
address = "0x5069cdfB3D9E56d23B1cAeE83CE6109A7E4fd62d"
deployment_block = 41105199
```

#### JSON Format (config.json)

```json
{
  "rpc": {
    "url": "https://rpc.gnosis.gateway.fm"
  },
  "database": {
    "path": "./stamp-cache.db"
  },
  "blockchain": {
    "chunk_size": 10000,
    "block_time_seconds": 5.0
  },
  "contracts": [
    {
      "name": "PostageStamp",
      "contract_type": "PostageStamp",
      "address": "0x45a1502382541Cd610CC9068e88727426b696293",
      "deployment_block": 31305656
    },
    {
      "name": "StampsRegistry",
      "contract_type": "StampsRegistry",
      "address": "0x5EBfBeFB1E88391eFb022d5d33302f50a46bF4f3",
      "deployment_block": 42390510
    },
    {
      "name": "PriceOracle",
      "contract_type": "PriceOracle",
      "address": "0x47EeF336e7fE5bED98499A4696bce8f28c1B0a8b",
      "deployment_block": 37339168
    },
    {
      "name": "StakeRegistry",
      "contract_type": "StakeRegistry",
      "address": "0xda2a16EE889E7f04980A8d597b48c8D51B9518F4",
      "deployment_block": 40430237
    },
    {
      "name": "Redistribution",
      "contract_type": "Redistribution",
      "address": "0x5069cdfB3D9E56d23B1cAeE83CE6109A7E4fd62d",
      "deployment_block": 41105199
    }
  ],
  "retry": {
    "max_retries": 5,
    "initial_delay_ms": 100,
    "backoff_multiplier": 4,
    "extended_retry_wait_seconds": 300
  }
}
```

### Using Config Files

```bash
# Specify config file
beeport-stamp-stats --config production.yaml fetch

# Override specific values
beeport-stamp-stats --config production.yaml --rpc-url http://custom.rpc fetch

# Use defaults (no config file needed)
beeport-stamp-stats fetch
```

### Environment Variables

Override any config value using `BEEPORT__` prefix:

```bash
# Override RPC URL
export BEEPORT__RPC__URL="https://rpc.gnosischain.com"

# Override database path
export BEEPORT__DATABASE__PATH="/data/stamps.db"

# Override retry settings
export BEEPORT__RETRY__MAX_RETRIES=10
```

### Adding New Contracts

Simply add to your config file - no code changes needed:

```yaml
contracts:
  - name: "MyNewContract"
    contract_type: "MyNewContract"  # Must match implementation
    address: "0x..."
    deployment_block: 12345678
```

For implementation details, see [CLAUDE.md](./CLAUDE.md#adding-a-new-contract).

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

#### 4. Sync Database

Update the local database with latest blockchain events:

```bash
# Sync from last synced block to latest
beeport-stamp-stats sync

# Sync specific block range
beeport-stamp-stats sync --from-block 38000000 --to-block 38500000

# Sync from specific block to latest
beeport-stamp-stats sync --from-block 38000000
```

**Difference from `fetch`:** The `sync` command is optimized for keeping the database up to date without displaying events. Use `fetch` when you want to see the events as they're retrieved, and `sync` for background updates.

### Understanding sync vs fetch

While both commands update the database with blockchain events, they serve different purposes and have distinct behaviors:

#### What sync does that fetch does not:

**1. Caches the Current Storage Price** (src/cli.rs:843-844)

After syncing events, `sync` queries and caches the current storage price from the blockchain:
```rust
let current_price = client.get_current_price().await?;
cache.cache_price(current_price).await?;
```
This cached price is then used by other commands like `batch-status` and `expiry-analytics` for accurate TTL calculations.

**2. Always Works Incrementally by Default** (src/cli.rs:790-798)

`sync` automatically resumes from the last synced block without requiring any flags:
```rust
let from = from_block
    .or_else(|| {
        // Get last synced block from cache
        futures::executor::block_on(cache.get_last_block())
            .ok()
            .flatten()
            .map(|b| b + 1)
    })
    .unwrap_or(DEFAULT_START_BLOCK);
```

**3. Optimized for Routine Updates**

- Uses hardcoded retry settings (5 retries, 100ms delay) suitable for normal syncing
- Shows "Database is already up to date!" when there are no new events
- Provides a simple summary output instead of detailed event listings
- Minimal output - designed for scripts and cron jobs

#### What fetch does that sync does not:

**1. Displays Detailed Event Table** (src/cli.rs:516)

`fetch` shows all retrieved events in a markdown table format with full details.

**2. Configurable Retry Settings**

`fetch` accepts `--max-retries` and `--initial-delay-ms` for customized retry behavior during large historical fetches where RPC issues are more likely.

**3. Non-Incremental Mode**

`fetch` can start from scratch or any specific block range. The `--incremental` flag must be explicitly provided for incremental behavior.

**4. Detailed Progress Output**

`fetch` provides verbose output about what's being retrieved, making it useful for monitoring large historical syncs.

#### Summary

**Use `sync` for:**
- Daily/hourly automated database updates via cron
- Keeping the database current with minimal output
- Ensuring the price cache is updated for analytics commands
- Background tasks where you don't need to see individual events

**Use `fetch` for:**
- Initial historical data loading
- Investigating specific block ranges
- Viewing detailed event information as it's retrieved
- Custom retry configurations for unreliable RPC endpoints
- When you want to see what's being synchronized

**Example workflow:**
```bash
# Initial setup - fetch all historical data
beeport-stamp-stats fetch --from-block 19275989

# Daily automated updates (in crontab)
0 */6 * * * /path/to/beeport-stamp-stats sync

# Investigating recent activity
beeport-stamp-stats fetch --from-block 38000000 --incremental
```

#### 5. Batch Status Analysis

Display detailed status information for all batches, including time-to-live (TTL) and expiry dates:

```bash
# View batch status with default sorting (by batch ID)
beeport-stamp-stats batch-status

# Sort by time to live (batches expiring soonest first)
beeport-stamp-stats batch-status --sort-by ttl

# Sort by expiry date
beeport-stamp-stats batch-status --sort-by expiry

# Sort by batch size (largest first)
beeport-stamp-stats batch-status --sort-by size

# Export to CSV
beeport-stamp-stats batch-status --output csv > batch-status.csv

# Export to JSON
beeport-stamp-stats batch-status --output json > batch-status.json

# Use custom storage price (in PLUR per chunk per block)
beeport-stamp-stats batch-status --price 30000

# Model price increase: 200% increase over 10 days
beeport-stamp-stats batch-status --price-change 200:10

# Combine custom price and price change
beeport-stamp-stats batch-status --price 25000 --price-change 150:7
```

**Output includes:**
- Batch ID
- Depth (storage capacity)
- Size in chunks (2^depth)
- TTL in blocks
- TTL in days
- Estimated expiry date (based on current block and price)

#### 6. Expiry Analytics

Analyze when batches will expire, aggregated by time period:

```bash
# View expiry analytics by day (default)
beeport-stamp-stats expiry-analytics

# Group by week
beeport-stamp-stats expiry-analytics --period week

# Group by month
beeport-stamp-stats expiry-analytics --period month

# Sort by number of chunks expiring (largest first)
beeport-stamp-stats expiry-analytics --sort-by chunks

# Sort by storage capacity expiring
beeport-stamp-stats expiry-analytics --sort-by storage

# Export to CSV
beeport-stamp-stats expiry-analytics --output csv > expiry-analytics.csv

# Export to JSON
beeport-stamp-stats expiry-analytics --output json > expiry-analytics.json

# With custom price
beeport-stamp-stats expiry-analytics --period week --price 28000

# Model declining storage prices: -50% over 30 days
beeport-stamp-stats expiry-analytics --period month --price-change -50:30

# Model increasing prices: 300% over 14 days
beeport-stamp-stats expiry-analytics --period day --price-change 300:14
```

**Output includes:**
- Time period (formatted based on grouping)
- Number of batches expiring in that period
- Total chunks expiring
- Total storage capacity expiring (in human-readable format: KB, MB, GB, TB, PB)

**Use cases:**
- Identify when to expect capacity to expire
- Plan for batch renewals
- Understand storage lifecycle patterns
- Model different price scenarios

#### 7. Export Data

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

## Understanding Price Calculations and TTL

### How Batch TTL is Calculated

The Time To Live (TTL) for a postage stamp batch represents how long the batch will remain valid before it needs to be topped up. The calculation is based on:

1. **Normalised Balance**: The amount of PLUR tokens allocated to the batch
2. **Batch Depth**: Determines the number of chunks (storage slots) in the batch
3. **Storage Price**: The cost per chunk per block in PLUR

**Formula:**
```
chunks = 2^depth
total_price_per_block = price_per_chunk_per_block × chunks
ttl_blocks = normalised_balance / total_price_per_block
ttl_days = ttl_blocks × 5_seconds_per_block / 86400_seconds_per_day
```

**Example:**
- Normalised Balance: 10,000,000,000 PLUR (10^10)
- Depth: 20 (= 1,048,576 chunks)
- Price: 24,000 PLUR per chunk per block
- TTL: 10,000,000,000 / (24,000 × 1,048,576) ≈ 397 blocks ≈ 0.023 days

### Price Change Modeling

The `--price-change` flag allows you to model scenarios where storage prices change over time. This is crucial for accurate TTL predictions in dynamic market conditions.

**Format:** `--price-change PERCENTAGE:DAYS`

Where:
- **PERCENTAGE**: The percentage change in price (positive for increase, negative for decrease)
- **DAYS**: The number of days over which this change occurs

**Examples:**

```bash
# 200% increase over 10 days (price triples from P to 3P)
--price-change 200:10

# 50% increase over 7 days
--price-change 50:7

# 50% decrease over 30 days (price halves)
--price-change -50:30

# 100% increase over 1 day (price doubles)
--price-change 100:1
```

### How Price Change Affects TTL

When prices change exponentially over time, we can't simply use the current price to calculate TTL. Instead, we need to calculate the **average effective price** over the batch's lifetime.

**Mathematical Model:**

1. **Daily Growth Rate**:
   ```
   r = (1 + percentage/100)^(1/days)
   ```

2. **Average Price** (integrating the exponential curve):
   ```
   avg_price = current_price × (r^ttl_days - 1) / (ln(r) × ttl_days)
   ```

**Example:**

Current price: 24,000 PLUR per chunk per block
Price change: 200% over 10 days (will become 72,000)
Initial TTL estimate: 30 days at current price

The effective average price accounts for the exponential increase:
- Day 1: 24,000 PLUR
- Day 5: ~35,834 PLUR
- Day 10: 72,000 PLUR

Average effective price ≈ 43,500 PLUR
Adjusted TTL ≈ 16.5 days (instead of 30 days)

### Practical Use Cases

**1. Conservative Planning (Rising Prices):**
```bash
# Expect prices to triple over the next 2 weeks
beeport-stamp-stats batch-status --price-change 200:14 --sort-by ttl
```
This shows which batches will expire soonest under rising price pressure.

**2. Optimistic Planning (Falling Prices):**
```bash
# Expect prices to halve over the next month
beeport-stamp-stats expiry-analytics --price-change -50:30 --period week
```
This models how much longer batches will last if prices decrease.

**3. Scenario Comparison:**
```bash
# Current price scenario
beeport-stamp-stats batch-status --output json > current.json

# Rising price scenario
beeport-stamp-stats batch-status --price-change 150:7 --output json > rising.json

# Falling price scenario
beeport-stamp-stats batch-status --price-change -40:14 --output json > falling.json
```
Compare the different scenarios to make informed decisions.

**4. Custom Price Override:**
```bash
# Use a specific price (e.g., from recent on-chain data)
beeport-stamp-stats batch-status --price 28500

# Combine with price change for full control
beeport-stamp-stats expiry-analytics --price 28500 --price-change 100:5
```

### Important Notes

- **Default Price**: When no `--price` is specified, the tool uses a default value of 24,000 PLUR per chunk per block (a reasonable estimate based on historical data)
- **Block Time**: Calculations assume 5 seconds per block on Gnosis Chain
- **Exponential Model**: Price changes are modeled as exponential growth/decay, not linear
- **Precision**: All calculations use 128-bit integers to avoid overflow with large PLUR values

### Why This Matters

Storage prices on decentralized networks can be volatile. Understanding how price changes affect batch lifetimes helps you:

- **Avoid Unexpected Expiry**: Model price increases to ensure you top up in time
- **Optimize Costs**: Identify the best time to create or top up batches
- **Plan Capacity**: Forecast when significant storage capacity will expire
- **Risk Management**: Understand your exposure to price fluctuations

## Contract Information

### Postage Stamp Contracts

#### PostageStamp Contract
- **Network:** Gnosis Chain
- **Contract Address:** `0x45a1502382541Cd610CC9068e88727426b696293`
- **Contract Type:** PostageStamp (Direct stamp purchases)
- **Deployment Block:** 31,305,656
- **Explorer:** [View on GnosisScan](https://gnosisscan.io/address/0x45a1502382541Cd610CC9068e88727426b696293)

#### StampsRegistry Contract
- **Network:** Gnosis Chain
- **Contract Address:** `0x5EBfBeFB1E88391eFb022d5d33302f50a46bF4f3`
- **Contract Type:** StampsRegistry (UI-based stamp purchases with payer tracking)
- **Deployment Block:** 42,390,510
- **Explorer:** [View on GnosisScan](https://gnosisscan.io/address/0x5EBfBeFB1E88391eFb022d5d33302f50a46bF4f3)

### Storage Incentives Contracts

#### PriceOracle Contract
- **Network:** Gnosis Chain
- **Contract Address:** `0x47EeF336e7fE5bED98499A4696bce8f28c1B0a8b`
- **Contract Type:** PriceOracle (Dynamic price adjustment mechanism)
- **Deployment Block:** 37,339,168
- **Events:** PriceUpdate, StampPriceUpdateFailed
- **Explorer:** [View on GnosisScan](https://gnosisscan.io/address/0x47EeF336e7fE5bED98499A4696bce8f28c1B0a8b)

#### StakeRegistry Contract
- **Network:** Gnosis Chain
- **Contract Address:** `0xda2a16EE889E7f04980A8d597b48c8D51B9518F4`
- **Contract Type:** StakeRegistry (Node staking for redistribution game)
- **Deployment Block:** 40,430,237
- **Events:** StakeUpdated, StakeSlashed, StakeFrozen, OverlayChanged, StakeWithdrawn
- **Explorer:** [View on GnosisScan](https://gnosisscan.io/address/0xda2a16EE889E7f04980A8d597b48c8D51B9518F4)

#### Redistribution Contract
- **Network:** Gnosis Chain
- **Contract Address:** `0x5069cdfB3D9E56d23B1cAeE83CE6109A7E4fd62d`
- **Contract Type:** Redistribution (Schelling coordination game for storage incentives)
- **Deployment Block:** 41,105,199
- **Events:** Committed, Revealed, WinnerSelected, TruthSelected, CurrentRevealAnchor, CountCommits, CountReveals, ChunkCount, PriceAdjustmentSkipped, WithdrawFailed, transformedChunkAddressFromInclusionProof
- **Game Phases:** Commit (blocks 0-37), Reveal (blocks 38-75), Claim (blocks 76-151) per 152-block round
- **Explorer:** [View on GnosisScan](https://gnosisscan.io/address/0x5069cdfB3D9E56d23B1cAeE83CE6109A7E4fd62d)

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
├── price.rs         # Price calculations and TTL modeling
├── error.rs         # Error types and handling
└── commands/
    ├── mod.rs              # Commands module
    ├── batch_status.rs     # Batch status analysis command
    └── expiry_analytics.rs # Expiry analytics command
```

### Key Dependencies

- `alloy = 0.8` - Ethereum interactions (full feature set)
- `clap = 4.5` - CLI framework with derive macros
- `tokio = 1.43` - Async runtime
- `sqlx = 0.8` - SQLite with async support
- `tabled = 0.17` - Table formatting
- `csv = 1.3` - CSV parsing and writing
- `chrono = 0.4` - Date/time handling
- `serde = 1.0` - Serialization
- `anyhow = 1.0` - Error handling
- `thiserror = 2.0` - Custom error types
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
