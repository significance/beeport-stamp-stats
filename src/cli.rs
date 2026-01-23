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

    /// Analyze addresses involved in stamp purchases
    ///
    /// Shows unique addresses (owners, payers, transaction senders) and their activity.
    /// Identifies when owner/payer/from addresses differ.
    AddressSummary {
        /// Output format
        #[arg(long, default_value = "table")]
        output: OutputFormat,

        /// Minimum number of stamps to include address
        #[arg(long, default_value = "1")]
        min_stamps: u32,

        /// Show only addresses where owner != from_address
        #[arg(long, default_value = "false")]
        show_delegated_only: bool,

        /// Filter by role (owner, sender, or owner-and-sender)
        #[arg(long)]
        role: Option<RoleFilter>,

        /// Show only addresses with live (non-expired) stamps
        #[arg(long, default_value = "false")]
        live_only: bool,

        /// Override current storage price for TTL calculations (PLUR per chunk per block)
        #[arg(long)]
        price: Option<String>,

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

    /// Show database migration status
    ///
    /// Displays which migrations have been applied to the database.
    Migrations,

    /// Drop and recreate the database (DESTRUCTIVE)
    ///
    /// This will permanently delete all data in the database and recreate it.
    /// Requires confirmation before proceeding.
    Reset,

    /// Discover chequebooks deployed from SimpleSwapFactory contracts
    ///
    /// Scans factory contracts for SimpleSwapDeployed events and stores
    /// discovered chequebook addresses in the database for subsequent tracking.
    DiscoverChequebooks {
        /// Start block number (defaults to factory deployment block)
        #[arg(long)]
        from_block: Option<u64>,

        /// End block number (defaults to latest)
        #[arg(long)]
        to_block: Option<u64>,

        /// Reprocess blocks even if they have been cached
        #[arg(long, default_value = "false")]
        refresh: bool,

        /// Specific factory to scan (defaults to all active factories)
        #[arg(long)]
        factory: Option<String>,
    },

    /// Sync events from discovered chequebook contracts
    ///
    /// Fetches all events (ChequeCashed, ChequeBounced, HardDeposit*, Withdraw)
    /// from chequebooks discovered via the discover-chequebooks command.
    SyncChequebooks {
        /// Start block number (defaults to chequebook deployment block)
        #[arg(long)]
        from_block: Option<u64>,

        /// End block number (defaults to latest)
        #[arg(long)]
        to_block: Option<u64>,

        /// Reprocess blocks even if they have been cached
        #[arg(long, default_value = "false")]
        refresh: bool,

        /// Specific chequebook address to sync (defaults to all discovered chequebooks)
        #[arg(long)]
        chequebook: Option<String>,
    },

    /// Analyze cheque cashing activity per chequebook
    ///
    /// Shows total cheques cashed and amounts per chequebook over a specified block range.
    ChequeSummary {
        /// Start block number
        #[arg(long)]
        from_block: Option<u64>,

        /// End block number (defaults to latest)
        #[arg(long)]
        to_block: Option<u64>,

        /// Output format
        #[arg(long, default_value = "table")]
        output: OutputFormat,
    },

    /// Export chequebook addresses with current balances
    ///
    /// Retrieves all chequebook addresses and their current balances from the blockchain.
    ChequebookBalances {
        /// Output format
        #[arg(long, default_value = "table")]
        output: OutputFormat,

        /// Refresh balance data from blockchain (otherwise uses cache if available)
        #[arg(long, default_value = "false")]
        refresh: bool,
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

#[derive(Debug, Clone, clap::ValueEnum)]
pub enum RoleFilter {
    Owner,
    Sender,
    OwnerAndSender,
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
        // Handle reset command early (before connecting to database)
        if matches!(&self.command, Commands::Reset) {
            return self.execute_reset().await;
        }

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
            Commands::AddressSummary {
                output,
                min_stamps,
                show_delegated_only,
                role,
                live_only,
                price,
                refresh,
                max_retries: _,  // Ignored, use config
                cache_validity_blocks,
            } => {
                self.execute_address_summary(
                    cache,
                    client,
                    &registry,
                    &config,
                    output.clone(),
                    *min_stamps,
                    *show_delegated_only,
                    role.clone(),
                    *live_only,
                    price.clone(),
                    *refresh,
                    *cache_validity_blocks,
                )
                .await
            }
            Commands::Migrations => self.execute_migrations(cache).await,
            Commands::Reset => unreachable!("Reset command handled early"),
            Commands::DiscoverChequebooks {
                from_block,
                to_block,
                refresh,
                factory,
            } => {
                self.execute_discover_chequebooks(
                    cache,
                    client,
                    &config,
                    *from_block,
                    *to_block,
                    *refresh,
                    factory.clone(),
                )
                .await
            }
            Commands::SyncChequebooks {
                from_block,
                to_block,
                refresh,
                chequebook,
            } => {
                self.execute_sync_chequebooks(
                    cache,
                    client,
                    &config,
                    *from_block,
                    *to_block,
                    *refresh,
                    chequebook.clone(),
                )
                .await
            }
            Commands::ChequeSummary {
                from_block,
                to_block,
                output,
            } => {
                self.execute_cheque_summary(cache, *from_block, *to_block, output.clone())
                    .await
            }
            Commands::ChequebookBalances { output, refresh } => {
                self.execute_chequebook_balances(cache, client, &config, output.clone(), *refresh)
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
        let retry_config = config.retry.clone();
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
                    let retry = retry_config.clone();
                    async move {
                        // Populate from_address for this chunk
                        let mut events_with_from = chunk_events;
                        client.populate_from_addresses(&mut events_with_from, &retry).await?;

                        // Store events from this chunk (with from_address populated)
                        cache.store_events(&events_with_from).await?;

                        // Store batch info for BatchCreated events in this chunk
                        let batches = client.fetch_batch_info(&events_with_from).await?;
                        cache.store_batches(&batches).await?;

                        tracing::debug!(
                            "Stored {} postage stamp events and {} batches from chunk",
                            events_with_from.len(),
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
        let retry_config = config.retry.clone();
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
                    let retry = retry_config.clone();
                    async move {
                        // Populate from_address for this chunk
                        let mut events_with_from = chunk_events;
                        client.populate_from_addresses(&mut events_with_from, &retry).await?;

                        // Store events from this chunk (with from_address populated)
                        cache.store_events(&events_with_from).await?;

                        // Store batch info for BatchCreated events in this chunk
                        let batches = client.fetch_batch_info(&events_with_from).await?;
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

    #[allow(clippy::too_many_arguments)]
    async fn execute_address_summary(
        &self,
        cache: Cache,
        client: BlockchainClient,
        registry: &ContractRegistry,
        config: &AppConfig,
        output: OutputFormat,
        min_stamps: u32,
        show_delegated_only: bool,
        role: Option<RoleFilter>,
        live_only: bool,
        price: Option<String>,
        refresh: bool,
        cache_validity_blocks: u64,
    ) -> Result<()> {
        crate::commands::address_summary::execute(
            cache,
            &client,
            registry,
            config,
            output,
            min_stamps,
            show_delegated_only,
            role,
            live_only,
            price,
            refresh,
            cache_validity_blocks,
        )
        .await
        .map_err(|e| anyhow::anyhow!(e))
    }

    async fn execute_discover_chequebooks(
        &self,
        cache: Cache,
        client: BlockchainClient,
        config: &AppConfig,
        from_block: Option<u64>,
        to_block: Option<u64>,
        refresh: bool,
        factory_filter: Option<String>,
    ) -> Result<()> {
        use crate::contracts::impls::SimpleSwapFactoryContract;

        tracing::info!("Discovering chequebooks from factory contracts...");

        // Filter factories by active status and optional name filter
        let factories: Vec<_> = config
            .payment_channel_factories
            .iter()
            .filter(|f| {
                // Apply name filter if specified
                if let Some(ref filter) = factory_filter {
                    if !f.name.contains(filter) {
                        return false;
                    }
                }
                // Only scan active factories unless specific factory requested
                factory_filter.is_some() || f.active
            })
            .collect();

        if factories.is_empty() {
            if factory_filter.is_some() {
                return Err(anyhow::anyhow!(
                    "No factory found matching filter '{}'",
                    factory_filter.unwrap()
                ));
            } else {
                println!("⚠️ No active payment channel factories configured.");
                println!("   Enable factories in config.yaml or use --factory to specify one.");
                return Ok(());
            }
        }

        println!(
            "📡 Scanning {} factory contract{}...\n",
            factories.len(),
            if factories.len() == 1 { "" } else { "s" }
        );

        let mut total_discovered = 0;

        // Process each factory
        for factory_config in factories {
            println!("🏭 Factory: {}", factory_config.name);
            println!("   Address: {}", factory_config.address);
            println!("   Network: {}", factory_config.network);

            // Create factory contract instance
            let factory = SimpleSwapFactoryContract::new(
                factory_config.address.clone(),
                factory_config.deployment_block,
                factory_config.name.clone(),
            );

            // Determine block range
            let from = from_block.unwrap_or(factory_config.deployment_block);
            let to = to_block.unwrap_or(u64::MAX);

            tracing::info!(
                "Scanning factory '{}' from block {} to {}",
                factory_config.name,
                from,
                if to == u64::MAX {
                    "latest".to_string()
                } else {
                    to.to_string()
                }
            );

            // Fetch deployment events with incremental storage
            let cache_clone = cache.clone();
            let client_clone = client.clone();
            let retry_config = config.retry.clone();
            let deployments = client
                .fetch_factory_deployment_events(
                    from,
                    to,
                    &cache,
                    &factory,
                    &config.blockchain,
                    &config.retry,
                    refresh,
                    |chunk_deployments| {
                        let cache = cache_clone.clone();
                        let client = client_clone.clone();
                        let retry = retry_config.clone();
                        async move {
                            // Store deployments from this chunk immediately
                            // Also populate issuer address from RPC
                            for deployment in &chunk_deployments {
                                // First store the deployment
                                cache.store_chequebook_deployment(deployment).await?;

                                // Then query and update issuer address
                                match client.get_chequebook_issuer(&deployment.chequebook_address, &retry).await {
                                    Ok(issuer) => {
                                        tracing::debug!("Got issuer {} for chequebook {}", issuer, deployment.chequebook_address);
                                        cache.update_chequebook_issuer(&deployment.chequebook_address, &issuer).await?;
                                    }
                                    Err(e) => {
                                        tracing::warn!("Failed to get issuer for {}: {}", deployment.chequebook_address, e);
                                    }
                                }
                            }

                            tracing::debug!(
                                "Stored {} chequebook deployments from chunk",
                                chunk_deployments.len()
                            );

                            Ok(())
                        }
                    },
                )
                .await?;

            total_discovered += deployments.len();

            if deployments.is_empty() {
                println!("   ℹ️  No new deployments found\n");
            } else {
                println!(
                    "   ✅ Discovered {} chequebook{}\n",
                    deployments.len(),
                    if deployments.len() == 1 { "" } else { "s" }
                );

                // Show first few discovered addresses
                let show_count = deployments.len().min(5);
                for deployment in deployments.iter().take(show_count) {
                    println!("      • {}", deployment.chequebook_address);
                }

                if deployments.len() > show_count {
                    println!("      ... and {} more", deployments.len() - show_count);
                }
                println!();
            }
        }

        // Summary statistics
        let total_stored = cache.count_chequebooks().await?;
        println!("📊 Discovery Complete");
        println!("   New discoveries: {total_discovered}");
        println!("   Total in database: {total_stored}");

        Ok(())
    }

    async fn execute_sync_chequebooks(
        &self,
        cache: Cache,
        client: BlockchainClient,
        config: &AppConfig,
        from_block: Option<u64>,
        to_block: Option<u64>,
        refresh: bool,
        chequebook_filter: Option<String>,
    ) -> Result<()> {
        use crate::contracts::impls::ERC20SimpleSwapContract;

        tracing::info!("Syncing events from discovered chequebooks...");

        // Load all discovered chequebooks from database
        let mut chequebooks = cache.get_discovered_chequebooks().await?;

        if chequebooks.is_empty() {
            println!("⚠️ No chequebooks found in database.");
            println!("   Run 'discover-chequebooks' first to discover chequebook contracts.");
            return Ok(());
        }

        // Apply chequebook filter if specified
        if let Some(ref filter) = chequebook_filter {
            let before = chequebooks.len();
            chequebooks.retain(|c| c.chequebook_address.contains(filter));
            if chequebooks.is_empty() {
                return Err(anyhow::anyhow!(
                    "No chequebook found matching filter '{}'",
                    filter
                ));
            }
            tracing::info!(
                "Chequebook filter: {} -> {} chequebooks",
                before,
                chequebooks.len()
            );
        }

        println!(
            "💳 Syncing {} chequebook{}...\n",
            chequebooks.len(),
            if chequebooks.len() == 1 { "" } else { "s" }
        );

        let mut total_events = 0;

        // Process each chequebook
        for (idx, deployment) in chequebooks.iter().enumerate() {
            println!(
                "📦 [{}/{}] Chequebook: {}",
                idx + 1,
                chequebooks.len(),
                &deployment.chequebook_address
            );
            println!("   Deployed at block: {}", deployment.deployed_at_block);

            // Create chequebook contract instance
            let chequebook = ERC20SimpleSwapContract::new(
                deployment.chequebook_address.clone(),
                deployment.deployed_at_block,
            );

            // Determine block range
            let from = from_block.unwrap_or(deployment.deployed_at_block);
            let to = to_block.unwrap_or(u64::MAX);

            tracing::info!(
                "Syncing chequebook '{}' from block {} to {}",
                deployment.chequebook_address,
                from,
                if to == u64::MAX {
                    "latest".to_string()
                } else {
                    to.to_string()
                }
            );

            // Fetch events from this chequebook with incremental storage
            let cache_clone = cache.clone();
            let events = client
                .fetch_chequebook_events(
                    from,
                    to,
                    &cache,
                    &chequebook,
                    &config.blockchain,
                    &config.retry,
                    refresh,
                    |chunk_events| {
                        let cache = cache_clone.clone();
                        async move {
                            // Store events from this chunk immediately
                            cache.store_payment_channel_events(&chunk_events).await?;

                            tracing::debug!(
                                "Stored {} payment channel events from chunk",
                                chunk_events.len()
                            );

                            Ok(())
                        }
                    },
                )
                .await?;

            total_events += events.len();

            if events.is_empty() {
                println!("   ℹ️  No new events found\n");
            } else {
                println!(
                    "   ✅ Synced {} event{}\n",
                    events.len(),
                    if events.len() == 1 { "" } else { "s" }
                );

                // Show event breakdown
                let mut event_counts = std::collections::HashMap::new();
                for event in &events {
                    *event_counts.entry(event.event_type.to_string()).or_insert(0) += 1;
                }

                for (event_type, count) in event_counts {
                    println!("      • {}: {}", event_type, count);
                }
                println!();
            }
        }

        // Summary statistics
        let total_stored = cache.count_payment_channel_events().await?;
        println!("📊 Sync Complete");
        println!("   New events: {total_events}");
        println!("   Total in database: {total_stored}");

        Ok(())
    }

    async fn execute_cheque_summary(
        &self,
        cache: Cache,
        from_block: Option<u64>,
        to_block: Option<u64>,
        output: OutputFormat,
    ) -> Result<()> {
        tracing::info!("Generating cheque summary...");

        // Get all chequebook deployments for address mapping
        let deployments = cache.get_discovered_chequebooks().await?;
        if deployments.is_empty() {
            println!("⚠️ No chequebooks found in database.");
            println!("   Run 'discover-chequebooks' first.");
            return Ok(());
        }

        // Create a map of chequebook address -> deployment info
        let deployment_map: std::collections::HashMap<_, _> = deployments
            .iter()
            .map(|d| (d.chequebook_address.clone(), d))
            .collect();

        // Query for ChequeCashed events
        let events = cache
            .get_payment_channel_events(from_block, to_block, Some("ChequeCashed"))
            .await?;

        if events.is_empty() {
            println!("ℹ️  No ChequeCashed events found in specified range.");
            return Ok(());
        }

        // Aggregate by chequebook
        #[derive(Default, tabled::Tabled)]
        struct ChequebookStats {
            #[tabled(rename = "Chequebook Address")]
            address: String,
            #[tabled(rename = "Issuer")]
            issuer: String,
            #[tabled(rename = "Overlay")]
            overlay: String,
            #[tabled(rename = "Cheques Cashed")]
            total_cheques: u64,
            #[tabled(rename = "Total Amount (PLUR)")]
            total_amount: String,
        }

        use crate::events::PaymentChannelEventData;
        let mut stats_map: std::collections::HashMap<String, (u64, u128, Option<String>, Option<String>)> =
            std::collections::HashMap::new();

        for event in &events {
            let (count, total_amount, issuer, overlay) = stats_map
                .entry(event.chequebook_address.clone())
                .or_insert((0, 0, None, None));
            *count += 1;

            // Parse and add total_payout from event
            if let PaymentChannelEventData::ChequeCashed { total_payout, .. } = &event.data {
                if let Ok(amount) = total_payout.parse::<u128>() {
                    *total_amount += amount;
                }
            }

            // Add issuer and overlay from deployment info
            if issuer.is_none() || overlay.is_none() {
                if let Some(deployment) = deployment_map.get(&event.chequebook_address) {
                    if issuer.is_none() {
                        *issuer = deployment.issuer_address.clone();
                    }
                    if overlay.is_none() {
                        *overlay = deployment.overlay_address.clone();
                    }
                }
            }
        }

        // Convert to sorted vec
        let mut results: Vec<ChequebookStats> = stats_map
            .into_iter()
            .map(|(address, (count, total, issuer, overlay))| ChequebookStats {
                address,
                issuer: issuer.unwrap_or_else(|| "N/A".to_string()),
                overlay: overlay.unwrap_or_else(|| "N/A".to_string()),
                total_cheques: count,
                total_amount: total.to_string(),
            })
            .collect();
        results.sort_by(|a, b| b.total_cheques.cmp(&a.total_cheques));

        // Output results
        match output {
            OutputFormat::Table => {
                use tabled::Table;
                let table = Table::new(&results).to_string();
                println!("\n{table}\n");
                println!(
                    "Total chequebooks: {} | Total events: {}",
                    results.len(),
                    events.len()
                );
            }
            OutputFormat::Json => {
                let json_results: Vec<_> = results
                    .iter()
                    .map(|stat| {
                        serde_json::json!({
                            "chequebook_address": stat.address,
                            "issuer_address": stat.issuer,
                            "overlay_address": stat.overlay,
                            "total_cheques_cashed": stat.total_cheques,
                            "total_amount": stat.total_amount,
                        })
                    })
                    .collect();
                println!("{}", serde_json::to_string_pretty(&json_results)?);
            }
            OutputFormat::Csv => {
                println!("chequebook_address,issuer_address,overlay_address,total_cheques_cashed,total_amount");
                for stat in &results {
                    println!(
                        "{},{},{},{},{}",
                        stat.address, stat.issuer, stat.overlay, stat.total_cheques, stat.total_amount
                    );
                }
            }
        }

        Ok(())
    }

    async fn execute_chequebook_balances(
        &self,
        cache: Cache,
        client: BlockchainClient,
        config: &AppConfig,
        output: OutputFormat,
        refresh: bool,
    ) -> Result<()> {
        tracing::info!("Retrieving chequebook balances...");

        // Get all chequebook deployments
        let deployments = cache.get_discovered_chequebooks().await?;
        if deployments.is_empty() {
            println!("⚠️ No chequebooks found in database.");
            println!("   Run 'discover-chequebooks' first.");
            return Ok(());
        }

        if matches!(output, OutputFormat::Table) {
            println!(
                "💳 Retrieving balances for {} chequebook{}...\n",
                deployments.len(),
                if deployments.len() == 1 { "" } else { "s" }
            );
        }

        #[derive(serde::Serialize, tabled::Tabled)]
        struct BalanceInfo {
            #[tabled(rename = "Chequebook Address")]
            chequebook_address: String,
            #[tabled(rename = "Issuer")]
            issuer_address: String,
            #[tabled(rename = "Overlay")]
            overlay_address: String,
            #[tabled(rename = "Balance (PLUR)")]
            balance: String,
        }

        let mut balances = Vec::new();

        for deployment in &deployments {
            // Always query RPC for balance (refresh flag can be used for future caching)
            tracing::debug!("Querying balance for {}", deployment.chequebook_address);
            let balance = match client
                .get_chequebook_balance(&deployment.chequebook_address, &config.retry)
                .await
            {
                Ok(bal) => bal.to_string(),
                Err(e) => {
                    tracing::warn!(
                        "Failed to get balance for {}: {}",
                        deployment.chequebook_address,
                        e
                    );
                    "ERROR".to_string()
                }
            };

            balances.push(BalanceInfo {
                chequebook_address: deployment.chequebook_address.clone(),
                issuer_address: deployment
                    .issuer_address
                    .clone()
                    .unwrap_or_else(|| "N/A".to_string()),
                overlay_address: deployment
                    .overlay_address
                    .clone()
                    .unwrap_or_else(|| "N/A".to_string()),
                balance,
            });
        }

        // Output results
        match output {
            OutputFormat::Table => {
                use tabled::Table;
                let table = Table::new(&balances).to_string();
                println!("\n{table}\n");
                println!("Total chequebooks: {}", balances.len());
            }
            OutputFormat::Json => {
                println!("{}", serde_json::to_string_pretty(&balances)?);
            }
            OutputFormat::Csv => {
                println!("chequebook_address,issuer_address,overlay_address,balance_plur");
                for info in &balances {
                    println!(
                        "{},{},{},{}",
                        info.chequebook_address, info.issuer_address, info.overlay_address, info.balance
                    );
                }
            }
        }

        Ok(())
    }

    async fn execute_migrations(&self, cache: Cache) -> Result<()> {
        let migrations = cache.get_migration_status().await?;
        let total = migrations.len();

        println!("\n## Database Migration Status\n");
        println!("{:<20} {:<50} {:<30}", "Version", "Description", "Applied At");
        println!("{}", "-".repeat(100));

        for migration in migrations {
            println!(
                "{:<20} {:<50} {:<30}",
                migration.version,
                migration.description,
                migration.installed_on
            );
        }

        println!("\n**Total migrations applied:** {total}\n");

        Ok(())
    }

    async fn execute_reset(&self) -> Result<()> {
        // Get database path from config
        let config = self.resolve_config()?;
        let db_path = &config.database.path;

        // Determine if PostgreSQL or SQLite
        let is_postgres = db_path.starts_with("postgres://") || db_path.starts_with("postgresql://");

        // Show warning and get confirmation
        println!("\n😱 WARNING: This will PERMANENTLY DELETE all data in the database!\n");
        if is_postgres {
            println!("Database: PostgreSQL ({db_path})");
        } else {
            println!("Database: SQLite ({db_path})");
        }
        println!("\nType 'yes' to confirm: ");

        // Read user input
        use std::io::{self, Write};
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        if input.trim().to_lowercase() != "yes" {
            println!("\n❌ Reset cancelled.");
            return Ok(());
        }

        // Perform reset based on database type
        if is_postgres {
            // Extract database name from PostgreSQL URL
            let db_name = if let Some(last_slash) = db_path.rfind('/') {
                &db_path[last_slash + 1..]
            } else {
                return Err(anyhow::anyhow!("Invalid PostgreSQL URL: cannot extract database name"));
            };

            println!("\n♻️ Dropping PostgreSQL database '{db_name}'...");

            // Drop and recreate database using psql
            let drop_status = std::process::Command::new("psql")
                .args(["-c", &format!("DROP DATABASE IF EXISTS {db_name};")])
                .status()?;

            if !drop_status.success() {
                return Err(anyhow::anyhow!("Failed to drop database"));
            }

            let create_status = std::process::Command::new("psql")
                .args(["-c", &format!("CREATE DATABASE {db_name};")])
                .status()?;

            if !create_status.success() {
                return Err(anyhow::anyhow!("Failed to create database"));
            }

            println!("✅ PostgreSQL database '{db_name}' has been reset successfully!");
        } else {
            // SQLite - just delete the file
            println!("\n♻️ Deleting SQLite database file...");

            if std::path::Path::new(db_path).exists() {
                std::fs::remove_file(db_path)?;
            }

            println!("✅ SQLite database '{db_path}' has been reset successfully!");
            println!("   (Database will be recreated on next run)");
        }

        Ok(())
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
