use crate::batch::aggregate_events;
use crate::cli::GroupBy;
use crate::error::Result;
use crate::events::{BatchInfo, EventData, EventType, StampEvent};
use tabled::{
    Table, Tabled,
    settings::{Alignment, Modify, Style, object::Rows},
};

/// Display events in a markdown table
pub fn display_events(events: &[StampEvent]) -> Result<()> {
    if events.is_empty() {
        println!("\nNo events found.\n");
        return Ok(());
    }

    println!("\n## Postage Stamp Events\n");

    #[derive(Tabled)]
    struct EventRow {
        #[tabled(rename = "Block")]
        block: String,
        #[tabled(rename = "Type")]
        event_type: String,
        #[tabled(rename = "Contract")]
        contract: String,
        #[tabled(rename = "Batch ID")]
        batch_id: String,
        #[tabled(rename = "Details")]
        details: String,
        #[tabled(rename = "Timestamp")]
        timestamp: String,
    }

    let rows: Vec<EventRow> = events
        .iter()
        .map(|event| EventRow {
            block: event.block_number.to_string(),
            event_type: event.event_type.to_string(),
            contract: truncate_contract_name(&event.contract_source),
            batch_id: event.batch_id.as_deref().map(truncate_hash).unwrap_or_else(|| "N/A".to_string()),
            details: format_event_details(&event.data),
            timestamp: event.block_timestamp.format("%Y-%m-%d %H:%M").to_string(),
        })
        .collect();

    let mut table = Table::new(rows);
    table
        .with(Style::markdown())
        .with(Modify::new(Rows::new(1..)).with(Alignment::left()));

    println!("{table}\n");
    println!("**Total events:** {}\n", events.len());

    Ok(())
}

/// Display summary statistics
pub fn display_summary(
    events: &[StampEvent],
    batches: &[BatchInfo],
    group_by: GroupBy,
) -> Result<()> {
    if events.is_empty() {
        println!("\nNo events found in cache.\n");
        return Ok(());
    }

    println!("\n## Postage Stamp Statistics Summary\n");

    // Overall statistics
    println!("### Overall Statistics\n");
    let batch_created = events
        .iter()
        .filter(|e| matches!(e.event_type, EventType::BatchCreated))
        .count();
    let batch_topup = events
        .iter()
        .filter(|e| matches!(e.event_type, EventType::BatchTopUp))
        .count();
    let batch_depth_increase = events
        .iter()
        .filter(|e| matches!(e.event_type, EventType::BatchDepthIncrease))
        .count();

    // Count events by contract
    let postage_stamp_count = events
        .iter()
        .filter(|e| e.contract_source == "PostageStamp")
        .count();
    let stamps_registry_count = events
        .iter()
        .filter(|e| e.contract_source == "StampsRegistry")
        .count();

    println!("- **Total Events:** {}", events.len());
    println!("  - PostageStamp: {postage_stamp_count}");
    println!("  - StampsRegistry: {stamps_registry_count}");
    println!("- **Batch Created:** {batch_created}");
    println!("- **Batch Top-ups:** {batch_topup}");
    println!("- **Batch Depth Increases:** {batch_depth_increase}");
    println!("- **Unique Batches:** {}\n", batches.len());

    // Time range
    if let (Some(first), Some(last)) = (events.first(), events.last()) {
        println!("### Time Range\n");
        println!(
            "- **From:** {}",
            first.block_timestamp.format("%Y-%m-%d %H:%M")
        );
        println!(
            "- **To:** {}",
            last.block_timestamp.format("%Y-%m-%d %H:%M")
        );
        println!(
            "- **Duration:** {} days\n",
            (last.block_timestamp - first.block_timestamp).num_days()
        );
    }

    // Aggregate by period
    let period_stats = aggregate_events(events, &group_by);

    println!("### Activity by {group_by:?}\n");

    #[derive(Tabled)]
    struct PeriodRow {
        #[tabled(rename = "Period")]
        period: String,
        #[tabled(rename = "Created")]
        created: usize,
        #[tabled(rename = "Top-ups")]
        topups: usize,
        #[tabled(rename = "Depth Inc.")]
        depth_inc: usize,
        #[tabled(rename = "Total Events")]
        total: usize,
        #[tabled(rename = "Unique Batches")]
        unique: usize,
    }

    let rows: Vec<PeriodRow> = period_stats
        .iter()
        .map(|stats| PeriodRow {
            period: stats.period_label.clone(),
            created: stats.batch_created_count,
            topups: stats.batch_topup_count,
            depth_inc: stats.batch_depth_increase_count,
            total: stats.total_events,
            unique: stats.unique_batches,
        })
        .collect();

    let mut table = Table::new(rows);
    table
        .with(Style::markdown())
        .with(Modify::new(Rows::new(1..)).with(Alignment::right()));

    println!("{table}\n");

    // Most active period
    if let Some(most_active) = period_stats.iter().max_by_key(|s| s.total_events) {
        println!("### Most Active Period\n");
        println!(
            "**{}** with {} events\n",
            most_active.period_label, most_active.total_events
        );
    }

    // Batch details
    if !batches.is_empty() {
        println!("### Recent Batches\n");

        #[derive(Tabled)]
        struct BatchRow {
            #[tabled(rename = "Batch ID")]
            batch_id: String,
            #[tabled(rename = "Owner")]
            owner: String,
            #[tabled(rename = "Depth")]
            depth: u8,
            #[tabled(rename = "Bucket Depth")]
            bucket_depth: u8,
            #[tabled(rename = "Immutable")]
            immutable: String,
            #[tabled(rename = "Created")]
            created: String,
        }

        let recent_batches: Vec<BatchRow> = batches
            .iter()
            .rev()
            .take(10)
            .map(|batch| BatchRow {
                batch_id: truncate_hash(&batch.batch_id),
                owner: truncate_hash(&batch.owner),
                depth: batch.depth,
                bucket_depth: batch.bucket_depth,
                immutable: if batch.immutable { "Yes" } else { "No" }.to_string(),
                created: batch.created_at.format("%Y-%m-%d %H:%M").to_string(),
            })
            .collect();

        let mut table = Table::new(recent_batches);
        table
            .with(Style::markdown())
            .with(Modify::new(Rows::new(1..)).with(Alignment::left()));

        println!("{table}\n");
    }

    Ok(())
}

/// Format event details for display
fn format_event_details(data: &EventData) -> String {
    match data {
        EventData::BatchCreated {
            owner,
            depth,
            bucket_depth,
            immutable_flag,
            ..
        } => {
            format!(
                "Owner: {}, Depth: {}, Bucket: {}, Immutable: {}",
                truncate_hash(owner),
                depth,
                bucket_depth,
                if *immutable_flag { "Yes" } else { "No" }
            )
        }
        EventData::BatchTopUp { topup_amount, .. } => {
            format!("Top-up: {} BZZ", format_amount(topup_amount))
        }
        EventData::BatchDepthIncrease { new_depth, .. } => {
            format!("New Depth: {new_depth}")
        }
        EventData::PotWithdrawn { recipient, total_amount } => {
            format!("Recipient: {}, Amount: {} BZZ", truncate_hash(recipient), format_amount(total_amount))
        }
        EventData::PriceUpdate { price } => {
            format!("Price: {} PLUR", format_amount(price))
        }
        EventData::CopyBatchFailed { index, batch_id } => {
            format!("Index: {}, Batch: {}", index, truncate_hash(batch_id))
        }
    }
}

/// Truncate hash to first 6 and last 4 characters
fn truncate_hash(hash: &str) -> String {
    if hash.len() > 12 {
        format!("{}...{}", &hash[..6], &hash[hash.len() - 4..])
    } else {
        hash.to_string()
    }
}

/// Truncate contract name for display
fn truncate_contract_name(contract: &str) -> String {
    match contract {
        "PostageStamp" => "PostageStamp".to_string(),
        "StampsRegistry" => "StampsReg".to_string(),
        _ => contract.to_string(),
    }
}

/// Format amount from wei to a more readable format
fn format_amount(amount: &str) -> String {
    if let Ok(value) = amount.parse::<u128>() {
        let eth_value = value as f64 / 1e16;
        format!("{eth_value:.4}")
    } else {
        amount.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_hash() {
        let hash = "0x1234567890abcdef1234567890abcdef";
        let truncated = truncate_hash(hash);
        assert_eq!(truncated, "0x1234...cdef");
    }

    #[test]
    fn test_truncate_short_hash() {
        let hash = "0x1234";
        let truncated = truncate_hash(hash);
        assert_eq!(truncated, "0x1234");
    }

    #[test]
    fn test_format_amount() {
        let amount = "1000000000000000000"; // 1e18 = 100 PLUR
        let formatted = format_amount(amount);
        assert_eq!(formatted, "100.0000");
    }

    #[test]
    fn test_format_event_details() {
        let data = EventData::BatchCreated {
            total_amount: "1000000000000000000".to_string(),
            normalised_balance: "500000000000000000".to_string(),
            owner: "0x1234567890abcdef".to_string(),
            depth: 20,
            bucket_depth: 16,
            immutable_flag: false,
            payer: None,
        };

        let formatted = format_event_details(&data);
        assert!(formatted.contains("Depth: 20"));
        assert!(formatted.contains("Bucket: 16"));
        assert!(formatted.contains("Immutable: No"));
    }
}
