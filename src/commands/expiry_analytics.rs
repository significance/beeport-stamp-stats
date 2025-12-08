use crate::blockchain::BlockchainClient;
use crate::cache::Cache;
use crate::cli::{ExpiryAnalyticsSortBy, OutputFormat, TimePeriod};
use crate::error::Result;
use crate::events::BatchInfo;
use crate::price::{blocks_to_days, calculate_ttl_blocks, PriceChange, PriceConfig};
use chrono::{DateTime, Datelike, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tabled::Tabled;

/// Expiry analytics entry showing aggregated data for a time period
#[derive(Debug, Clone, Serialize, Deserialize, Tabled)]
pub struct ExpiryPeriod {
    #[tabled(rename = "Period")]
    pub period: String,

    #[tabled(rename = "Batches Expiring")]
    pub batch_count: usize,

    #[tabled(rename = "Total Chunks")]
    pub total_chunks: String,

    #[tabled(rename = "Total Storage")]
    pub total_storage: String,

    #[tabled(skip)]
    pub period_start: DateTime<Utc>,

    #[tabled(skip)]
    pub chunks_raw: u128,
}

impl ExpiryPeriod {
    /// Format period based on time period type
    fn format_period(timestamp: DateTime<Utc>, period: &TimePeriod) -> (String, DateTime<Utc>) {
        match period {
            TimePeriod::Day => {
                let formatted = timestamp.format("%Y-%m-%d").to_string();
                let period_start = timestamp
                    .date_naive()
                    .and_hms_opt(0, 0, 0)
                    .unwrap()
                    .and_utc();
                (formatted, period_start)
            }
            TimePeriod::Week => {
                let iso_week = timestamp.iso_week();
                let formatted = format!("{}-W{:02}", iso_week.year(), iso_week.week());
                // Get the Monday of this week
                let days_from_monday = timestamp.weekday().num_days_from_monday();
                let period_start = (timestamp - chrono::Duration::days(days_from_monday as i64))
                    .date_naive()
                    .and_hms_opt(0, 0, 0)
                    .unwrap()
                    .and_utc();
                (formatted, period_start)
            }
            TimePeriod::Month => {
                let formatted = timestamp.format("%Y-%m").to_string();
                let period_start = timestamp
                    .date_naive()
                    .with_day(1)
                    .unwrap()
                    .and_hms_opt(0, 0, 0)
                    .unwrap()
                    .and_utc();
                (formatted, period_start)
            }
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

    /// Format storage size in human-readable format
    fn format_storage(chunks: u128) -> String {
        // Each chunk is 4KB
        const CHUNK_SIZE: u128 = 4096;
        let bytes = chunks * CHUNK_SIZE;

        const KB: u128 = 1024;
        const MB: u128 = KB * 1024;
        const GB: u128 = MB * 1024;
        const TB: u128 = GB * 1024;
        const PB: u128 = TB * 1024;

        if bytes >= PB {
            format!("{:.2} PB", bytes as f64 / PB as f64)
        } else if bytes >= TB {
            format!("{:.2} TB", bytes as f64 / TB as f64)
        } else if bytes >= GB {
            format!("{:.2} GB", bytes as f64 / GB as f64)
        } else if bytes >= MB {
            format!("{:.2} MB", bytes as f64 / MB as f64)
        } else if bytes >= KB {
            format!("{:.2} KB", bytes as f64 / KB as f64)
        } else {
            format!("{} B", bytes)
        }
    }
}

/// Execute the expiry analytics command
pub async fn execute(
    cache: Cache,
    blockchain_client: &BlockchainClient,
    period: TimePeriod,
    output: OutputFormat,
    sort_by: ExpiryAnalyticsSortBy,
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
    let _current_block = blockchain_client.get_current_block().await?;

    // Calculate expiry for each batch and group by period
    let mut period_map: HashMap<String, (DateTime<Utc>, Vec<BatchInfo>)> = HashMap::new();

    tracing::info!("Fetching current balances for {} batches from blockchain...", batches.len());

    for batch in &batches {
        // Fetch current remaining balance from blockchain
        let remaining_balance = blockchain_client
            .get_remaining_balance(&batch.batch_id)
            .await
            .unwrap_or_else(|_| "0".to_string());

        // Skip batches with zero balance (already expired)
        if remaining_balance == "0" {
            continue;
        }

        // Create a modified batch with current balance
        let mut current_batch = batch.clone();
        current_batch.normalised_balance = remaining_balance;
        // Calculate TTL using current balance
        let ttl_blocks = calculate_ttl_blocks(
            &current_batch.normalised_balance,
            current_batch.depth,
            price_config.base_price,
        )?;

        let ttl_days_value = blocks_to_days(ttl_blocks);

        // If price change is configured, recalculate with effective price
        let final_ttl_blocks = if let Some(ref price_change) = price_config.price_change {
            let effective_price = price_change.average_price(price_config.base_price, ttl_days_value);
            calculate_ttl_blocks(&current_batch.normalised_balance, current_batch.depth, effective_price)?
        } else {
            ttl_blocks
        };

        // Calculate expiry timestamp
        let seconds_until_expiry = final_ttl_blocks * 5;
        let expiry_timestamp = Utc::now() + chrono::Duration::seconds(seconds_until_expiry as i64);

        // Group by period
        let (period_key, period_start) = ExpiryPeriod::format_period(expiry_timestamp, &period);

        period_map
            .entry(period_key)
            .or_insert((period_start, Vec::new()))
            .1
            .push(current_batch);
    }

    // Create expiry periods
    let mut periods: Vec<ExpiryPeriod> = period_map
        .into_iter()
        .map(|(period_key, (period_start, batches))| {
            let batch_count = batches.len();
            let total_chunks: u128 = batches.iter().map(|b| 1u128 << b.depth).sum();

            ExpiryPeriod {
                period: period_key,
                batch_count,
                total_chunks: ExpiryPeriod::format_number(total_chunks),
                total_storage: ExpiryPeriod::format_storage(total_chunks),
                period_start,
                chunks_raw: total_chunks,
            }
        })
        .collect();

    // Sort results
    match sort_by {
        ExpiryAnalyticsSortBy::Period => {
            periods.sort_by(|a, b| a.period_start.cmp(&b.period_start))
        }
        ExpiryAnalyticsSortBy::Chunks => {
            periods.sort_by(|a, b| b.chunks_raw.cmp(&a.chunks_raw))
        }
        ExpiryAnalyticsSortBy::Storage => {
            periods.sort_by(|a, b| b.chunks_raw.cmp(&a.chunks_raw))
        }
    }

    // Output results
    match output {
        OutputFormat::Table => {
            use tabled::Table;
            let table = Table::new(&periods).to_string();
            println!("\n{}\n", table);
            let total_batches: usize = periods.iter().map(|p| p.batch_count).sum();
            let total_chunks: u128 = periods.iter().map(|p| p.chunks_raw).sum();
            println!(
                "Total periods: {} | Total batches: {} | Total storage: {}",
                periods.len(),
                total_batches,
                ExpiryPeriod::format_storage(total_chunks)
            );
        }
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(&periods)?;
            println!("{}", json);
        }
        OutputFormat::Csv => {
            let mut wtr = csv::Writer::from_writer(std::io::stdout());
            for period in &periods {
                wtr.serialize(period)?;
            }
            wtr.flush()?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Timelike};

    #[test]
    fn test_format_period_day() {
        let timestamp = Utc.with_ymd_and_hms(2025, 1, 15, 14, 30, 0).unwrap();
        let (formatted, period_start) = ExpiryPeriod::format_period(timestamp, &TimePeriod::Day);
        assert_eq!(formatted, "2025-01-15");
        assert_eq!(period_start.hour(), 0);
        assert_eq!(period_start.minute(), 0);
    }

    #[test]
    fn test_format_period_month() {
        let timestamp = Utc.with_ymd_and_hms(2025, 1, 15, 14, 30, 0).unwrap();
        let (formatted, period_start) = ExpiryPeriod::format_period(timestamp, &TimePeriod::Month);
        assert_eq!(formatted, "2025-01");
        assert_eq!(period_start.day(), 1);
    }

    #[test]
    fn test_format_storage() {
        assert_eq!(ExpiryPeriod::format_storage(1), "4.00 KB");
        assert_eq!(ExpiryPeriod::format_storage(256), "1.00 MB");
        assert_eq!(ExpiryPeriod::format_storage(262144), "1.00 GB");
    }
}
