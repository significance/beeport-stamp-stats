# Beeport Postage Stamp Statistics

A high-performance Rust CLI tool for tracking and analyzing Swarm postage stamp events on Gnosis Chain using direct Ethereum RPC calls.

## Overview

This tool provides real-time tracking and historical analysis of postage stamp batch events from the Swarm network's PostageStamp contract on Gnosis Chain. Built with modern Rust (2024 edition) and the Alloy framework, it offers efficient blockchain querying, SQLite caching, and beautiful markdown-formatted output.

## Features

### Core Capabilities
- **Direct RPC Access**: Uses Ethereum RPC directly instead of third-party APIs for reliable, rate-limit-free access
- **Event Tracking**: Monitors all PostageStamp contract events:
  - `BatchCreated` - New postage batch creation
  - `BatchTopUp` - Batch balance increases
  - `BatchDepthIncrease` - Batch storage depth expansions
- **SQLite Caching**: Persistent local database for fast historical queries
- **Beautiful Output**: Markdown-formatted tables for easy reading
- **Incremental Sync**: Fetch only new events since last run
- **Comprehensive Testing**: 18 unit tests with 100% coverage of core functionality

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

Display analytics from cached data:

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

## Contract Information

- **Network:** Gnosis Chain
- **Contract Address:** `0x6a1A21eca3aB28BE85C7Ba22b2d6eAe5907c9008`
- **Contract Type:** PostageStamp (Swarm Storage)
- **Deployment Block:** 19,275,989
- **Explorer:** [View on GnosisScan](https://gnosisscan.io/address/0x6a1A21eca3aB28BE85C7Ba22b2d6eAe5907c9008)

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
├── blockchain.rs    # Ethereum RPC client and event fetching
├── cache.rs         # SQLite database operations
├── events.rs        # Event types and contract ABI
├── batch.rs         # Batch aggregation and statistics
├── display.rs       # Markdown table formatting
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
