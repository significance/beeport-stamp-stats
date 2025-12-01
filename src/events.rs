use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// Re-export from contracts module
pub use crate::contracts::DEFAULT_START_BLOCK;

/// Unified event type that can represent any PostageStamp event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StampEvent {
    pub event_type: EventType,
    pub batch_id: String,
    pub block_number: u64,
    pub block_timestamp: DateTime<Utc>,
    pub transaction_hash: String,
    pub log_index: u64,
    pub contract_source: String, // Which contract emitted this event
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
        payer: Option<String>, // Only present in StampsRegistry events
    },
    BatchTopUp {
        topup_amount: String,
        normalised_balance: String,
        payer: Option<String>, // Only present in StampsRegistry events
    },
    BatchDepthIncrease {
        new_depth: u8,
        normalised_balance: String,
        payer: Option<String>, // Only present in StampsRegistry events
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
            contract_source: "PostageStamp".to_string(),
            data: EventData::BatchCreated {
                total_amount: "1000000000000000000".to_string(),
                normalised_balance: "500000000000000000".to_string(),
                owner: "0x5678".to_string(),
                depth: 20,
                bucket_depth: 16,
                immutable_flag: false,
                payer: None,
            },
        };

        let json = serde_json::to_string(&event).unwrap();
        let deserialized: StampEvent = serde_json::from_str(&json).unwrap();

        assert_eq!(event.batch_id, deserialized.batch_id);
        assert_eq!(event.block_number, deserialized.block_number);
    }

    #[test]
    fn test_default_start_block() {
        // Verify the default start block is set
        assert!(DEFAULT_START_BLOCK > 0);
    }
}
