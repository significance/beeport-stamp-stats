//! Contract metadata module
//!
//! Separates "what is this contract" (metadata) from "how does it work" (behavior).
//! This enables querying contract information without needing contract implementations.

use crate::types::{BlockNumber, ContractAddress, ContractVersion};
use serde::{Deserialize, Serialize};

/// Metadata about a contract deployment
///
/// Contains information about a specific contract version including its address,
/// deployment block, version identifier, and lifecycle information.
///
/// # Example
///
/// ```ignore
/// let metadata = ContractMetadata {
///     name: "Redistribution".to_string(),
///     contract_type: "Redistribution".to_string(),
///     address: ContractAddress::new("0x5069cdfB3D9E56d23B1cAeE83CE6109A7E4fd62d")?,
///     version: ContractVersion::new("v0.9.4"),
///     deployment_block: BlockNumber(41105199),
///     end_block: None,
///     active: true,
///     paused_at: None,
/// };
///
/// // Check if contract was active at a specific block
/// assert!(metadata.active_at_block(BlockNumber(41200000)));
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractMetadata {
    /// Unique name (e.g., "PostageStamp", "Redistribution-v0.9.3")
    pub name: String,

    /// Contract type (e.g., "PostageStamp", "Redistribution")
    ///
    /// Multiple contracts can have the same type (different versions)
    pub contract_type: String,

    /// On-chain address
    pub address: ContractAddress,

    /// Human-readable version (e.g., "v0.9.4", "Phase 4")
    pub version: ContractVersion,

    /// Block number when contract was deployed
    pub deployment_block: BlockNumber,

    /// Optional: Last active block (when superseded or stopped)
    ///
    /// If None, the contract is still active (or unknown end)
    pub end_block: Option<BlockNumber>,

    /// Whether this is the currently active version
    ///
    /// At most one contract of each type should be active
    pub active: bool,

    /// Optional: Block when contract was paused
    ///
    /// Useful for tracking when contracts were deliberately stopped
    pub paused_at: Option<BlockNumber>,
}

impl ContractMetadata {
    /// Check if this contract was active at a given block
    ///
    /// A contract is considered active at a block if:
    /// - The block is >= deployment_block
    /// - The block is < end_block (if end_block is set)
    ///
    /// # Arguments
    ///
    /// * `block` - Block number to check
    ///
    /// # Returns
    ///
    /// `true` if contract was active at that block, `false` otherwise
    ///
    /// # Example
    ///
    /// ```ignore
    /// let metadata = ContractMetadata {
    ///     deployment_block: BlockNumber(1000),
    ///     end_block: Some(BlockNumber(2000)),
    ///     // ... other fields
    /// };
    ///
    /// assert!(!metadata.active_at_block(BlockNumber(999)));  // Before deployment
    /// assert!(metadata.active_at_block(BlockNumber(1000)));  // At deployment
    /// assert!(metadata.active_at_block(BlockNumber(1500)));  // During lifetime
    /// assert!(!metadata.active_at_block(BlockNumber(2000))); // After end
    /// ```
    pub fn active_at_block(&self, block: BlockNumber) -> bool {
        // Not yet deployed
        if block < self.deployment_block {
            return false;
        }

        // Check if past end of life
        if let Some(end) = self.end_block
            && block >= end
        {
            return false;
        }

        true
    }

    /// Get block range for this contract
    ///
    /// Returns (deployment_block, optional end_block)
    ///
    /// # Returns
    ///
    /// Tuple of (start_block, optional_end_block)
    #[allow(dead_code)]
    pub fn block_range(&self) -> (BlockNumber, Option<BlockNumber>) {
        (self.deployment_block, self.end_block)
    }

    /// Check if contract is paused
    ///
    /// Note: This only checks if a pause event was recorded, not the current
    /// on-chain state (which would require an RPC call)
    #[allow(dead_code)]
    pub fn is_paused(&self) -> bool {
        self.paused_at.is_some()
    }

    /// Get a human-readable status string
    ///
    /// # Returns
    ///
    /// Status description (e.g., "Active", "Superseded", "Paused")
    #[allow(dead_code)]
    pub fn status(&self) -> String {
        if self.active {
            return "Active".to_string();
        }

        if self.is_paused() {
            return "Paused".to_string();
        }

        if self.end_block.is_some() {
            return "Superseded".to_string();
        }

        "Inactive".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_metadata() -> ContractMetadata {
        ContractMetadata {
            name: "Redistribution-v0.9.3".to_string(),
            contract_type: "Redistribution".to_string(),
            address: ContractAddress::new("0x9f9A8dA5A0Db2611f9802ba1a0B99cC4A1c3b6A2").unwrap(),
            version: ContractVersion::new("v0.9.3"),
            deployment_block: BlockNumber(40430261),
            end_block: Some(BlockNumber(41105199)),
            active: false,
            paused_at: Some(BlockNumber(41150000)),
        }
    }

    #[test]
    fn test_active_at_block_before_deployment() {
        let metadata = test_metadata();
        assert!(!metadata.active_at_block(BlockNumber(40430260)));
    }

    #[test]
    fn test_active_at_block_at_deployment() {
        let metadata = test_metadata();
        assert!(metadata.active_at_block(BlockNumber(40430261)));
    }

    #[test]
    fn test_active_at_block_during_lifetime() {
        let metadata = test_metadata();
        assert!(metadata.active_at_block(BlockNumber(40500000)));
    }

    #[test]
    fn test_active_at_block_at_end() {
        let metadata = test_metadata();
        assert!(!metadata.active_at_block(BlockNumber(41105199)));
    }

    #[test]
    fn test_active_at_block_after_end() {
        let metadata = test_metadata();
        assert!(!metadata.active_at_block(BlockNumber(41200000)));
    }

    #[test]
    fn test_active_at_block_no_end() {
        let mut metadata = test_metadata();
        metadata.end_block = None;

        // Should be active forever after deployment
        assert!(metadata.active_at_block(BlockNumber(50000000)));
    }

    #[test]
    fn test_block_range() {
        let metadata = test_metadata();
        let (start, end) = metadata.block_range();

        assert_eq!(start, BlockNumber(40430261));
        assert_eq!(end, Some(BlockNumber(41105199)));
    }

    #[test]
    fn test_is_paused() {
        let metadata = test_metadata();
        assert!(metadata.is_paused());

        let mut unpaused = metadata.clone();
        unpaused.paused_at = None;
        assert!(!unpaused.is_paused());
    }

    #[test]
    fn test_status_active() {
        let mut metadata = test_metadata();
        metadata.active = true;
        assert_eq!(metadata.status(), "Active");
    }

    #[test]
    fn test_status_paused() {
        let metadata = test_metadata();
        assert_eq!(metadata.status(), "Paused");
    }

    #[test]
    fn test_status_superseded() {
        let mut metadata = test_metadata();
        metadata.paused_at = None;
        assert_eq!(metadata.status(), "Superseded");
    }

    #[test]
    fn test_status_inactive() {
        let mut metadata = test_metadata();
        metadata.end_block = None;
        metadata.paused_at = None;
        metadata.active = false;
        assert_eq!(metadata.status(), "Inactive");
    }
}
