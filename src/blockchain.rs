use crate::cache::Cache;
use crate::config::BlockchainConfig;
use crate::contracts::{
    abi::PostageStamp, Contract, ContractRegistry, StorageIncentivesContract,
    StorageIncentivesContractRegistry,
};
use crate::error::{Result, StampError};
use crate::events::{BatchInfo, EventData, EventType, StampEvent, StorageIncentivesEvent};
use crate::retry::RetryConfig;
use alloy::primitives::Address;
use alloy::providers::{Provider, ProviderBuilder, RootProvider};
use alloy::rpc::types::{Block, BlockTransactionsKind, Filter, Log};
use alloy::transports::http::{Client, Http};
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::str::FromStr;

#[derive(Clone)]
pub struct BlockchainClient {
    provider: RootProvider<Http<Client>>,
}

impl BlockchainClient {
    /// Create a new blockchain client
    pub async fn new(rpc_url: &str) -> Result<Self> {
        let provider = ProviderBuilder::new().on_http(
            rpc_url
                .parse()
                .map_err(|e| StampError::Rpc(format!("Invalid RPC URL: {e}")))?,
        );

        Ok(Self { provider })
    }

    /// Fetch all batch-related events from all configured contracts
    ///
    /// The `on_chunk_complete` callback is called after each chunk is fetched and can be used
    /// to store events incrementally to avoid data loss on interruption.
    ///
    /// If `refresh` is true, cached chunks will be reprocessed (useful after adding new event types).
    #[allow(clippy::too_many_arguments)]
    pub async fn fetch_batch_events<F, Fut>(
        &self,
        from_block: u64,
        to_block: u64,
        cache: &Cache,
        registry: &ContractRegistry,
        blockchain_config: &BlockchainConfig,
        retry_config: &RetryConfig,
        refresh: bool,
        on_chunk_complete: F,
    ) -> Result<Vec<StampEvent>>
    where
        F: Fn(Vec<StampEvent>) -> Fut + Copy,
        Fut: std::future::Future<Output = Result<()>>,
    {
        let mut all_events = Vec::new();

        // Fetch events from each contract
        for contract in registry.all() {
            let events = self
                .fetch_contract_events(
                    contract.as_ref(),
                    from_block,
                    to_block,
                    cache,
                    blockchain_config,
                    retry_config,
                    refresh,
                    on_chunk_complete,
                )
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
        format!("{result:x}")
    }

    /// Fetch events from a specific contract
    ///
    /// The `on_chunk_complete` callback is called after each chunk is fetched with the events
    /// from that chunk, allowing for incremental storage.
    ///
    /// If `refresh` is true, cached chunks will be reprocessed (useful after adding new event types).
    #[allow(clippy::too_many_arguments)]
    async fn fetch_contract_events<F, Fut>(
        &self,
        contract: &dyn Contract,
        from_block: u64,
        to_block: u64,
        cache: &Cache,
        blockchain_config: &BlockchainConfig,
        retry_config: &RetryConfig,
        refresh: bool,
        on_chunk_complete: F,
    ) -> Result<Vec<StampEvent>>
    where
        F: Fn(Vec<StampEvent>) -> Fut,
        Fut: std::future::Future<Output = Result<()>>,
    {
        let contract_address = Address::from_str(contract.address())
            .map_err(|e| StampError::Contract(format!("Invalid contract address: {e}")))?;

        let mut events = Vec::new();
        let mut block_cache: HashMap<u64, Block> = HashMap::new();

        // Determine the actual to_block
        let to_block = if to_block == u64::MAX {
            tracing::debug!("RPC: get_block_number()");
            self.provider
                .get_block_number()
                .await
                .map_err(|e| StampError::Rpc(format!("Failed to get latest block: {e}")))?
        } else {
            to_block
        };

        // Adjust from_block to not start before contract deployment
        let deployment_block = contract.deployment_block();
        let adjusted_from_block = std::cmp::max(from_block, deployment_block);

        // Skip if the requested range is entirely before deployment
        if adjusted_from_block > to_block {
            tracing::info!(
                "Skipping {} - contract deployed at block {} (after requested range)",
                contract.name(),
                deployment_block
            );
            return Ok(events);
        }

        tracing::info!(
            "Fetching {} events from block {} to {} (contract deployed at {})",
            contract.name(),
            adjusted_from_block,
            to_block,
            deployment_block
        );

        // Fetch events in chunks to avoid RPC limits
        let chunk_size = blockchain_config.chunk_size;
        let mut current_from = adjusted_from_block;

        let total_blocks = to_block - adjusted_from_block + 1;
        let total_chunks = total_blocks.div_ceil(chunk_size);
        let mut chunk_num = 0;

        while current_from <= to_block {
            let current_to = std::cmp::min(current_from + chunk_size - 1, to_block);
            chunk_num += 1;

            // Generate cache hash for this chunk
            let chunk_hash =
                Self::generate_chunk_hash(contract.address(), current_from, current_to);

            // Check if chunk is already cached (skip check if refresh mode enabled)
            if !refresh && cache.is_chunk_cached(&chunk_hash).await? {
                tracing::info!(
                    "  {} - Chunk {}/{}: blocks {} to {} [CACHED]",
                    contract.name(),
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
                contract.name(),
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

            // Use retry policy for rate limit handling
            tracing::debug!(
                "RPC: get_logs(contract={}, from_block={}, to_block={})",
                contract.address(),
                current_from,
                current_to
            );
            let provider = &self.provider;
            let logs = retry_config
                .execute(|| async { provider.get_logs(&filter).await })
                .await
                .map_err(StampError::Rpc)?;

            if !logs.is_empty() {
                tracing::info!(
                    "    Found {} logs from {} in this chunk",
                    logs.len(),
                    contract.name()
                );
            }

            // Parse each log
            let chunk_event_count = events.len();
            let mut chunk_events = Vec::new();
            for log in logs {
                if let Some(event) = self
                    .parse_log(
                        contract,
                        log,
                        cache,
                        &mut block_cache,
                        retry_config,
                    )
                    .await?
                {
                    chunk_events.push(event.clone());
                    events.push(event);
                }
            }
            let parsed_events = events.len() - chunk_event_count;

            // Cache this chunk
            cache
                .cache_chunk(
                    &chunk_hash,
                    contract.address(),
                    current_from,
                    current_to,
                    parsed_events,
                )
                .await?;

            // Call the callback with chunk events for incremental storage
            if !chunk_events.is_empty() {
                on_chunk_complete(chunk_events).await?;
            }

            current_from = current_to + 1;
        }

        tracing::info!(
            "Total {} events from {}: {}",
            contract.name(),
            contract.name(),
            events.len()
        );

        tracing::debug!(
            "Block cache for {}: {} unique blocks cached",
            contract.name(),
            block_cache.len()
        );

        Ok(events)
    }

    /// Parse a log into a StampEvent by delegating to the contract's parser
    async fn parse_log(
        &self,
        contract: &dyn Contract,
        log: Log,
        cache: &Cache,
        block_cache: &mut HashMap<u64, Block>,
        retry_config: &RetryConfig,
    ) -> Result<Option<StampEvent>> {
        let block_number = log
            .block_number
            .ok_or_else(|| StampError::Parse("Missing block number".to_string()))?;

        let transaction_hash = log
            .transaction_hash
            .ok_or_else(|| StampError::Parse("Missing transaction hash".to_string()))?;

        let log_index = log
            .log_index
            .ok_or_else(|| StampError::Parse("Missing log index".to_string()))?;

        // Get block timestamp - check in-memory cache first, then database, then RPC
        let block_timestamp = if let Some(cached_block) = block_cache.get(&block_number) {
            tracing::debug!("Block cache HIT (memory) for block {}", block_number);
            let timestamp = cached_block.header.timestamp;
            DateTime::from_timestamp(timestamp as i64, 0).unwrap_or_else(Utc::now)
        } else if let Some(db_timestamp) = cache.get_block_timestamp(block_number).await? {
            tracing::debug!("Block cache HIT (database) for block {}", block_number);
            DateTime::from_timestamp(db_timestamp, 0).unwrap_or_else(Utc::now)
        } else {
            tracing::debug!("Block cache MISS - RPC: get_block_by_number(block={})", block_number);

            // Wrap get_block_by_number with retry logic
            let provider = &self.provider;
            let fetched_block = retry_config
                .execute(|| async {
                    let block = provider
                        .get_block_by_number(block_number.into(), BlockTransactionsKind::Hashes)
                        .await
                        .map_err(|e| {
                            std::io::Error::other(
                                format!("Failed to get block: {e}"),
                            )
                        })?
                        .ok_or_else(|| {
                            std::io::Error::new(
                                std::io::ErrorKind::NotFound,
                                format!("Block {block_number} not found"),
                            )
                        })?;
                    Ok::<Block, std::io::Error>(block)
                })
                .await
                .map_err(StampError::Rpc)?;

            let timestamp = fetched_block.header.timestamp;

            // Store in in-memory cache for future use in this session
            block_cache.insert(block_number, fetched_block);

            DateTime::from_timestamp(timestamp as i64, 0).unwrap_or_else(Utc::now)
        };

        // Delegate to the contract's parse_log implementation
        contract.parse_log(log, block_number, block_timestamp, transaction_hash, log_index)
    }

    /// Fetch all storage incentives events from all configured contracts
    ///
    /// Similar to fetch_batch_events but for PriceOracle, StakeRegistry, and Redistribution contracts.
    /// The `on_chunk_complete` callback is called after each chunk is fetched.
    #[allow(clippy::too_many_arguments)]
    pub async fn fetch_storage_incentives_events<F, Fut>(
        &self,
        from_block: u64,
        to_block: u64,
        cache: &Cache,
        registry: &StorageIncentivesContractRegistry,
        blockchain_config: &BlockchainConfig,
        retry_config: &RetryConfig,
        refresh: bool,
        on_chunk_complete: F,
    ) -> Result<Vec<StorageIncentivesEvent>>
    where
        F: Fn(Vec<StorageIncentivesEvent>) -> Fut + Copy,
        Fut: std::future::Future<Output = Result<()>>,
    {
        let mut all_events = Vec::new();

        // Fetch events from each storage incentives contract
        for contract in registry.all() {
            let events = self
                .fetch_storage_incentives_contract_events(
                    contract.as_ref(),
                    from_block,
                    to_block,
                    cache,
                    blockchain_config,
                    retry_config,
                    refresh,
                    on_chunk_complete,
                )
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

    /// Fetch events from a specific storage incentives contract
    ///
    /// If `refresh` is true, cached chunks will be reprocessed (useful after adding new event types).
    #[allow(clippy::too_many_arguments)]
    async fn fetch_storage_incentives_contract_events<F, Fut>(
        &self,
        contract: &dyn StorageIncentivesContract,
        from_block: u64,
        to_block: u64,
        cache: &Cache,
        blockchain_config: &BlockchainConfig,
        retry_config: &RetryConfig,
        refresh: bool,
        on_chunk_complete: F,
    ) -> Result<Vec<StorageIncentivesEvent>>
    where
        F: Fn(Vec<StorageIncentivesEvent>) -> Fut,
        Fut: std::future::Future<Output = Result<()>>,
    {
        let contract_address = Address::from_str(contract.address())
            .map_err(|e| StampError::Contract(format!("Invalid contract address: {e}")))?;

        let mut events = Vec::new();
        let mut block_cache: HashMap<u64, Block> = HashMap::new();

        // Determine the actual to_block
        let to_block = if to_block == u64::MAX {
            tracing::debug!("RPC: get_block_number()");
            self.provider
                .get_block_number()
                .await
                .map_err(|e| StampError::Rpc(format!("Failed to get latest block: {e}")))?
        } else {
            to_block
        };

        // Adjust from_block to not start before contract deployment
        let deployment_block = contract.deployment_block();
        let adjusted_from_block = std::cmp::max(from_block, deployment_block);

        // Skip if the requested range is entirely before deployment
        if adjusted_from_block > to_block {
            tracing::info!(
                "Skipping {} - contract deployed at block {} (after requested range)",
                contract.name(),
                deployment_block
            );
            return Ok(events);
        }

        tracing::info!(
            "Fetching {} events from block {} to {} (contract deployed at {})",
            contract.name(),
            adjusted_from_block,
            to_block,
            deployment_block
        );

        // Fetch events in chunks to avoid RPC limits
        let chunk_size = blockchain_config.chunk_size;
        let mut current_from = adjusted_from_block;

        let total_blocks = to_block - adjusted_from_block + 1;
        let total_chunks = total_blocks.div_ceil(chunk_size);
        let mut chunk_num = 0;

        while current_from <= to_block {
            let current_to = std::cmp::min(current_from + chunk_size - 1, to_block);
            chunk_num += 1;

            // Generate cache hash for this chunk
            let chunk_hash =
                Self::generate_chunk_hash(contract.address(), current_from, current_to);

            // Check if chunk is already cached (skip check if refresh mode enabled)
            if !refresh && cache.is_chunk_cached(&chunk_hash).await? {
                tracing::info!(
                    "  {} - Chunk {}/{}: blocks {} to {} [CACHED]",
                    contract.name(),
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
                contract.name(),
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

            // Use retry policy for rate limit handling
            tracing::debug!(
                "RPC: get_logs(contract={}, from_block={}, to_block={})",
                contract.address(),
                current_from,
                current_to
            );
            let provider = &self.provider;
            let logs = retry_config
                .execute(|| async { provider.get_logs(&filter).await })
                .await
                .map_err(StampError::Rpc)?;

            if !logs.is_empty() {
                tracing::info!(
                    "    Found {} logs from {} in this chunk",
                    logs.len(),
                    contract.name()
                );
            }

            // Parse each log
            let chunk_event_count = events.len();
            let mut chunk_events = Vec::new();
            for log in logs {
                if let Some(event) = self
                    .parse_storage_incentives_log(
                        contract,
                        log,
                        cache,
                        &mut block_cache,
                        retry_config,
                    )
                    .await?
                {
                    chunk_events.push(event.clone());
                    events.push(event);
                }
            }
            let parsed_events = events.len() - chunk_event_count;

            // Cache this chunk
            cache
                .cache_chunk(
                    &chunk_hash,
                    contract.address(),
                    current_from,
                    current_to,
                    parsed_events,
                )
                .await?;

            // Call the callback with chunk events for incremental storage
            if !chunk_events.is_empty() {
                on_chunk_complete(chunk_events).await?;
            }

            current_from = current_to + 1;
        }

        tracing::info!(
            "Total {} events from {}: {}",
            contract.name(),
            contract.name(),
            events.len()
        );

        tracing::debug!(
            "Block cache for {}: {} unique blocks cached",
            contract.name(),
            block_cache.len()
        );

        Ok(events)
    }

    /// Parse a log into a StorageIncentivesEvent by delegating to the contract's parser
    async fn parse_storage_incentives_log(
        &self,
        contract: &dyn StorageIncentivesContract,
        log: Log,
        cache: &Cache,
        block_cache: &mut HashMap<u64, Block>,
        retry_config: &RetryConfig,
    ) -> Result<Option<StorageIncentivesEvent>> {
        let block_number = log
            .block_number
            .ok_or_else(|| StampError::Parse("Missing block number".to_string()))?;

        let transaction_hash = log
            .transaction_hash
            .ok_or_else(|| StampError::Parse("Missing transaction hash".to_string()))?;

        let log_index = log
            .log_index
            .ok_or_else(|| StampError::Parse("Missing log index".to_string()))?;

        // Get block timestamp - check in-memory cache first, then database, then RPC
        let block_timestamp = if let Some(cached_block) = block_cache.get(&block_number) {
            tracing::debug!("Block cache HIT (memory) for block {}", block_number);
            let timestamp = cached_block.header.timestamp;
            DateTime::from_timestamp(timestamp as i64, 0).unwrap_or_else(Utc::now)
        } else if let Some(db_timestamp) = cache.get_block_timestamp(block_number).await? {
            tracing::debug!("Block cache HIT (database) for block {}", block_number);
            DateTime::from_timestamp(db_timestamp, 0).unwrap_or_else(Utc::now)
        } else {
            tracing::debug!("Block cache MISS - RPC: get_block_by_number(block={})", block_number);

            // Wrap get_block_by_number with retry logic
            let provider = &self.provider;
            let fetched_block = retry_config
                .execute(|| async {
                    let block = provider
                        .get_block_by_number(block_number.into(), BlockTransactionsKind::Hashes)
                        .await
                        .map_err(|e| {
                            std::io::Error::other(
                                format!("Failed to get block: {e}"),
                            )
                        })?
                        .ok_or_else(|| {
                            std::io::Error::new(
                                std::io::ErrorKind::NotFound,
                                format!("Block {block_number} not found"),
                            )
                        })?;
                    Ok::<Block, std::io::Error>(block)
                })
                .await
                .map_err(StampError::Rpc)?;

            let timestamp = fetched_block.header.timestamp;

            // Store in in-memory cache for future use in this session
            block_cache.insert(block_number, fetched_block);

            DateTime::from_timestamp(timestamp as i64, 0).unwrap_or_else(Utc::now)
        };

        // Delegate to the contract's parse_log implementation
        contract.parse_log(log, block_number, block_timestamp, transaction_hash, log_index)
    }

    /// Get current storage price from blockchain
    ///
    /// Uses the first contract from the registry that supports price queries
    pub async fn get_current_price(&self, registry: &ContractRegistry) -> Result<u128> {
        use alloy::primitives::Address;

        let contract = registry
            .find_price_query_contract()
            .ok_or_else(|| {
                StampError::Config("No contract supports price queries in the registry".to_string())
            })?;

        let contract_address = Address::from_str(contract.address())
            .map_err(|e| StampError::Contract(format!("Invalid contract address: {e}")))?;

        let postage_stamp_contract = PostageStamp::new(contract_address, &self.provider);

        tracing::debug!("RPC: lastPrice()");
        let price = postage_stamp_contract
            .lastPrice()
            .call()
            .await
            .map_err(|e| StampError::Rpc(format!("Failed to get current price: {e}")))?;

        Ok(price._0 as u128)
    }

    /// Get current block number
    pub async fn get_current_block(&self) -> Result<u64> {
        tracing::debug!("RPC: get_block_number()");
        self.provider
            .get_block_number()
            .await
            .map_err(|e| StampError::Rpc(format!("Failed to get current block: {e}")))
    }

    /// Get remaining balance for a batch from the blockchain with retry logic
    ///
    /// Uses the first contract from the registry that supports balance queries
    pub async fn get_remaining_balance(
        &self,
        batch_id: &str,
        registry: &ContractRegistry,
        retry_config: &RetryConfig,
    ) -> Result<String> {
        use alloy::primitives::{Address, FixedBytes};

        let contract = registry
            .find_balance_query_contract()
            .ok_or_else(|| {
                StampError::Config(
                    "No contract supports balance queries in the registry".to_string(),
                )
            })?;

        let contract_address = Address::from_str(contract.address())
            .map_err(|e| StampError::Contract(format!("Invalid contract address: {e}")))?;

        // Parse batch ID as bytes32
        let batch_id_bytes = FixedBytes::<32>::from_str(batch_id.trim_start_matches("0x"))
            .map_err(|e| StampError::Parse(format!("Invalid batch ID: {e}")))?;

        let postage_stamp_contract = PostageStamp::new(contract_address, &self.provider);

        // Use retry policy for rate limit handling
        tracing::debug!("RPC: remainingBalance(batch_id={})", batch_id);
        retry_config
            .execute(|| async {
                postage_stamp_contract
                    .remainingBalance(batch_id_bytes)
                    .call()
                    .await
                    .map(|balance| balance._0.to_string())
            })
            .await
            .map_err(StampError::Rpc)
    }

    /// Fetch batch information for BatchCreated events
    pub async fn fetch_batch_info(&self, events: &[StampEvent]) -> Result<Vec<BatchInfo>> {
        let mut batches = Vec::new();

        for event in events {
            if matches!(event.event_type, EventType::BatchCreated)
                && let EventData::BatchCreated {
                    owner,
                    depth,
                    bucket_depth,
                    immutable_flag,
                    normalised_balance,
                    payer,
                    ..
                } = &event.data
            {
                batches.push(BatchInfo {
                    batch_id: event.batch_id.clone().unwrap_or_default(),
                    owner: owner.clone(),
                    payer: payer.clone(),
                    contract_source: event.contract_source.clone(),
                    depth: *depth,
                    bucket_depth: *bucket_depth,
                    immutable: *immutable_flag,
                    normalised_balance: normalised_balance.clone(),
                    created_at: event.block_timestamp,
                    block_number: event.block_number,
                });
            }
        }

        Ok(batches)
    }

    /// Fetch transaction details to get the from address
    ///
    /// Returns the sender address of the transaction.
    pub async fn get_transaction_from_address(
        &self,
        transaction_hash: &str,
        retry_config: &RetryConfig,
    ) -> Result<String> {
        tracing::debug!("RPC: get_transaction_by_hash(hash={})", transaction_hash);

        let provider = &self.provider;
        let tx_hash_bytes = transaction_hash
            .parse()
            .map_err(|e| StampError::Parse(format!("Invalid transaction hash: {e}")))?;

        retry_config
            .execute(|| async {
                let tx = provider
                    .get_transaction_by_hash(tx_hash_bytes)
                    .await
                    .map_err(|e| std::io::Error::other(format!("Failed to get transaction: {e}")))?
                    .ok_or_else(|| {
                        std::io::Error::new(
                            std::io::ErrorKind::NotFound,
                            format!("Transaction {transaction_hash} not found"),
                        )
                    })?;

                Ok::<String, std::io::Error>(format!("{:?}", tx.from))
            })
            .await
            .map_err(StampError::Rpc)
    }

    /// Populate from_address for all events by fetching transaction details
    ///
    /// This modifies the events in place, setting the from_address field.
    pub async fn populate_from_addresses(
        &self,
        events: &mut [StampEvent],
        retry_config: &RetryConfig,
    ) -> Result<()> {
        for event in events {
            if event.from_address.is_none() {
                match self
                    .get_transaction_from_address(&event.transaction_hash, retry_config)
                    .await
                {
                    Ok(from_addr) => {
                        event.from_address = Some(from_addr);
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to fetch from_address for tx {}: {}",
                            event.transaction_hash,
                            e
                        );
                        // Continue processing other events even if one fails
                    }
                }
            }
        }
        Ok(())
    }
}

// Note: Integration tests with actual RPC would go in tests/ directory
// to avoid making network calls during unit tests
