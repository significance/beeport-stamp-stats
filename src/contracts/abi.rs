/// Contract ABIs and constants
///
/// This module contains the Solidity contract ABIs using alloy's sol! macro.
/// The ABIs are kept in Rust code (not moved to config files) because the sol!
/// macro provides compile-time type safety for event decoding.
use alloy::sol;

// Contract deployment blocks
#[allow(dead_code)]
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
    fn test_constants() {
        assert_eq!(POSTAGE_STAMP_DEPLOYMENT_BLOCK, 31_305_656);
        assert_eq!(STAMPS_REGISTRY_DEPLOYMENT_BLOCK, 42_390_510);
        assert_eq!(DEFAULT_START_BLOCK, POSTAGE_STAMP_DEPLOYMENT_BLOCK);
    }
}
