/// Concrete contract implementations
///
/// This module provides implementations of the Contract trait for:
/// - PostageStamp: Main contract for direct stamp purchases
/// - StampsRegistry: UI-based stamp purchases with payer tracking
use super::parser::{parse_postage_stamp_event, parse_stamps_registry_event};
use super::Contract;
use crate::error::Result;
use crate::events::StampEvent;
use alloy::primitives::TxHash;
use alloy::rpc::types::Log;
use chrono::{DateTime, Utc};

/// PostageStamp contract implementation
///
/// The PostageStamp contract is the main contract for direct postage stamp purchases
/// on the Swarm network. It tracks batch creation, top-ups, and depth increases.
///
/// # Capabilities
///
/// - Price queries: Yes (via lastPrice())
/// - Balance queries: Yes (via remainingBalance())
///
/// # Events
///
/// - BatchCreated
/// - BatchTopUp
/// - BatchDepthIncrease
pub struct PostageStampContract {
    address: String,
    deployment_block: u64,
}

impl PostageStampContract {
    /// Create a new PostageStamp contract instance
    ///
    /// # Arguments
    ///
    /// * `address` - Contract address (hex string with 0x prefix)
    /// * `deployment_block` - Block number when contract was deployed
    pub fn new(address: String, deployment_block: u64) -> Self {
        Self {
            address,
            deployment_block,
        }
    }
}

impl Contract for PostageStampContract {
    fn name(&self) -> &str {
        "PostageStamp"
    }

    fn address(&self) -> &str {
        &self.address
    }

    fn deployment_block(&self) -> u64 {
        self.deployment_block
    }

    fn parse_log(
        &self,
        log: Log,
        block_number: u64,
        block_timestamp: DateTime<Utc>,
        transaction_hash: TxHash,
        log_index: u64,
    ) -> Result<Option<StampEvent>> {
        // Use dedicated PostageStamp parser
        parse_postage_stamp_event(
            log,
            block_number,
            block_timestamp,
            transaction_hash,
            log_index,
            self.name(),
        )
    }

    fn supports_price_query(&self) -> bool {
        true // PostageStamp has lastPrice() function
    }

    fn supports_balance_query(&self) -> bool {
        true // PostageStamp has remainingBalance() function
    }
}

/// StampsRegistry contract implementation
///
/// The StampsRegistry contract is used for UI-based stamp purchases and includes
/// additional payer tracking information. It internally calls the PostageStamp
/// contract, so both contracts emit BatchCreated events for the same batch.
///
/// # Capabilities
///
/// - Price queries: No
/// - Balance queries: No
///
/// # Events
///
/// - BatchCreated (with payer field)
/// - BatchTopUp (with payer field)
/// - BatchDepthIncrease (with payer field)
pub struct StampsRegistryContract {
    address: String,
    deployment_block: u64,
}

impl StampsRegistryContract {
    /// Create a new StampsRegistry contract instance
    ///
    /// # Arguments
    ///
    /// * `address` - Contract address (hex string with 0x prefix)
    /// * `deployment_block` - Block number when contract was deployed
    pub fn new(address: String, deployment_block: u64) -> Self {
        Self {
            address,
            deployment_block,
        }
    }
}

impl Contract for StampsRegistryContract {
    fn name(&self) -> &str {
        "StampsRegistry"
    }

    fn address(&self) -> &str {
        &self.address
    }

    fn deployment_block(&self) -> u64 {
        self.deployment_block
    }

    fn parse_log(
        &self,
        log: Log,
        block_number: u64,
        block_timestamp: DateTime<Utc>,
        transaction_hash: TxHash,
        log_index: u64,
    ) -> Result<Option<StampEvent>> {
        // Use dedicated StampsRegistry parser
        parse_stamps_registry_event(
            log,
            block_number,
            block_timestamp,
            transaction_hash,
            log_index,
            self.name(),
        )
    }

    fn supports_price_query(&self) -> bool {
        false // StampsRegistry doesn't have price query functions
    }

    fn supports_balance_query(&self) -> bool {
        false // StampsRegistry doesn't have balance query functions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_postage_stamp_contract_metadata() {
        let contract = PostageStampContract::new(
            "0x1234567890123456789012345678901234567890".to_string(),
            1000,
        );

        assert_eq!(contract.name(), "PostageStamp");
        assert_eq!(
            contract.address(),
            "0x1234567890123456789012345678901234567890"
        );
        assert_eq!(contract.deployment_block(), 1000);
        assert!(contract.supports_price_query());
        assert!(contract.supports_balance_query());
    }

    #[test]
    fn test_stamps_registry_contract_metadata() {
        let contract = StampsRegistryContract::new(
            "0x1234567890123456789012345678901234567890".to_string(),
            2000,
        );

        assert_eq!(contract.name(), "StampsRegistry");
        assert_eq!(
            contract.address(),
            "0x1234567890123456789012345678901234567890"
        );
        assert_eq!(contract.deployment_block(), 2000);
        assert!(!contract.supports_price_query());
        assert!(!contract.supports_balance_query());
    }
}
