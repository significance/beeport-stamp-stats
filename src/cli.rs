use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

use crate::{
    batch,
    blockchain::BlockchainClient,
    cache::Cache,
    display,
    events::{DEFAULT_START_BLOCK, EventType},
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
    /// RPC endpoint URL
    #[arg(long, env = "RPC_URL", default_value = "https://rpc.gnosis.gateway.fm")]
    pub rpc_url: String,

    /// Path to the cache database
    #[arg(long, env = "CACHE_DB", default_value = "./stamp-cache.db")]
    pub cache_db: PathBuf,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Fetch postage stamp events from the blockchain and cache them
    Fetch {
        /// Start block number (defaults to contract deployment block)
        #[arg(long)]
        from_block: Option<u64>,

        /// End block number (defaults to latest)
        #[arg(long)]
        to_block: Option<u64>,

        /// Only fetch new events since last run
        #[arg(long, default_value = "false")]
        incremental: bool,
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
    },
}

#[derive(Debug, Clone, clap::ValueEnum)]
pub enum GroupBy {
    Day,
    Week,
    Month,
}

#[derive(Debug, Clone, clap::ValueEnum)]
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
    pub async fn execute(&self) -> Result<()> {
        // Initialize cache
        let cache = Cache::new(&self.cache_db).await?;

        match &self.command {
            Commands::Fetch {
                from_block,
                to_block,
                incremental,
            } => {
                self.execute_fetch(cache, *from_block, *to_block, *incremental)
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
            } => self.execute_follow(cache, *poll_interval, *display).await,
            Commands::Sync {
                from_block,
                to_block,
                contract,
            } => {
                self.execute_sync(cache, *from_block, *to_block, contract.clone())
                    .await
            }
            Commands::Price => self.execute_price().await,
            Commands::BatchStatus {
                sort_by,
                output,
                price,
                price_change,
            } => {
                self.execute_batch_status(
                    cache,
                    sort_by.clone(),
                    output.clone(),
                    price.clone(),
                    price_change.clone(),
                )
                .await
            }
            Commands::ExpiryAnalytics {
                period,
                output,
                sort_by,
                price,
                price_change,
            } => {
                self.execute_expiry_analytics(
                    cache,
                    period.clone(),
                    output.clone(),
                    sort_by.clone(),
                    price.clone(),
                    price_change.clone(),
                )
                .await
            }
        }
    }

    async fn execute_fetch(
        &self,
        cache: Cache,
        from_block: Option<u64>,
        to_block: Option<u64>,
        incremental: bool,
    ) -> Result<()> {
        tracing::info!("Fetching events from blockchain...");

        // Create blockchain client
        let client = BlockchainClient::new(&self.rpc_url).await?;

        // Determine block range
        let from = if incremental {
            cache.get_last_block().await?.map(|b| b + 1)
        } else {
            from_block
        }
        .unwrap_or(DEFAULT_START_BLOCK);

        let to = to_block.unwrap_or_else(|| {
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

        // Fetch and display events
        let events = client.fetch_batch_events(from, to, &cache).await?;

        tracing::info!("Found {} events", events.len());

        // Display events in markdown table
        display::display_events(&events)?;

        // Fetch batch information for BatchCreated events
        let batches = client.fetch_batch_info(&events).await?;

        // Cache everything
        cache.store_events(&events).await?;
        cache.store_batches(&batches).await?;

        tracing::info!(
            "Cached {} events and {} batches",
            events.len(),
            batches.len()
        );

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
            events.retain(|e| e.batch_id.contains(filter));
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
                    events.retain(|e| e.batch_id.contains(filter));
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
                    events.retain(|e| e.batch_id.contains(filter));
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

    async fn execute_follow(&self, cache: Cache, poll_interval: u64, display: bool) -> Result<()> {
        use tokio::time::{Duration, interval};

        tracing::info!("Starting follow mode with {}s poll interval", poll_interval);

        // Create blockchain client
        let client = BlockchainClient::new(&self.rpc_url).await?;

        // Create event hook
        let hook = StubHook;

        // First, ensure historical sync
        let last_synced_block = cache.get_last_block().await?.unwrap_or(DEFAULT_START_BLOCK);
        tracing::info!(
            "Last synced block: {} - catching up to latest...",
            last_synced_block
        );

        // Fetch all events up to current block
        let latest_block = client
            .fetch_batch_events(last_synced_block + 1, u64::MAX, &cache)
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

            // Cache historical events
            let batches = client.fetch_batch_info(&latest_block).await?;
            cache.store_events(&latest_block).await?;
            cache.store_batches(&batches).await?;

            if display {
                display::display_events(&latest_block)?;
            }
        } else {
            tracing::info!("Already up to date at block {}", last_synced_block);
        }

        println!(
            "\n🔄 Following blockchain for new events (polling every {}s)...",
            poll_interval
        );
        println!("Press Ctrl+C to stop\n");

        // Now follow for new events
        let mut poll_timer = interval(Duration::from_secs(poll_interval));
        let mut last_checked_block = current_latest;

        loop {
            poll_timer.tick().await;

            // Fetch new events since last check
            let new_events = client
                .fetch_batch_events(last_checked_block + 1, u64::MAX, &cache)
                .await?;

            if !new_events.is_empty() {
                tracing::info!("Found {} new events", new_events.len());

                // Invoke hooks for each new event
                for event in &new_events {
                    hook.on_event(event);
                }

                // Cache new events
                let batches = client.fetch_batch_info(&new_events).await?;
                cache.store_events(&new_events).await?;
                cache.store_batches(&batches).await?;

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

    async fn execute_price(&self) -> Result<()> {
        tracing::info!("Querying current storage price from blockchain...");

        let client = BlockchainClient::new(&self.rpc_url).await?;
        let price = client.get_current_price().await?;
        let current_block = client.get_current_block().await?;

        println!("\n📊 Current Storage Price\n");
        println!("Price per chunk per block: {} PLUR", format_number(price));
        println!("Current block: {}", format_number(current_block as u128));
        println!("\nThis price is used to calculate batch TTL (Time To Live).");
        println!("Use --price {} with batch-status or expiry-analytics commands.", price);

        Ok(())
    }

    async fn execute_sync(
        &self,
        cache: Cache,
        from_block: Option<u64>,
        to_block: Option<u64>,
        _contract: Option<String>,
    ) -> Result<()> {
        tracing::info!("Syncing database with blockchain...");

        let client = BlockchainClient::new(&self.rpc_url).await?;

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

        // Fetch events
        let events = client.fetch_batch_events(from, to, &cache).await?;

        if events.is_empty() {
            println!("✅ Database is already up to date!");
            return Ok(());
        }

        tracing::info!("Found {} new events", events.len());

        // Fetch batch information
        let batches = client.fetch_batch_info(&events).await?;

        // Store in database
        cache.store_events(&events).await?;
        cache.store_batches(&batches).await?;

        println!(
            "✅ Synced {} events and {} batches to database",
            events.len(),
            batches.len()
        );

        Ok(())
    }

    async fn execute_batch_status(
        &self,
        cache: Cache,
        sort_by: BatchStatusSortBy,
        output: OutputFormat,
        price: Option<String>,
        price_change: Option<String>,
    ) -> Result<()> {
        let client = BlockchainClient::new(&self.rpc_url).await?;
        crate::commands::batch_status::execute(cache, &client, sort_by, output, price, price_change)
            .await
            .map_err(|e| anyhow::anyhow!(e))
    }

    async fn execute_expiry_analytics(
        &self,
        cache: Cache,
        period: TimePeriod,
        output: OutputFormat,
        sort_by: ExpiryAnalyticsSortBy,
        price: Option<String>,
        price_change: Option<String>,
    ) -> Result<()> {
        let client = BlockchainClient::new(&self.rpc_url).await?;
        crate::commands::expiry_analytics::execute(
            cache,
            &client,
            period,
            output,
            sort_by,
            price,
            price_change,
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
        let cli = Cli::parse_from(&[
            "beeport-stamp-stats",
            "--rpc-url",
            "http://localhost:8545",
            "fetch",
            "--from-block",
            "1000000",
        ]);

        assert_eq!(cli.rpc_url, "http://localhost:8545");
        match cli.command {
            Commands::Fetch { from_block, .. } => {
                assert_eq!(from_block, Some(1000000));
            }
            _ => panic!("Expected Fetch command"),
        }
    }

    #[test]
    fn test_summary_parsing() {
        let cli = Cli::parse_from(&[
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
