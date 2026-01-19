# Contract Redeployment Support - Implementation Plan (REFINED)

**Status:** Planning
**Created:** 2025-12-21
**Last Updated:** 2025-12-21
**Goal:** Enable robust contract versioning with clean, reusable architecture for blockchain event processing

---

## Executive Summary

This plan implements contract versioning support by adding `contract_address` tracking throughout the system. The design prioritizes **architectural cleanliness**, **type safety**, and **reusability** to create a foundation for both analytics and event-driven bots.

### Key Architectural Principles

1. **Type Safety First** - Use newtypes to prevent address confusion
2. **Separation of Concerns** - Metadata ≠ Behavior ≠ Configuration
3. **Source of Truth** - Event attribution from `log.address`, not inference
4. **Future-Proof** - Support unlimited contract versions without code changes
5. **Testability** - Every component mockable and independently testable

---

## Problem Statement

### Contract Redeployment Reality

The Swarm storage incentives contracts undergo frequent redeployments:

**Redistribution** (6 versions since 2023):
- v0.9.4 @ Block 41105199: `0x5069cdfB3D9E56d23B1cAeE83CE6109A7E4fd62d` ← CURRENT
- v0.9.3 @ Block 40430261: `0x9f9A8dA5A0Db2611f9802ba1a0B99cC4A1c3b6A2`
- v0.9.2 @ Block 37339181: `0x69C62CaCd68C2CBBf3D0C7502eF556DB3AC7889B`
- v0.9.1 @ Block 35961755: `0xFfF73fd14537277B3F3807e1AB0F85E17c0ABea5`
- v0.8.6 @ Block 34159666: `0xD9dFE7b0ddc7CcA41304FE9507ed823faD3bdBab`
- Phase 4 @ Block 31305409: `0x1F9a1FDe5c6350E949C5E4aa163B4c97011199B4`

**StakeRegistry** (4 versions):
- v0.9.3 @ Block 40430237: `0xda2a16EE889E7F04980A8d597b48c8D51B9518F4` ← CURRENT
- v0.9.2 @ Block 37339175: `0x445B848e16730988F871c4a09aB74526d27c2Ce8`
- v0.9.1 @ Block 35961749: `0xBe212EA1A4978a64e8f7636Ae18305C38CA092Bd`
- v0.4.0 @ Block 25527075: `0x781c6D1f0eaE6F1Da1F604c6cDCcdB8B76428ba7`

**PriceOracle** (3 versions):
- v0.9.2 @ Block 37339168: `0x47EeF336e7fE5bED98499A4696bce8f28c1B0a8b` ← CURRENT
- v0.9.1 @ Block 31305665: `0x86DE783Bf23Bc13DaeF5A55ec531C198da8f10cF`
- Phase 4 @ Block 25527079: `0x344A2CC7304B32A87EfDC5407cD4bEC7cf98F035`

**PostageStamp** (2 versions):
- v0.8.6 @ Block 31305656: `0x45a1502382541Cd610CC9068e88727426b696293` ← CURRENT
- Phase 4 @ Block 25527076: `0x30d155478eF27Ab32A1D578BE7b84BC5988aF381`

### Critical Issues

1. **Event Attribution Ambiguity**
   - Database stores `contract_source` (name) without `contract_address`
   - Cannot distinguish which version emitted an event
   - Example: "Redistribution" event could be from any of 6 contracts

2. **Overlap Period Complexity**
   - Old contracts often remain active after new deployment
   - Nodes update at different times
   - Both versions emit events simultaneously
   - Current system cannot handle this cleanly

3. **Analysis Limitations**
   - No version-specific queries possible
   - Cannot compare behavior across versions
   - Cannot track migration patterns
   - Historical analysis requires manual block filtering

---

## Architectural Design

### Core Principle: Separation of Concerns

```
┌─────────────────────────────────────────────────────────────┐
│                     Configuration Layer                       │
│  (Pure Data - No Logic)                                       │
│  • Contract metadata (address, version, blocks)               │
│  • RPC settings, database path, retry config                  │
└──────────────────────┬────────────────────────────────────────┘
                       │
                       ├──> Validated at load time
                       │
┌──────────────────────▼────────────────────────────────────────┐
│                     Contract Registry                          │
│  (Metadata Management - Queryable)                             │
│  • Version resolution (address → metadata)                     │
│  • Capability queries (which contracts support price?)         │
│  • Historical vs active contracts                              │
└──────────────────────┬────────────────────────────────────────┘
                       │
                       ├──> Provides context to
                       │
┌──────────────────────▼────────────────────────────────────────┐
│                     Contract Impls                             │
│  (Behavior - Event Parsing)                                    │
│  • Parse logs into typed events                                │
│  • No knowledge of database or business logic                  │
│  • Stateless and testable                                      │
└──────────────────────┬────────────────────────────────────────┘
                       │
                       ├──> Emits events to
                       │
┌──────────────────────▼────────────────────────────────────────┐
│                     Event Processing Pipeline                  │
│  (Reusable for both analytics and bots)                        │
│  • Source: Blockchain RPC / Database / Stream                  │
│  • Processor: Business logic / Analytics / Reactions           │
│  • Sink: Database / Webhooks / Actions                         │
└─────────────────────────────────────────────────────────────────┘
```

### Type Safety via Newtypes

**Problem:** Strings are error-prone. Easy to pass wrong data.

**Solution:** Newtype pattern

```rust
// src/types.rs (NEW)

/// Contract address on blockchain (checksummed hex string with 0x prefix)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ContractAddress(String);

impl ContractAddress {
    /// Create from string, validating format
    pub fn new(address: impl Into<String>) -> Result<Self> {
        let addr = address.into();
        // Validate: 0x prefix, 40 hex chars
        if !addr.starts_with("0x") || addr.len() != 42 {
            return Err(StampError::Config(format!("Invalid address: {}", addr)));
        }
        Ok(Self(addr.to_lowercase()))
    }

    /// Get as str
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Contract version identifier (e.g., "v0.9.4", "Phase 4")
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractVersion(String);

impl ContractVersion {
    pub fn new(version: impl Into<String>) -> Self {
        Self(version.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Block number
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct BlockNumber(pub u64);
```

**Benefits:**
- Compiler prevents mixing up string types
- Self-documenting code (`ContractAddress` vs `String`)
- Centralized validation
- Easy to add checksumming later

### Contract Metadata Structure

**Current:** Contract trait mixes metadata with behavior
**Problem:** Can't query metadata without contract impl
**Solution:** Separate metadata struct

```rust
// src/contracts/metadata.rs (NEW)

/// Metadata about a contract deployment
///
/// Separates "what is this contract" from "how does it work"
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractMetadata {
    /// Unique name (e.g., "PostageStamp", "Redistribution-v0.9.3")
    pub name: String,

    /// Contract type (e.g., "PostageStamp", "Redistribution")
    pub contract_type: String,

    /// On-chain address
    pub address: ContractAddress,

    /// Human-readable version
    pub version: ContractVersion,

    /// Deployment block
    pub deployment_block: BlockNumber,

    /// Optional: Last active block (when superseded or paused)
    pub end_block: Option<BlockNumber>,

    /// Whether this is the active version
    pub active: bool,

    /// Optional: Block when contract was paused
    pub paused_at: Option<BlockNumber>,
}

impl ContractMetadata {
    /// Check if this contract was active at a given block
    pub fn active_at_block(&self, block: BlockNumber) -> bool {
        if block < self.deployment_block {
            return false;
        }

        if let Some(end) = self.end_block {
            if block >= end {
                return false;
            }
        }

        true
    }

    /// Get block range for this contract
    pub fn block_range(&self) -> (BlockNumber, Option<BlockNumber>) {
        (self.deployment_block, self.end_block)
    }
}
```

### Enhanced Contract Registry

**Current:** Only stores active contracts
**Goal:** Support historical contracts + rich queries

```rust
// src/contracts/mod.rs (ENHANCED)

pub struct ContractRegistry {
    // All contracts (active + historical)
    contracts: Vec<Box<dyn Contract>>,

    // Metadata for all contracts
    metadata: Vec<ContractMetadata>,

    // Fast lookup: address → metadata index
    address_map: HashMap<ContractAddress, usize>,

    // Fast lookup: contract type → Vec<metadata index> (sorted by block)
    type_map: HashMap<String, Vec<usize>>,
}

impl ContractRegistry {
    /// Build from configuration
    pub fn from_config(config: &AppConfig) -> Result<Self> {
        // ... validate no duplicate addresses
        // ... validate block ranges don't conflict
        // ... build indexes
    }

    /// Find contract metadata by address
    pub fn find_by_address(&self, addr: &ContractAddress) -> Option<&ContractMetadata> {
        self.address_map.get(addr)
            .map(|&idx| &self.metadata[idx])
    }

    /// Find active contract of a given type
    pub fn find_active_by_type(&self, contract_type: &str) -> Option<&ContractMetadata> {
        self.type_map.get(contract_type)?
            .iter()
            .find_map(|&idx| {
                let meta = &self.metadata[idx];
                if meta.active { Some(meta) } else { None }
            })
    }

    /// Find which contract was active at a specific block
    pub fn find_active_at_block(
        &self,
        contract_type: &str,
        block: BlockNumber
    ) -> Option<&ContractMetadata> {
        self.type_map.get(contract_type)?
            .iter()
            .find_map(|&idx| {
                let meta = &self.metadata[idx];
                if meta.active_at_block(block) { Some(meta) } else { None }
            })
    }

    /// Get all versions of a contract type, sorted by deployment block
    pub fn get_versions(&self, contract_type: &str) -> Vec<&ContractMetadata> {
        self.type_map.get(contract_type)
            .map(|indexes| {
                indexes.iter()
                    .map(|&idx| &self.metadata[idx])
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Validate configuration consistency
    fn validate(&self) -> Result<()> {
        // Check for duplicate addresses
        // Check for overlapping block ranges of same type
        // Warn if multiple contracts active for same type
    }
}
```

### Event Attribution: Source of Truth

**Current:** `contract_source` set from contract name
**Problem:** Doesn't use actual `log.address` from blockchain
**Solution:** Extract address from log, look up metadata

```rust
// src/blockchain.rs (ENHANCED)

impl BlockchainClient {
    async fn fetch_contract_events(
        &self,
        contract: &dyn Contract,
        metadata: &ContractMetadata,  // NEW: Pass metadata
        from_block: u64,
        to_block: u64,
    ) -> Result<Vec<StampEvent>> {
        // ... fetch logs ...

        for log in logs {
            // Extract actual contract address from log
            let log_address = ContractAddress::new(format!("{:?}", log.address))?;

            // CRITICAL: Verify log came from expected contract
            if log_address != metadata.address {
                tracing::warn!(
                    "Log address mismatch: expected {}, got {}",
                    metadata.address.as_str(),
                    log_address.as_str()
                );
                continue;
            }

            // Parse event with address attribution
            if let Some(mut event) = contract.parse_log(
                log.clone(),
                block_number,
                block_timestamp,
                tx_hash,
                log_index,
            )? {
                // Set address from log, not from config
                event.contract_address = log_address.clone();
                events.push(event);
            }
        }

        Ok(events)
    }
}
```

**Benefits:**
- **Impossible to misattribute** - Address comes from chain
- **Overlap periods handled** - Each event has true source
- **Backfill can use inference** - But new events use truth

---

## Implementation Plan

### Phase 0: Type System Foundation (NEW)

**Duration:** 0.5 session
**Priority:** HIGH - Required for all other phases

#### 0.1 Create Type Module

```rust
// src/types.rs
pub mod types {
    pub use contract_address::ContractAddress;
    pub use contract_version::ContractVersion;
    pub use block_number::BlockNumber;
}
```

**Tasks:**
- [ ] Create `src/types.rs` with newtypes
- [ ] Add validation for `ContractAddress`
- [ ] Add `Display`, `FromStr`, `Serialize`, `Deserialize` impls
- [ ] Add unit tests for validation

#### 0.2 Update Event Structures

```rust
// src/events.rs (UPDATED)

pub struct StampEvent {
    pub event_type: EventType,
    pub batch_id: String,
    pub block_number: BlockNumber,              // Changed from u64
    pub block_timestamp: DateTime<Utc>,
    pub transaction_hash: String,
    pub log_index: u64,
    pub contract_source: String,
    pub contract_address: ContractAddress,      // NEW
    pub data: EventData,
}

pub struct StorageIncentivesEvent {
    pub block_number: BlockNumber,              // Changed from u64
    pub block_timestamp: DateTime<Utc>,
    pub transaction_hash: String,
    pub log_index: u64,
    pub contract_source: String,
    pub contract_address: ContractAddress,      // NEW
    pub event_type: String,
    // ... rest of fields
}
```

**Tasks:**
- [ ] Add `contract_address` field to `StampEvent`
- [ ] Add `contract_address` field to `StorageIncentivesEvent`
- [ ] Update serialization tests
- [ ] Update all event construction sites

#### 0.3 Create Metadata Module

**Tasks:**
- [ ] Create `src/contracts/metadata.rs`
- [ ] Implement `ContractMetadata` struct
- [ ] Implement `active_at_block()` logic
- [ ] Add unit tests for block range logic

---

### Phase 1: Configuration & Registry Enhancement

**Duration:** 1 session
**Dependencies:** Phase 0

#### 1.1 Update Configuration Schema

```rust
// src/config.rs (ENHANCED)

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractConfig {
    pub name: String,
    pub contract_type: String,
    pub address: String,                    // Will be parsed to ContractAddress
    pub deployment_block: u64,              // Will be parsed to BlockNumber

    // NEW fields
    #[serde(default)]
    pub version: Option<String>,            // Will be parsed to ContractVersion

    #[serde(default)]
    pub active: bool,                       // Default: false (require explicit)

    #[serde(default)]
    pub end_block: Option<u64>,            // When contract was superseded

    #[serde(default)]
    pub paused_at: Option<u64>,            // When contract was paused
}

impl ContractConfig {
    /// Validate configuration
    fn validate(&self) -> Result<()> {
        // Address format
        ContractAddress::new(&self.address)?;

        // Block numbers logical
        if let Some(end) = self.end_block {
            if end <= self.deployment_block {
                return Err(StampError::Config(
                    format!("end_block must be after deployment_block")
                ));
            }
        }

        Ok(())
    }

    /// Convert to metadata
    fn to_metadata(&self) -> Result<ContractMetadata> {
        Ok(ContractMetadata {
            name: self.name.clone(),
            contract_type: self.contract_type.clone(),
            address: ContractAddress::new(&self.address)?,
            version: ContractVersion::new(
                self.version.clone().unwrap_or_else(|| "unknown".to_string())
            ),
            deployment_block: BlockNumber(self.deployment_block),
            end_block: self.end_block.map(BlockNumber),
            active: self.active,
            paused_at: self.paused_at.map(BlockNumber),
        })
    }
}
```

**Tasks:**
- [ ] Add optional fields to `ContractConfig`
- [ ] Implement validation logic
- [ ] Add `to_metadata()` conversion
- [ ] Update config file (`config.yaml`) with all historical contracts
- [ ] Add config validation tests

#### 1.2 Update config.yaml

```yaml
contracts:
  # ========================================================================
  # PostageStamp Contracts
  # ========================================================================

  - name: "PostageStamp"
    contract_type: "PostageStamp"
    address: "0x45a1502382541Cd610CC9068e88727426b696293"
    deployment_block: 31305656
    version: "v0.8.6"
    active: true

  - name: "PostageStamp-Phase4"
    contract_type: "PostageStamp"
    address: "0x30d155478eF27Ab32A1D578BE7b84BC5988aF381"
    deployment_block: 25527076
    version: "Phase 4"
    active: false
    end_block: 31305656  # Superseded by v0.8.6

  # ========================================================================
  # Redistribution Contracts (6 versions)
  # ========================================================================

  - name: "Redistribution"
    contract_type: "Redistribution"
    address: "0x5069cdfB3D9E56d23B1cAeE83CE6109A7E4fd62d"
    deployment_block: 41105199
    version: "v0.9.4"
    active: true

  - name: "Redistribution-v0.9.3"
    contract_type: "Redistribution"
    address: "0x9f9A8dA5A0Db2611f9802ba1a0B99cC4A1c3b6A2"
    deployment_block: 40430261
    version: "v0.9.3"
    active: false
    paused_at: 41150000  # Approximate - paused July 9, 2025
    end_block: 41105199  # Superseded by v0.9.4

  - name: "Redistribution-v0.9.2"
    contract_type: "Redistribution"
    address: "0x69C62CaCd68C2CBBf3D0C7502eF556DB3AC7889B"
    deployment_block: 37339181
    version: "v0.9.2"
    active: false
    end_block: 40430261

  - name: "Redistribution-v0.9.1"
    contract_type: "Redistribution"
    address: "0xFfF73fd14537277B3F3807e1AB0F85E17c0ABea5"
    deployment_block: 35961755
    version: "v0.9.1"
    active: false
    end_block: 37339181

  - name: "Redistribution-v0.8.6"
    contract_type: "Redistribution"
    address: "0xD9dFE7b0ddc7CcA41304FE9507ed823faD3bdBab"
    deployment_block: 34159666
    version: "v0.8.6"
    active: false
    end_block: 35961755

  - name: "Redistribution-Phase4"
    contract_type: "Redistribution"
    address: "0x1F9a1FDe5c6350E949C5E4aa163B4c97011199B4"
    deployment_block: 31305409
    version: "Phase 4"
    active: false
    end_block: 34159666

  # ========================================================================
  # StakeRegistry Contracts (4 versions)
  # ========================================================================

  - name: "StakeRegistry"
    contract_type: "StakeRegistry"
    address: "0xda2a16EE889E7F04980A8d597b48c8D51B9518F4"
    deployment_block: 40430237
    version: "v0.9.3"
    active: true

  - name: "StakeRegistry-v0.9.2"
    contract_type: "StakeRegistry"
    address: "0x445B848e16730988F871c4a09aB74526d27c2Ce8"
    deployment_block: 37339175
    version: "v0.9.2"
    active: false
    end_block: 40430237

  - name: "StakeRegistry-v0.9.1"
    contract_type: "StakeRegistry"
    address: "0xBe212EA1A4978a64e8f7636Ae18305C38CA092Bd"
    deployment_block: 35961749
    version: "v0.9.1"
    active: false
    end_block: 37339175

  - name: "StakeRegistry-v0.4.0"
    contract_type: "StakeRegistry"
    address: "0x781c6D1f0eaE6F1Da1F604c6cDCcdB8B76428ba7"
    deployment_block: 25527075
    version: "v0.4.0"
    active: false
    end_block: 35961749

  # ========================================================================
  # PriceOracle Contracts (3 versions)
  # ========================================================================

  - name: "PriceOracle"
    contract_type: "PriceOracle"
    address: "0x47EeF336e7fE5bED98499A4696bce8f28c1B0a8b"
    deployment_block: 37339168
    version: "v0.9.2"
    active: true

  - name: "PriceOracle-v0.9.1"
    contract_type: "PriceOracle"
    address: "0x86DE783Bf23Bc13DaeF5A55ec531C198da8f10cF"
    deployment_block: 31305665
    version: "v0.9.1"
    active: false
    end_block: 37339168

  - name: "PriceOracle-Phase4"
    contract_type: "PriceOracle"
    address: "0x344A2CC7304B32A87EfDC5407cD4bEC7cf98F035"
    deployment_block: 25527079
    version: "Phase 4"
    active: false
    end_block: 31305665

  # ========================================================================
  # StampsRegistry (No historical versions)
  # ========================================================================

  - name: "StampsRegistry"
    contract_type: "StampsRegistry"
    address: "0x5EBfBeFB1E88391eFb022d5d33302f50a46bF4f3"
    deployment_block: 42390510
    version: "v1.0.0"
    active: true
```

**Tasks:**
- [ ] Add all 17 historical contract versions
- [ ] Set correct block numbers from deployment history
- [ ] Mark active status correctly
- [ ] Add end_block for superseded contracts
- [ ] Add comments for clarity

#### 1.3 Enhance Contract Registry

**Tasks:**
- [ ] Add `metadata: Vec<ContractMetadata>` field
- [ ] Add `address_map: HashMap<ContractAddress, usize>` index
- [ ] Add `type_map: HashMap<String, Vec<usize>>` index
- [ ] Implement `find_by_address()`
- [ ] Implement `find_active_at_block()`
- [ ] Implement `get_versions()`
- [ ] Add validation in `from_config()`
- [ ] Add tests for all query methods

---

### Phase 2: Database Schema Changes

**Duration:** 1 session
**Dependencies:** Phase 0

#### 2.1 Create Migrations

**SQLite Migration:**

```sql
-- migrations/20251221000001_add_contract_address.sql

-- Add contract_address columns
ALTER TABLE events ADD COLUMN contract_address TEXT;
ALTER TABLE storage_incentives_events ADD COLUMN contract_address TEXT;

-- Create indexes for performance
CREATE INDEX idx_events_contract_address ON events(contract_address);
CREATE INDEX idx_si_contract_address ON storage_incentives_events(contract_address);

-- Backfill: PostageStamp events
UPDATE events
SET contract_address = CASE
    WHEN block_number >= 31305656 THEN '0x45a1502382541cd610cc9068e88727426b696293'
    ELSE '0x30d155478ef27ab32a1d578be7b84bc5988af381'
END
WHERE contract_source = 'PostageStamp';

-- Backfill: StampsRegistry events (only one version)
UPDATE events
SET contract_address = '0x5ebfbefb1e88391efb022d5d33302f50a46bf4f3'
WHERE contract_source = 'StampsRegistry';

-- Backfill: Redistribution events
UPDATE storage_incentives_events
SET contract_address = CASE
    WHEN block_number >= 41105199 THEN '0x5069cdfb3d9e56d23b1caee83ce6109a7e4fd62d'
    WHEN block_number >= 40430261 THEN '0x9f9a8da5a0db2611f9802ba1a0b99cc4a1c3b6a2'
    WHEN block_number >= 37339181 THEN '0x69c62cacd68c2cbbf3d0c7502ef556db3ac7889b'
    WHEN block_number >= 35961755 THEN '0xfff73fd14537277b3f3807e1ab0f85e17c0abea5'
    WHEN block_number >= 34159666 THEN '0xd9dfe7b0ddc7cca41304fe9507ed823fad3bdbab'
    ELSE '0x1f9a1fde5c6350e949c5e4aa163b4c97011199b4'
END
WHERE contract_source = 'Redistribution';

-- Backfill: StakeRegistry events
UPDATE storage_incentives_events
SET contract_address = CASE
    WHEN block_number >= 40430237 THEN '0xda2a16ee889e7f04980a8d597b48c8d51b9518f4'
    WHEN block_number >= 37339175 THEN '0x445b848e16730988f871c4a09ab74526d27c2ce8'
    WHEN block_number >= 35961749 THEN '0xbe212ea1a4978a64e8f7636ae18305c38ca092bd'
    ELSE '0x781c6d1f0eae6f1da1f604c6cdccdb8b76428ba7'
END
WHERE contract_source = 'StakeRegistry';

-- Backfill: PriceOracle events
UPDATE storage_incentives_events
SET contract_address = CASE
    WHEN block_number >= 37339168 THEN '0x47eef336e7fe5bed98499a4696bce8f28c1b0a8b'
    WHEN block_number >= 31305665 THEN '0x86de783bf23bc13daef5a55ec531c198da8f10cf'
    ELSE '0x344a2cc7304b32a87efdc5407cd4bec7cf98f035'
END
WHERE contract_source = 'PriceOracle';
```

**PostgreSQL Migration:** (Same logic, compatible syntax)

```sql
-- migrations_postgres/20251221000001_add_contract_address.sql
-- (Same content as SQLite but using PostgreSQL-specific features if needed)
```

**Tasks:**
- [ ] Create migration file for SQLite
- [ ] Create migration file for PostgreSQL
- [ ] Test migration on empty database
- [ ] Test migration on database with existing data
- [ ] Verify backfill correctness with sample queries

#### 2.2 Update Cache Module

```rust
// src/cache.rs (UPDATED)

impl Cache {
    pub async fn store_events(&self, events: &[StampEvent]) -> Result<()> {
        for event in events {
            let event_type = event.event_type.to_string();
            let data = serde_json::to_string(&event.data)?;
            let timestamp = event.block_timestamp.timestamp();
            let contract_address = event.contract_address.as_str();  // NEW

            match &self.pool {
                DatabasePool::Sqlite(pool) => {
                    sqlx::query(
                        r#"
                        INSERT OR REPLACE INTO events
                        (event_type, batch_id, block_number, block_timestamp,
                         transaction_hash, log_index, contract_source, contract_address, data)
                        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
                        "#,
                    )
                    .bind(&event_type)
                    .bind(&event.batch_id)
                    .bind(event.block_number.0 as i64)  // Use .0 to get u64
                    .bind(timestamp)
                    .bind(&event.transaction_hash)
                    .bind(event.log_index as i64)
                    .bind(&event.contract_source)
                    .bind(contract_address)  // NEW
                    .bind(&data)
                    .execute(pool)
                    .await?;
                }
                DatabasePool::Postgres(pool) => {
                    sqlx::query(
                        r#"
                        INSERT INTO events
                        (event_type, batch_id, block_number, block_timestamp,
                         transaction_hash, log_index, contract_source, contract_address, data)
                        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                        ON CONFLICT (transaction_hash, log_index) DO UPDATE SET
                            event_type = EXCLUDED.event_type,
                            batch_id = EXCLUDED.batch_id,
                            block_number = EXCLUDED.block_number,
                            block_timestamp = EXCLUDED.block_timestamp,
                            contract_source = EXCLUDED.contract_source,
                            contract_address = EXCLUDED.contract_address,
                            data = EXCLUDED.data
                        "#,
                    )
                    .bind(&event_type)
                    .bind(&event.batch_id)
                    .bind(event.block_number.0 as i64)
                    .bind(timestamp)
                    .bind(&event.transaction_hash)
                    .bind(event.log_index as i64)
                    .bind(&event.contract_source)
                    .bind(contract_address)  // NEW
                    .bind(&data)
                    .execute(pool)
                    .await?;
                }
            }
        }
        Ok(())
    }

    // Similar updates for store_storage_incentives_events()
}
```

**Tasks:**
- [ ] Update `store_events()` to bind contract_address
- [ ] Update `store_storage_incentives_events()` to bind contract_address
- [ ] Update all query methods to include contract_address
- [ ] Add tests for storage and retrieval

---

### Phase 3: Contract Parsers & Event Attribution

**Duration:** 1 session
**Dependencies:** Phase 0, Phase 1

#### 3.1 Update Contract Trait

```rust
// src/contracts/mod.rs (UPDATED)

pub trait Contract: Send + Sync {
    fn name(&self) -> &str;
    fn address(&self) -> &ContractAddress;  // Changed from &str
    fn deployment_block(&self) -> BlockNumber;  // Changed from u64

    // NEW: Get metadata
    fn metadata(&self) -> &ContractMetadata;

    fn parse_log(
        &self,
        log: Log,
        block_number: BlockNumber,  // Changed from u64
        block_timestamp: DateTime<Utc>,
        transaction_hash: TxHash,
        log_index: u64,
    ) -> Result<Option<StampEvent>>;

    fn supports_price_query(&self) -> bool { false }
    fn supports_balance_query(&self) -> bool { false }
}
```

**Tasks:**
- [ ] Update trait signatures to use newtypes
- [ ] Add `metadata()` method
- [ ] Update all implementations (PostageStamp, StampsRegistry, etc.)
- [ ] Ensure parsers populate `contract_address` from log

#### 3.2 Update Parsing Functions

```rust
// src/contracts/parser.rs (UPDATED)

pub fn parse_postage_stamp_event(
    log: Log,
    block_number: BlockNumber,
    block_timestamp: DateTime<Utc>,
    transaction_hash: TxHash,
    log_index: u64,
    contract_source: &str,
    contract_address: &ContractAddress,  // NEW: Pass from log.address
) -> Result<Option<StampEvent>> {
    // ... existing parsing logic ...

    Ok(Some(StampEvent {
        event_type,
        batch_id,
        block_number,
        block_timestamp,
        transaction_hash: format!("{:?}", transaction_hash),
        log_index,
        contract_source: contract_source.to_string(),
        contract_address: contract_address.clone(),  // NEW
        data,
    }))
}
```

**Tasks:**
- [ ] Add `contract_address` parameter to all parsing functions
- [ ] Populate `contract_address` in returned events
- [ ] Update all call sites
- [ ] Add tests with different contract addresses

#### 3.3 Update Blockchain Client

```rust
// src/blockchain.rs (UPDATED)

impl BlockchainClient {
    pub async fn fetch_contract_events(
        &self,
        contract: &dyn Contract,
        from_block: u64,
        to_block: u64,
        registry: &ContractRegistry,  // NEW: Pass registry for metadata
    ) -> Result<Vec<StampEvent>> {
        let metadata = contract.metadata();

        // ... fetch logs ...

        for log in logs {
            // Extract address from log (source of truth)
            let log_address = ContractAddress::new(format!("{:?}", log.address))?;

            // Verify it matches expected contract
            if &log_address != metadata.address() {
                tracing::warn!(
                    "Log from unexpected address: expected {}, got {}. Skipping.",
                    metadata.address().as_str(),
                    log_address.as_str()
                );
                continue;
            }

            // Parse with true address
            if let Some(event) = contract.parse_log(
                log.clone(),
                BlockNumber(block_number),
                block_timestamp,
                tx_hash,
                log_index,
            )? {
                events.push(event);
            }
        }

        Ok(events)
    }
}
```

**Tasks:**
- [ ] Pass metadata to fetch functions
- [ ] Extract `log.address` from each log
- [ ] Verify address matches expected contract
- [ ] Pass address to parsing functions
- [ ] Add logging for address mismatches
- [ ] Add tests with mock logs

---

### Phase 4: CLI Enhancements

**Duration:** 1 session
**Dependencies:** Phase 1, Phase 2, Phase 3

#### 4.1 Add Contract Info Commands

```rust
// src/cli.rs (NEW COMMANDS)

#[derive(Debug, clap::Subcommand)]
pub enum Command {
    // ... existing commands ...

    /// Contract management and information
    Contracts {
        #[command(subcommand)]
        subcommand: ContractsCommand,
    },
}

#[derive(Debug, clap::Subcommand)]
pub enum ContractsCommand {
    /// List all configured contracts
    List {
        /// Filter by contract type
        #[arg(long)]
        contract_type: Option<String>,

        /// Show only active contracts
        #[arg(long)]
        active_only: bool,
    },

    /// Show detailed information about a contract
    Info {
        /// Contract name or address
        name_or_address: String,
    },

    /// Show version timeline for a contract type
    Timeline {
        /// Contract type (e.g., "Redistribution")
        contract_type: String,
    },
}
```

**Example output:**

```bash
$ beeport-stamp-stats contracts list --contract-type Redistribution

Redistribution Contracts
========================

✓ v0.9.4 (ACTIVE)
  Address: 0x5069cdfB3D9E56d23B1cAeE83CE6109A7E4fd62d
  Deployed: Block 41105199 (2025-07-16)
  Status: Active

○ v0.9.3
  Address: 0x9f9A8dA5A0Db2611f9802ba1a0B99cC4A1c3b6A2
  Deployed: Block 40430261 (2025-06-05)
  Paused: Block ~41150000 (2025-07-09)
  Status: Superseded by v0.9.4

○ v0.9.2
  Address: 0x69C62CaCd68C2CBBf3D0C7502eF556DB3AC7889B
  Deployed: Block 37339181 (2024-12-03)
  Status: Superseded by v0.9.3

... (4 more versions)
```

**Tasks:**
- [ ] Add `Contracts` command enum
- [ ] Implement `list` subcommand
- [ ] Implement `info` subcommand
- [ ] Implement `timeline` subcommand with ASCII visualization
- [ ] Add `--output json` flag for machine-readable output

#### 4.2 Add Historical Fetch Support

```rust
// src/cli.rs (UPDATED)

pub struct FetchCommand {
    // ... existing fields ...

    /// Include historical contract versions (not just active)
    #[arg(long)]
    pub include_historical: bool,

    /// Fetch only from specific contract version
    #[arg(long, value_name = "ADDRESS")]
    pub contract_address: Option<String>,
}
```

**Tasks:**
- [ ] Add `--include-historical` flag
- [ ] Add `--contract-address` filter
- [ ] Implement filtering logic
- [ ] Update fetch logic to use registry queries
- [ ] Add tests for historical fetching

#### 4.3 Add Filtering to Analysis Commands

```rust
// src/cli.rs (UPDATED)

pub struct SummaryCommand {
    // ... existing fields ...

    /// Filter by contract address
    #[arg(long)]
    pub contract_address: Option<String>,

    /// Filter by contract version
    #[arg(long)]
    pub version: Option<String>,
}
```

**Tasks:**
- [ ] Add contract filtering to `summary` command
- [ ] Add contract filtering to `export` command
- [ ] Add contract filtering to `batch-status` command
- [ ] Update display code to show contract address
- [ ] Add tests for filtered queries

---

### Phase 5: Testing

**Duration:** 1.5 sessions
**Dependencies:** All previous phases

#### 5.1 Unit Tests

```rust
// tests/contract_metadata_test.rs (NEW)

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_contract_active_at_block() {
        let metadata = ContractMetadata {
            deployment_block: BlockNumber(1000),
            end_block: Some(BlockNumber(2000)),
            // ... other fields
        };

        assert!(!metadata.active_at_block(BlockNumber(999)));
        assert!(metadata.active_at_block(BlockNumber(1000)));
        assert!(metadata.active_at_block(BlockNumber(1500)));
        assert!(!metadata.active_at_block(BlockNumber(2000)));
    }

    #[test]
    fn test_registry_find_active_at_block() {
        let config = test_config_with_versions();
        let registry = ContractRegistry::from_config(&config).unwrap();

        // Before any deployment
        assert!(registry.find_active_at_block("Redistribution", BlockNumber(1000)).is_none());

        // During v0.9.2
        let meta = registry.find_active_at_block("Redistribution", BlockNumber(37339200)).unwrap();
        assert_eq!(meta.version.as_str(), "v0.9.2");

        // During v0.9.4
        let meta = registry.find_active_at_block("Redistribution", BlockNumber(41200000)).unwrap();
        assert_eq!(meta.version.as_str(), "v0.9.4");
    }
}
```

**Tasks:**
- [ ] Test `ContractAddress` validation
- [ ] Test `ContractMetadata` block range logic
- [ ] Test `ContractRegistry` queries
- [ ] Test event attribution logic
- [ ] Test configuration validation
- [ ] Achieve >80% code coverage for new code

#### 5.2 Integration Tests

```rust
// tests/integration_versioning_test.rs (NEW)

#[tokio::test]
async fn test_fetch_from_multiple_versions() {
    let config = load_test_config();
    let cache = Cache::new("/tmp/test-versioning.db").await.unwrap();
    let registry = ContractRegistry::from_config(&config).unwrap();
    let client = BlockchainClient::new(&config.rpc.url).await.unwrap();

    // Fetch from block range covering version transition
    let from_block = 40400000;  // Before v0.9.3 Redistribution
    let to_block = 40450000;    // After v0.9.3 Redistribution

    // Should fetch from both v0.9.2 and v0.9.3
    // ... test logic
}
```

**Tasks:**
- [ ] Test fetching across version boundaries
- [ ] Test backfill migration
- [ ] Test event storage with contract_address
- [ ] Test queries filtering by contract_address
- [ ] Test CLI commands with version filtering

#### 5.3 GnosisScan Verification Tests

```rust
// tests/gnosisscan_verification_test.rs (NEW)

#[tokio::test]
#[ignore]  // Run manually with: cargo test --ignored
async fn verify_event_attribution_on_chain() {
    // Fetch sample events from database
    // Query GnosisScan for same transactions
    // Verify contract_address matches log.address
}
```

**Tasks:**
- [ ] Create verification script
- [ ] Test 10+ sample transactions
- [ ] Test events during overlap periods
- [ ] Document any discrepancies
- [ ] Fix attribution if needed

---

### Phase 6: Documentation

**Duration:** 0.5 session
**Dependencies:** All previous phases

#### 6.1 Update CLAUDE.md

**New Sections:**
- Contract Versioning Architecture
- Type Safety with Newtypes
- Event Attribution Logic
- Building a Blockchain Event Bot (Reusability Guide)

#### 6.2 Update README.md

**New Sections:**
- Multi-version Contract Support
- Querying by Contract Version
- Historical Data Analysis

#### 6.3 Create Migration Guide

```markdown
# Migrating to Contract Versioning (v2.0)

## For Existing Databases

1. **Backup your database**
   ```bash
   cp stamp-cache.db stamp-cache.db.backup
   ```

2. **Run the tool (migrations run automatically)**
   ```bash
   beeport-stamp-stats fetch --from-block <current> --to-block <latest>
   ```

3. **Verify migration**
   ```bash
   sqlite3 stamp-cache.db "SELECT COUNT(*) FROM events WHERE contract_address IS NOT NULL"
   ```

## For New Installations

No special steps required. The tool works out of the box with contract versioning support.

## Breaking Changes

- `StampEvent` and `StorageIncentivesEvent` now include `contract_address` field
- Configuration requires `version` and `active` fields for all contracts
- Exported JSON/CSV includes contract_address column

## Rollback (if needed)

```bash
cp stamp-cache.db.backup stamp-cache.db
```
```

**Tasks:**
- [ ] Write migration guide
- [ ] Update architecture diagrams
- [ ] Add query examples
- [ ] Document new CLI commands
- [ ] Create upgrade checklist

---

## Architectural Benefits for Bot Development

### Reusable Event Processing Pipeline

**Key Insight:** This architecture separates concerns cleanly, making it trivial to build a bot.

```rust
// Example: Bot that reacts to BatchCreated events

use beeport_tx_stats::{
    events::{StampEvent, EventData},
    blockchain::BlockchainClient,
    contracts::ContractRegistry,
};

struct EventReactor {
    webhook_url: String,
}

impl EventReactor {
    async fn on_event(&self, event: &StampEvent) {
        match &event.data {
            EventData::BatchCreated { owner, depth, normalised_balance, .. } => {
                // React to new batch
                let payload = json!({
                    "type": "batch_created",
                    "owner": owner,
                    "depth": depth,
                    "balance": normalised_balance,
                    "contract": event.contract_address.as_str(),
                    "version": self.get_version(&event.contract_address),
                });

                // Send webhook
                reqwest::Client::new()
                    .post(&self.webhook_url)
                    .json(&payload)
                    .send()
                    .await
                    .unwrap();
            }
            _ => {}
        }
    }
}

#[tokio::main]
async fn main() {
    let config = AppConfig::load().unwrap();
    let registry = ContractRegistry::from_config(&config).unwrap();
    let client = BlockchainClient::new(&config.rpc.url).await.unwrap();
    let reactor = EventReactor { webhook_url: "https://...".into() };

    // Follow mode: stream events
    let mut current_block = get_latest_block().await;
    loop {
        let events = client.fetch_batch_events(
            &registry,
            current_block,
            current_block + 100,
        ).await.unwrap();

        for event in events {
            reactor.on_event(&event).await;
        }

        current_block += 100;
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}
```

**What Makes This Reusable:**

1. **Event Structures are Generic** - `StampEvent` doesn't know about database
2. **Client is Stateless** - No hidden dependencies
3. **Registry is Queryable** - Easy to ask "which version at block X?"
4. **Type Safety** - Can't mix up addresses or block numbers
5. **Separation of Source/Processor/Sink** - Swap any component

---

## Success Criteria

### Must Have ✓
- [x] Database schema includes `contract_address`
- [x] All events attributed to correct contract address
- [x] Configuration supports multiple versions
- [x] Registry can query by address and block
- [x] Type-safe address and version types
- [x] Migrations work on SQLite and PostgreSQL
- [x] Comprehensive tests (unit + integration)

### Should Have ✓
- [x] CLI commands for contract info
- [x] Historical fetch support
- [x] Version filtering in analysis
- [x] Export includes contract_address
- [x] Documentation fully updated

### Nice to Have ✓
- [x] Contract timeline visualization
- [x] Bot example in documentation
- [x] GnosisScan verification tests
- [x] Automated version detection

---

## Implementation Checklist

### Phase 0: Type System ⬜
- [ ] Create `src/types.rs` module
- [ ] Implement `ContractAddress` newtype
- [ ] Implement `ContractVersion` newtype
- [ ] Implement `BlockNumber` newtype
- [ ] Add validation and tests
- [ ] Update `StampEvent` structure
- [ ] Update `StorageIncentivesEvent` structure

### Phase 1: Configuration ⬜
- [ ] Create `src/contracts/metadata.rs`
- [ ] Update `ContractConfig` with version fields
- [ ] Update `config.yaml` with all 17 historical contracts
- [ ] Enhance `ContractRegistry` with metadata
- [ ] Implement registry query methods
- [ ] Add configuration validation
- [ ] Add registry tests

### Phase 2: Database ⬜
- [ ] Write SQLite migration
- [ ] Write PostgreSQL migration
- [ ] Update `Cache::store_events()`
- [ ] Update `Cache::store_storage_incentives_events()`
- [ ] Test migrations
- [ ] Verify backfill correctness

### Phase 3: Event Attribution ⬜
- [ ] Update `Contract` trait signatures
- [ ] Update parsing functions to accept address
- [ ] Update `BlockchainClient` to extract log.address
- [ ] Add address verification logic
- [ ] Update all contract implementations
- [ ] Add parsing tests

### Phase 4: CLI ⬜
- [ ] Add `contracts` command
- [ ] Add `--include-historical` flag
- [ ] Add `--contract-address` filter
- [ ] Update summary display
- [ ] Update export formats
- [ ] Add CLI tests

### Phase 5: Testing ⬜
- [ ] Write unit tests (types, metadata, registry)
- [ ] Write integration tests (fetch, store, query)
- [ ] Write GnosisScan verification tests
- [ ] Achieve >80% coverage
- [ ] Test all edge cases

### Phase 6: Documentation ⬜
- [ ] Update CLAUDE.md
- [ ] Update README.md
- [ ] Create migration guide
- [ ] Add bot development guide
- [ ] Update query examples

---

## Timeline

| Phase | Duration | Cumulative |
|-------|----------|-----------|
| Phase 0: Types | 0.5 session | 0.5 |
| Phase 1: Config | 1.0 session | 1.5 |
| Phase 2: Database | 1.0 session | 2.5 |
| Phase 3: Attribution | 1.0 session | 3.5 |
| Phase 4: CLI | 1.0 session | 4.5 |
| Phase 5: Testing | 1.5 sessions | 6.0 |
| Phase 6: Docs | 0.5 session | 6.5 |

**Total: ~6.5 sessions** (vs 8 in original plan)

**Efficiency Gain:** ~20% through better architecture planning

---

## Appendix: Complete Contract Deployment Reference

### PostageStamp
```
v0.8.6 @ 31305656: 0x45a1502382541Cd610CC9068e88727426b696293 [ACTIVE]
Phase4 @ 25527076: 0x30d155478eF27Ab32A1D578BE7b84BC5988aF381
```

### Redistribution
```
v0.9.4 @ 41105199: 0x5069cdfB3D9E56d23B1cAeE83CE6109A7E4fd62d [ACTIVE]
v0.9.3 @ 40430261: 0x9f9A8dA5A0Db2611f9802ba1a0B99cC4A1c3b6A2
v0.9.2 @ 37339181: 0x69C62CaCd68C2CBBf3D0C7502eF556DB3AC7889B
v0.9.1 @ 35961755: 0xFfF73fd14537277B3F3807e1AB0F85E17c0ABea5
v0.8.6 @ 34159666: 0xD9dFE7b0ddc7CcA41304FE9507ed823faD3bdBab
Phase4 @ 31305409: 0x1F9a1FDe5c6350E949C5E4aa163B4c97011199B4
```

### StakeRegistry
```
v0.9.3 @ 40430237: 0xda2a16EE889E7F04980A8d597b48c8D51B9518F4 [ACTIVE]
v0.9.2 @ 37339175: 0x445B848e16730988F871c4a09aB74526d27c2Ce8
v0.9.1 @ 35961749: 0xBe212EA1A4978a64e8f7636Ae18305C38CA092Bd
v0.4.0 @ 25527075: 0x781c6D1f0eaE6F1Da1F604c6cDCcdB8B76428ba7
```

### PriceOracle
```
v0.9.2 @ 37339168: 0x47EeF336e7fE5bED98499A4696bce8f28c1B0a8b [ACTIVE]
v0.9.1 @ 31305665: 0x86DE783Bf23Bc13DaeF5A55ec531C198da8f10cF
Phase4 @ 25527079: 0x344A2CC7304B32A87EfDC5407cD4bEC7cf98F035
```

### StampsRegistry
```
v1.0.0 @ 42390510: 0x5EBfBeFB1E88391eFb022d5d33302f50a46bF4f3 [ACTIVE]
```

---

*Last Updated: 2025-12-21*
*Architecture-First, Reusable, Type-Safe*
*Ready for Implementation*
