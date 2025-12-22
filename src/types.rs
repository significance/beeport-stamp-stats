//! Type-safe wrappers for blockchain primitives
//!
//! This module provides newtype wrappers around primitive types to prevent
//! mixing up different kinds of data and enable compile-time type checking.

use crate::error::{Result, StampError};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Contract address on blockchain (checksummed hex string with 0x prefix)
///
/// # Example
///
/// ```ignore
/// let addr = ContractAddress::new("0x45a1502382541Cd610CC9068e88727426b696293")?;
/// assert_eq!(addr.as_str(), "0x45a1502382541cd610cc9068e88727426b696293");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ContractAddress(String);

impl ContractAddress {
    /// Create from string, validating format and normalizing to lowercase
    ///
    /// # Arguments
    ///
    /// * `address` - Ethereum address with 0x prefix (42 chars total)
    ///
    /// # Errors
    ///
    /// Returns error if address format is invalid
    pub fn new(address: impl Into<String>) -> Result<Self> {
        let addr = address.into();

        // Validate: 0x prefix
        if !addr.starts_with("0x") {
            return Err(StampError::Config(format!(
                "Invalid address '{addr}': must start with 0x"
            )));
        }

        // Validate: 40 hex chars after 0x
        if addr.len() != 42 {
            return Err(StampError::Config(format!(
                "Invalid address '{}': must be 42 characters (0x + 40 hex chars), got {}",
                addr,
                addr.len()
            )));
        }

        // Validate: all chars after 0x are hex
        if !addr[2..].chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(StampError::Config(format!(
                "Invalid address '{addr}': contains non-hex characters"
            )));
        }

        // Normalize to lowercase for consistent comparisons
        Ok(Self(addr.to_lowercase()))
    }

    /// Get as string slice
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ContractAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for ContractAddress {
    type Err = StampError;

    fn from_str(s: &str) -> Result<Self> {
        Self::new(s)
    }
}

/// Contract version identifier (e.g., "v0.9.4", "Phase 4")
///
/// # Example
///
/// ```ignore
/// let version = ContractVersion::new("v0.9.4");
/// assert_eq!(version.as_str(), "v0.9.4");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractVersion(String);

impl ContractVersion {
    /// Create new contract version
    pub fn new(version: impl Into<String>) -> Self {
        Self(version.into())
    }

    /// Get as string slice
    #[allow(dead_code)]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ContractVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for ContractVersion {
    type Err = StampError;

    fn from_str(s: &str) -> Result<Self> {
        Ok(Self::new(s))
    }
}

/// Block number on blockchain
///
/// # Example
///
/// ```ignore
/// let block = BlockNumber(31305656);
/// assert!(block > BlockNumber(31305655));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct BlockNumber(pub u64);

impl BlockNumber {
    /// Create new block number
    pub fn new(block: u64) -> Self {
        Self(block)
    }

    /// Get as u64
    pub fn as_u64(&self) -> u64 {
        self.0
    }
}

impl fmt::Display for BlockNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<u64> for BlockNumber {
    fn from(block: u64) -> Self {
        Self(block)
    }
}

impl From<BlockNumber> for u64 {
    fn from(block: BlockNumber) -> Self {
        block.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_contract_address_valid() {
        let addr = ContractAddress::new("0x45a1502382541Cd610CC9068e88727426b696293").unwrap();
        assert_eq!(addr.as_str(), "0x45a1502382541cd610cc9068e88727426b696293");
    }

    #[test]
    fn test_contract_address_normalizes_case() {
        let addr1 = ContractAddress::new("0xABCDEF1234567890ABCDef1234567890abcDEF12").unwrap();
        let addr2 = ContractAddress::new("0xabcdef1234567890abcdef1234567890abcdef12").unwrap();
        assert_eq!(addr1, addr2);
    }

    #[test]
    fn test_contract_address_missing_0x() {
        let result = ContractAddress::new("45a1502382541Cd610CC9068e88727426b696293");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("must start with 0x"));
    }

    #[test]
    fn test_contract_address_wrong_length() {
        let result = ContractAddress::new("0x123");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("must be 42 characters"));
    }

    #[test]
    fn test_contract_address_invalid_hex() {
        let result = ContractAddress::new("0x45a1502382541Cd610CC9068e88727426b696zz");
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        // The error message format is from StampError::Config
        assert!(err_msg.contains("Invalid address") || err_msg.contains("non-hex"));
    }

    #[test]
    fn test_contract_address_equality() {
        let addr1 = ContractAddress::new("0x45a1502382541Cd610CC9068e88727426b696293").unwrap();
        let addr2 = ContractAddress::new("0x45A1502382541CD610CC9068E88727426B696293").unwrap();
        assert_eq!(addr1, addr2);
    }

    #[test]
    fn test_contract_version() {
        let version = ContractVersion::new("v0.9.4");
        assert_eq!(version.as_str(), "v0.9.4");
    }

    #[test]
    fn test_block_number_ordering() {
        let block1 = BlockNumber(100);
        let block2 = BlockNumber(200);
        assert!(block1 < block2);
        assert!(block2 > block1);
        assert_eq!(block1, BlockNumber(100));
    }

    #[test]
    fn test_block_number_conversion() {
        let block = BlockNumber::from(12345u64);
        assert_eq!(block.as_u64(), 12345);
        assert_eq!(u64::from(block), 12345);
    }
}
