use crate::cache::Cache;
use crate::config::BlockchainConfig;
use crate::contracts::{
    abi::PostageStamp, Contract, ContractRegistry, StorageIncentivesContract,
    StorageIncentivesContractRegistry,
};
use crate::error::{Result, StampError};
use crate::events::{BatchInfo, EventData, EventType, StampEvent, StorageIncentivesEvent};
use crate::retry::RetryConfig;
use crate::rpc_scheduler::RpcScheduler;
use alloy::primitives::Address;
use alloy::providers::{Provider, ProviderBuilder, RootProvider};
use alloy::rpc::types::{Block, BlockTransactionsKind, Filter, Log};
use alloy::transports::http::{Client, Http};
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;

/// Information about a chunk to be fetched
#[derive(Clone)]
pub struct ChunkRequest {
    pub contract_name: String,
    #[allow(dead_code)]
    pub contract_address: Address,
    pub from_block: u64,
    pub to_block: u64,
    pub chunk_num: usize,
    pub total_chunks: usize,
    pub chunk_hash: String,
    pub filter: Filter,
}

/// Result of fetching logs for a chunk
pub struct ChunkResult {
    pub chunk: ChunkRequest,
    pub logs: Vec<Log>,
}

#[derive(Clone)]
pub struct BlockchainClient {
    provider: RootProvider<Http<Client>>,
    scheduler: Option<Arc<RpcScheduler>>,
}

impl BlockchainClient {
    /// Create a new blockchain client with a single RPC endpoint
    pub async fn new(rpc_url: &str) -> Result<Self> {
        let provider = ProviderBuilder::new().on_http(
            rpc_url
                .parse()
                .map_err(|e| StampError::Rpc(format!("Invalid RPC URL: {e}")))?,
        );

        Ok(Self {
            provider,
            scheduler: None,
        })
    }

    /// Create a new blockchain client with multi-RPC scheduler
    pub async fn with_scheduler(scheduler: Arc<RpcScheduler>) -> Result<Self> {
        // Use the first endpoint's provider as fallback for non-parallelizable operations
        let provider = scheduler.primary_provider().clone();

        Ok(Self {
            provider,
            scheduler: Some(scheduler),
        })
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

    /// Collect chunks that need to be fetched (not cached)
    ///
    /// Returns a list of ChunkRequest objects for chunks that are not already cached.
    /// If `refresh` is true, all chunks are returned regardless of cache status.
    #[allow(clippy::too_many_arguments)]
    async fn collect_chunks_to_fetch(
        &self,
        contract_address: Address,
        contract_name: &str,
        from_block: u64,
        to_block: u64,
        cache: &Cache,
        chunk_size: u64,
        refresh: bool,
    ) -> Result<Vec<ChunkRequest>> {
        let total_blocks = to_block - from_block + 1;
        let total_chunks = total_blocks.div_ceil(chunk_size);

        let mut chunks_to_fetch = Vec::new();
        let mut current_from = from_block;
        let mut chunk_num = 0;

        while current_from <= to_block {
            let current_to = std::cmp::min(current_from + chunk_size - 1, to_block);
            chunk_num += 1;

            let chunk_hash =
                Self::generate_chunk_hash(&format!("{contract_address:?}"), current_from, current_to);

            // Check if chunk is already cached (skip check if refresh mode enabled)
            if !refresh && cache.is_chunk_cached(&chunk_hash).await? {
                tracing::info!(
                    "  {} - Chunk {}/{}: blocks {} to {} [CACHED]",
                    contract_name,
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
                contract_name,
                chunk_num,
                total_chunks,
                current_from,
                current_to
            );

            let filter = Filter::new()
                .address(contract_address)
                .from_block(current_from)
                .to_block(current_to);

            chunks_to_fetch.push(ChunkRequest {
                contract_name: contract_name.to_string(),
                contract_address,
                from_block: current_from,
                to_block: current_to,
                chunk_num,
                total_chunks: total_chunks as usize,
                chunk_hash,
                filter,
            });

            current_from = current_to + 1;
        }

        Ok(chunks_to_fetch)
    }

    /// Fetch logs for a list of chunks using either multi-RPC scheduler or single provider
    ///
    /// Returns results in the same order as the input chunks.
    /// Uses parallel fetching when scheduler is available, sequential otherwise.
    async fn fetch_logs_for_chunks(
        &self,
        chunks: &[ChunkRequest],
        retry_config: &RetryConfig,
        cache: &Cache,
        contract_address: &str,
    ) -> Result<Vec<ChunkResult>> {
        if chunks.is_empty() {
            return Ok(Vec::new());
        }

        if let Some(ref scheduler) = self.scheduler {
            // Multi-RPC mode: fetch chunks in batches
            const BATCH_SIZE: usize = 10;
            tracing::info!(
                "Fetching {} chunks in batches of {} (multi-RPC mode)",
                chunks.len(),
                BATCH_SIZE
            );

            let mut results = Vec::with_capacity(chunks.len());

            for (batch_idx, chunk_batch) in chunks.chunks(BATCH_SIZE).enumerate() {
                tracing::info!(
                    "Processing batch {}/{} ({} chunks)",
                    batch_idx + 1,
                    chunks.len().div_ceil(BATCH_SIZE),
                    chunk_batch.len()
                );

                // Create futures for this batch
                let futures: Vec<_> = chunk_batch
                    .iter()
                    .map(|chunk| {
                        let filter = chunk.filter.clone();
                        let from_block = chunk.from_block;
                        let to_block = chunk.to_block;
                        let contract_name = chunk.contract_name.clone();
                        let scheduler_clone = scheduler.clone();

                        async move {
                            tracing::debug!(
                                "RPC: get_logs(contract={}, from_block={}, to_block={})",
                                contract_name,
                                from_block,
                                to_block
                            );

                            scheduler_clone
                                .execute_with_retry(|provider| {
                                    let filter = filter.clone();
                                    let provider = provider.clone();
                                    let contract_name = contract_name.clone();
                                    async move {
                                        let result = provider
                                            .get_logs(&filter)
                                            .await
                                            .map_err(|e| StampError::Rpc(e.to_string()));

                                        match &result {
                                            Ok(logs) => {
                                                tracing::debug!(
                                                    "RPC: get_logs SUCCESS - {} logs for {}, blocks {}-{}",
                                                    logs.len(),
                                                    contract_name,
                                                    from_block,
                                                    to_block
                                                );
                                            }
                                            Err(e) => {
                                                tracing::debug!(
                                                    "RPC: get_logs FAILED - {}, blocks {}-{}: {}",
                                                    contract_name,
                                                    from_block,
                                                    to_block,
                                                    e
                                                );
                                            }
                                        }

                                        result
                                    }
                                })
                                .await
                        }
                    })
                    .collect();

                // Execute this batch
                let batch_results = futures::future::join_all(futures).await;

                // Process results
                for (idx, result) in batch_results.into_iter().enumerate() {
                    let chunk = chunk_batch[idx].clone();

                    let logs = match result {
                        Ok(logs) => logs,
                        Err(StampError::DataUnavailable(_)) => {
                            tracing::warn!(
                                "Skipping chunk {}-{} for {}: data unavailable (pruned) on all RPC endpoints",
                                chunk.from_block,
                                chunk.to_block,
                                chunk.contract_name
                            );
                            // Cache this chunk as having 0 events
                            cache
                                .cache_chunk(
                                    &chunk.chunk_hash,
                                    contract_address,
                                    chunk.from_block,
                                    chunk.to_block,
                                    0,
                                )
                                .await?;
                            Vec::new()
                        }
                        Err(e) => {
                            return Err(e);
                        }
                    };

                    if !logs.is_empty() {
                        tracing::info!(
                            "    Found {} logs from {} in chunk {}/{}",
                            logs.len(),
                            chunk.contract_name,
                            chunk.chunk_num,
                            chunk.total_chunks
                        );
                    } else {
                        tracing::debug!(
                            "    No logs found from {} in chunk {}/{} (blocks {}-{})",
                            chunk.contract_name,
                            chunk.chunk_num,
                            chunk.total_chunks,
                            chunk.from_block,
                            chunk.to_block
                        );
                    }

                    results.push(ChunkResult { chunk, logs });
                }
            }

            Ok(results)
        } else {
            // Single RPC mode: fetch chunks sequentially with retry logic
            tracing::info!(
                "Fetching {} chunks sequentially (single RPC mode)",
                chunks.len()
            );

            let mut results = Vec::with_capacity(chunks.len());

            for chunk in chunks {
                tracing::debug!(
                    "RPC: get_logs(contract={}, from_block={}, to_block={})",
                    chunk.contract_name,
                    chunk.from_block,
                    chunk.to_block
                );

                let provider = &self.provider;
                let filter = &chunk.filter;
                let logs = retry_config
                    .execute(|| async { provider.get_logs(filter).await })
                    .await
                    .map_err(StampError::Rpc)?;

                if !logs.is_empty() {
                    tracing::info!(
                        "    Found {} logs from {} in chunk {}/{}",
                        logs.len(),
                        chunk.contract_name,
                        chunk.chunk_num,
                        chunk.total_chunks
                    );
                } else {
                    tracing::debug!(
                        "    No logs found from {} in chunk {}/{} (blocks {}-{})",
                        chunk.contract_name,
                        chunk.chunk_num,
                        chunk.total_chunks,
                        chunk.from_block,
                        chunk.to_block
                    );
                }

                results.push(ChunkResult {
                    chunk: chunk.clone(),
                    logs,
                });
            }

            Ok(results)
        }
    }

    /// Get actual block range, adjusting for contract deployment
    ///
    /// Returns (adjusted_from_block, actual_to_block) or None if range is before deployment
    async fn get_block_range(
        &self,
        from_block: u64,
        to_block: u64,
        deployment_block: u64,
    ) -> Result<Option<(u64, u64)>> {
        // Determine the actual to_block
        let actual_to_block = if to_block == u64::MAX {
            tracing::debug!("RPC: get_block_number()");
            self.provider
                .get_block_number()
                .await
                .map_err(|e| StampError::Rpc(format!("Failed to get latest block: {e}")))?
        } else {
            to_block
        };

        // Adjust from_block to not start before deployment
        let adjusted_from_block = std::cmp::max(from_block, deployment_block);

        // Skip if the requested range is entirely before deployment
        if adjusted_from_block > actual_to_block {
            return Ok(None);
        }

        Ok(Some((adjusted_from_block, actual_to_block)))
    }

    /// Get block timestamp with caching (memory, database, then RPC)
    async fn get_block_timestamp_cached(
        &self,
        block_number: u64,
        cache: &Cache,
        block_cache: &mut HashMap<u64, Block>,
        retry_config: &RetryConfig,
    ) -> Result<DateTime<Utc>> {
        // Check in-memory cache first
        if let Some(cached_block) = block_cache.get(&block_number) {
            tracing::debug!("Block cache HIT (memory) for block {}", block_number);
            let timestamp = cached_block.header.timestamp;
            return Ok(DateTime::from_timestamp(timestamp as i64, 0).unwrap_or_else(Utc::now));
        }

        // Check database cache
        if let Some(db_timestamp) = cache.get_block_timestamp(block_number).await? {
            tracing::debug!("Block cache HIT (database) for block {}", block_number);
            return Ok(DateTime::from_timestamp(db_timestamp, 0).unwrap_or_else(Utc::now));
        }

        tracing::debug!(
            "Block cache MISS - RPC: get_block_by_number(block={})",
            block_number
        );

        // Fetch from RPC
        let fetched_block = if let Some(ref scheduler) = self.scheduler {
            // Multi-RPC mode
            scheduler
                .execute_with_retry(|provider| {
                    let provider = provider.clone();
                    async move {
                        let block = provider
                            .get_block_by_number(block_number.into(), BlockTransactionsKind::Hashes)
                            .await
                            .map_err(|e| StampError::Rpc(format!("Failed to get block: {e}")))?
                            .ok_or_else(|| {
                                StampError::Rpc(format!("Block {block_number} not found"))
                            })?;
                        Ok(block)
                    }
                })
                .await?
        } else {
            // Single RPC mode
            let provider = &self.provider;
            retry_config
                .execute(|| async {
                    let block = provider
                        .get_block_by_number(block_number.into(), BlockTransactionsKind::Hashes)
                        .await
                        .map_err(|e| std::io::Error::other(format!("Failed to get block: {e}")))?
                        .ok_or_else(|| {
                            std::io::Error::new(
                                std::io::ErrorKind::NotFound,
                                format!("Block {block_number} not found"),
                            )
                        })?;
                    Ok::<Block, std::io::Error>(block)
                })
                .await
                .map_err(StampError::Rpc)?
        };

        let timestamp = fetched_block.header.timestamp;
        block_cache.insert(block_number, fetched_block);

        Ok(DateTime::from_timestamp(timestamp as i64, 0).unwrap_or_else(Utc::now))
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

        // Get adjusted block range
        let Some((adjusted_from_block, actual_to_block)) = self
            .get_block_range(from_block, to_block, contract.deployment_block())
            .await?
        else {
            tracing::info!(
                "Skipping {} - contract deployed at block {} (after requested range)",
                contract.name(),
                contract.deployment_block()
            );
            return Ok(Vec::new());
        };

        tracing::info!(
            "Fetching {} events from block {} to {} (contract deployed at {})",
            contract.name(),
            adjusted_from_block,
            actual_to_block,
            contract.deployment_block()
        );

        // Collect chunks to fetch
        let chunks = self
            .collect_chunks_to_fetch(
                contract_address,
                contract.name(),
                adjusted_from_block,
                actual_to_block,
                cache,
                blockchain_config.chunk_size,
                refresh,
            )
            .await?;

        if chunks.is_empty() {
            tracing::info!("All chunks cached for {}", contract.name());
            return Ok(Vec::new());
        }

        // Fetch logs for all chunks (parallel if scheduler, sequential otherwise)
        let chunk_results = self
            .fetch_logs_for_chunks(&chunks, retry_config, cache, contract.address())
            .await?;

        // Process results: parse logs, cache chunks, call callbacks
        let mut events = Vec::new();
        let mut block_cache: HashMap<u64, Block> = HashMap::new();

        for chunk_result in chunk_results {
            let mut chunk_events = Vec::new();

            for log in &chunk_result.logs {
                if let Some(event) = self
                    .parse_log(contract, log.clone(), cache, &mut block_cache, retry_config)
                    .await?
                {
                    chunk_events.push(event.clone());
                    events.push(event);
                }
            }

            // Cache this chunk
            cache
                .cache_chunk(
                    &chunk_result.chunk.chunk_hash,
                    contract.address(),
                    chunk_result.chunk.from_block,
                    chunk_result.chunk.to_block,
                    chunk_events.len(),
                )
                .await?;

            // Call the callback with chunk events for incremental storage
            if !chunk_events.is_empty() {
                on_chunk_complete(chunk_events).await?;
            }
        }

        tracing::info!(
            "Total events from {}: {}",
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

        // Get block timestamp using the helper
        let block_timestamp = self
            .get_block_timestamp_cached(block_number, cache, block_cache, retry_config)
            .await?;

        // Delegate to the contract's parse_log implementation
        contract.parse_log(log, block_number, block_timestamp, transaction_hash, log_index)
    }

    /// Fetch factory deployment events from a payment channel factory contract
    ///
    /// Scans for SimpleSwapDeployed events from the specified factory contract.
    /// The `on_chunk_complete` callback is called after each chunk is fetched to enable
    /// incremental storage.
    ///
    /// If `refresh` is true, cached chunks will be reprocessed.
    /// Uses multi-RPC scheduler when available for parallel fetching.
    #[allow(clippy::too_many_arguments)]
    pub async fn fetch_factory_deployment_events<F, Fut>(
        &self,
        from_block: u64,
        to_block: u64,
        cache: &Cache,
        factory: &dyn crate::contracts::PaymentChannelFactory,
        blockchain_config: &BlockchainConfig,
        retry_config: &RetryConfig,
        refresh: bool,
        on_chunk_complete: F,
    ) -> Result<Vec<crate::events::ChequebookDeployment>>
    where
        F: Fn(Vec<crate::events::ChequebookDeployment>) -> Fut,
        Fut: std::future::Future<Output = Result<()>>,
    {
        let factory_address = Address::from_str(factory.address())
            .map_err(|e| StampError::Contract(format!("Invalid factory address: {e}")))?;

        // Get adjusted block range
        let Some((adjusted_from_block, actual_to_block)) = self
            .get_block_range(from_block, to_block, factory.deployment_block())
            .await?
        else {
            tracing::info!(
                "Skipping {} - factory deployed at block {} (after requested range)",
                factory.name(),
                factory.deployment_block()
            );
            return Ok(Vec::new());
        };

        tracing::info!(
            "Fetching {} deployment events from block {} to {} (factory deployed at {})",
            factory.name(),
            adjusted_from_block,
            actual_to_block,
            factory.deployment_block()
        );

        // Collect chunks to fetch
        let chunks = self
            .collect_chunks_to_fetch(
                factory_address,
                factory.name(),
                adjusted_from_block,
                actual_to_block,
                cache,
                blockchain_config.chunk_size,
                refresh,
            )
            .await?;

        if chunks.is_empty() {
            tracing::info!("All chunks cached for {}", factory.name());
            return Ok(Vec::new());
        }

        // Fetch logs for all chunks (parallel if scheduler, sequential otherwise)
        let chunk_results = self
            .fetch_logs_for_chunks(&chunks, retry_config, cache, factory.address())
            .await?;

        // Process results: parse logs, cache chunks, call callbacks
        let mut deployments = Vec::new();
        let mut block_cache: HashMap<u64, Block> = HashMap::new();

        for chunk_result in chunk_results {
            let mut chunk_deployments = Vec::new();

            for log in chunk_result.logs {
                let block_number = log
                    .block_number
                    .ok_or_else(|| StampError::Parse("Missing block number".to_string()))?;

                let transaction_hash = log
                    .transaction_hash
                    .ok_or_else(|| StampError::Parse("Missing transaction hash".to_string()))?;

                // Get block timestamp using the helper
                let block_timestamp = self
                    .get_block_timestamp_cached(block_number, cache, &mut block_cache, retry_config)
                    .await?;

                // Parse deployment event
                if let Some(deployment) =
                    factory.parse_deployment_event(log, block_number, block_timestamp, transaction_hash)?
                {
                    chunk_deployments.push(deployment.clone());
                    deployments.push(deployment);
                }
            }

            // Cache this chunk
            cache
                .cache_chunk(
                    &chunk_result.chunk.chunk_hash,
                    factory.address(),
                    chunk_result.chunk.from_block,
                    chunk_result.chunk.to_block,
                    chunk_deployments.len(),
                )
                .await?;

            // Call the callback with chunk deployments for incremental storage
            if !chunk_deployments.is_empty() {
                on_chunk_complete(chunk_deployments).await?;
            }
        }

        tracing::info!(
            "Total deployments from {}: {}",
            factory.name(),
            deployments.len()
        );

        Ok(deployments)
    }

    /// Fetch payment channel events from a specific chequebook contract
    ///
    /// Fetches all events (ChequeCashed, ChequeBounced, HardDeposit*, Withdraw) from
    /// an ERC20SimpleSwap chequebook contract. The `on_chunk_complete` callback is called
    /// after each chunk is fetched to enable incremental storage.
    ///
    /// If `refresh` is true, cached chunks will be reprocessed.
    /// Uses multi-RPC scheduler when available for parallel fetching.
    #[allow(clippy::too_many_arguments)]
    pub async fn fetch_chequebook_events<F, Fut>(
        &self,
        from_block: u64,
        to_block: u64,
        cache: &Cache,
        chequebook: &dyn crate::contracts::PaymentChannelContract,
        blockchain_config: &BlockchainConfig,
        retry_config: &RetryConfig,
        refresh: bool,
        on_chunk_complete: F,
    ) -> Result<Vec<crate::events::PaymentChannelEvent>>
    where
        F: Fn(Vec<crate::events::PaymentChannelEvent>) -> Fut,
        Fut: std::future::Future<Output = Result<()>>,
    {
        let chequebook_address = Address::from_str(chequebook.address())
            .map_err(|e| StampError::Contract(format!("Invalid chequebook address: {e}")))?;

        // Get adjusted block range
        let Some((adjusted_from_block, actual_to_block)) = self
            .get_block_range(from_block, to_block, chequebook.deployment_block())
            .await?
        else {
            tracing::debug!(
                "Skipping {} - chequebook deployed at block {} (after requested range)",
                chequebook.address(),
                chequebook.deployment_block()
            );
            return Ok(Vec::new());
        };

        tracing::debug!(
            "Fetching events from chequebook {} from block {} to {} (deployed at {})",
            chequebook.address(),
            adjusted_from_block,
            actual_to_block,
            chequebook.deployment_block()
        );

        // Collect chunks to fetch (use short name for logging since chequebook addresses are long)
        let short_name = &chequebook.address()[..10];
        let chunks = self
            .collect_chunks_to_fetch(
                chequebook_address,
                short_name,
                adjusted_from_block,
                actual_to_block,
                cache,
                blockchain_config.chunk_size,
                refresh,
            )
            .await?;

        if chunks.is_empty() {
            tracing::debug!("All chunks cached for chequebook {}", chequebook.address());
            return Ok(Vec::new());
        }

        // Fetch logs for all chunks (parallel if scheduler, sequential otherwise)
        let chunk_results = self
            .fetch_logs_for_chunks(&chunks, retry_config, cache, chequebook.address())
            .await?;

        // Process results: parse logs, cache chunks, call callbacks
        let mut events = Vec::new();
        let mut block_cache: HashMap<u64, Block> = HashMap::new();

        for chunk_result in chunk_results {
            let mut chunk_events = Vec::new();

            for log in chunk_result.logs {
                let block_number = log
                    .block_number
                    .ok_or_else(|| StampError::Parse("Missing block number".to_string()))?;

                let transaction_hash = log
                    .transaction_hash
                    .ok_or_else(|| StampError::Parse("Missing transaction hash".to_string()))?;

                let log_index = log
                    .log_index
                    .ok_or_else(|| StampError::Parse("Missing log index".to_string()))?;

                // Get block timestamp using the helper
                let block_timestamp = self
                    .get_block_timestamp_cached(block_number, cache, &mut block_cache, retry_config)
                    .await?;

                // Parse payment channel event
                if let Some(event) = chequebook.parse_log(
                    log,
                    block_number,
                    block_timestamp,
                    transaction_hash,
                    log_index,
                )? {
                    chunk_events.push(event.clone());
                    events.push(event);
                }
            }

            // Cache this chunk
            cache
                .cache_chunk(
                    &chunk_result.chunk.chunk_hash,
                    chequebook.address(),
                    chunk_result.chunk.from_block,
                    chunk_result.chunk.to_block,
                    chunk_events.len(),
                )
                .await?;

            // Call the callback with chunk events for incremental storage
            if !chunk_events.is_empty() {
                on_chunk_complete(chunk_events).await?;
            }
        }

        tracing::debug!(
            "Total events from chequebook {}: {}",
            chequebook.address(),
            events.len()
        );

        Ok(events)
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
    /// Uses multi-RPC scheduler when available for parallel fetching.
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

        // Get adjusted block range
        let Some((adjusted_from_block, actual_to_block)) = self
            .get_block_range(from_block, to_block, contract.deployment_block())
            .await?
        else {
            tracing::info!(
                "Skipping {} - contract deployed at block {} (after requested range)",
                contract.name(),
                contract.deployment_block()
            );
            return Ok(Vec::new());
        };

        tracing::info!(
            "Fetching {} events from block {} to {} (contract deployed at {})",
            contract.name(),
            adjusted_from_block,
            actual_to_block,
            contract.deployment_block()
        );

        // Collect chunks to fetch
        let chunks = self
            .collect_chunks_to_fetch(
                contract_address,
                contract.name(),
                adjusted_from_block,
                actual_to_block,
                cache,
                blockchain_config.chunk_size,
                refresh,
            )
            .await?;

        if chunks.is_empty() {
            tracing::info!("All chunks cached for {}", contract.name());
            return Ok(Vec::new());
        }

        // Fetch logs for all chunks (parallel if scheduler, sequential otherwise)
        let chunk_results = self
            .fetch_logs_for_chunks(&chunks, retry_config, cache, contract.address())
            .await?;

        // Process results: parse logs, cache chunks, call callbacks
        let mut events = Vec::new();
        let mut block_cache: HashMap<u64, Block> = HashMap::new();

        for chunk_result in chunk_results {
            let mut chunk_events = Vec::new();

            for log in &chunk_result.logs {
                if let Some(event) = self
                    .parse_storage_incentives_log(
                        contract,
                        log.clone(),
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

            // Cache this chunk
            cache
                .cache_chunk(
                    &chunk_result.chunk.chunk_hash,
                    contract.address(),
                    chunk_result.chunk.from_block,
                    chunk_result.chunk.to_block,
                    chunk_events.len(),
                )
                .await?;

            // Call the callback with chunk events for incremental storage
            if !chunk_events.is_empty() {
                on_chunk_complete(chunk_events).await?;
            }
        }

        tracing::info!(
            "Total events from {}: {}",
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

        // Get block timestamp using the helper
        let block_timestamp = self
            .get_block_timestamp_cached(block_number, cache, block_cache, retry_config)
            .await?;

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

    /// Get balance for a chequebook from the blockchain with retry logic
    ///
    /// Queries the ERC20SimpleSwap contract's balance() function
    pub async fn get_chequebook_balance(
        &self,
        chequebook_address: &str,
        retry_config: &RetryConfig,
    ) -> Result<u128> {
        use crate::contracts::abi::ERC20SimpleSwap;

        let address = Address::from_str(chequebook_address)
            .map_err(|e| StampError::Contract(format!("Invalid chequebook address: {e}")))?;

        tracing::debug!("RPC: balance() for chequebook {}", chequebook_address);

        let contract = ERC20SimpleSwap::new(address, &self.provider);

        // Use retry policy for rate limit handling
        let balance = retry_config
            .execute(|| async {
                contract
                    .balance()
                    .call()
                    .await
                    .map(|b| b._0.to::<u128>())
            })
            .await
            .map_err(StampError::Rpc)?;

        Ok(balance)
    }

    /// Get issuer address for a chequebook from the blockchain with retry logic
    ///
    /// Queries the ERC20SimpleSwap contract's issuer() function
    pub async fn get_chequebook_issuer(
        &self,
        chequebook_address: &str,
        retry_config: &RetryConfig,
    ) -> Result<String> {
        use crate::contracts::abi::ERC20SimpleSwap;

        let address = Address::from_str(chequebook_address)
            .map_err(|e| StampError::Contract(format!("Invalid chequebook address: {e}")))?;

        tracing::debug!("RPC: issuer() for chequebook {}", chequebook_address);

        let contract = ERC20SimpleSwap::new(address, &self.provider);

        // Use retry policy for rate limit handling
        let issuer = retry_config
            .execute(|| async {
                contract
                    .issuer()
                    .call()
                    .await
                    .map(|i| format!("{:?}", i._0))
            })
            .await
            .map_err(StampError::Rpc)?;

        Ok(issuer)
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
