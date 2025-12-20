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

// Storage Incentives contract deployment blocks
#[allow(dead_code)]
pub const PRICE_ORACLE_DEPLOYMENT_BLOCK: u64 = 37_339_168;
#[allow(dead_code)]
pub const STAKE_REGISTRY_DEPLOYMENT_BLOCK: u64 = 40_430_237;
#[allow(dead_code)]
pub const REDISTRIBUTION_DEPLOYMENT_BLOCK: u64 = 41_105_199;

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

// PriceOracle contract - handles price adjustments for storage
sol! {
    #[allow(missing_docs)]
    #[sol(rpc)]
    PriceOracle,
    r#"[
        {
            "anonymous": false,
            "inputs": [
                {
                    "indexed": false,
                    "internalType": "uint256",
                    "name": "price",
                    "type": "uint256"
                }
            ],
            "name": "PriceUpdate",
            "type": "event"
        },
        {
            "anonymous": false,
            "inputs": [
                {
                    "indexed": false,
                    "internalType": "uint256",
                    "name": "attemptedPrice",
                    "type": "uint256"
                }
            ],
            "name": "StampPriceUpdateFailed",
            "type": "event"
        }
    ]"#
}

// StakeRegistry contract - handles node staking for redistribution game
sol! {
    #[allow(missing_docs)]
    #[sol(rpc)]
    StakeRegistry,
    r#"[
        {
            "anonymous": false,
            "inputs": [
                {
                    "indexed": true,
                    "internalType": "address",
                    "name": "owner",
                    "type": "address"
                },
                {
                    "indexed": false,
                    "internalType": "uint256",
                    "name": "committedStake",
                    "type": "uint256"
                },
                {
                    "indexed": false,
                    "internalType": "uint256",
                    "name": "potentialStake",
                    "type": "uint256"
                },
                {
                    "indexed": false,
                    "internalType": "bytes32",
                    "name": "overlay",
                    "type": "bytes32"
                },
                {
                    "indexed": false,
                    "internalType": "uint256",
                    "name": "lastUpdatedBlock",
                    "type": "uint256"
                },
                {
                    "indexed": false,
                    "internalType": "uint8",
                    "name": "height",
                    "type": "uint8"
                }
            ],
            "name": "StakeUpdated",
            "type": "event"
        },
        {
            "anonymous": false,
            "inputs": [
                {
                    "indexed": false,
                    "internalType": "address",
                    "name": "slashed",
                    "type": "address"
                },
                {
                    "indexed": false,
                    "internalType": "bytes32",
                    "name": "overlay",
                    "type": "bytes32"
                },
                {
                    "indexed": false,
                    "internalType": "uint256",
                    "name": "amount",
                    "type": "uint256"
                }
            ],
            "name": "StakeSlashed",
            "type": "event"
        },
        {
            "anonymous": false,
            "inputs": [
                {
                    "indexed": false,
                    "internalType": "address",
                    "name": "frozen",
                    "type": "address"
                },
                {
                    "indexed": false,
                    "internalType": "bytes32",
                    "name": "overlay",
                    "type": "bytes32"
                },
                {
                    "indexed": false,
                    "internalType": "uint256",
                    "name": "time",
                    "type": "uint256"
                }
            ],
            "name": "StakeFrozen",
            "type": "event"
        },
        {
            "anonymous": false,
            "inputs": [
                {
                    "indexed": false,
                    "internalType": "address",
                    "name": "owner",
                    "type": "address"
                },
                {
                    "indexed": false,
                    "internalType": "bytes32",
                    "name": "overlay",
                    "type": "bytes32"
                }
            ],
            "name": "OverlayChanged",
            "type": "event"
        },
        {
            "anonymous": false,
            "inputs": [
                {
                    "indexed": false,
                    "internalType": "address",
                    "name": "node",
                    "type": "address"
                },
                {
                    "indexed": false,
                    "internalType": "uint256",
                    "name": "amount",
                    "type": "uint256"
                }
            ],
            "name": "StakeWithdrawn",
            "type": "event"
        }
    ]"#
}

// Redistribution contract - Schelling coordination game for storage incentives
sol! {
    #[allow(missing_docs)]
    #[sol(rpc)]
    Redistribution,
    r#"[
        {
            "anonymous": false,
            "inputs": [
                {
                    "indexed": false,
                    "internalType": "uint256",
                    "name": "roundNumber",
                    "type": "uint256"
                },
                {
                    "indexed": false,
                    "internalType": "bytes32",
                    "name": "overlay",
                    "type": "bytes32"
                },
                {
                    "indexed": false,
                    "internalType": "uint8",
                    "name": "height",
                    "type": "uint8"
                }
            ],
            "name": "Committed",
            "type": "event"
        },
        {
            "anonymous": false,
            "inputs": [
                {
                    "indexed": false,
                    "internalType": "uint256",
                    "name": "roundNumber",
                    "type": "uint256"
                },
                {
                    "indexed": false,
                    "internalType": "bytes32",
                    "name": "overlay",
                    "type": "bytes32"
                },
                {
                    "indexed": false,
                    "internalType": "uint256",
                    "name": "stake",
                    "type": "uint256"
                },
                {
                    "indexed": false,
                    "internalType": "uint256",
                    "name": "stakeDensity",
                    "type": "uint256"
                },
                {
                    "indexed": false,
                    "internalType": "bytes32",
                    "name": "reserveCommitment",
                    "type": "bytes32"
                },
                {
                    "indexed": false,
                    "internalType": "uint8",
                    "name": "depth",
                    "type": "uint8"
                }
            ],
            "name": "Revealed",
            "type": "event"
        },
        {
            "anonymous": false,
            "inputs": [
                {
                    "components": [
                        {
                            "internalType": "bytes32",
                            "name": "overlay",
                            "type": "bytes32"
                        },
                        {
                            "internalType": "address",
                            "name": "owner",
                            "type": "address"
                        },
                        {
                            "internalType": "uint8",
                            "name": "depth",
                            "type": "uint8"
                        },
                        {
                            "internalType": "uint256",
                            "name": "stake",
                            "type": "uint256"
                        },
                        {
                            "internalType": "uint256",
                            "name": "stakeDensity",
                            "type": "uint256"
                        },
                        {
                            "internalType": "bytes32",
                            "name": "hash",
                            "type": "bytes32"
                        }
                    ],
                    "indexed": false,
                    "internalType": "struct Redistribution.Reveal",
                    "name": "winner",
                    "type": "tuple"
                }
            ],
            "name": "WinnerSelected",
            "type": "event"
        },
        {
            "anonymous": false,
            "inputs": [
                {
                    "indexed": false,
                    "internalType": "bytes32",
                    "name": "hash",
                    "type": "bytes32"
                },
                {
                    "indexed": false,
                    "internalType": "uint8",
                    "name": "depth",
                    "type": "uint8"
                }
            ],
            "name": "TruthSelected",
            "type": "event"
        },
        {
            "anonymous": false,
            "inputs": [
                {
                    "indexed": false,
                    "internalType": "uint256",
                    "name": "roundNumber",
                    "type": "uint256"
                },
                {
                    "indexed": false,
                    "internalType": "bytes32",
                    "name": "anchor",
                    "type": "bytes32"
                }
            ],
            "name": "CurrentRevealAnchor",
            "type": "event"
        },
        {
            "anonymous": false,
            "inputs": [
                {
                    "indexed": false,
                    "internalType": "uint256",
                    "name": "_count",
                    "type": "uint256"
                }
            ],
            "name": "CountCommits",
            "type": "event"
        },
        {
            "anonymous": false,
            "inputs": [
                {
                    "indexed": false,
                    "internalType": "uint256",
                    "name": "_count",
                    "type": "uint256"
                }
            ],
            "name": "CountReveals",
            "type": "event"
        },
        {
            "anonymous": false,
            "inputs": [
                {
                    "indexed": false,
                    "internalType": "uint256",
                    "name": "validChunkCount",
                    "type": "uint256"
                }
            ],
            "name": "ChunkCount",
            "type": "event"
        },
        {
            "anonymous": false,
            "inputs": [
                {
                    "indexed": false,
                    "internalType": "uint16",
                    "name": "redundancyCount",
                    "type": "uint16"
                }
            ],
            "name": "PriceAdjustmentSkipped",
            "type": "event"
        },
        {
            "anonymous": false,
            "inputs": [
                {
                    "indexed": false,
                    "internalType": "address",
                    "name": "owner",
                    "type": "address"
                }
            ],
            "name": "WithdrawFailed",
            "type": "event"
        },
        {
            "anonymous": false,
            "inputs": [
                {
                    "indexed": false,
                    "internalType": "uint256",
                    "name": "indexInRC",
                    "type": "uint256"
                },
                {
                    "indexed": false,
                    "internalType": "bytes32",
                    "name": "chunkAddress",
                    "type": "bytes32"
                }
            ],
            "name": "transformedChunkAddressFromInclusionProof",
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

        // Storage Incentives contracts
        assert_eq!(PRICE_ORACLE_DEPLOYMENT_BLOCK, 37_339_168);
        assert_eq!(STAKE_REGISTRY_DEPLOYMENT_BLOCK, 40_430_237);
        assert_eq!(REDISTRIBUTION_DEPLOYMENT_BLOCK, 41_105_199);
    }
}
