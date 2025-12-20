# Beeport TX Stats - Development Guide

## Architecture Philosophy

This document describes the architectural decisions, design patterns, and development philosophy for beeport-tx-stats. It serves as a guide for AI-assisted development and human contributors.

### Core Principles

**1. Separation of Concerns**
- Each module has a single, well-defined responsibility
- No circular dependencies
- Clear boundaries between layers
- Configuration is pure data, business logic is separate

**2. Dependency Injection**
- All dependencies passed explicitly via parameters
- No global state or singletons  
- Enables testing in isolation
- Configuration flows: CLI → Config → Modules

**3. Type Safety**
- Leverage Rust's type system for compile-time guarantees
- Use `sol!` macro for contract ABIs (type-safe event decoding)
- Configuration validated at load time
- Errors handled with Result types

**4. Minimal Complexity**
- Don't add features beyond what's requested
- Keep solutions simple and focused
- Three similar lines > premature abstraction
- Only abstract when duplication reaches ~3+ instances

---

## Module Architecture

### Configuration Layer (`src/config.rs`)

**Responsibility:** Load and merge configuration from multiple sources

**Design Decisions:**
- Multi-format support (YAML/TOML/JSON) via `config` crate
- Priority: CLI args > Env vars > Config file > Defaults
- Pure data structures (no business logic)
- Validation at load time

**Configuration Priority System:**
```
1. CLI arguments (highest priority)
2. Environment variables (BEEPORT__ prefix)
3. Configuration file
4. Built-in defaults (lowest priority)
```

**Why this approach:**
- Zero-config experience (works with defaults)
- Progressive customization (add config as needed)  
- Runtime flexibility (override without editing files)
- Standard Rust ecosystem patterns

---

### Retry Logic (`src/retry.rs`)

**Responsibility:** Generic retry policy for rate-limited operations

**Design Decisions:**
- Extracted into separate module (reusable across RPC providers)
- Generic over operation type
- Two-phase retry strategy
- Configurable delays and multipliers

**Two-Phase Retry Algorithm:**
```
Phase 1: Exponential backoff (fast retry)
  delay = initial_delay_ms * backoff_multiplier^retry_count
  Example: 100ms → 400ms → 1600ms → 6400ms → 25600ms
  Retries: up to max_retries (default: 5)

Phase 2: Extended retry (when Phase 1 exhausted)
  delay = extended_retry_wait_seconds (default: 300s / 5 min)
  Resets Phase 1 counter
  Continues indefinitely until success
```

**Why this approach:**
- Handles temporary rate limits gracefully
- Prevents data loss during long-running operations
- Reusable across different RPC providers
- No blockchain-specific coupling
- Testable in isolation

---

### Contract Abstraction (`src/contracts/`)

**Responsibility:** Abstract contract definitions and event parsing

**Module Structure:**
```
contracts/
├── mod.rs     - Contract trait and ContractRegistry
├── abi.rs     - Sol! macro ABIs (kept in Rust for type safety)
├── impls.rs   - Concrete contract implementations
└── parser.rs  - Event parsing functions
```

**Design Decisions:**

**1. Contract Trait (Polymorphism)**
```rust
pub trait Contract: Send + Sync {
    fn name(&self) -> &str;
    fn address(&self) -> &str;
    fn deployment_block(&self) -> u64;
    fn parse_log(...) -> Result<Option<StampEvent>>;
    fn supports_price_query(&self) -> bool;
    fn supports_balance_query(&self) -> bool;
}
```

**Benefits:**
- Polymorphic behavior without code duplication
- Easy to add new contracts (just implement trait)
- Capabilities pattern (supports_*) for optional features
- Config-driven contract registration

**2. Event Parsing Strategy:**

**Before refactoring:**
- 2 nearly identical functions (~140 lines duplicated)
- `parse_postage_stamp_log()` and `parse_stamps_registry_log()`
- Adding new contract = copy-paste entire function

**After refactoring:**
- 2 focused parsing functions (~70 lines each)
- Type-safe event decoding using sol! macro types
- Shared event structure handling
- 50% code reduction through elimination of duplication

**Why dedicated functions instead of generics:**
- Sol! macro creates modules (not types)
- Type-safe event decoding requires specific types
- Still achieves code organization and clarity
- Easier to maintain than complex generic implementations

**3. Contract Registry:**
- Built from configuration at startup
- Enables iteration over all contracts
- Supports lookup by name or capability
- Config-driven (no code changes to add contracts)

---

### Blockchain Client (`src/blockchain.rs`)

**Responsibility:** Generic RPC operations (no contract-specific code)

**Design Decisions:**
- All contract operations delegated to Contract trait
- No hardcoded constants (chunk size, delays, block time, etc.)
- Retry logic extracted to retry.rs
- Block timestamp caching for efficiency

**Anti-patterns avoided:**
- ❌ Contract-specific methods
- ❌ Hardcoded addresses or deployment blocks
- ❌ Duplicate parsing logic
- ❌ Retry logic mixed with business logic
- ❌ Global configuration

**Key Methods:**
- `fetch_batch_events()` - Iterates all contracts via registry
- `fetch_contract_events()` - Generic fetching for any contract
- Block caching with SHA256-based keys

---

### CLI Layer (`src/cli.rs`)

**Responsibility:** Orchestration only (no business logic)

**Pattern:**
```rust
pub async fn execute(&self) -> Result<()> {
    // 1. Resolve configuration (merge sources)
    let config = self.resolve_config()?;

    // 2. Build dependencies
    let cache = Cache::new(&config.database.path).await?;
    let registry = ContractRegistry::from_config(&config)?;
    let client = BlockchainClient::new(&config.rpc.url).await?;

    // 3. Delegate to command
    match &self.command {
        Command::Fetch { ... } => {
            self.execute_fetch(cache, client, registry, config, ...).await
        }
    }
}
```

**Why this approach:**
- Clear separation: CLI ≠ business logic
- Testable commands (inject mock dependencies)
- Configuration changes don't affect command logic
- Single responsibility principle

---

## Adding a New Contract

**Step-by-step guide:**

### 1. Add contract to config.yaml
```yaml
contracts:
  - name: "MyNewContract"
    contract_type: "MyNewContract"
    address: "0x..."
    deployment_block: 12345678
```

### 2. Define contract ABI (in src/contracts/abi.rs)
```rust
sol! {
    #[allow(missing_docs)]
    #[sol(rpc)]
    MyNewContract,
    r#"[
        {
            "anonymous": false,
            "inputs": [...],
            "name": "BatchCreated",
            "type": "event"
        }
    ]"#
}
```

### 3. Create parser function (in src/contracts/parser.rs)
```rust
pub fn parse_my_new_contract_event(
    log: Log,
    block_number: u64,
    block_timestamp: DateTime<Utc>,
    transaction_hash: TxHash,
    log_index: u64,
    contract_source: &str,
) -> Result<Option<StampEvent>> {
    // Decode events using abi::MyNewContract types
}
```

### 4. Implement Contract trait (in src/contracts/impls.rs)
```rust
pub struct MyNewContractImpl {
    address: String,
    deployment_block: u64,
}

impl Contract for MyNewContractImpl {
    fn name(&self) -> &str { "MyNewContract" }
    fn address(&self) -> &str { &self.address }
    fn deployment_block(&self) -> u64 { self.deployment_block }

    fn parse_log(...) -> Result<Option<StampEvent>> {
        parse_my_new_contract_event(...)
    }
}
```

### 5. Register in ContractRegistry (in src/contracts/mod.rs)
```rust
impl ContractRegistry {
    pub fn from_config(config: &AppConfig) -> Result<Self> {
        match contract_config.contract_type.as_str() {
            "PostageStamp" => Box::new(...),
            "StampsRegistry" => Box::new(...),
            "MyNewContract" => Box::new(MyNewContractImpl::new(...)),
            _ => return Err(...),
        }
    }
}
```

**That's it!** No changes needed to blockchain.rs, cli.rs, or other modules.

---

## Testing Strategy

### Test Pyramid
```
         E2E Tests
            /\
           /  \
          /    \
    Integration  \
       Tests      \
        /\         \
       /  \         \
    Unit Tests       \
     (49+)           /
```

### Current Test Coverage
- **Config tests:** 17 tests (validation, defaults, error cases)
- **Retry tests:** 22 tests (backoff, predicates, rate limits)
- **Price tests:** 10 tests (TTL calculations, conversions)
- **Total:** 49 unit tests, 733 lines of test code

### Testing Philosophy
- Dependency injection enables mocking
- Pure functions for calculations (price.rs)
- No global state
- Each layer independently testable

---

## Project Management & Planning

### Implementation Plans (plan.md)

**When to create plan.md:**
For any multi-step task or feature implementation that involves:
- Adding new functionality
- Refactoring across multiple modules
- Adding new contracts or event types
- Any work that spans multiple sessions
- Complex changes requiring coordination

**What plan.md should contain:**
1. **Overview** - Goal, context, scope
2. **Requirements** - What needs to be accomplished
3. **Architecture decisions** - Database schema, API design, etc.
4. **Phased implementation plan** - Broken into logical steps
5. **Technical challenges** - Known difficult areas and solutions
6. **Success criteria** - How to know when done
7. **Testing strategy** - Unit, integration, functional tests
8. **Progress tracking** - Checkboxes for each task

**Maintaining plan.md:**
- ✅ Mark items as complete using `[x]` checkboxes
- 🚧 Update status section at top of file
- 📝 Add notes/learnings as work progresses
- 🔄 Revise plan if requirements change
- 📅 Update "Last Updated" timestamp

**Example plan.md structure:**
```markdown
# Feature Name - Implementation Plan

**Status:** In Progress
**Started:** YYYY-MM-DD
**Goal:** Brief description

## Overview
...

## Phases

### ✅ Phase 1: Planning
- [x] Task 1
- [x] Task 2

### 🚧 Phase 2: Implementation
- [x] Completed task
- [ ] In progress task
- [ ] Pending task

### ⬜ Phase 3: Testing
- [ ] Task 1
...

## Progress Tracking
**Current Phase:** Phase 2
**Next Step:** Complete task X

*Last Updated: YYYY-MM-DD*
```

**Benefits:**
- Clear roadmap for multi-session work
- Easy to resume after interruptions
- Tracks what's done vs what remains
- Documents architectural decisions
- Reduces repeated planning overhead

---

## Configuration Philosophy

**Three-Layer Configuration:**

1. **Defaults** (built into code)
   - Always available
   - Sensible for most use cases
   - Production-ready values

2. **Config File** (YAML/TOML/JSON)
   - Instance-specific customization
   - Version controlled
   - Environment-agnostic

3. **Runtime Overrides** (CLI args, env vars)
   - Temporary changes
   - CI/CD integration
   - One-off operations

**Example:**
```bash
# Use defaults
beeport-stamp-stats fetch

# Use config file
beeport-stamp-stats --config production.yaml fetch

# Override specific values
beeport-stamp-stats --rpc-url http://custom.rpc fetch

# Environment variable (highest priority after CLI)
BEEPORT__RPC__URL=http://custom.rpc beeport-stamp-stats fetch
```

---

## Error Handling

**Pattern: Fail fast with context**

```rust
// ❌ Bad: Generic error
Err(StampError::Config("Invalid config".into()))

// ✓ Good: Specific error with context
Err(StampError::Config(format!(
    "Unknown contract type '{}' in config. Valid types: PostageStamp, StampsRegistry",
    contract_type
)))
```

**Error Types:**
- `StampError::Config` - Configuration errors
- `StampError::Rpc` - RPC/network errors
- `StampError::Parse` - Event parsing errors
- `StampError::Cache` - Database errors
- `StampError::Contract` - Contract-specific errors

---

## Performance Considerations

### RPC Efficiency
- Chunked fetching (configurable chunk size)
- SHA256-based chunk caching (avoids re-fetching)
- Block timestamp caching (reduces RPC calls)
- Retry with exponential backoff

### Database Efficiency
- Batch inserts for events
- SQLite for local deployments
- PostgreSQL for production/shared deployments
- Balance caching to avoid redundant RPC calls

### Memory Management
- Streaming event processing
- Incremental chunk callbacks
- No full dataset in memory

---

## Common Patterns

### 1. Dependency Flow
```
CLI → Config → Registry → Client → Commands
```

### 2. Error Propagation
```rust
// Use ? operator for Result chaining
let config = self.resolve_config()?;
let registry = ContractRegistry::from_config(&config)?;
```

### 3. Async Operations
```rust
#[tokio::main]
async fn main() -> Result<()> {
    // Async operations with tokio runtime
}
```

### 4. Configuration Override
```rust
// CLI args override config file
if let Some(value) = cli.value {
    config.field = value;
}
```

---

## Anti-Patterns to Avoid

### ❌ Hardcoded Constants in Business Logic
```rust
// Bad
const CHUNK_SIZE: u64 = 10000;

// Good  
config.blockchain.chunk_size
```

### ❌ Contract-Specific Code in Generic Modules
```rust
// Bad (in blockchain.rs)
if contract_type == "PostageStamp" {
    // Special handling
}

// Good
contract.parse_log(...) // Polymorphic
```

### ❌ Duplicate Code
```rust
// Bad
fn parse_postage_stamp_log(...) { /* 70 lines */ }
fn parse_stamps_registry_log(...) { /* 70 lines, 95% identical */ }

// Good
fn parse_postage_stamp_event(...) { /* 70 lines */ }
fn parse_stamps_registry_event(...) { /* 70 lines, focused */ }
// Shared structure, different implementations
```

### ❌ Global State
```rust
// Bad
static mut CONFIG: Option<AppConfig> = None;

// Good
fn execute(&self, config: &AppConfig) // Passed explicitly
```

---

## Future Extensibility

### Multi-Chain Support
Current architecture supports multi-chain:
- Contract address/deployment per config
- RPC URL per instance
- Each chain = separate config file

### Additional Event Types
To add new event types:
1. Add to `EventType` enum
2. Add to `EventData` enum  
3. Update contract parser functions
4. No changes needed to core logic

### New Commands
To add commands:
1. Add to `Command` enum in cli.rs
2. Create `execute_*` method
3. Delegate to command module
4. Add tests

---

## Code Quality Standards

- **All code must pass clippy with `-D warnings`**
- **Prefer `#[allow]` for false positives, document why**
- **Use inline format strings** (`format!("{x}")` not `format!("{}", x)`)
- **Document panic conditions** (if any)
- **Test edge cases** (zero, overflow, empty collections)

---

## Testing Strategy

### When to Request Comprehensive Testing

After significant code changes (refactoring, new features, bug fixes), consider asking the user:

> **"Would you like me to run comprehensive tests to verify all functionality is working correctly, including validation against blockchain data (GnosisScan)?"**

This gives the user control over whether they want:
1. **Quick testing** - Just run unit tests (`cargo test`)
2. **Comprehensive testing** - Full functional verification (see below)

### Comprehensive Testing Checklist

When the user requests comprehensive testing, perform all of the following:

#### 1. Unit Tests ✓
```bash
cargo test
cargo clippy -- -D warnings
```
- Verify all tests pass
- Zero clippy warnings
- Check test coverage is maintained

#### 2. Fetch Command ✓
```bash
./target/release/beeport-stamp-stats --cache-db ./test.db fetch --from-block <start> --to-block <end>
```
- Use a small block range (10-20 blocks)
- Verify events are fetched correctly
- Check both PostageStamp and StampsRegistry events

#### 3. Blockchain Verification ✓
For each fetched transaction, verify on GnosisScan:
```bash
# Example transaction verification
https://gnosisscan.io/tx/0x<transaction_hash>
```
- Verify block numbers match
- Verify timestamps match
- Verify batch IDs match
- Verify event data (owner, depth, balance, etc.)
- Ensure both contract events are captured for StampsRegistry transactions

#### 4. Sync Command ✓
```bash
./target/release/beeport-stamp-stats --cache-db ./test.db sync --from-block <next> --to-block <end>
```
- Verify incremental sync works
- Check price caching
- Ensure database updates correctly

#### 5. Summary Command with Filters ✓
```bash
# Test all filter combinations
./target/release/beeport-stamp-stats --cache-db ./test.db summary --months 0
./target/release/beeport-stamp-stats --cache-db ./test.db summary --contract postage-stamp
./target/release/beeport-stamp-stats --cache-db ./test.db summary --event-type batch-created
./target/release/beeport-stamp-stats --cache-db ./test.db summary --batch-id <partial-id>
```
- Verify filtering works correctly
- Check contract source breakdown
- Validate event counts

#### 6. Batch Status Command ✓
```bash
# Test sorting and output formats
./target/release/beeport-stamp-stats --cache-db ./test.db batch-status --sort-by ttl
./target/release/beeport-stamp-stats --cache-db ./test.db batch-status --output json
./target/release/beeport-stamp-stats --cache-db ./test.db batch-status --output csv
./target/release/beeport-stamp-stats --cache-db ./test.db batch-status --price 200000
./target/release/beeport-stamp-stats --cache-db ./test.db batch-status --price-change 100:7
```
- Verify TTL calculations
- Test price override
- Test price change modeling
- Validate JSON/CSV output

#### 7. Expiry Analytics Command ✓
```bash
./target/release/beeport-stamp-stats --cache-db ./test.db expiry-analytics --period week
./target/release/beeport-stamp-stats --cache-db ./test.db expiry-analytics --period month
./target/release/beeport-stamp-stats --cache-db ./test.db expiry-analytics --output json
```
- Verify period grouping
- Check storage calculations
- Validate output formats

#### 8. Export Command ✓
```bash
# Test both formats
./target/release/beeport-stamp-stats --cache-db ./test.db export --output /tmp/test.json --format json
./target/release/beeport-stamp-stats --cache-db ./test.db export --output /tmp/test.csv --format csv

# Verify output
jq 'length' /tmp/test.json
head /tmp/test.csv
```
- Verify valid JSON output
- Verify CSV formatting
- Check event count matches

#### 9. Follow Mode ✓
```bash
# Test briefly in background
./target/release/beeport-stamp-stats --cache-db ./test.db follow --poll-interval 5 &
sleep 15
kill $!
```
- Verify follow mode starts
- Check sync-up behavior
- Ensure clean termination

#### 10. Configuration System ✓
```bash
# Create test config file
cat > test-config.yaml <<EOF
database:
  path: "./config-test.db"
blockchain:
  chunk_size: 1000
EOF

# Test config file loading
./target/release/beeport-stamp-stats --config test-config.yaml fetch --from-block <start> --to-block <end>

# Test environment variable override
BEEPORT__DATABASE__PATH="/tmp/env-test.db" ./target/release/beeport-stamp-stats --config test-config.yaml fetch --from-block <start> --to-block <end>

# Test CLI argument override (highest priority)
BEEPORT__DATABASE__PATH="/tmp/env-test.db" ./target/release/beeport-stamp-stats --config test-config.yaml --cache-db /tmp/cli-test.db fetch --from-block <start> --to-block <end>
```
- Verify config file loads correctly
- Verify environment variable overrides config file
- Verify CLI arguments override both

#### 11. Price Calculations ✓
Manually verify calculations:
```python
# Example verification
balance = 327099827630
ttl_blocks = 2183518
price = balance / ttl_blocks  # Should match tool output
days = (ttl_blocks * 5) / 86400  # Should match TTL days
```
- Verify TTL calculation formula
- Verify days conversion
- Test price change impact

### Testing Summary Template

After completing comprehensive testing, provide a summary:

```markdown
## ✅ Comprehensive Testing Complete

### Test Results
- ✅ Unit Tests: X passed, 0 failed
- ✅ Clippy: 0 warnings
- ✅ Fetch Command: Verified X events
- ✅ Blockchain Verification: All events match GnosisScan
- ✅ Sync Command: Working correctly
- ✅ Summary Command: All filters working
- ✅ Batch Status: All sorting/output modes working
- ✅ Expiry Analytics: Period grouping correct
- ✅ Export: JSON and CSV valid
- ✅ Follow Mode: Starts and terminates correctly
- ✅ Configuration: Priority system working (CLI > Env > Config > Defaults)
- ✅ Price Calculations: Manually verified, accurate

### Blockchain Verification
Verified sample transactions:
- Block X: <GnosisScan link> - ✅ Matches
- Block Y: <GnosisScan link> - ✅ Matches

### Ready for Production ✓
```

---

## Resources

- [Alloy Documentation](https://alloy.rs/) - Ethereum library
- [Config Crate](https://docs.rs/config/) - Configuration management
- [SQLx Documentation](https://docs.rs/sqlx/) - SQL toolkit
- [Tokio Documentation](https://tokio.rs/) - Async runtime

---

*This document is maintained alongside code changes. Update when architectural decisions are made.*
- use jq to read json to reduce token usage