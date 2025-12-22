/// Contract abstraction module
///
/// This module provides a trait-based abstraction for Ethereum contracts,
/// enabling polymorphic behavior and eliminating code duplication.
///
/// # Architecture
///
/// - `Contract` trait: Defines the interface all contracts must implement
/// - `ContractRegistry`: Manages multiple contracts and provides lookup
/// - `StorageIncentivesContract` trait: For storage incentives contracts
/// - `impls`: Concrete contract implementations
/// - `parser`: Generic event parsing logic (eliminates duplication)
/// - `abi`: Contract ABIs using sol! macro
/// - `metadata`: Contract metadata and version information
pub mod abi;
pub mod impls;
pub mod metadata;
pub mod parser;

// Contract implementations are available in the impls module
// They are instantiated through the registry rather than being used directly

use crate::config::AppConfig;
use crate::error::Result;
use crate::events::{StampEvent, StorageIncentivesEvent};
use alloy::primitives::TxHash;
use alloy::rpc::types::Log;
use chrono::{DateTime, Utc};

/// Trait defining contract behavior
///
/// All contracts must implement this trait to participate in the event
/// tracking system. The trait provides:
///
/// - Metadata (name, address, deployment block)
/// - Event parsing capability
/// - Optional capabilities (price queries, balance queries)
///
/// # Example
///
/// ```ignore
/// struct MyContract {
///     address: String,
///     deployment_block: u64,
/// }
///
/// impl Contract for MyContract {
///     fn name(&self) -> &str { "MyContract" }
///     fn address(&self) -> &str { &self.address }
///     fn deployment_block(&self) -> u64 { self.deployment_block }
///
///     fn parse_log(...) -> Result<Option<StampEvent>> {
///         // Parse contract events
///     }
/// }
/// ```
pub trait Contract: Send + Sync {
    /// Contract name (e.g., "PostageStamp")
    fn name(&self) -> &str;

    /// Contract address on blockchain (hex string with 0x prefix)
    fn address(&self) -> &str;

    /// Block number when contract was deployed
    ///
    /// Events before this block are not fetched.
    fn deployment_block(&self) -> u64;

    /// Parse a raw log into a StampEvent
    ///
    /// # Arguments
    ///
    /// * `log` - Raw log from RPC
    /// * `block_number` - Block containing the log
    /// * `block_timestamp` - Timestamp of the block
    /// * `transaction_hash` - Transaction that emitted the log
    /// * `log_index` - Index of log within the transaction
    ///
    /// # Returns
    ///
    /// - `Ok(Some(event))` if log was successfully parsed
    /// - `Ok(None)` if log is not a recognized event type
    /// - `Err(...)` if parsing failed
    fn parse_log(
        &self,
        log: Log,
        block_number: u64,
        block_timestamp: DateTime<Utc>,
        transaction_hash: TxHash,
        log_index: u64,
    ) -> Result<Option<StampEvent>>;

    /// Whether this contract supports price queries
    ///
    /// Default: false
    fn supports_price_query(&self) -> bool {
        false
    }

    /// Whether this contract supports balance queries
    ///
    /// Default: false
    fn supports_balance_query(&self) -> bool {
        false
    }
}

/// Trait defining storage incentives contract behavior
///
/// Storage incentives contracts (PriceOracle, StakeRegistry, Redistribution)
/// emit different event types than postage stamp contracts.
///
/// This trait is similar to Contract but returns StorageIncentivesEvent.
pub trait StorageIncentivesContract: Send + Sync {
    /// Contract name (e.g., "PriceOracle")
    fn name(&self) -> &str;

    /// Contract address on blockchain (hex string with 0x prefix)
    fn address(&self) -> &str;

    /// Block number when contract was deployed
    fn deployment_block(&self) -> u64;

    /// Parse a raw log into a StorageIncentivesEvent
    fn parse_log(
        &self,
        log: Log,
        block_number: u64,
        block_timestamp: DateTime<Utc>,
        transaction_hash: TxHash,
        log_index: u64,
    ) -> Result<Option<StorageIncentivesEvent>>;
}

/// Registry to manage all active contracts
///
/// The registry is built from configuration and provides:
/// - Iteration over all contracts
/// - Lookup by name
/// - Lookup by capability (price query, balance query)
/// - Lookup by address (for event attribution)
/// - Lookup by block number (for historical queries)
/// - Version management (active vs historical contracts)
///
/// # Example
///
/// ```ignore
/// let registry = ContractRegistry::from_config(&config)?;
///
/// // Iterate all contracts
/// for contract in registry.all() {
///     println!("Contract: {}", contract.name());
/// }
///
/// // Find by name
/// if let Some(contract) = registry.find_by_name("PostageStamp") {
///     println!("Found: {}", contract.address());
/// }
///
/// // Find active contract at specific block
/// if let Some(meta) = registry.find_active_at_block("Redistribution", BlockNumber(40500000)) {
///     println!("Active version: {}", meta.version.as_str());
/// }
/// ```
pub struct ContractRegistry {
    contracts: Vec<Box<dyn Contract>>,

    // Metadata for all contracts (active + historical)
    metadata: Vec<metadata::ContractMetadata>,

    // Fast lookup: address → metadata index
    address_map: std::collections::HashMap<crate::types::ContractAddress, usize>,

    // Fast lookup: contract type → Vec<metadata index> (sorted by deployment block)
    type_map: std::collections::HashMap<String, Vec<usize>>,
}

impl std::fmt::Debug for ContractRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ContractRegistry")
            .field("contracts", &self.contracts.iter().map(|c| c.name()).collect::<Vec<_>>())
            .field("metadata_count", &self.metadata.len())
            .field("active_count", &self.metadata.iter().filter(|m| m.active).count())
            .field("historical_count", &self.metadata.iter().filter(|m| !m.active).count())
            .finish()
    }
}

impl ContractRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            contracts: Vec::new(),
            metadata: Vec::new(),
            address_map: std::collections::HashMap::new(),
            type_map: std::collections::HashMap::new(),
        }
    }

    /// Register a contract
    ///
    /// # Arguments
    ///
    /// * `contract` - Boxed contract implementation
    pub fn register(&mut self, contract: Box<dyn Contract>) {
        self.contracts.push(contract);
    }

    /// Get all registered contracts
    ///
    /// Returns a slice of contract trait objects.
    pub fn all(&self) -> &[Box<dyn Contract>] {
        &self.contracts
    }

    /// Find a contract by name
    ///
    /// # Arguments
    ///
    /// * `name` - Contract name to search for
    ///
    /// # Returns
    ///
    /// - `Some(&dyn Contract)` if found
    /// - `None` if not found
    #[allow(dead_code)]
    pub fn find_by_name(&self, name: &str) -> Option<&dyn Contract> {
        self.contracts
            .iter()
            .find(|c| c.name() == name)
            .map(|b| b.as_ref())
    }

    /// Find the first contract that supports price queries
    ///
    /// # Returns
    ///
    /// - `Some(&dyn Contract)` if found
    /// - `None` if no contract supports price queries
    pub fn find_price_query_contract(&self) -> Option<&dyn Contract> {
        self.contracts
            .iter()
            .find(|c| c.supports_price_query())
            .map(|b| b.as_ref())
    }

    /// Find the first contract that supports balance queries
    ///
    /// # Returns
    ///
    /// - `Some(&dyn Contract)` if found
    /// - `None` if no contract supports balance queries
    pub fn find_balance_query_contract(&self) -> Option<&dyn Contract> {
        self.contracts
            .iter()
            .find(|c| c.supports_balance_query())
            .map(|b| b.as_ref())
    }

    /// Find contract metadata by address
    ///
    /// # Arguments
    ///
    /// * `addr` - Contract address to search for
    ///
    /// # Returns
    ///
    /// - `Some(&ContractMetadata)` if found
    /// - `None` if not found
    #[allow(dead_code)]
    pub fn find_by_address(&self, addr: &crate::types::ContractAddress) -> Option<&metadata::ContractMetadata> {
        self.address_map.get(addr)
            .map(|&idx| &self.metadata[idx])
    }

    /// Find active contract of a given type
    ///
    /// Returns the contract marked as `active: true` for the specified type.
    ///
    /// # Arguments
    ///
    /// * `contract_type` - Contract type to search for (e.g., "Redistribution")
    ///
    /// # Returns
    ///
    /// - `Some(&ContractMetadata)` if found
    /// - `None` if not found or no active contract of that type
    #[allow(dead_code)]
    pub fn find_active_by_type(&self, contract_type: &str) -> Option<&metadata::ContractMetadata> {
        self.type_map.get(contract_type)?
            .iter()
            .find_map(|&idx| {
                let meta = &self.metadata[idx];
                if meta.active { Some(meta) } else { None }
            })
    }

    /// Find which contract was active at a specific block
    ///
    /// Uses block ranges to determine which version was active at the given block.
    ///
    /// # Arguments
    ///
    /// * `contract_type` - Contract type to search for (e.g., "Redistribution")
    /// * `block` - Block number to query
    ///
    /// # Returns
    ///
    /// - `Some(&ContractMetadata)` if found
    /// - `None` if no contract was active at that block
    #[allow(dead_code)]
    pub fn find_active_at_block(
        &self,
        contract_type: &str,
        block: crate::types::BlockNumber
    ) -> Option<&metadata::ContractMetadata> {
        self.type_map.get(contract_type)?
            .iter()
            .find_map(|&idx| {
                let meta = &self.metadata[idx];
                if meta.active_at_block(block) { Some(meta) } else { None }
            })
    }

    /// Get all versions of a contract type, sorted by deployment block
    ///
    /// # Arguments
    ///
    /// * `contract_type` - Contract type to search for (e.g., "Redistribution")
    ///
    /// # Returns
    ///
    /// Vector of metadata references, sorted by deployment block (oldest first)
    #[allow(dead_code)]
    pub fn get_versions(&self, contract_type: &str) -> Vec<&metadata::ContractMetadata> {
        self.type_map.get(contract_type)
            .map(|indexes| {
                indexes.iter()
                    .map(|&idx| &self.metadata[idx])
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get all contract metadata (active + historical)
    ///
    /// # Returns
    ///
    /// Slice of all contract metadata
    #[allow(dead_code)]
    pub fn get_all_metadata(&self) -> &[metadata::ContractMetadata] {
        &self.metadata
    }

    /// Build a registry from configuration
    ///
    /// # Arguments
    ///
    /// * `config` - Application configuration
    ///
    /// # Returns
    ///
    /// - `Ok(ContractRegistry)` if all contracts were successfully registered
    /// - `Err(...)` if any contract type is unknown or validation fails
    ///
    /// # Example
    ///
    /// ```ignore
    /// let config = AppConfig::load()?;
    /// let registry = ContractRegistry::from_config(&config)?;
    /// ```
    pub fn from_config(config: &AppConfig) -> Result<Self> {
        let mut registry = Self::new();
        let mut address_map = std::collections::HashMap::new();
        let mut type_map: std::collections::HashMap<String, Vec<usize>> = std::collections::HashMap::new();

        // First pass: Build metadata for ALL contracts (active + historical)
        // and validate configuration
        for (idx, contract_config) in config.contracts.iter().enumerate() {
            // Validate configuration
            contract_config.validate()
                .map_err(crate::error::StampError::Config)?;

            // Convert to metadata
            let meta = contract_config.to_metadata()
                .map_err(crate::error::StampError::Config)?;

            // Check for duplicate addresses
            if address_map.contains_key(&meta.address) {
                return Err(crate::error::StampError::Config(format!(
                    "Duplicate contract address '{}' for contract '{}'",
                    meta.address.as_str(), meta.name
                )));
            }

            // Add to address map
            address_map.insert(meta.address.clone(), idx);

            // Add to type map
            type_map.entry(meta.contract_type.clone())
                .or_default()
                .push(idx);

            // Store metadata
            registry.metadata.push(meta);
        }

        // Sort type_map entries by deployment block
        for indexes in type_map.values_mut() {
            indexes.sort_by_key(|&idx| registry.metadata[idx].deployment_block);
        }

        // Store indexes
        registry.address_map = address_map;
        registry.type_map = type_map;

        // Second pass: Build Contract trait objects for ACTIVE contracts only
        // (only PostageStamp and StampsRegistry, not storage incentives)
        for contract_config in &config.contracts {
            // Only create Contract trait objects for active PostageStamp/StampsRegistry
            if !contract_config.active {
                continue;
            }

            let contract: Option<Box<dyn Contract>> = match contract_config.contract_type.as_str() {
                "PostageStamp" => Some(Box::new(impls::PostageStampContract::new(
                    contract_config.address.clone(),
                    contract_config.deployment_block,
                ))),
                "StampsRegistry" => Some(Box::new(impls::StampsRegistryContract::new(
                    contract_config.address.clone(),
                    contract_config.deployment_block,
                ))),
                // Skip storage incentives contracts (handled by StorageIncentivesContractRegistry)
                "PriceOracle" | "StakeRegistry" | "Redistribution" => None,
                _ => {
                    return Err(crate::error::StampError::Config(format!(
                        "Unknown contract type '{}' for contract '{}'. Valid types: PostageStamp, StampsRegistry, PriceOracle, StakeRegistry, Redistribution",
                        contract_config.contract_type, contract_config.name
                    )))
                }
            };

            if let Some(contract) = contract {
                registry.register(contract);
            }
        }

        Ok(registry)
    }
}

impl Default for ContractRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Registry to manage all active storage incentives contracts
///
/// Similar to ContractRegistry but for storage incentives contracts
/// (PriceOracle, StakeRegistry, Redistribution) that emit different event types.
pub struct StorageIncentivesContractRegistry {
    contracts: Vec<Box<dyn StorageIncentivesContract>>,
}

impl std::fmt::Debug for StorageIncentivesContractRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StorageIncentivesContractRegistry")
            .field("contracts", &self.contracts.iter().map(|c| c.name()).collect::<Vec<_>>())
            .finish()
    }
}

impl StorageIncentivesContractRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            contracts: Vec::new(),
        }
    }

    /// Register a storage incentives contract
    pub fn register(&mut self, contract: Box<dyn StorageIncentivesContract>) {
        self.contracts.push(contract);
    }

    /// Get all registered contracts
    pub fn all(&self) -> &[Box<dyn StorageIncentivesContract>] {
        &self.contracts
    }

    /// Find a contract by name
    #[allow(dead_code)]
    pub fn find_by_name(&self, name: &str) -> Option<&dyn StorageIncentivesContract> {
        self.contracts
            .iter()
            .find(|c| c.name() == name)
            .map(|b| b.as_ref())
    }

    /// Build a registry from configuration
    ///
    /// Extracts PriceOracle, StakeRegistry, and Redistribution contracts from config.
    pub fn from_config(config: &AppConfig) -> Result<Self> {
        let mut registry = Self::new();

        for contract_config in &config.contracts {
            let contract: Option<Box<dyn StorageIncentivesContract>> =
                match contract_config.contract_type.as_str() {
                    "PriceOracle" => Some(Box::new(impls::PriceOracleContract::new(
                        contract_config.address.clone(),
                        contract_config.deployment_block,
                    ))),
                    "StakeRegistry" => Some(Box::new(impls::StakeRegistryContract::new(
                        contract_config.address.clone(),
                        contract_config.deployment_block,
                    ))),
                    "Redistribution" => Some(Box::new(impls::RedistributionContract::new(
                        contract_config.address.clone(),
                        contract_config.deployment_block,
                    ))),
                    // Skip non-storage-incentives contracts
                    "PostageStamp" | "StampsRegistry" => None,
                    _ => {
                        return Err(crate::error::StampError::Config(format!(
                            "Unknown contract type '{}' for contract '{}'. Valid types: PostageStamp, StampsRegistry, PriceOracle, StakeRegistry, Redistribution",
                            contract_config.contract_type, contract_config.name
                        )))
                    }
                };

            if let Some(contract) = contract {
                registry.register(contract);
            }
        }

        Ok(registry)
    }
}

impl Default for StorageIncentivesContractRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;

    #[test]
    fn test_registry_creation() {
        let registry = ContractRegistry::new();
        assert_eq!(registry.all().len(), 0);
    }

    #[test]
    fn test_registry_from_config() {
        let config = AppConfig::default();
        let registry = ContractRegistry::from_config(&config).unwrap();

        // Default config has 2 ACTIVE PostageStamp/StampsRegistry contracts for trait objects
        assert_eq!(registry.all().len(), 2);

        // Metadata includes ALL contracts in default config (5 active contracts)
        // Note: config.yaml has 17 contracts, but AppConfig::default() only has 5
        assert_eq!(registry.metadata.len(), 5);

        // All 5 are active in default config
        assert_eq!(registry.metadata.iter().filter(|m| m.active).count(), 5);

        // Find by name (active contracts only for trait objects)
        assert!(registry.find_by_name("PostageStamp").is_some());
        assert!(registry.find_by_name("StampsRegistry").is_some());
        assert!(registry.find_by_name("UnknownContract").is_none());

        // Find by address
        let postage_addr = crate::types::ContractAddress::new("0x45a1502382541Cd610CC9068e88727426b696293").unwrap();
        assert!(registry.find_by_address(&postage_addr).is_some());

        // Find active by type
        assert!(registry.find_active_by_type("PostageStamp").is_some());
        assert!(registry.find_active_by_type("Redistribution").is_some());

        // Get versions (only 1 Redistribution version in default config)
        let redistribution_versions = registry.get_versions("Redistribution");
        assert_eq!(redistribution_versions.len(), 1);
    }

    #[test]
    fn test_registry_unknown_contract_type() {
        let mut config = AppConfig::default();
        config.contracts.push(crate::config::ContractConfig {
            name: "Unknown".to_string(),
            contract_type: "UnknownContract".to_string(),
            address: "0x1234567890123456789012345678901234567890".to_string(),
            deployment_block: 1000,
            version: Some("v1.0.0".to_string()),
            active: true,
            end_block: None,
            paused_at: None,
        });

        let result = ContractRegistry::from_config(&config);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Unknown contract type"));
    }

    #[test]
    fn test_storage_incentives_registry_from_config() {
        let config = AppConfig::default();
        let registry = StorageIncentivesContractRegistry::from_config(&config).unwrap();

        // Default config has 3 storage incentives contracts
        assert_eq!(registry.all().len(), 3);

        // Find by name
        assert!(registry.find_by_name("PriceOracle").is_some());
        assert!(registry.find_by_name("StakeRegistry").is_some());
        assert!(registry.find_by_name("Redistribution").is_some());
        assert!(registry.find_by_name("PostageStamp").is_none());
    }

    #[test]
    fn test_find_price_query_contract() {
        let config = AppConfig::default();
        let registry = ContractRegistry::from_config(&config).unwrap();

        // PostageStamp supports price queries
        let contract = registry.find_price_query_contract();
        assert!(contract.is_some());
        assert_eq!(contract.unwrap().name(), "PostageStamp");
    }

    #[test]
    fn test_find_balance_query_contract() {
        let config = AppConfig::default();
        let registry = ContractRegistry::from_config(&config).unwrap();

        // PostageStamp supports balance queries
        let contract = registry.find_balance_query_contract();
        assert!(contract.is_some());
        assert_eq!(contract.unwrap().name(), "PostageStamp");
    }
}
