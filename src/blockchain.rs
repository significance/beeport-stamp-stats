use crate::cache::Cache;
use crate::contracts::{ContractType, PostageStamp, StampsRegistry};
use crate::error::{Result, StampError};
use crate::events::{BatchInfo, EventData, EventType, StampEvent};
use alloy::primitives::Address;
use alloy::providers::{Provider, ProviderBuilder, RootProvider};
use alloy::rpc::types::{BlockTransactionsKind, Filter, Log};
use alloy::sol_types::SolEvent;
use alloy::transports::http::{Client, Http};
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use std::str::FromStr;

pub struct BlockchainClient {
    provider: RootProvider<Http<Client>>,
}

impl BlockchainClient {
    /// Create a new blockchain client
    pub async fn new(rpc_url: &str) -> Result<Self> {
        let provider = ProviderBuilder::new().on_http(
            rpc_url
                .parse()
                .map_err(|e| StampError::Rpc(format!("Invalid RPC URL: {}", e)))?,
        );

        Ok(Self { provider })
    }

    /// Fetch all batch-related events from all configured contracts
    pub async fn fetch_batch_events(
        &self,
        from_block: u64,
        to_block: u64,
        cache: &Cache,
    ) -> Result<Vec<StampEvent>> {
        let mut all_events = Vec::new();

        // Fetch events from each contract
        for contract_type in ContractType::all() {
            let events = self
                .fetch_contract_events(contract_type, from_block, to_block, cache)
                .await?;
            all_events.extend(events);
        }

        // Sort by block number and log index
        all_events.sort_by(|a, b| {
            a.block_number
                .cmp(&b.block_number)
                .then(a.log_index.cmp(&b.log_index))
        });

        Ok(all_events)
    }

    /// Generate a cache key for a chunk request
    fn generate_chunk_hash(contract_address: &str, from_block: u64, to_block: u64) -> String {
        let mut hasher = Sha256::new();
        hasher.update(contract_address.as_bytes());
        hasher.update(from_block.to_le_bytes());
        hasher.update(to_block.to_le_bytes());
        let result = hasher.finalize();
        format!("{:x}", result)
    }

    /// Fetch events from a specific contract
    async fn fetch_contract_events(
        &self,
        contract_type: ContractType,
        from_block: u64,
        to_block: u64,
        cache: &Cache,
    ) -> Result<Vec<StampEvent>> {
        let contract_address = Address::from_str(contract_type.address())
            .map_err(|e| StampError::Contract(format!("Invalid contract address: {}", e)))?;

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

        tracing::info!(
            "Fetching {} events from block {} to {}",
            contract_type.name(),
            from_block,
            to_block
        );

        // Fetch events in chunks to avoid RPC limits
        const CHUNK_SIZE: u64 = 10000;
        let mut current_from = from_block;

        let total_blocks = to_block - from_block + 1;
        let total_chunks = (total_blocks + CHUNK_SIZE - 1) / CHUNK_SIZE;
        let mut chunk_num = 0;

        while current_from <= to_block {
            let current_to = std::cmp::min(current_from + CHUNK_SIZE - 1, to_block);
            chunk_num += 1;

            // Generate cache hash for this chunk
            let chunk_hash =
                Self::generate_chunk_hash(contract_type.address(), current_from, current_to);

            // Check if chunk is already cached
            if cache.is_chunk_cached(&chunk_hash).await? {
                tracing::info!(
                    "  {} - Chunk {}/{}: blocks {} to {} [CACHED]",
                    contract_type.name(),
                    chunk_num,
                    total_chunks,
                    current_from,
                    current_to
                );
                current_from = current_to + 1;
                continue;
            }

            tracing::info!(
                "  {} - Chunk {}/{}: blocks {} to {}",
                contract_type.name(),
                chunk_num,
                total_chunks,
                current_from,
                current_to
            );

            // Create filter for all events from this contract
            let filter = Filter::new()
                .address(contract_address)
                .from_block(current_from)
                .to_block(current_to);

            let logs = self
                .provider
                .get_logs(&filter)
                .await
                .map_err(|e| StampError::Rpc(format!("Failed to fetch logs: {}", e)))?;

            if logs.len() > 0 {
                tracing::info!(
                    "    Found {} logs from {} in this chunk",
                    logs.len(),
                    contract_type.name()
                );
            }

            // Parse each log
            let chunk_event_count = events.len();
            for log in logs {
                if let Some(event) = self.parse_log(contract_type, log).await? {
                    events.push(event);
                }
            }
            let parsed_events = events.len() - chunk_event_count;

            // Cache this chunk
            cache
                .cache_chunk(
                    &chunk_hash,
                    contract_type.address(),
                    current_from,
                    current_to,
                    parsed_events,
                )
                .await?;

            current_from = current_to + 1;
        }

        tracing::info!(
            "Total {} events from {}: {}",
            contract_type.name(),
            contract_type.name(),
            events.len()
        );

        Ok(events)
    }

    /// Parse a log into a StampEvent based on contract type
    async fn parse_log(&self, contract_type: ContractType, log: Log) -> Result<Option<StampEvent>> {
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

        let contract_source = contract_type.name().to_string();

        match contract_type {
            ContractType::PostageStamp => self.parse_postage_stamp_log(
                log,
                block_number,
                block_timestamp,
                transaction_hash,
                log_index,
                contract_source,
            ),
            ContractType::StampsRegistry => self.parse_stamps_registry_log(
                log,
                block_number,
                block_timestamp,
                transaction_hash,
                log_index,
                contract_source,
            ),
        }
    }

    /// Parse PostageStamp contract events
    fn parse_postage_stamp_log(
        &self,
        log: Log,
        block_number: u64,
        block_timestamp: DateTime<Utc>,
        transaction_hash: alloy::primitives::TxHash,
        log_index: u64,
        contract_source: String,
    ) -> Result<Option<StampEvent>> {
        // Try to parse as each event type
        if let Ok(event) = PostageStamp::BatchCreated::decode_log(&log.inner, true) {
            return Ok(Some(StampEvent {
                event_type: EventType::BatchCreated,
                batch_id: format!("{:?}", event.batchId),
                block_number,
                block_timestamp,
                transaction_hash: format!("{:?}", transaction_hash),
                log_index,
                contract_source,
                data: EventData::BatchCreated {
                    total_amount: event.totalAmount.to_string(),
                    normalised_balance: event.normalisedBalance.to_string(),
                    owner: format!("{:?}", event.owner),
                    depth: event.depth,
                    bucket_depth: event.bucketDepth,
                    immutable_flag: event.immutableFlag,
                    payer: None, // PostageStamp doesn't track payer
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
                log_index,
                contract_source,
                data: EventData::BatchTopUp {
                    topup_amount: event.topupAmount.to_string(),
                    normalised_balance: event.normalisedBalance.to_string(),
                    payer: None,
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
                log_index,
                contract_source,
                data: EventData::BatchDepthIncrease {
                    new_depth: event.newDepth,
                    normalised_balance: event.normalisedBalance.to_string(),
                    payer: None,
                },
            }));
        }

        // Unknown event type
        Ok(None)
    }

    /// Parse StampsRegistry contract events
    fn parse_stamps_registry_log(
        &self,
        log: Log,
        block_number: u64,
        block_timestamp: DateTime<Utc>,
        transaction_hash: alloy::primitives::TxHash,
        log_index: u64,
        contract_source: String,
    ) -> Result<Option<StampEvent>> {
        // Try to parse as each event type
        if let Ok(event) = StampsRegistry::BatchCreated::decode_log(&log.inner, true) {
            return Ok(Some(StampEvent {
                event_type: EventType::BatchCreated,
                batch_id: format!("{:?}", event.batchId),
                block_number,
                block_timestamp,
                transaction_hash: format!("{:?}", transaction_hash),
                log_index,
                contract_source,
                data: EventData::BatchCreated {
                    total_amount: event.totalAmount.to_string(),
                    normalised_balance: event.normalisedBalance.to_string(),
                    owner: format!("{:?}", event.owner),
                    depth: event.depth,
                    bucket_depth: event.bucketDepth,
                    immutable_flag: event.immutableFlag,
                    payer: Some(format!("{:?}", event.payer)), // StampsRegistry tracks payer
                },
            }));
        }

        if let Ok(event) = StampsRegistry::BatchTopUp::decode_log(&log.inner, true) {
            return Ok(Some(StampEvent {
                event_type: EventType::BatchTopUp,
                batch_id: format!("{:?}", event.batchId),
                block_number,
                block_timestamp,
                transaction_hash: format!("{:?}", transaction_hash),
                log_index,
                contract_source,
                data: EventData::BatchTopUp {
                    topup_amount: event.topupAmount.to_string(),
                    normalised_balance: event.normalisedBalance.to_string(),
                    payer: Some(format!("{:?}", event.payer)),
                },
            }));
        }

        if let Ok(event) = StampsRegistry::BatchDepthIncrease::decode_log(&log.inner, true) {
            return Ok(Some(StampEvent {
                event_type: EventType::BatchDepthIncrease,
                batch_id: format!("{:?}", event.batchId),
                block_number,
                block_timestamp,
                transaction_hash: format!("{:?}", transaction_hash),
                log_index,
                contract_source,
                data: EventData::BatchDepthIncrease {
                    new_depth: event.newDepth,
                    normalised_balance: event.normalisedBalance.to_string(),
                    payer: Some(format!("{:?}", event.payer)),
                },
            }));
        }

        // Unknown event type
        Ok(None)
    }

    /// Get current storage price from blockchain
    pub async fn get_current_price(&self) -> Result<u128> {
        use crate::contracts::{PostageStamp, POSTAGE_STAMP_ADDRESS};
        use alloy::primitives::Address;

        let contract_address = Address::from_str(POSTAGE_STAMP_ADDRESS)
            .map_err(|e| StampError::Contract(format!("Invalid contract address: {}", e)))?;

        let contract = PostageStamp::new(contract_address, &self.provider);

        let price = contract
            .lastPrice()
            .call()
            .await
            .map_err(|e| StampError::Rpc(format!("Failed to get current price: {}", e)))?;

        Ok(price._0 as u128)
    }

    /// Get current block number
    pub async fn get_current_block(&self) -> Result<u64> {
        self.provider
            .get_block_number()
            .await
            .map_err(|e| StampError::Rpc(format!("Failed to get current block: {}", e)))
    }

    /// Get remaining balance for a batch from the blockchain with retry logic
    pub async fn get_remaining_balance(&self, batch_id: &str, max_retries: u32) -> Result<String> {
        use crate::contracts::{PostageStamp, POSTAGE_STAMP_ADDRESS};
        use alloy::primitives::{Address, FixedBytes};
        use tokio::time::{sleep, Duration};

        let contract_address = Address::from_str(POSTAGE_STAMP_ADDRESS)
            .map_err(|e| StampError::Contract(format!("Invalid contract address: {}", e)))?;

        // Parse batch ID as bytes32
        let batch_id_bytes = FixedBytes::<32>::from_str(batch_id.trim_start_matches("0x"))
            .map_err(|e| StampError::Parse(format!("Invalid batch ID: {}", e)))?;

        let contract = PostageStamp::new(contract_address, &self.provider);

        // Retry with exponential backoff for rate limit errors
        let mut retries = 0;

        loop {
            match contract.remainingBalance(batch_id_bytes).call().await {
                Ok(balance) => return Ok(balance._0.to_string()),
                Err(e) => {
                    let error_msg = e.to_string();

                    // Check if it's a rate limit error (429)
                    if error_msg.contains("429") || error_msg.contains("Too Many Requests") {
                        if retries < max_retries {
                            // Exponential backoff: 100ms, 200ms, 400ms, 800ms, 1600ms, etc.
                            let delay_ms = 100 * 2u64.pow(retries);
                            tracing::debug!("Rate limited, retrying after {}ms (attempt {}/{})", delay_ms, retries + 1, max_retries);
                            sleep(Duration::from_millis(delay_ms)).await;
                            retries += 1;
                            continue;
                        }
                    }

                    // For other errors or max retries exceeded, return the error
                    return Err(StampError::Rpc(format!("Failed to get remaining balance: {}", e)));
                }
            }
        }
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
    use crate::contracts::{POSTAGE_STAMP_ADDRESS, STAMPS_REGISTRY_ADDRESS};

    #[test]
    fn test_contract_address_parsing() {
        let addr1 = Address::from_str(POSTAGE_STAMP_ADDRESS);
        assert!(addr1.is_ok());

        let addr2 = Address::from_str(STAMPS_REGISTRY_ADDRESS);
        assert!(addr2.is_ok());
    }

    // Note: Integration tests with actual RPC would go in tests/ directory
    // to avoid making network calls during unit tests
}
