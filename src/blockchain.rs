use crate::error::{Result, StampError};
use crate::events::{
    BatchInfo, EventData, EventType, PostageStamp, StampEvent, POSTAGE_STAMP_ADDRESS,
};
use alloy::primitives::Address;
use alloy::providers::{Provider, ProviderBuilder, RootProvider};
use alloy::rpc::types::{BlockTransactionsKind, Filter, Log};
use alloy::sol_types::SolEvent;
use alloy::transports::http::{Client, Http};
use chrono::{DateTime, Utc};
use std::str::FromStr;

pub struct BlockchainClient {
    provider: RootProvider<Http<Client>>,
    contract_address: Address,
}

impl BlockchainClient {
    /// Create a new blockchain client
    pub async fn new(rpc_url: &str) -> Result<Self> {
        let provider = ProviderBuilder::new()
            .on_http(rpc_url.parse().map_err(|e| {
                StampError::Rpc(format!("Invalid RPC URL: {}", e))
            })?);

        let contract_address = Address::from_str(POSTAGE_STAMP_ADDRESS)
            .map_err(|e| StampError::Contract(format!("Invalid contract address: {}", e)))?;

        Ok(Self {
            provider,
            contract_address,
        })
    }

    /// Fetch all batch-related events from the blockchain
    pub async fn fetch_batch_events(
        &self,
        from_block: u64,
        to_block: u64,
    ) -> Result<Vec<StampEvent>> {
        let mut events = Vec::new();

        // Determine the actual to_block
        let to_block = if to_block == u64::MAX {
            self.provider
                .get_block_number()
                .await
                .map_err(|e| StampError::Rpc(format!("Failed to get latest block: {}", e)))?
        } else {
            to_block
        };

        tracing::info!("Fetching events from block {} to {}", from_block, to_block);

        // Fetch events in chunks to avoid RPC limits
        const CHUNK_SIZE: u64 = 10000;
        let mut current_from = from_block;

        while current_from <= to_block {
            let current_to = std::cmp::min(current_from + CHUNK_SIZE - 1, to_block);

            tracing::info!(
                "Fetching chunk: blocks {} to {}",
                current_from,
                current_to
            );

            // Create filter for all PostageStamp events
            let filter = Filter::new()
                .address(self.contract_address)
                .from_block(current_from)
                .to_block(current_to);

            let logs = self
                .provider
                .get_logs(&filter)
                .await
                .map_err(|e| StampError::Rpc(format!("Failed to fetch logs: {}", e)))?;

            tracing::info!("Found {} logs in this chunk", logs.len());

            // Parse each log
            for log in logs {
                if let Some(event) = self.parse_log(log).await? {
                    events.push(event);
                }
            }

            current_from = current_to + 1;
        }

        Ok(events)
    }

    /// Parse a log into a StampEvent
    async fn parse_log(&self, log: Log) -> Result<Option<StampEvent>> {
        let block_number = log
            .block_number
            .ok_or_else(|| StampError::Parse("Missing block number".to_string()))?;

        let transaction_hash = log
            .transaction_hash
            .ok_or_else(|| StampError::Parse("Missing transaction hash".to_string()))?;

        let log_index = log
            .log_index
            .ok_or_else(|| StampError::Parse("Missing log index".to_string()))?;

        // Get block timestamp
        let block = self
            .provider
            .get_block_by_number(block_number.into(), BlockTransactionsKind::Hashes)
            .await
            .map_err(|e| StampError::Rpc(format!("Failed to get block: {}", e)))?
            .ok_or_else(|| StampError::Rpc(format!("Block {} not found", block_number)))?;

        let timestamp = block.header.timestamp;
        let block_timestamp =
            DateTime::from_timestamp(timestamp as i64, 0).unwrap_or_else(|| Utc::now());

        // Try to parse as each event type
        if let Ok(event) = PostageStamp::BatchCreated::decode_log(&log.inner, true) {
            return Ok(Some(StampEvent {
                event_type: EventType::BatchCreated,
                batch_id: format!("{:?}", event.batchId),
                block_number,
                block_timestamp,
                transaction_hash: format!("{:?}", transaction_hash),
                log_index: log_index as u64,
                data: EventData::BatchCreated {
                    total_amount: event.totalAmount.to_string(),
                    normalised_balance: event.normalisedBalance.to_string(),
                    owner: format!("{:?}", event.owner),
                    depth: event.depth,
                    bucket_depth: event.bucketDepth,
                    immutable_flag: event.immutableFlag,
                },
            }));
        }

        if let Ok(event) = PostageStamp::BatchTopUp::decode_log(&log.inner, true) {
            return Ok(Some(StampEvent {
                event_type: EventType::BatchTopUp,
                batch_id: format!("{:?}", event.batchId),
                block_number,
                block_timestamp,
                transaction_hash: format!("{:?}", transaction_hash),
                log_index: log_index as u64,
                data: EventData::BatchTopUp {
                    topup_amount: event.topupAmount.to_string(),
                    normalised_balance: event.normalisedBalance.to_string(),
                },
            }));
        }

        if let Ok(event) = PostageStamp::BatchDepthIncrease::decode_log(&log.inner, true) {
            return Ok(Some(StampEvent {
                event_type: EventType::BatchDepthIncrease,
                batch_id: format!("{:?}", event.batchId),
                block_number,
                block_timestamp,
                transaction_hash: format!("{:?}", transaction_hash),
                log_index: log_index as u64,
                data: EventData::BatchDepthIncrease {
                    new_depth: event.newDepth,
                    normalised_balance: event.normalisedBalance.to_string(),
                },
            }));
        }

        // Unknown event type
        Ok(None)
    }

    /// Fetch batch information for BatchCreated events
    pub async fn fetch_batch_info(&self, events: &[StampEvent]) -> Result<Vec<BatchInfo>> {
        let mut batches = Vec::new();

        for event in events {
            if matches!(event.event_type, EventType::BatchCreated) {
                if let EventData::BatchCreated {
                    owner,
                    depth,
                    bucket_depth,
                    immutable_flag,
                    normalised_balance,
                    ..
                } = &event.data
                {
                    batches.push(BatchInfo {
                        batch_id: event.batch_id.clone(),
                        owner: owner.clone(),
                        depth: *depth,
                        bucket_depth: *bucket_depth,
                        immutable: *immutable_flag,
                        normalised_balance: normalised_balance.clone(),
                        created_at: event.block_timestamp,
                    });
                }
            }
        }

        Ok(batches)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_contract_address_parsing() {
        let addr = Address::from_str(POSTAGE_STAMP_ADDRESS);
        assert!(addr.is_ok());
    }

    // Note: Integration tests with actual RPC would go in tests/ directory
    // to avoid making network calls during unit tests
}
