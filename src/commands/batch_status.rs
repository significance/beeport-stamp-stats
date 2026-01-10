use crate::blockchain::BlockchainClient;
use crate::cache::Cache;
use crate::cli::{BatchStatusSortBy, OutputFormat};
use crate::error::Result;
use crate::events::BatchInfo;
use crate::price::{blocks_to_days, PriceChange, PriceConfig};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tabled::Tabled;

/// Batch status entry with TTL and expiry information
#[derive(Debug, Clone, Serialize, Deserialize, Tabled)]
pub struct BatchStatus {
    #[tabled(rename = "Batch ID")]
    pub batch_id: String,

    #[tabled(rename = "Owner")]
    pub owner: String,

    #[tabled(rename = "Payer")]
    pub payer: String,

    #[tabled(rename = "Depth")]
    pub depth: u8,

    #[tabled(rename = "Size (chunks)")]
    pub size_chunks: String,

    #[tabled(rename = "Balance (PLUR/chunk)")]
    pub normalised_balance: String,

    #[tabled(rename = "TTL (blocks)")]
    pub ttl_blocks: String,

    #[tabled(rename = "TTL (days)")]
    pub ttl_days: String,

    #[tabled(rename = "Expiry Date")]
    pub expiry_date: String,

    #[tabled(skip)]
    pub expiry_timestamp: DateTime<Utc>,
}

impl BatchStatus {
    /// Create a batch status from batch info and price configuration
    pub fn from_batch(
        batch: &BatchInfo,
        price_config: &PriceConfig,
        _current_block: u64,
        block_time_seconds: f64,
    ) -> Result<Self> {
        // Calculate size in chunks (2^depth)
        let size_chunks = 1u128 << batch.depth;

        // Parse balance for calculations
        let balance_value = batch.normalised_balance.parse::<u128>()
            .unwrap_or(0);

        // Calculate TTL in blocks (normalised_balance / price)
        // Note: normalised_balance is already per-chunk, so we just divide by price (per-chunk per-block)
        let ttl_blocks = if balance_value > 0 && price_config.base_price > 0 {
            balance_value / price_config.base_price
        } else {
            0
        };

        // Convert TTL blocks to days
        let ttl_days_value = blocks_to_days(ttl_blocks as u64, block_time_seconds);

        // Calculate expiry timestamp
        let seconds_until_expiry = (ttl_blocks as f64 * block_time_seconds) as u128;
        let expiry_timestamp = Utc::now() + chrono::Duration::seconds(seconds_until_expiry as i64);

        Ok(Self {
            batch_id: batch.batch_id.clone(),
            owner: batch.owner.clone(),
            payer: batch.payer.clone().unwrap_or_else(|| "-".to_string()),
            depth: batch.depth,
            size_chunks: format_number(size_chunks),
            normalised_balance: format_number(balance_value),
            ttl_blocks: format_number(ttl_blocks),
            ttl_days: format!("{ttl_days_value:.2}"),
            expiry_date: expiry_timestamp.format("%Y-%m-%d %H:%M UTC").to_string(),
            expiry_timestamp,
        })
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

/// Execute the batch status command
#[allow(clippy::too_many_arguments)]
pub async fn execute(
    cache: Cache,
    blockchain_client: &BlockchainClient,
    registry: &crate::contracts::ContractRegistry,
    config: &crate::config::AppConfig,
    sort_by: BatchStatusSortBy,
    output: OutputFormat,
    price_override: Option<String>,
    price_change_str: Option<String>,
    refresh: bool,
    only_missing: bool,
    hide_zero_balance: bool,
    contract_filter: Option<String>,
    cache_validity_blocks: u64,
) -> Result<()> {
    // Get all batches from cache
    let mut batches = cache.get_batches(0).await?;

    if batches.is_empty() {
        println!("No batches found in database. Run 'sync' or 'fetch' first.");
        return Ok(());
    }

    // Filter by contract source if requested
    if let Some(filter) = contract_filter {
        let contract_source = match filter.to_lowercase().as_str() {
            "postage-stamp" | "postagestamp" => "PostageStamp",
            "stamps-registry" | "stampsregistry" => "StampsRegistry",
            _ => {
                return Err(crate::error::StampError::Parse(format!(
                    "Invalid contract filter '{}'. Use 'postage-stamp' or 'stamps-registry'",
                    filter
                )));
            }
        };
        let before = batches.len();
        batches.retain(|b| b.contract_source == contract_source);
        println!("Filtered to {} batches from {} (was {})", batches.len(), contract_source, before);
    }

    // Determine price configuration
    let base_price = if let Some(price_str) = price_override {
        // User provided explicit price
        price_str
            .parse::<u128>()
            .map_err(|_| crate::error::StampError::Parse("Invalid price value".to_string()))?
    } else if refresh {
        // Refresh mode: fetch current price from blockchain and cache it
        let price = blockchain_client.get_current_price(registry).await?;
        cache.cache_price(price).await?;
        price
    } else {
        // Use cached price if available, otherwise fetch from blockchain
        if let Some(cached_price) = cache.get_cached_price().await? {
            cached_price
        } else {
            let price = blockchain_client.get_current_price(registry).await?;
            cache.cache_price(price).await?;
            price
        }
    };

    let price_config = if let Some(change_str) = price_change_str {
        let price_change = change_str.parse::<PriceChange>()?;
        PriceConfig::with_price_change(base_price, price_change)
    } else {
        PriceConfig::new(base_price)
    };

    // Get current block
    let current_block = blockchain_client.get_current_block().await?;

    // Calculate status for each batch, fetching current balance from blockchain
    let mut statuses: Vec<BatchStatus> = Vec::new();

    if refresh && only_missing {
        println!("📊 Fetching balances only for batches without cached data...");
        println!("Using max_retries={} for rate-limited requests. Progress will be shown every 100 batches.\n", config.retry.max_retries);
    } else if refresh {
        println!("📊 Fetching current balances for {} batches from blockchain...", batches.len());
        println!("Using max_retries={} for rate-limited requests. Progress will be shown every 100 batches.\n", config.retry.max_retries);
    } else {
        println!("📊 Using cached balances for {} batches...", batches.len());
        println!("Note: Batches without cached balance will show creation-time balance (pass --refresh to fetch current balances)");
        println!("Progress will be shown every 100 batches.\n");
    }

    let total = batches.len();
    let mut cache_hits = 0;
    let mut cache_misses = 0;
    let mut skipped = 0;

    for (idx, batch) in batches.iter().enumerate() {
        // Show progress every 100 batches
        if idx % 100 == 0 && idx > 0 {
            println!(
                "  ⏳ Progress: {}/{} batches ({:.1}%) - Cache: {} hits, {} misses, {} skipped",
                idx, total, (idx as f64 / total as f64) * 100.0, cache_hits, cache_misses, skipped
            );
        }

        // Check if we have a cached balance
        let cached_balance = cache.get_cached_balance(&batch.batch_id, current_block, cache_validity_blocks).await.ok().flatten();

        // Get balance based on refresh and only_missing flags
        let remaining_balance = if !refresh {
            // When refresh=false, use cache if available, otherwise use original balance
            if let Some(cached) = cached_balance {
                cache_hits += 1;
                tracing::debug!("Cache hit for batch {}", batch.batch_id);
                cached
            } else {
                cache_misses += 1;
                tracing::debug!("No cached balance for batch {}, using original balance from creation", batch.batch_id);
                batch.normalised_balance.clone() // Use last known balance (creation balance)
            }
        } else if only_missing && cached_balance.is_some() {
            // Skip batches that already have cached balance when only_missing=true
            skipped += 1;
            cache_hits += 1;
            tracing::debug!("Skipping batch {} (already cached)", batch.batch_id);
            cached_balance.unwrap()
        } else {
            // Fetch from blockchain (either refresh=true without only_missing, or refresh=true with only_missing but no cache)
            cache_misses += 1;
            match blockchain_client.get_remaining_balance(&batch.batch_id, registry, &config.retry).await {
                Ok(balance) => {
                    // Only cache successful fetches
                    if let Err(e) = cache.cache_balance(&batch.batch_id, &balance, current_block).await {
                        tracing::warn!("Failed to cache balance: {}", e);
                    }

                    // Small delay to avoid rate limiting (1ms between requests)
                    tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;

                    balance
                }
                Err(e) => {
                    // Don't cache failures - this allows retry with --only-missing later
                    // Only log if it's not the common "batch doesn't exist" error
                    if !e.to_string().contains("0x4ee9bc0f") {
                        tracing::warn!("Failed to get balance for {}: {}", batch.batch_id, e);
                    }
                    "0".to_string()
                }
            }
        };

        // Create a modified batch with current balance
        let mut current_batch = batch.clone();
        current_batch.normalised_balance = remaining_balance;

        if let Ok(status) = BatchStatus::from_batch(&current_batch, &price_config, current_block, config.blockchain.block_time_seconds) {
            statuses.push(status);
        }
    }

    if skipped > 0 {
        println!(
            "  ✅ Completed: {}/{} batches - Cache: {} hits ({:.1}%), {} fetched, {} skipped\n",
            total, total, cache_hits, (cache_hits as f64 / total as f64) * 100.0, cache_misses, skipped
        );
    } else {
        println!(
            "  ✅ Completed: {}/{} batches - Cache: {} hits ({:.1}%), {} misses\n",
            total, total, cache_hits, (cache_hits as f64 / total as f64) * 100.0, cache_misses
        );
    }

    // Filter out zero balance batches if requested
    let total_before_filter = statuses.len();
    if hide_zero_balance {
        statuses.retain(|s| s.normalised_balance != "0");
        let filtered_count = total_before_filter - statuses.len();
        if filtered_count > 0 {
            println!("  🔍 Filtered out {filtered_count} batches with zero balance\n");
        }
    }

    // Sort results
    match sort_by {
        BatchStatusSortBy::BatchId => statuses.sort_by(|a, b| a.batch_id.cmp(&b.batch_id)),
        BatchStatusSortBy::Depth => statuses.sort_by(|a, b| b.depth.cmp(&a.depth)), // Descending order (highest depth first)
        BatchStatusSortBy::Ttl => {
            statuses.sort_by(|a, b| {
                // Parse ttl_blocks strings (removing commas) for numeric comparison
                let a_ttl = a.ttl_blocks.replace(",", "").parse::<u128>().unwrap_or(0);
                let b_ttl = b.ttl_blocks.replace(",", "").parse::<u128>().unwrap_or(0);
                b_ttl.cmp(&a_ttl) // Descending order (highest TTL first)
            })
        }
        BatchStatusSortBy::Expiry => {
            statuses.sort_by(|a, b| a.expiry_timestamp.cmp(&b.expiry_timestamp))
        }
        BatchStatusSortBy::Size => {
            statuses.sort_by(|a, b| {
                let a_size = 1u128 << a.depth;
                let b_size = 1u128 << b.depth;
                b_size.cmp(&a_size)
            })
        }
    }

    // Output results
    match output {
        OutputFormat::Table => {
            use tabled::Table;
            let table = Table::new(&statuses).to_string();
            println!("\n{table}\n");

            let price_info = format!(
                "Total batches: {} | Price: {} PLUR/chunk/block | TTL (blocks) = Balance / Price",
                statuses.len(),
                format_number(base_price)
            );
            println!("{price_info}");
        }
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(&statuses)?;
            println!("{json}");
        }
        OutputFormat::Csv => {
            let mut wtr = csv::Writer::from_writer(std::io::stdout());
            for status in &statuses {
                wtr.serialize(status)?;
            }
            wtr.flush()?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_format_number() {
        assert_eq!(format_number(1000), "1,000");
        assert_eq!(format_number(1000000), "1,000,000");
        assert_eq!(format_number(1048576), "1,048,576");
    }

    #[test]
    fn test_batch_status_creation() {
        let batch = BatchInfo {
            batch_id: "0x1234".to_string(),
            owner: "0x5678".to_string(),
            payer: None,
            contract_source: "PostageStamp".to_string(),
            depth: 20,
            bucket_depth: 16,
            immutable: false,
            normalised_balance: "240000000".to_string(), // 240M PLUR - reasonable for testing
            created_at: Utc::now(),
            block_number: 1000,
        };

        let price_config = PriceConfig::new(24000);
        let status = BatchStatus::from_batch(&batch, &price_config, 38000000, 5.0).unwrap();

        assert_eq!(status.batch_id, "0x1234");
        assert_eq!(status.depth, 20);
        assert!(status.ttl_blocks != "0");
        assert!(!status.ttl_blocks.is_empty());
        // With balance=240M and price=24000, TTL should be 10,000 blocks
        assert_eq!(status.ttl_blocks, "10,000");
    }
}
