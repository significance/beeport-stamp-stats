//! Unit tests for configuration module
//!
//! Tests cover:
//! - Default configuration
//! - Configuration validation
//! - Environment variable overrides
//! - Invalid configurations

use beeport_stamp_stats::config::{AppConfig, BlockchainConfig, ContractConfig, RpcConfig};

#[test]
fn test_default_config() {
    let config = AppConfig::default();

    assert_eq!(config.rpc.url, "https://rpc.gnosis.gateway.fm");
    assert_eq!(config.database.path, "./stamp-cache.db");
    assert_eq!(config.blockchain.chunk_size, 10000);
    assert_eq!(config.blockchain.block_time_seconds, 5.0);
    assert_eq!(config.contracts.len(), 2);
    assert_eq!(config.retry.max_retries, 5);
    assert_eq!(config.retry.initial_delay_ms, 100);
    assert_eq!(config.retry.backoff_multiplier, 4);
    assert_eq!(config.retry.extended_retry_wait_seconds, 300);
}

#[test]
fn test_config_validation_success() {
    let config = AppConfig::default();
    assert!(config.validate().is_ok());
}

#[test]
fn test_config_validation_invalid_rpc_url() {
    let mut config = AppConfig::default();
    config.rpc.url = "invalid-url".to_string();

    let result = config.validate();
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Invalid RPC URL"));
}

#[test]
fn test_config_validation_empty_database_path() {
    let mut config = AppConfig::default();
    config.database.path = "".to_string();

    let result = config.validate();
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Database path cannot be empty"));
}

#[test]
fn test_config_validation_zero_chunk_size() {
    let mut config = AppConfig::default();
    config.blockchain.chunk_size = 0;

    let result = config.validate();
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .contains("chunk_size must be greater than 0"));
}

#[test]
fn test_config_validation_invalid_block_time() {
    let mut config = AppConfig::default();
    config.blockchain.block_time_seconds = 0.0;

    let result = config.validate();
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .contains("block_time_seconds must be greater than 0"));
}

#[test]
fn test_config_validation_no_contracts() {
    let mut config = AppConfig::default();
    config.contracts.clear();

    let result = config.validate();
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .contains("At least one contract must be configured"));
}

#[test]
fn test_config_validation_invalid_contract_type() {
    let mut config = AppConfig::default();
    config.contracts[0].contract_type = "UnknownContract".to_string();

    let result = config.validate();
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Unknown contract type"));
}

#[test]
fn test_config_validation_invalid_address_format() {
    let mut config = AppConfig::default();
    config.contracts[0].address = "invalid".to_string();

    let result = config.validate();
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("must start with 0x"));
}

#[test]
fn test_config_validation_invalid_address_length() {
    let mut config = AppConfig::default();
    config.contracts[0].address = "0x123".to_string();

    let result = config.validate();
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("must be 42 characters"));
}

#[test]
fn test_config_validation_zero_deployment_block() {
    let mut config = AppConfig::default();
    config.contracts[0].deployment_block = 0;

    let result = config.validate();
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .contains("Deployment block for contract"));
}

#[test]
fn test_config_validation_zero_retry_delay() {
    let mut config = AppConfig::default();
    config.retry.initial_delay_ms = 0;

    let result = config.validate();
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .contains("initial_delay_ms must be greater than 0"));
}

#[test]
fn test_config_validation_zero_backoff_multiplier() {
    let mut config = AppConfig::default();
    config.retry.backoff_multiplier = 0;

    let result = config.validate();
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .contains("backoff_multiplier must be greater than 0"));
}

#[test]
fn test_config_load_uses_defaults_when_no_file() {
    // Loading without a file should use defaults
    let config = AppConfig::load();

    // Should succeed with defaults
    assert!(config.is_ok());

    if let Ok(config) = config {
        // Verify it has default values
        assert_eq!(config.rpc.url, "https://rpc.gnosis.gateway.fm");
        assert_eq!(config.blockchain.chunk_size, 10000);
    }
}

#[test]
fn test_blockchain_config_defaults() {
    let config = BlockchainConfig {
        chunk_size: 10000,
        block_time_seconds: 5.0,
    };

    assert_eq!(config.chunk_size, 10000);
    assert_eq!(config.block_time_seconds, 5.0);
}

#[test]
fn test_contract_config_creation() {
    let contract = ContractConfig {
        name: "TestContract".to_string(),
        contract_type: "PostageStamp".to_string(),
        address: "0x1234567890123456789012345678901234567890".to_string(),
        deployment_block: 12345,
    };

    assert_eq!(contract.name, "TestContract");
    assert_eq!(contract.contract_type, "PostageStamp");
    assert_eq!(
        contract.address,
        "0x1234567890123456789012345678901234567890"
    );
    assert_eq!(contract.deployment_block, 12345);
}

#[test]
fn test_rpc_config_creation() {
    let rpc = RpcConfig {
        url: "https://test.rpc".to_string(),
    };

    assert_eq!(rpc.url, "https://test.rpc");
}
