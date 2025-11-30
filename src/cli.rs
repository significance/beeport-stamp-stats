use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

use crate::{cache::Cache, blockchain::BlockchainClient, display};

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
    },
}

#[derive(Debug, Clone, clap::ValueEnum)]
pub enum GroupBy {
    Day,
    Week,
    Month,
}

impl Cli {
    pub async fn execute(&self) -> Result<()> {
        // Initialize cache
        let cache = Cache::new(&self.cache_db).await?;

        match &self.command {
            Commands::Fetch { from_block, to_block, incremental } => {
                self.execute_fetch(cache, *from_block, *to_block, *incremental).await
            }
            Commands::Summary { group_by, months } => {
                self.execute_summary(cache, group_by.clone(), *months).await
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
        }.unwrap_or(19275989); // PostageStamp contract deployment block on Gnosis

        let to = to_block.unwrap_or_else(|| {
            // We'll get latest block from the client
            u64::MAX
        });

        tracing::info!("Fetching events from block {} to {}", from, if to == u64::MAX { "latest".to_string() } else { to.to_string() });

        // Fetch and display events
        let events = client.fetch_batch_events(from, to).await?;

        tracing::info!("Found {} events", events.len());

        // Display events in markdown table
        display::display_events(&events)?;

        // Fetch batch information for BatchCreated events
        let batches = client.fetch_batch_info(&events).await?;

        // Cache everything
        cache.store_events(&events).await?;
        cache.store_batches(&batches).await?;

        tracing::info!("Cached {} events and {} batches", events.len(), batches.len());

        Ok(())
    }

    async fn execute_summary(
        &self,
        cache: Cache,
        group_by: GroupBy,
        months: u32,
    ) -> Result<()> {
        tracing::info!("Generating summary from cached data...");

        // Retrieve events from cache
        let events = cache.get_events(months).await?;
        let batches = cache.get_batches(months).await?;

        tracing::info!("Loaded {} events and {} batches from cache", events.len(), batches.len());

        // Display summary
        display::display_summary(&events, &batches, group_by)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_parsing() {
        let cli = Cli::parse_from(&[
            "beeport-stamp-stats",
            "--rpc-url", "http://localhost:8545",
            "fetch",
            "--from-block", "1000000",
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
            "--group-by", "month",
            "--months", "6",
        ]);

        match cli.command {
            Commands::Summary { months, .. } => {
                assert_eq!(months, 6);
            }
            _ => panic!("Expected Summary command"),
        }
    }
}
