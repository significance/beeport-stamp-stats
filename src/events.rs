use alloy::sol;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// PostageStamp contract address on Gnosis Chain (from ethersphere/storage-incentives mainnet_deployed.json)
// https://gnosisscan.io/address/0x45a1502382541Cd610CC9068e88727426b696293
pub const POSTAGE_STAMP_ADDRESS: &str = "0x45a1502382541Cd610CC9068e88727426b696293";

// Solidity contract definition using alloy's sol! macro
sol! {
    #[allow(missing_docs)]
    #[sol(rpc)]
    PostageStamp,
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

/// Unified event type that can represent any PostageStamp event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StampEvent {
    pub event_type: EventType,
    pub batch_id: String,
    pub block_number: u64,
    pub block_timestamp: DateTime<Utc>,
    pub transaction_hash: String,
    pub log_index: u64,
    pub data: EventData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EventType {
    BatchCreated,
    BatchTopUp,
    BatchDepthIncrease,
}

impl std::fmt::Display for EventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EventType::BatchCreated => write!(f, "BatchCreated"),
            EventType::BatchTopUp => write!(f, "BatchTopUp"),
            EventType::BatchDepthIncrease => write!(f, "BatchDepthIncrease"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum EventData {
    BatchCreated {
        total_amount: String,
        normalised_balance: String,
        owner: String,
        depth: u8,
        bucket_depth: u8,
        immutable_flag: bool,
    },
    BatchTopUp {
        topup_amount: String,
        normalised_balance: String,
    },
    BatchDepthIncrease {
        new_depth: u8,
        normalised_balance: String,
    },
}

/// Information about a batch retrieved from the blockchain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchInfo {
    pub batch_id: String,
    pub owner: String,
    pub depth: u8,
    pub bucket_depth: u8,
    pub immutable: bool,
    pub normalised_balance: String,
    pub created_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_type_display() {
        assert_eq!(EventType::BatchCreated.to_string(), "BatchCreated");
        assert_eq!(EventType::BatchTopUp.to_string(), "BatchTopUp");
        assert_eq!(
            EventType::BatchDepthIncrease.to_string(),
            "BatchDepthIncrease"
        );
    }

    #[test]
    fn test_event_serialization() {
        let event = StampEvent {
            event_type: EventType::BatchCreated,
            batch_id: "0x1234".to_string(),
            block_number: 1000,
            block_timestamp: Utc::now(),
            transaction_hash: "0xabcd".to_string(),
            log_index: 0,
            data: EventData::BatchCreated {
                total_amount: "1000000000000000000".to_string(),
                normalised_balance: "500000000000000000".to_string(),
                owner: "0x5678".to_string(),
                depth: 20,
                bucket_depth: 16,
                immutable_flag: false,
            },
        };

        let json = serde_json::to_string(&event).unwrap();
        let deserialized: StampEvent = serde_json::from_str(&json).unwrap();

        assert_eq!(event.batch_id, deserialized.batch_id);
        assert_eq!(event.block_number, deserialized.block_number);
    }

    #[test]
    fn test_postage_stamp_address() {
        // Verify the address is valid
        assert!(!POSTAGE_STAMP_ADDRESS.is_empty());
        assert!(POSTAGE_STAMP_ADDRESS.starts_with("0x"));
    }
}
