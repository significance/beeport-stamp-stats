/// Configuration module for beeport-stamp-stats
///
/// This module provides a comprehensive configuration system that supports:
/// - Multiple formats (YAML, TOML, JSON)
/// - Layered configuration (defaults → file → env vars → CLI args)
/// - Type-safe configuration with validation
/// - Environment-agnostic deployment
///
/// # Configuration Priority
///
/// 1. CLI arguments (highest priority)
/// 2. Environment variables (BEEPORT_ prefix)
/// 3. Configuration file
/// 4. Built-in defaults (lowest priority)
///
/// # Example
///
/// ```ignore
/// // Load from default locations
/// let config = AppConfig::load()?;
///
/// // Load from specific file
/// let config = AppConfig::load_from_file("config.yaml")?;
/// ```
use config::{Config, ConfigError, Environment, File};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Main application configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// RPC configuration
    pub rpc: RpcConfig,

    /// Database configuration
    pub database: DatabaseConfig,

    /// Blockchain configuration
    pub blockchain: BlockchainConfig,

    /// Contract configurations
    pub contracts: Vec<ContractConfig>,

    /// Retry configuration
    pub retry: RetryConfig,

    /// Rate limiting configuration (optional)
    #[serde(default)]
    pub rate_limiting: RateLimitingConfig,
}

/// RPC configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcConfig {
    /// Single URL (legacy format - mutually exclusive with endpoints)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Multiple endpoints with rate limiting
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoints: Option<Vec<RpcEndpointConfig>>,
}

impl RpcConfig {
    /// Get all RPC URLs from config
    pub fn urls(&self) -> Vec<String> {
        if let Some(ref endpoints) = self.endpoints {
            endpoints.iter().map(|e| e.url.clone()).collect()
        } else if let Some(ref url) = self.url {
            vec![url.clone()]
        } else {
            vec!["https://rpc.gnosis.gateway.fm".to_string()]
        }
    }

    /// Get primary URL (first endpoint) - for backwards compatibility
    pub fn primary_url(&self) -> String {
        if let Some(ref endpoints) = self.endpoints {
            endpoints.first().map(|e| e.url.clone())
                .unwrap_or_else(|| "https://rpc.gnosis.gateway.fm".to_string())
        } else if let Some(ref url) = self.url {
            url.clone()
        } else {
            "https://rpc.gnosis.gateway.fm".to_string()
        }
    }

    /// Set the URL (for CLI overrides) - clears endpoints and sets single URL
    pub fn set_url(&mut self, new_url: String) {
        self.url = Some(new_url);
        self.endpoints = None;
    }

    /// Get all endpoint configurations
    pub fn endpoints(&self) -> Vec<RpcEndpointConfig> {
        if let Some(ref endpoints) = self.endpoints {
            endpoints.clone()
        } else if let Some(ref url) = self.url {
            vec![RpcEndpointConfig {
                url: url.clone(),
                rate_limit: None,
                priority: 0,
                weight: 1,
            }]
        } else {
            vec![RpcEndpointConfig {
                url: "https://rpc.gnosis.gateway.fm".to_string(),
                rate_limit: None,
                priority: 0,
                weight: 1,
            }]
        }
    }
}

/// Individual RPC endpoint configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcEndpointConfig {
    /// RPC endpoint URL
    pub url: String,

    /// Rate limit strategy (optional)
    #[serde(default)]
    pub rate_limit: Option<RateLimitMode>,

    /// Priority (0 = highest, for paid RPCs)
    #[serde(default)]
    pub priority: u8,

    /// Weight for weighted round-robin (higher = selected more often)
    #[serde(default = "default_weight")]
    pub weight: u32,
}

fn default_weight() -> u32 {
    1
}

/// Rate limit configuration mode
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RateLimitMode {
    /// Manual rate limit (fixed value)
    Manual(f64),
    /// Named strategy
    Strategy(RateLimitStrategyName),
}

/// Rate limiting strategy name
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum RateLimitStrategyName {
    Adaptive,
    Aggressive,
}

/// Rate limiting global configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitingConfig {
    /// Maximum concurrent requests across all RPCs
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent: usize,

    /// Health check interval in seconds (0 = disabled)
    #[serde(default)]
    pub health_check_interval_seconds: u64,

    /// Log rate limit stats every N seconds (0 = disabled)
    #[serde(default = "default_stats_interval")]
    pub stats_interval_seconds: u64,

    /// Adaptive strategy configuration
    #[serde(default)]
    pub adaptive: AdaptiveStrategyConfig,

    /// Aggressive strategy configuration
    #[serde(default)]
    pub aggressive: AggressiveStrategyConfig,

    /// Recovery configuration for automatic rate limit recovery
    #[serde(default)]
    pub recovery: RecoveryConfig,
}

fn default_max_concurrent() -> usize {
    100
}

fn default_stats_interval() -> u64 {
    30
}

impl Default for RateLimitingConfig {
    fn default() -> Self {
        Self {
            max_concurrent: default_max_concurrent(),
            health_check_interval_seconds: 0,
            stats_interval_seconds: default_stats_interval(),
            adaptive: AdaptiveStrategyConfig::default(),
            aggressive: AggressiveStrategyConfig::default(),
            recovery: RecoveryConfig::default(),
        }
    }
}

/// Adaptive rate limiting strategy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdaptiveStrategyConfig {
    /// Starting rate limit (requests per second)
    #[serde(default = "default_adaptive_start")]
    pub start_rps: f64,

    /// Maximum rate limit (safety cap)
    #[serde(default = "default_adaptive_max")]
    pub max_rps: f64,

    /// Minimum rate limit (floor - prevents deadlock)
    #[serde(default = "default_adaptive_min")]
    pub min_rps: f64,

    /// Ramp up multiplier (e.g., 1.2 = 20% increase)
    #[serde(default = "default_adaptive_ramp_up")]
    pub ramp_up_factor: f64,

    /// Back off multiplier on rate limit error (e.g., 0.5 = 50% decrease)
    #[serde(default = "default_adaptive_back_off")]
    pub back_off_factor: f64,

    /// Number of successes before ramping up
    #[serde(default = "default_adaptive_ramp_threshold")]
    pub ramp_up_threshold: u32,
}

fn default_adaptive_start() -> f64 {
    10.0
}

fn default_adaptive_max() -> f64 {
    1000.0
}

fn default_adaptive_min() -> f64 {
    0.5
}

fn default_adaptive_ramp_up() -> f64 {
    1.2
}

fn default_adaptive_back_off() -> f64 {
    0.5
}

fn default_adaptive_ramp_threshold() -> u32 {
    100
}

impl Default for AdaptiveStrategyConfig {
    fn default() -> Self {
        Self {
            start_rps: default_adaptive_start(),
            max_rps: default_adaptive_max(),
            min_rps: default_adaptive_min(),
            ramp_up_factor: default_adaptive_ramp_up(),
            back_off_factor: default_adaptive_back_off(),
            ramp_up_threshold: default_adaptive_ramp_threshold(),
        }
    }
}

/// Aggressive rate limiting strategy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggressiveStrategyConfig {
    /// Starting rate limit (requests per second) - high initial value
    #[serde(default = "default_aggressive_start")]
    pub start_rps: f64,

    /// Minimum rate limit (floor)
    #[serde(default = "default_aggressive_min")]
    pub min_rps: f64,

    /// Back off multiplier on rate limit error (e.g., 0.7 = 30% decrease)
    #[serde(default = "default_aggressive_back_off")]
    pub back_off_factor: f64,
}

fn default_aggressive_start() -> f64 {
    100.0
}

fn default_aggressive_min() -> f64 {
    1.0
}

fn default_aggressive_back_off() -> f64 {
    0.7
}

impl Default for AggressiveStrategyConfig {
    fn default() -> Self {
        Self {
            start_rps: default_aggressive_start(),
            min_rps: default_aggressive_min(),
            back_off_factor: default_aggressive_back_off(),
        }
    }
}

/// Recovery configuration for automatic rate limit recovery
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryConfig {
    /// How long to wait after last 429 before attempting recovery (seconds)
    #[serde(default = "default_cool_down_period")]
    pub cool_down_period_secs: u64,

    /// Target rate to reset to after cool-down (requests per second)
    #[serde(default = "default_min_healthy_rate")]
    pub min_healthy_rate: f64,

    /// Factor to multiply rate by during gradual recovery (e.g., 1.05 = 5% increase)
    #[serde(default = "default_gradual_recovery_factor")]
    pub gradual_recovery_factor: f64,

    /// How often to check for recovery opportunities (seconds)
    #[serde(default = "default_recovery_check_interval")]
    pub recovery_check_interval_secs: u64,
}

fn default_cool_down_period() -> u64 {
    60
}

fn default_min_healthy_rate() -> f64 {
    5.0
}

fn default_gradual_recovery_factor() -> f64 {
    1.05
}

fn default_recovery_check_interval() -> u64 {
    30
}

impl Default for RecoveryConfig {
    fn default() -> Self {
        Self {
            cool_down_period_secs: default_cool_down_period(),
            min_healthy_rate: default_min_healthy_rate(),
            gradual_recovery_factor: default_gradual_recovery_factor(),
            recovery_check_interval_secs: default_recovery_check_interval(),
        }
    }
}

/// Database configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    /// Database path (SQLite file path or PostgreSQL connection string)
    ///
    /// Examples:
    /// - SQLite: "./stamp-cache.db"
    /// - PostgreSQL: "postgres://user:pass@localhost/stamps"
    pub path: String,
}

/// Blockchain configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockchainConfig {
    /// Number of blocks to fetch per RPC chunk
    ///
    /// Larger values reduce RPC calls but may hit provider limits.
    /// Default: 10000
    pub chunk_size: u64,

    /// Block time in seconds (chain-specific)
    ///
    /// Used for TTL calculations.
    /// Default: 5.0 (Gnosis Chain)
    pub block_time_seconds: f64,
}

/// Contract configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractConfig {
    /// Human-readable contract name
    pub name: String,

    /// Contract type identifier (must match implementation)
    ///
    /// Valid values: "PostageStamp", "StampsRegistry", "PriceOracle", "StakeRegistry", "Redistribution"
    pub contract_type: String,

    /// Contract address on blockchain (hex string with 0x prefix)
    pub address: String,

    /// Block number when contract was deployed
    pub deployment_block: u64,

    /// Optional: Human-readable version (e.g., "v0.9.4", "Phase 4")
    #[serde(default)]
    pub version: Option<String>,

    /// Whether this is the currently active version (defaults to false)
    #[serde(default)]
    pub active: bool,

    /// Optional: Last active block (when superseded or stopped)
    #[serde(default)]
    pub end_block: Option<u64>,

    /// Optional: Block when contract was paused
    #[serde(default)]
    pub paused_at: Option<u64>,
}

// Re-export RetryConfig from retry module to avoid duplication
pub use crate::retry::RetryConfig;

impl ContractConfig {
    /// Validate configuration
    pub fn validate(&self) -> Result<(), String> {
        use crate::types::ContractAddress;

        // Validate address format
        ContractAddress::new(&self.address)
            .map_err(|e| format!("Invalid address in contract '{}': {}", self.name, e))?;

        // Validate block numbers are logical
        if let Some(end) = self.end_block
            && end <= self.deployment_block
        {
            return Err(format!(
                "Contract '{}': end_block ({}) must be after deployment_block ({})",
                self.name, end, self.deployment_block
            ));
        }

        if let Some(paused) = self.paused_at
            && paused < self.deployment_block
        {
            return Err(format!(
                "Contract '{}': paused_at ({}) cannot be before deployment_block ({})",
                self.name, paused, self.deployment_block
            ));
        }

        Ok(())
    }

    /// Convert to ContractMetadata
    pub fn to_metadata(&self) -> Result<crate::contracts::metadata::ContractMetadata, String> {
        use crate::types::{BlockNumber, ContractAddress, ContractVersion};

        Ok(crate::contracts::metadata::ContractMetadata {
            name: self.name.clone(),
            contract_type: self.contract_type.clone(),
            address: ContractAddress::new(&self.address)
                .map_err(|e| format!("Invalid address: {e}"))?,
            version: ContractVersion::new(
                self.version
                    .clone()
                    .unwrap_or_else(|| "unknown".to_string()),
            ),
            deployment_block: BlockNumber(self.deployment_block),
            end_block: self.end_block.map(BlockNumber),
            active: self.active,
            paused_at: self.paused_at.map(BlockNumber),
        })
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            rpc: RpcConfig {
                url: Some("https://rpc.gnosis.gateway.fm".to_string()),
                endpoints: None,
            },
            database: DatabaseConfig {
                path: "./stamp-cache.db".to_string(),
            },
            blockchain: BlockchainConfig {
                chunk_size: 10000,
                block_time_seconds: 5.0,
            },
            contracts: vec![
                ContractConfig {
                    name: "PostageStamp".to_string(),
                    contract_type: "PostageStamp".to_string(),
                    address: "0x45a1502382541Cd610CC9068e88727426b696293".to_string(),
                    deployment_block: 31305656,
                    version: Some("v0.8.6".to_string()),
                    active: true,
                    end_block: None,
                    paused_at: None,
                },
                ContractConfig {
                    name: "StampsRegistry".to_string(),
                    contract_type: "StampsRegistry".to_string(),
                    address: "0x5EBfBeFB1E88391eFb022d5d33302f50a46bF4f3".to_string(),
                    deployment_block: 42390510,
                    version: Some("v1.0.0".to_string()),
                    active: true,
                    end_block: None,
                    paused_at: None,
                },
                ContractConfig {
                    name: "PriceOracle".to_string(),
                    contract_type: "PriceOracle".to_string(),
                    address: "0x47EeF336e7fE5bED98499A4696bce8f28c1B0a8b".to_string(),
                    deployment_block: 37339168,
                    version: Some("v0.9.2".to_string()),
                    active: true,
                    end_block: None,
                    paused_at: None,
                },
                ContractConfig {
                    name: "StakeRegistry".to_string(),
                    contract_type: "StakeRegistry".to_string(),
                    address: "0xda2a16EE889E7f04980A8d597b48c8D51B9518F4".to_string(),
                    deployment_block: 40430237,
                    version: Some("v0.9.3".to_string()),
                    active: true,
                    end_block: None,
                    paused_at: None,
                },
                ContractConfig {
                    name: "Redistribution".to_string(),
                    contract_type: "Redistribution".to_string(),
                    address: "0x5069cdfB3D9E56d23B1cAeE83CE6109A7E4fd62d".to_string(),
                    deployment_block: 41105199,
                    version: Some("v0.9.4".to_string()),
                    active: true,
                    end_block: None,
                    paused_at: None,
                },
            ],
            retry: RetryConfig {
                max_retries: 5,
                initial_delay_ms: 100,
                backoff_multiplier: 4,
                extended_retry_wait_seconds: 300,
            },
            rate_limiting: RateLimitingConfig::default(),
        }
    }
}

impl AppConfig {
    /// Load configuration with default search paths
    ///
    /// Searches for config files in this order:
    /// 1. `./config.{yaml,toml,json}` (current directory)
    /// 2. `~/.config/beeport/config.{yaml,toml,json}` (user config)
    ///
    /// If no config file is found, uses built-in defaults.
    /// Environment variables with `BEEPORT_` prefix can override any setting.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let config = AppConfig::load()?;
    /// ```
    pub fn load() -> Result<Self, ConfigError> {
        Self::load_from_optional_file(None)
    }

    /// Load configuration from a specific file
    ///
    /// # Arguments
    ///
    /// * `path` - Path to configuration file (supports .yaml, .toml, .json)
    ///
    /// # Example
    ///
    /// ```ignore
    /// let config = AppConfig::load_from_file("my-config.yaml")?;
    /// ```
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self, ConfigError> {
        Self::load_from_optional_file(Some(path.as_ref()))
    }

    /// Internal method to load configuration with optional file path
    fn load_from_optional_file(path: Option<&Path>) -> Result<Self, ConfigError> {
        // Start with defaults
        let mut builder = Config::builder()
            .add_source(Config::try_from(&AppConfig::default())?);

        // Add config file if specified or search default locations
        if let Some(config_path) = path {
            // Specific file path provided - must exist
            builder = builder.add_source(File::from(config_path).required(true));
        } else {
            // Search default locations (optional)
            builder = builder
                .add_source(File::with_name("config").required(false))
                .add_source(File::with_name("~/.config/beeport/config").required(false));
        }

        // Add environment variable overrides
        // Environment variables use double underscore for nesting:
        // BEEPORT__RPC__URL=https://... overrides rpc.url
        // BEEPORT__RETRY__MAX_RETRIES=10 overrides retry.max_retries
        builder = builder.add_source(
            Environment::with_prefix("BEEPORT")
                .separator("__")
                .try_parsing(true),
        );

        builder.build()?.try_deserialize()
    }

    /// Validate the configuration
    ///
    /// Checks for:
    /// - Valid URLs
    /// - Hex addresses with 0x prefix
    /// - Positive values for numeric fields
    /// - Known contract types
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if valid, or an error message describing the problem.
    pub fn validate(&self) -> Result<(), String> {
        // Validate RPC URLs format
        for url in self.rpc.urls() {
            if !url.starts_with("http://") && !url.starts_with("https://") {
                return Err(format!(
                    "Invalid RPC URL '{url}': must start with http:// or https://"
                ));
            }
        }

        // Validate database path is not empty
        if self.database.path.is_empty() {
            return Err("Database path cannot be empty".to_string());
        }

        // Validate blockchain config
        if self.blockchain.chunk_size == 0 {
            return Err("Blockchain chunk_size must be greater than 0".to_string());
        }

        if self.blockchain.block_time_seconds <= 0.0 {
            return Err("Blockchain block_time_seconds must be greater than 0".to_string());
        }

        // Validate contracts
        if self.contracts.is_empty() {
            return Err("At least one contract must be configured".to_string());
        }

        for contract in &self.contracts {
            // Validate contract name
            if contract.name.is_empty() {
                return Err("Contract name cannot be empty".to_string());
            }

            // Validate contract type
            let valid_types = [
                "PostageStamp",
                "StampsRegistry",
                "PriceOracle",
                "StakeRegistry",
                "Redistribution",
            ];
            if !valid_types.contains(&contract.contract_type.as_str()) {
                return Err(format!(
                    "Unknown contract type '{}' for contract '{}'. Valid types: {}",
                    contract.contract_type,
                    contract.name,
                    valid_types.join(", ")
                ));
            }

            // Validate address format
            if !contract.address.starts_with("0x") {
                return Err(format!(
                    "Contract address '{}' for contract '{}' must start with 0x",
                    contract.address, contract.name
                ));
            }

            if contract.address.len() != 42 {
                return Err(format!(
                    "Contract address '{}' for contract '{}' must be 42 characters (0x + 40 hex digits)",
                    contract.address, contract.name
                ));
            }

            // Validate deployment block
            if contract.deployment_block == 0 {
                return Err(format!(
                    "Deployment block for contract '{}' must be greater than 0",
                    contract.name
                ));
            }
        }

        // Validate retry config
        if self.retry.initial_delay_ms == 0 {
            return Err("Retry initial_delay_ms must be greater than 0".to_string());
        }

        if self.retry.backoff_multiplier == 0 {
            return Err("Retry backoff_multiplier must be greater than 0".to_string());
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AppConfig::default();

        assert_eq!(config.rpc.url, "https://rpc.gnosis.gateway.fm");
        assert_eq!(config.database.path, "./stamp-cache.db");
        assert_eq!(config.blockchain.chunk_size, 10000);
        assert_eq!(config.blockchain.block_time_seconds, 5.0);
        assert_eq!(config.contracts.len(), 5);
        assert_eq!(config.retry.max_retries, 5);
        assert_eq!(config.retry.backoff_multiplier, 4);
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
        assert!(result
            .unwrap_err()
            .contains("Invalid RPC URL"));
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
        assert!(result.unwrap_err().contains("chunk_size must be greater than 0"));
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
}
