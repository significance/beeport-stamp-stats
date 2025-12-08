use crate::blockchain::BlockchainClient;
use crate::cache::Cache;
use crate::cli::{BatchStatusSortBy, OutputFormat};
use crate::error::Result;
use crate::events::BatchInfo;
use crate::price::{blocks_to_days, calculate_ttl_blocks, PriceChange, PriceConfig};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tabled::Tabled;

/// Batch status entry with TTL and expiry information
#[derive(Debug, Clone, Serialize, Deserialize, Tabled)]
pub struct BatchStatus {
    #[tabled(rename = "Batch ID")]
    pub batch_id: String,

    #[tabled(rename = "Depth")]
    pub depth: u8,

    #[tabled(rename = "Size (chunks)")]
    pub size_chunks: String,

    #[tabled(rename = "TTL (blocks)")]
    pub ttl_blocks: u64,

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
    ) -> Result<Self> {
        // Calculate size in chunks (2^depth)
        let size_chunks = 1u128 << batch.depth;

        // Calculate TTL in blocks using the base price first
        let ttl_blocks = calculate_ttl_blocks(
            &batch.normalised_balance,
            batch.depth,
            price_config.base_price,
        )?;

        // Convert TTL to days
        let ttl_days_value = blocks_to_days(ttl_blocks);

        // If price change is configured, recalculate with effective price
        let (final_ttl_blocks, final_ttl_days) = if let Some(ref price_change) = price_config.price_change {
            let effective_price = price_change.average_price(price_config.base_price, ttl_days_value);
            let adjusted_ttl_blocks = calculate_ttl_blocks(
                &batch.normalised_balance,
                batch.depth,
                effective_price,
            )?;
            let adjusted_ttl_days = blocks_to_days(adjusted_ttl_blocks);
            (adjusted_ttl_blocks, adjusted_ttl_days)
        } else {
            (ttl_blocks, ttl_days_value)
        };

        // Calculate expiry timestamp
        // Assuming 5 seconds per block on Gnosis Chain
        let seconds_until_expiry = final_ttl_blocks * 5;
        let expiry_timestamp = Utc::now() + chrono::Duration::seconds(seconds_until_expiry as i64);

        Ok(Self {
            batch_id: batch.batch_id.clone(),
            depth: batch.depth,
            size_chunks: format_number(size_chunks),
            ttl_blocks: final_ttl_blocks,
            ttl_days: format!("{:.2}", final_ttl_days),
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
pub async fn execute(
    cache: Cache,
    blockchain_client: &BlockchainClient,
    sort_by: BatchStatusSortBy,
    output: OutputFormat,
    price_override: Option<String>,
    price_change_str: Option<String>,
) -> Result<()> {
    // Get all batches from cache
    let batches = cache.get_batches(0).await?;

    if batches.is_empty() {
        println!("No batches found in database. Run 'sync' or 'fetch' first.");
        return Ok(());
    }

    // Determine price configuration
    let base_price = if let Some(price_str) = price_override {
        price_str
            .parse::<u128>()
            .map_err(|_| crate::error::StampError::Parse("Invalid price value".to_string()))?
    } else {
        blockchain_client.get_current_price().await?
    };

    let price_config = if let Some(change_str) = price_change_str {
        let price_change = PriceChange::from_str(&change_str)?;
        PriceConfig::with_price_change(base_price, price_change)
    } else {
        PriceConfig::new(base_price)
    };

    // Get current block
    let current_block = blockchain_client.get_current_block().await?;

    // Calculate status for each batch, fetching current balance from blockchain
    let mut statuses: Vec<BatchStatus> = Vec::new();

    println!("📊 Fetching current balances for {} batches from blockchain...", batches.len());
    println!("Using cache for recent queries. Progress will be shown every 100 batches.\n");

    let total = batches.len();
    let mut cache_hits = 0;
    let mut cache_misses = 0;

    for (idx, batch) in batches.iter().enumerate() {
        // Show progress every 100 batches
        if idx % 100 == 0 && idx > 0 {
            println!(
                "  ⏳ Progress: {}/{} batches ({:.1}%) - Cache: {} hits, {} misses",
                idx, total, (idx as f64 / total as f64) * 100.0, cache_hits, cache_misses
            );
        }

        // Try to get from cache first
        let remaining_balance = if let Ok(Some(cached)) = cache.get_cached_balance(&batch.batch_id, current_block).await {
            cache_hits += 1;
            tracing::debug!("Cache hit for batch {}", batch.batch_id);
            cached
        } else {
            cache_misses += 1;
            // Fetch current remaining balance from blockchain
            let balance = blockchain_client
                .get_remaining_balance(&batch.batch_id)
                .await
                .unwrap_or_else(|e| {
                    // Only log if it's not the common "batch doesn't exist" error
                    if !e.to_string().contains("0x4ee9bc0f") {
                        tracing::warn!("Failed to get balance for {}: {}", batch.batch_id, e);
                    }
                    "0".to_string()
                });

            // Cache the result
            if let Err(e) = cache.cache_balance(&batch.batch_id, &balance, current_block).await {
                tracing::warn!("Failed to cache balance: {}", e);
            }

            balance
        };

        // Small delay to avoid rate limiting (10ms between requests)
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // Create a modified batch with current balance
        let mut current_batch = batch.clone();
        current_batch.normalised_balance = remaining_balance;

        if let Ok(status) = BatchStatus::from_batch(&current_batch, &price_config, current_block) {
            statuses.push(status);
        }
    }

    println!(
        "  ✅ Completed: {}/{} batches - Cache: {} hits ({:.1}%), {} misses\n",
        total, total, cache_hits, (cache_hits as f64 / total as f64) * 100.0, cache_misses
    );

    // Sort results
    match sort_by {
        BatchStatusSortBy::BatchId => statuses.sort_by(|a, b| a.batch_id.cmp(&b.batch_id)),
        BatchStatusSortBy::Ttl => statuses.sort_by(|a, b| b.ttl_blocks.cmp(&a.ttl_blocks)),
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
            println!("\n{}\n", table);
            println!(
                "Total batches: {} | Base price: {} PLUR/chunk/block",
                statuses.len(),
                format_number(base_price)
            );
        }
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(&statuses)?;
            println!("{}", json);
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
            depth: 20,
            bucket_depth: 16,
            immutable: false,
            normalised_balance: "10000000000000000000".to_string(), // 10^19 PLUR
            created_at: Utc::now(),
        };

        let price_config = PriceConfig::new(24000);
        let status = BatchStatus::from_batch(&batch, &price_config, 38000000).unwrap();

        assert_eq!(status.batch_id, "0x1234");
        assert_eq!(status.depth, 20);
        assert!(status.ttl_blocks > 0);
    }
}
