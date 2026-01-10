use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

use crate::{
    batch,
    blockchain::BlockchainClient,
    cache::Cache,
    config::AppConfig,
    contracts::{abi::DEFAULT_START_BLOCK, ContractRegistry, StorageIncentivesContractRegistry},
    display,
    events::EventType,
    export,
    hooks::{EventHook, StubHook},
};

/// Beeport Postage Stamp Statistics Tool
///
/// Track and analyze Swarm postage stamp batch events on Gnosis Chain
#[derive(Parser, Debug)]
#[command(name = "beeport-stamp-stats")]
#[command(version, about, long_about = None)]
pub struct Cli {
    /// Path to configuration file (YAML, TOML, or JSON)
    ///
    /// If not provided, uses default configuration with environment variable overrides.
    /// Config file settings can be overridden by environment variables and CLI arguments.
    #[arg(long, short = 'c', env = "BEEPORT_CONFIG")]
    pub config: Option<PathBuf>,

    /// RPC endpoint URL (overrides config file)
    #[arg(long, env = "RPC_URL")]
    pub rpc_url: Option<String>,

    /// Path to the cache database (SQLite file path or PostgreSQL connection string)
    ///
    /// Examples:
    ///   - SQLite: ./stamp-cache.db
    ///   - PostgreSQL: postgres://user:pass@localhost/stamps
    ///
    /// Overrides config file setting.
    #[arg(long, short = 'd', alias = "database", env = "CACHE_DB")]
    pub cache_db: Option<PathBuf>,

    /// Enable verbose logging (shows all RPC requests)
    #[arg(short = 'v', long)]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Fetch postage stamp events from the blockchain and cache them
    ///
    /// Fetches events from both PostageStamp and StampsRegistry contracts.
    /// By default, starts from block 31,305,656 (PostageStamp contract deployment).
    /// Use --incremental to only fetch new events since the last run.
    Fetch {
        /// Start block number (defaults to block 31,305,656)
        #[arg(long)]
        from_block: Option<u64>,

        /// End block number (defaults to latest)
        #[arg(long)]
        to_block: Option<u64>,

        /// Only fetch new events since last run (resumes from last cached block)
        #[arg(long, default_value = "false")]
        incremental: bool,

        /// Reprocess blocks even if they have been cached (useful after adding new event types)
        #[arg(long, default_value = "false")]
        refresh: bool,

        /// Maximum number of retries for rate-limited requests
        #[arg(long, default_value = "5")]
        max_retries: u32,

        /// Initial delay in milliseconds for exponential backoff (doubles each retry)
        #[arg(long, default_value = "100")]
        initial_delay_ms: u64,
    },

    /// Display summary statistics from cached data
    Summary {
        /// Group statistics by time period
        #[arg(long, default_value = "week")]
        group_by: GroupBy,

        /// Number of months to look back (0 for all time)
        #[arg(long, default_value = "12")]
        months: u32,

        /// Filter by event type
        #[arg(long)]
        event_type: Option<FilterEventType>,

        /// Filter by batch ID (partial match supported)
        #[arg(long)]
        batch_id: Option<String>,

        /// Filter by contract source
        #[arg(long)]
        contract: Option<FilterContract>,
    },

    /// Export cached data to CSV or JSON
    Export {
        /// What to export
        #[arg(long, default_value = "events")]
        data_type: ExportDataType,

        /// Output file path
        #[arg(long)]
        output: PathBuf,

        /// Export format
        #[arg(long, default_value = "json")]
        format: ExportFormat,

        /// Number of months to export (0 for all time)
        #[arg(long, default_value = "0")]
        months: u32,

        /// Filter by event type (for events export)
        #[arg(long)]
        event_type: Option<FilterEventType>,

        /// Filter by batch ID (partial match supported)
        #[arg(long)]
        batch_id: Option<String>,

        /// Filter by contract source
        #[arg(long)]
        contract: Option<FilterContract>,
    },

    /// Follow blockchain for new events in real-time
    Follow {
        /// Poll interval in seconds
        #[arg(long, default_value = "12")]
        poll_interval: u64,

        /// Display events as they arrive
        #[arg(long, default_value = "true")]
        display: bool,
    },

    /// Sync database with blockchain (update with latest events)
    Sync {
        /// Start block number (defaults to last synced block in database)
        #[arg(long)]
        from_block: Option<u64>,

        /// End block number (defaults to latest block)
        #[arg(long)]
        to_block: Option<u64>,

        /// Specific contract to sync (defaults to all contracts)
        #[arg(long)]
        contract: Option<String>,

        /// Reprocess blocks even if they have been cached (useful after adding new event types)
        #[arg(long, default_value = "false")]
        refresh: bool,
    },

    /// Display batch status with TTL and expiry information
    BatchStatus {
        /// Sort results by field
        #[arg(long, default_value = "batch-id")]
        sort_by: BatchStatusSortBy,

        /// Output format
        #[arg(long, default_value = "table")]
        output: OutputFormat,

        /// Override current storage price (PLUR per chunk per block)
        #[arg(long)]
        price: Option<String>,

        /// Expected price change as percentage:days (e.g., "200:10" for 200% in 10 days)
        #[arg(long)]
        price_change: Option<String>,

        /// Refresh balance data from blockchain (otherwise uses cache if available)
        #[arg(long, default_value = "false")]
        refresh: bool,

        /// Only fetch batches that don't have cached balance (useful for retrying failures)
        #[arg(long, default_value = "false")]
        only_missing: bool,

        /// Maximum number of retries for rate-limited requests
        #[arg(long, default_value = "20")]
        max_retries: u32,

        /// Hide batches with zero balance (show only active batches)
        #[arg(long, default_value = "false")]
        hide_zero_balance: bool,

        /// Filter by contract source (postage-stamp or stamps-registry)
        #[arg(long)]
        contract: Option<String>,

        /// Cache validity in blocks (default: 518400 blocks = ~1 month at 5s/block)
        #[arg(long, default_value = "518400")]
        cache_validity_blocks: u64,
    },

    /// Get current storage price from the blockchain
    Price,

    /// Analyze batch expiry patterns over time
    ExpiryAnalytics {
        /// Time period for grouping
        #[arg(long, default_value = "day")]
        period: TimePeriod,

        /// Output format
        #[arg(long, default_value = "table")]
        output: OutputFormat,

        /// Sort results by field
        #[arg(long, default_value = "period")]
        sort_by: ExpiryAnalyticsSortBy,

        /// Override current storage price (PLUR per chunk per block)
        #[arg(long)]
        price: Option<String>,

        /// Expected price change as percentage:days (e.g., "200:10" for 200% in 10 days)
        #[arg(long)]
        price_change: Option<String>,

        /// Refresh balance data from blockchain (otherwise uses cache if available)
        #[arg(long, default_value = "false")]
        refresh: bool,

        /// Maximum number of retries for rate-limited requests
        #[arg(long, default_value = "20")]
        max_retries: u32,

        /// Cache validity in blocks (default: 518400 blocks = ~1 month at 5s/block)
        #[arg(long, default_value = "518400")]
        cache_validity_blocks: u64,
    },
}

#[derive(Debug, Clone, clap::ValueEnum)]
pub enum GroupBy {
    Day,
    Week,
    Month,
}

#[derive(Debug, Clone, clap::ValueEnum)]
#[allow(clippy::enum_variant_names)]
pub enum FilterEventType {
    BatchCreated,
    BatchTopUp,
    BatchDepthIncrease,
}

impl FilterEventType {
    fn matches(&self, event_type: &EventType) -> bool {
        matches!(
            (self, event_type),
            (FilterEventType::BatchCreated, EventType::BatchCreated)
                | (FilterEventType::BatchTopUp, EventType::BatchTopUp)
                | (
                    FilterEventType::BatchDepthIncrease,
                    EventType::BatchDepthIncrease
                )
        )
    }
}

#[derive(Debug, Clone, clap::ValueEnum)]
pub enum FilterContract {
    PostageStamp,
    StampsRegistry,
}

impl FilterContract {
    fn matches(&self, contract_source: &str) -> bool {
        matches!(
            (self, contract_source),
            (FilterContract::PostageStamp, "PostageStamp")
                | (FilterContract::StampsRegistry, "StampsRegistry")
        )
    }
}

#[derive(Debug, Clone, clap::ValueEnum)]
pub enum ExportDataType {
    Events,
    Batches,
    Stats,
}

#[derive(Debug, Clone, clap::ValueEnum)]
pub enum ExportFormat {
    Csv,
    Json,
}

#[derive(Debug, Clone, clap::ValueEnum)]
pub enum OutputFormat {
    Table,
    Csv,
    Json,
}

#[derive(Debug, Clone, clap::ValueEnum)]
pub enum TimePeriod {
    Day,
    Week,
    Month,
}

#[derive(Debug, Clone, clap::ValueEnum)]
pub enum BatchStatusSortBy {
    BatchId,
    Depth,
    Ttl,
    Expiry,
    Size,
}

#[derive(Debug, Clone, clap::ValueEnum)]
pub enum ExpiryAnalyticsSortBy {
    Period,
    Chunks,
    Storage,
}

impl From<ExportFormat> for export::ExportFormat {
    fn from(format: ExportFormat) -> Self {
        match format {
            ExportFormat::Csv => export::ExportFormat::Csv,
            ExportFormat::Json => export::ExportFormat::Json,
        }
    }
}

impl Cli {
    /// Resolve configuration from multiple sources with proper priority
    ///
    /// Priority: CLI args > Environment vars > Config file > Defaults
    fn resolve_config(&self) -> Result<AppConfig> {
        // Load base config (from file or defaults)
        let mut config = if let Some(config_path) = &self.config {
            AppConfig::load_from_file(config_path)?
        } else {
            AppConfig::load()?
        };

        // Apply CLI overrides
        if let Some(rpc_url) = &self.rpc_url {
            config.rpc.url = rpc_url.clone();
        }

        if let Some(cache_db) = &self.cache_db {
            config.database.path = cache_db.to_string_lossy().to_string();
        }

        // Validate config
        config.validate().map_err(|e| anyhow::anyhow!(e))?;

        Ok(config)
    }

    pub async fn execute(&self) -> Result<()> {
        // Resolve configuration
        let config = self.resolve_config()?;

        // Build contract registries from configuration
        let registry = ContractRegistry::from_config(&config)?;
        let si_registry = StorageIncentivesContractRegistry::from_config(&config)?;

        // Initialize blockchain client
        let client = BlockchainClient::new(&config.rpc.url).await?;

        // Initialize cache
        let cache = Cache::new(&PathBuf::from(&config.database.path)).await?;

        match &self.command {
            Commands::Fetch {
                from_block,
                to_block,
                incremental,
                refresh,
                max_retries: _,  // Ignored, use config
                initial_delay_ms: _,  // Ignored, use config
            } => {
                self.execute_fetch(
                    cache,
                    client,
                    &registry,
                    &si_registry,
                    &config,
                    *from_block,
                    *to_block,
                    *incremental,
                    *refresh,
                )
                .await
            }
            Commands::Summary {
                group_by,
                months,
                event_type,
                batch_id,
                contract,
            } => {
                self.execute_summary(
                    cache,
                    group_by.clone(),
                    *months,
                    event_type.clone(),
                    batch_id.clone(),
                    contract.clone(),
                )
                .await
            }
            Commands::Export {
                data_type,
                output,
                format,
                months,
                event_type,
                batch_id,
                contract,
            } => {
                self.execute_export(
                    cache,
                    data_type.clone(),
                    output,
                    format.clone(),
                    *months,
                    event_type.clone(),
                    batch_id.clone(),
                    contract.clone(),
                )
                .await
            }
            Commands::Follow {
                poll_interval,
                display,
            } => {
                self.execute_follow(cache, client, &registry, &config, *poll_interval, *display)
                    .await
            }
            Commands::Sync {
                from_block,
                to_block,
                contract,
                refresh,
            } => {
                self.execute_sync(
                    cache,
                    client,
                    &registry,
                    &config,
                    *from_block,
                    *to_block,
                    contract.clone(),
                    *refresh,
                )
                .await
            }
            Commands::Price => self.execute_price(client, &registry).await,
            Commands::BatchStatus {
                sort_by,
                output,
                price,
                price_change,
                refresh,
                only_missing,
                max_retries: _,  // Ignored, use config
                hide_zero_balance,
                contract,
                cache_validity_blocks,
            } => {
                self.execute_batch_status(
                    cache,
                    client,
                    &registry,
                    &config,
                    sort_by.clone(),
                    output.clone(),
                    price.clone(),
                    price_change.clone(),
                    *refresh,
                    *only_missing,
                    *hide_zero_balance,
                    contract.clone(),
                    *cache_validity_blocks,
                )
                .await
            }
            Commands::ExpiryAnalytics {
                period,
                output,
                sort_by,
                price,
                price_change,
                refresh,
                max_retries: _,  // Ignored, use config
                cache_validity_blocks,
            } => {
                self.execute_expiry_analytics(
                    cache,
                    client,
                    &registry,
                    &config,
                    period.clone(),
                    output.clone(),
                    sort_by.clone(),
                    price.clone(),
                    price_change.clone(),
                    *refresh,
                    *cache_validity_blocks,
                )
                .await
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn execute_fetch(
        &self,
        cache: Cache,
        client: BlockchainClient,
        registry: &ContractRegistry,
        si_registry: &StorageIncentivesContractRegistry,
        config: &AppConfig,
        from_block: Option<u64>,
        to_block: Option<u64>,
        incremental: bool,
        refresh: bool,
    ) -> Result<()> {
        tracing::info!("Fetching events from blockchain...");

        // Determine block range
        let from = if incremental {
            cache.get_last_block().await?.map(|b| b + 1)
        } else {
            from_block
        }
        .unwrap_or(DEFAULT_START_BLOCK);

        let to = to_block.unwrap_or({
            // We'll get latest block from the client
            u64::MAX
        });

        tracing::info!(
            "Fetching events from block {} to {}",
            from,
            if to == u64::MAX {
                "latest".to_string()
            } else {
                to.to_string()
            }
        );

        // Fetch and display postage stamp events with incremental storage
        let cache_clone = cache.clone();
        let client_clone = client.clone();
        let events = client
            .fetch_batch_events(
                from,
                to,
                &cache,
                registry,
                &config.blockchain,
                &config.retry,
                refresh,
                |chunk_events: Vec<crate::events::StampEvent>| {
                    let cache = cache_clone.clone();
                    let client = client_clone.clone();
                    async move {
                        // Store events from this chunk
                        cache.store_events(&chunk_events).await?;

                        // Store batch info for BatchCreated events in this chunk
                        let batches = client.fetch_batch_info(&chunk_events).await?;
                        cache.store_batches(&batches).await?;

                        tracing::debug!(
                            "Stored {} postage stamp events and {} batches from chunk",
                            chunk_events.len(),
                            batches.len()
                        );

                        Ok(())
                    }
                },
            )
            .await?;

        tracing::info!("Found {} total postage stamp events", events.len());

        // Fetch and display storage incentives events with incremental storage
        let cache_clone = cache.clone();
        let si_events = client
            .fetch_storage_incentives_events(
                from,
                to,
                &cache,
                si_registry,
                &config.blockchain,
                &config.retry,
                refresh,
                |chunk_events: Vec<crate::events::StorageIncentivesEvent>| {
                    let cache = cache_clone.clone();
                    async move {
                        // Store storage incentives events from this chunk
                        cache.store_storage_incentives_events(&chunk_events).await?;

                        tracing::debug!(
                            "Stored {} storage incentives events from chunk",
                            chunk_events.len()
                        );

                        Ok(())
                    }
                },
            )
            .await?;

        tracing::info!("Found {} total storage incentives events", si_events.len());

        // Display postage stamp events in markdown table
        display::display_events(&events)?;

        // TODO: Display storage incentives events (for now just log count)
        tracing::info!("Storage incentives events: {} (not displayed yet)", si_events.len());

        Ok(())
    }

    async fn execute_summary(
        &self,
        cache: Cache,
        group_by: GroupBy,
        months: u32,
        event_type_filter: Option<FilterEventType>,
        batch_id_filter: Option<String>,
        contract_filter: Option<FilterContract>,
    ) -> Result<()> {
        tracing::info!("Generating summary from cached data...");

        // Retrieve events from cache
        let mut events = cache.get_events(months).await?;
        let mut batches = cache.get_batches(months).await?;

        // Apply filters
        if let Some(ref filter) = event_type_filter {
            let before = events.len();
            events.retain(|e| filter.matches(&e.event_type));
            tracing::info!("Event type filter: {} -> {} events", before, events.len());
        }

        if let Some(ref filter) = batch_id_filter {
            let before = events.len();
            events.retain(|e| e.batch_id.as_ref().is_some_and(|id| id.contains(filter)));
            tracing::info!("Batch ID filter: {} -> {} events", before, events.len());

            batches.retain(|b| b.batch_id.contains(filter));
        }

        if let Some(ref filter) = contract_filter {
            let before = events.len();
            events.retain(|e| filter.matches(&e.contract_source));
            tracing::info!("Contract filter: {} -> {} events", before, events.len());
        }

        tracing::info!(
            "Loaded {} events and {} batches from cache",
            events.len(),
            batches.len()
        );

        // Display summary
        display::display_summary(&events, &batches, group_by)?;

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn execute_export(
        &self,
        cache: Cache,
        data_type: ExportDataType,
        output: &PathBuf,
        format: ExportFormat,
        months: u32,
        event_type_filter: Option<FilterEventType>,
        batch_id_filter: Option<String>,
        contract_filter: Option<FilterContract>,
    ) -> Result<()> {
        tracing::info!("Exporting data to {:?}...", output);

        let export_format = format.into();

        match data_type {
            ExportDataType::Events => {
                let mut events = cache.get_events(months).await?;

                // Apply filters
                if let Some(ref filter) = event_type_filter {
                    events.retain(|e| filter.matches(&e.event_type));
                }

                if let Some(ref filter) = batch_id_filter {
                    events.retain(|e| e.batch_id.as_ref().is_some_and(|id| id.contains(filter)));
                }

                if let Some(ref filter) = contract_filter {
                    events.retain(|e| filter.matches(&e.contract_source));
                }

                tracing::info!("Exporting {} events", events.len());
                export::export_events(&events, output, export_format)?;
            }
            ExportDataType::Batches => {
                let mut batches = cache.get_batches(months).await?;

                // Apply batch ID filter
                if let Some(ref filter) = batch_id_filter {
                    batches.retain(|b| b.batch_id.contains(filter));
                }

                tracing::info!("Exporting {} batches", batches.len());
                export::export_batches(&batches, output, export_format)?;
            }
            ExportDataType::Stats => {
                let mut events = cache.get_events(months).await?;

                // Apply filters
                if let Some(ref filter) = event_type_filter {
                    events.retain(|e| filter.matches(&e.event_type));
                }

                if let Some(ref filter) = batch_id_filter {
                    events.retain(|e| e.batch_id.as_ref().is_some_and(|id| id.contains(filter)));
                }

                if let Some(ref filter) = contract_filter {
                    events.retain(|e| filter.matches(&e.contract_source));
                }

                // Group by week for stats export (could be made configurable)
                let stats = batch::aggregate_events(&events, &GroupBy::Week);

                tracing::info!("Exporting {} period statistics", stats.len());
                export::export_stats(&stats, output, export_format)?;
            }
        }

        println!("✅ Exported to: {}", output.display());

        Ok(())
    }

    async fn execute_follow(
        &self,
        cache: Cache,
        client: BlockchainClient,
        registry: &ContractRegistry,
        config: &AppConfig,
        poll_interval: u64,
        display: bool,
    ) -> Result<()> {
        use tokio::time::{Duration, interval};

        tracing::info!("Starting follow mode with {}s poll interval", poll_interval);

        // Create event hook
        let hook = StubHook;

        // First, ensure historical sync
        let last_synced_block = cache.get_last_block().await?.unwrap_or(DEFAULT_START_BLOCK);
        tracing::info!(
            "Last synced block: {} - catching up to latest...",
            last_synced_block
        );

        // Fetch all events up to current block with incremental storage
        let cache_clone = cache.clone();
        let client_clone = client.clone();
        let latest_block = client
            .fetch_batch_events(
                last_synced_block + 1,
                u64::MAX,
                &cache,
                registry,
                &config.blockchain,
                &config.retry,
                false, // Don't refresh in follow mode - always fetching new events
                |chunk_events: Vec<crate::events::StampEvent>| {
                    let cache = cache_clone.clone();
                    let client = client_clone.clone();
                    async move {
                        // Store events from this chunk
                        cache.store_events(&chunk_events).await?;

                        // Store batch info for BatchCreated events in this chunk
                        let batches = client.fetch_batch_info(&chunk_events).await?;
                        cache.store_batches(&batches).await?;

                        Ok(())
                    }
                },
            )
            .await?;
        let current_latest = if !latest_block.is_empty() {
            latest_block.last().unwrap().block_number
        } else {
            last_synced_block
        };

        if !latest_block.is_empty() {
            tracing::info!(
                "Historical sync: found {} events from block {} to {}",
                latest_block.len(),
                last_synced_block + 1,
                current_latest
            );

            if display {
                display::display_events(&latest_block)?;
            }
        } else {
            tracing::info!("Already up to date at block {}", last_synced_block);
        }

        println!(
            "\n🔄 Following blockchain for new events (polling every {poll_interval}s)..."
        );
        println!("Press Ctrl+C to stop\n");

        // Now follow for new events
        let mut poll_timer = interval(Duration::from_secs(poll_interval));
        let mut last_checked_block = current_latest;

        loop {
            poll_timer.tick().await;

            // Fetch new events since last check with incremental storage
            let cache_clone = cache.clone();
            let client_clone = client.clone();
            let new_events = client
                .fetch_batch_events(
                    last_checked_block + 1,
                    u64::MAX,
                    &cache,
                    registry,
                    &config.blockchain,
                    &config.retry,
                    false, // Don't refresh in follow mode - always fetching new events
                    |chunk_events| {
                        let cache = cache_clone.clone();
                        let client = client_clone.clone();
                        async move {
                            // Store events from this chunk
                            cache.store_events(&chunk_events).await?;

                            // Store batch info for BatchCreated events in this chunk
                            let batches = client.fetch_batch_info(&chunk_events).await?;
                            cache.store_batches(&batches).await?;

                            Ok(())
                        }
                    },
                )
                .await?;

            if !new_events.is_empty() {
                tracing::info!("Found {} new events", new_events.len());

                // Invoke hooks for each new event
                for event in &new_events {
                    hook.on_event(event);
                }

                // Display if requested
                if display {
                    display::display_events(&new_events)?;
                }

                // Update last checked block
                last_checked_block = new_events.last().unwrap().block_number;

                println!(
                    "✅ Processed {} new events (now at block {})\n",
                    new_events.len(),
                    last_checked_block
                );
            } else {
                tracing::debug!("No new events at block {}", last_checked_block);
            }
        }
    }

    async fn execute_price(
        &self,
        client: BlockchainClient,
        registry: &ContractRegistry,
    ) -> Result<()> {
        tracing::info!("Querying current storage price from blockchain...");

        let price = client.get_current_price(registry).await?;
        let current_block = client.get_current_block().await?;

        println!("\n📊 Current Storage Price\n");
        println!("Price per chunk per block: {} PLUR", format_number(price));
        println!("Current block: {}", format_number(current_block as u128));
        println!("\nThis price is used to calculate batch TTL (Time To Live).");
        println!("Use --price {price} with batch-status or expiry-analytics commands.");

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn execute_sync(
        &self,
        cache: Cache,
        client: BlockchainClient,
        registry: &ContractRegistry,
        config: &AppConfig,
        from_block: Option<u64>,
        to_block: Option<u64>,
        _contract: Option<String>,
        refresh: bool,
    ) -> Result<()> {
        tracing::info!("Syncing database with blockchain...");

        // Determine start block
        let from = from_block
            .or_else(|| {
                // Get last synced block from cache
                futures::executor::block_on(cache.get_last_block())
                    .ok()
                    .flatten()
                    .map(|b| b + 1)
            })
            .unwrap_or(DEFAULT_START_BLOCK);

        let to = to_block.unwrap_or(u64::MAX);

        tracing::info!(
            "Syncing from block {} to {}",
            from,
            if to == u64::MAX {
                "latest".to_string()
            } else {
                to.to_string()
            }
        );

        // Fetch events with incremental storage
        let cache_clone = cache.clone();
        let client_clone = client.clone();
        let events = client
            .fetch_batch_events(
                from,
                to,
                &cache,
                registry,
                &config.blockchain,
                &config.retry,
                refresh,
                |chunk_events: Vec<crate::events::StampEvent>| {
                    let cache = cache_clone.clone();
                    let client = client_clone.clone();
                    async move {
                        // Store events from this chunk
                        cache.store_events(&chunk_events).await?;

                        // Store batch info for BatchCreated events in this chunk
                        let batches = client.fetch_batch_info(&chunk_events).await?;
                        cache.store_batches(&batches).await?;

                        Ok(())
                    }
                },
            )
            .await?;

        if events.is_empty() {
            println!("✅ Database is already up to date!");
            return Ok(());
        }

        tracing::info!("Found {} new events", events.len());

        // Count batches for display (already stored incrementally)
        let batch_count = events.iter().filter(|e| matches!(e.event_type, crate::events::EventType::BatchCreated)).count();

        // Cache the current price
        let current_price = client.get_current_price(registry).await?;
        cache.cache_price(current_price).await?;

        println!(
            "✅ Synced {} events and {} batches to database",
            events.len(),
            batch_count
        );
        println!("💰 Cached current price: {current_price} PLUR/chunk/block");

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn execute_batch_status(
        &self,
        cache: Cache,
        client: BlockchainClient,
        registry: &ContractRegistry,
        config: &AppConfig,
        sort_by: BatchStatusSortBy,
        output: OutputFormat,
        price: Option<String>,
        price_change: Option<String>,
        refresh: bool,
        only_missing: bool,
        hide_zero_balance: bool,
        contract: Option<String>,
        cache_validity_blocks: u64,
    ) -> Result<()> {
        crate::commands::batch_status::execute(
            cache,
            &client,
            registry,
            config,
            sort_by,
            output,
            price,
            price_change,
            refresh,
            only_missing,
            hide_zero_balance,
            contract,
            cache_validity_blocks,
        )
        .await
        .map_err(|e| anyhow::anyhow!(e))
    }

    #[allow(clippy::too_many_arguments)]
    async fn execute_expiry_analytics(
        &self,
        cache: Cache,
        client: BlockchainClient,
        registry: &ContractRegistry,
        config: &AppConfig,
        period: TimePeriod,
        output: OutputFormat,
        sort_by: ExpiryAnalyticsSortBy,
        price: Option<String>,
        price_change: Option<String>,
        refresh: bool,
        cache_validity_blocks: u64,
    ) -> Result<()> {
        crate::commands::expiry_analytics::execute(
            cache,
            &client,
            registry,
            config,
            period,
            output,
            sort_by,
            price,
            price_change,
            refresh,
            cache_validity_blocks,
        )
        .await
        .map_err(|e| anyhow::anyhow!(e))
    }
}

/// Format large numbers with thousand separators
fn format_number(n: u128) -> String {
    let s = n.to_string();
    let mut result = String::new();
    let len = s.len();

    for (i, c) in s.chars().enumerate() {
        if i > 0 && (len - i) % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_parsing() {
        let cli = Cli::parse_from([
            "beeport-stamp-stats",
            "--rpc-url",
            "http://localhost:8545",
            "fetch",
            "--from-block",
            "1000000",
        ]);

        assert_eq!(cli.rpc_url, Some("http://localhost:8545".to_string()));
        match cli.command {
            Commands::Fetch { from_block, .. } => {
                assert_eq!(from_block, Some(1000000));
            }
            _ => panic!("Expected Fetch command"),
        }
    }

    #[test]
    fn test_summary_parsing() {
        let cli = Cli::parse_from([
            "beeport-stamp-stats",
            "summary",
            "--group-by",
            "month",
            "--months",
            "6",
        ]);

        match cli.command {
            Commands::Summary { months, .. } => {
                assert_eq!(months, 6);
            }
            _ => panic!("Expected Summary command"),
        }
    }
}
