/// Contract abstraction module
///
/// This module provides a trait-based abstraction for Ethereum contracts,
/// enabling polymorphic behavior and eliminating code duplication.
///
/// # Architecture
///
/// - `Contract` trait: Defines the interface all contracts must implement
/// - `ContractRegistry`: Manages multiple contracts and provides lookup
/// - `impls`: Concrete contract implementations (PostageStamp, StampsRegistry)
/// - `parser`: Generic event parsing logic (eliminates duplication)
/// - `abi`: Contract ABIs using sol! macro
pub mod abi;
pub mod impls;
pub mod parser;

use crate::config::AppConfig;
use crate::error::Result;
use crate::events::StampEvent;
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

/// Registry to manage all active contracts
///
/// The registry is built from configuration and provides:
/// - Iteration over all contracts
/// - Lookup by name
/// - Lookup by capability (price query, balance query)
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
/// ```
pub struct ContractRegistry {
    contracts: Vec<Box<dyn Contract>>,
}

impl std::fmt::Debug for ContractRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ContractRegistry")
            .field("contracts", &self.contracts.iter().map(|c| c.name()).collect::<Vec<_>>())
            .finish()
    }
}

impl ContractRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            contracts: Vec::new(),
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

    /// Build a registry from configuration
    ///
    /// # Arguments
    ///
    /// * `config` - Application configuration
    ///
    /// # Returns
    ///
    /// - `Ok(ContractRegistry)` if all contracts were successfully registered
    /// - `Err(...)` if any contract type is unknown
    ///
    /// # Example
    ///
    /// ```ignore
    /// let config = AppConfig::load()?;
    /// let registry = ContractRegistry::from_config(&config)?;
    /// ```
    pub fn from_config(config: &AppConfig) -> Result<Self> {
        let mut registry = Self::new();

        for contract_config in &config.contracts {
            let contract: Box<dyn Contract> = match contract_config.contract_type.as_str() {
                "PostageStamp" => Box::new(impls::PostageStampContract::new(
                    contract_config.address.clone(),
                    contract_config.deployment_block,
                )),
                "StampsRegistry" => Box::new(impls::StampsRegistryContract::new(
                    contract_config.address.clone(),
                    contract_config.deployment_block,
                )),
                _ => {
                    return Err(crate::error::StampError::Config(format!(
                        "Unknown contract type '{}' for contract '{}'. Valid types: PostageStamp, StampsRegistry",
                        contract_config.contract_type, contract_config.name
                    )))
                }
            };

            registry.register(contract);
        }

        Ok(registry)
    }
}

impl Default for ContractRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AppConfig, ContractConfig};

    #[test]
    fn test_registry_creation() {
        let registry = ContractRegistry::new();
        assert_eq!(registry.all().len(), 0);
    }

    #[test]
    fn test_registry_from_config() {
        let config = AppConfig::default();
        let registry = ContractRegistry::from_config(&config).unwrap();

        // Default config has 2 contracts
        assert_eq!(registry.all().len(), 2);

        // Find by name
        assert!(registry.find_by_name("PostageStamp").is_some());
        assert!(registry.find_by_name("StampsRegistry").is_some());
        assert!(registry.find_by_name("UnknownContract").is_none());
    }

    #[test]
    fn test_registry_unknown_contract_type() {
        let mut config = AppConfig::default();
        config.contracts.push(ContractConfig {
            name: "Unknown".to_string(),
            contract_type: "UnknownContract".to_string(),
            address: "0x1234567890123456789012345678901234567890".to_string(),
            deployment_block: 1000,
        });

        let result = ContractRegistry::from_config(&config);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Unknown contract type"));
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
