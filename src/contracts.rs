use alloy::sol;

/// Identifier for different contract types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContractType {
    PostageStamp,
    StampsRegistry,
}

impl ContractType {
    pub fn address(&self) -> &'static str {
        match self {
            ContractType::PostageStamp => POSTAGE_STAMP_ADDRESS,
            ContractType::StampsRegistry => STAMPS_REGISTRY_ADDRESS,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            ContractType::PostageStamp => "PostageStamp",
            ContractType::StampsRegistry => "StampsRegistry",
        }
    }

    pub fn deployment_block(&self) -> u64 {
        match self {
            ContractType::PostageStamp => POSTAGE_STAMP_DEPLOYMENT_BLOCK,
            ContractType::StampsRegistry => STAMPS_REGISTRY_DEPLOYMENT_BLOCK,
        }
    }

    pub fn all() -> Vec<ContractType> {
        vec![ContractType::PostageStamp, ContractType::StampsRegistry]
    }
}

impl std::fmt::Display for ContractType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

// PostageStamp contract address on Gnosis Chain
// https://gnosisscan.io/address/0x647942035bb69C8e4d7EB17C8313EBC50b0bABFA
// Deployed at block 31,305,656 on Gnosis Chain
// Note: PostageStamp is the original contract, but we also track StampsRegistry for imported batches
pub const POSTAGE_STAMP_ADDRESS: &str = "0x647942035bb69C8e4d7EB17C8313EBC50b0bABFA";

// StampsRegistry contract address on Gnosis Chain
// https://gnosisscan.io/address/0x5EBfBeFB1E88391eFb022d5d33302f50a46bF4f3
// Deployed at block 42,390,510 on Gnosis Chain
pub const STAMPS_REGISTRY_ADDRESS: &str = "0x5EBfBeFB1E88391eFb022d5d33302f50a46bF4f3";

// Contract deployment blocks
pub const STAMPS_REGISTRY_DEPLOYMENT_BLOCK: u64 = 42_390_510;
pub const POSTAGE_STAMP_DEPLOYMENT_BLOCK: u64 = 31_305_656;

// Default starting block for fetching events
// Set to PostageStamp deployment block (first block with events)
pub const DEFAULT_START_BLOCK: u64 = POSTAGE_STAMP_DEPLOYMENT_BLOCK;

// Solidity contract definition for PostageStamp using alloy's sol! macro
sol! {
    #[allow(missing_docs)]
    #[sol(rpc)]
    PostageStamp,
    r#"[
        {
            "inputs": [
                {
                    "internalType": "bytes32",
                    "name": "_batchId",
                    "type": "bytes32"
                }
            ],
            "name": "remainingBalance",
            "outputs": [
                {
                    "internalType": "uint256",
                    "name": "",
                    "type": "uint256"
                }
            ],
            "stateMutability": "view",
            "type": "function"
        },
        {
            "inputs": [],
            "name": "lastPrice",
            "outputs": [
                {
                    "internalType": "uint64",
                    "name": "",
                    "type": "uint64"
                }
            ],
            "stateMutability": "view",
            "type": "function"
        },
        {
            "anonymous": false,
            "inputs": [
                {
                    "indexed": true,
                    "internalType": "bytes32",
                    "name": "batchId",
                    "type": "bytes32"
                },
                {
                    "indexed": false,
                    "internalType": "uint256",
                    "name": "totalAmount",
                    "type": "uint256"
                },
                {
                    "indexed": false,
                    "internalType": "uint256",
                    "name": "normalisedBalance",
                    "type": "uint256"
                },
                {
                    "indexed": false,
                    "internalType": "address",
                    "name": "owner",
                    "type": "address"
                },
                {
                    "indexed": false,
                    "internalType": "uint8",
                    "name": "depth",
                    "type": "uint8"
                },
                {
                    "indexed": false,
                    "internalType": "uint8",
                    "name": "bucketDepth",
                    "type": "uint8"
                },
                {
                    "indexed": false,
                    "internalType": "bool",
                    "name": "immutableFlag",
                    "type": "bool"
                }
            ],
            "name": "BatchCreated",
            "type": "event"
        },
        {
            "anonymous": false,
            "inputs": [
                {
                    "indexed": true,
                    "internalType": "bytes32",
                    "name": "batchId",
                    "type": "bytes32"
                },
                {
                    "indexed": false,
                    "internalType": "uint256",
                    "name": "topupAmount",
                    "type": "uint256"
                },
                {
                    "indexed": false,
                    "internalType": "uint256",
                    "name": "normalisedBalance",
                    "type": "uint256"
                }
            ],
            "name": "BatchTopUp",
            "type": "event"
        },
        {
            "anonymous": false,
            "inputs": [
                {
                    "indexed": true,
                    "internalType": "bytes32",
                    "name": "batchId",
                    "type": "bytes32"
                },
                {
                    "indexed": false,
                    "internalType": "uint8",
                    "name": "newDepth",
                    "type": "uint8"
                },
                {
                    "indexed": false,
                    "internalType": "uint256",
                    "name": "normalisedBalance",
                    "type": "uint256"
                }
            ],
            "name": "BatchDepthIncrease",
            "type": "event"
        }
    ]"#
}

// Solidity contract definition for StampsRegistry using alloy's sol! macro
sol! {
    #[allow(missing_docs)]
    #[sol(rpc)]
    StampsRegistry,
    r#"[
        {
            "anonymous": false,
            "inputs": [
                {
                    "indexed": true,
                    "internalType": "bytes32",
                    "name": "batchId",
                    "type": "bytes32"
                },
                {
                    "indexed": false,
                    "internalType": "uint256",
                    "name": "totalAmount",
                    "type": "uint256"
                },
                {
                    "indexed": false,
                    "internalType": "uint256",
                    "name": "normalisedBalance",
                    "type": "uint256"
                },
                {
                    "indexed": true,
                    "internalType": "address",
                    "name": "owner",
                    "type": "address"
                },
                {
                    "indexed": true,
                    "internalType": "address",
                    "name": "payer",
                    "type": "address"
                },
                {
                    "indexed": false,
                    "internalType": "uint8",
                    "name": "depth",
                    "type": "uint8"
                },
                {
                    "indexed": false,
                    "internalType": "uint8",
                    "name": "bucketDepth",
                    "type": "uint8"
                },
                {
                    "indexed": false,
                    "internalType": "bool",
                    "name": "immutableFlag",
                    "type": "bool"
                }
            ],
            "name": "BatchCreated",
            "type": "event"
        },
        {
            "anonymous": false,
            "inputs": [
                {
                    "indexed": true,
                    "internalType": "bytes32",
                    "name": "batchId",
                    "type": "bytes32"
                },
                {
                    "indexed": false,
                    "internalType": "uint256",
                    "name": "topupAmount",
                    "type": "uint256"
                },
                {
                    "indexed": false,
                    "internalType": "uint256",
                    "name": "normalisedBalance",
                    "type": "uint256"
                },
                {
                    "indexed": true,
                    "internalType": "address",
                    "name": "payer",
                    "type": "address"
                }
            ],
            "name": "BatchTopUp",
            "type": "event"
        },
        {
            "anonymous": false,
            "inputs": [
                {
                    "indexed": true,
                    "internalType": "bytes32",
                    "name": "batchId",
                    "type": "bytes32"
                },
                {
                    "indexed": false,
                    "internalType": "uint8",
                    "name": "newDepth",
                    "type": "uint8"
                },
                {
                    "indexed": false,
                    "internalType": "uint256",
                    "name": "normalisedBalance",
                    "type": "uint256"
                },
                {
                    "indexed": true,
                    "internalType": "address",
                    "name": "payer",
                    "type": "address"
                }
            ],
            "name": "BatchDepthIncrease",
            "type": "event"
        }
    ]"#
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_contract_type_all() {
        let contracts = ContractType::all();
        assert_eq!(contracts.len(), 2);
        assert!(contracts.contains(&ContractType::PostageStamp));
        assert!(contracts.contains(&ContractType::StampsRegistry));
    }

    #[test]
    fn test_contract_addresses() {
        assert!(POSTAGE_STAMP_ADDRESS.starts_with("0x"));
        assert!(STAMPS_REGISTRY_ADDRESS.starts_with("0x"));
        assert_ne!(POSTAGE_STAMP_ADDRESS, STAMPS_REGISTRY_ADDRESS);
    }
}
