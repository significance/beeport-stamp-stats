use crate::events::StampEvent;
use chrono::{DateTime, Datelike, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Statistics for a time period
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeriodStats {
    pub period_key: String,
    pub period_label: String,
    pub batch_created_count: usize,
    pub batch_topup_count: usize,
    pub batch_depth_increase_count: usize,
    pub total_events: usize,
    pub unique_batches: usize,
}

/// Aggregate events by time period
pub fn aggregate_events(events: &[StampEvent], group_by: &crate::cli::GroupBy) -> Vec<PeriodStats> {
    let mut periods: HashMap<String, PeriodStatsBuilder> = HashMap::new();

    for event in events {
        let period_key = get_period_key(&event.block_timestamp, group_by);
        let period_label = get_period_label(&event.block_timestamp, group_by);

        let stats = periods
            .entry(period_key.clone())
            .or_insert_with(|| PeriodStatsBuilder::new(period_key, period_label));

        stats.add_event(event);
    }

    let mut stats: Vec<_> = periods.into_values().map(|s| s.build()).collect();
    stats.sort_by(|a, b| a.period_key.cmp(&b.period_key));

    stats
}

/// Get period key for grouping
fn get_period_key(timestamp: &DateTime<Utc>, group_by: &crate::cli::GroupBy) -> String {
    match group_by {
        crate::cli::GroupBy::Day => timestamp.format("%Y-%m-%d").to_string(),
        crate::cli::GroupBy::Week => {
            let iso_week = timestamp.iso_week();
            format!("{}-W{:02}", iso_week.year(), iso_week.week())
        }
        crate::cli::GroupBy::Month => timestamp.format("%Y-%m").to_string(),
    }
}

/// Get human-readable period label
fn get_period_label(timestamp: &DateTime<Utc>, group_by: &crate::cli::GroupBy) -> String {
    match group_by {
        crate::cli::GroupBy::Day => timestamp.format("%b %d, %Y").to_string(),
        crate::cli::GroupBy::Week => {
            let iso_week = timestamp.iso_week();
            format!("Week {} of {}", iso_week.week(), iso_week.year())
        }
        crate::cli::GroupBy::Month => timestamp.format("%B %Y").to_string(),
    }
}

/// Builder for period statistics
struct PeriodStatsBuilder {
    period_key: String,
    period_label: String,
    batch_created_count: usize,
    batch_topup_count: usize,
    batch_depth_increase_count: usize,
    batch_ids: std::collections::HashSet<String>,
}

impl PeriodStatsBuilder {
    fn new(period_key: String, period_label: String) -> Self {
        Self {
            period_key,
            period_label,
            batch_created_count: 0,
            batch_topup_count: 0,
            batch_depth_increase_count: 0,
            batch_ids: std::collections::HashSet::new(),
        }
    }

    fn add_event(&mut self, event: &StampEvent) {
        use crate::events::EventType;

        match event.event_type {
            EventType::BatchCreated => self.batch_created_count += 1,
            EventType::BatchTopUp => self.batch_topup_count += 1,
            EventType::BatchDepthIncrease => self.batch_depth_increase_count += 1,
            EventType::PotWithdrawn => {} // PotWithdrawn events don't affect batch stats
            EventType::PriceUpdate => {} // PriceUpdate events don't affect batch stats
            EventType::CopyBatchFailed => {} // CopyBatchFailed events don't affect batch stats
        }

        if let Some(batch_id) = &event.batch_id {
            self.batch_ids.insert(batch_id.clone());
        }
    }

    fn build(self) -> PeriodStats {
        PeriodStats {
            period_key: self.period_key,
            period_label: self.period_label,
            batch_created_count: self.batch_created_count,
            batch_topup_count: self.batch_topup_count,
            batch_depth_increase_count: self.batch_depth_increase_count,
            total_events: self.batch_created_count
                + self.batch_topup_count
                + self.batch_depth_increase_count,
            unique_batches: self.batch_ids.len(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{EventData, EventType};
    use chrono::TimeZone;

    #[test]
    fn test_period_key_day() {
        let timestamp = Utc.with_ymd_and_hms(2025, 3, 15, 12, 0, 0).unwrap();
        let key = get_period_key(&timestamp, &crate::cli::GroupBy::Day);
        assert_eq!(key, "2025-03-15");
    }

    #[test]
    fn test_period_key_week() {
        let timestamp = Utc.with_ymd_and_hms(2025, 3, 15, 12, 0, 0).unwrap();
        let key = get_period_key(&timestamp, &crate::cli::GroupBy::Week);
        assert!(key.starts_with("2025-W"));
    }

    #[test]
    fn test_period_key_month() {
        let timestamp = Utc.with_ymd_and_hms(2025, 3, 15, 12, 0, 0).unwrap();
        let key = get_period_key(&timestamp, &crate::cli::GroupBy::Month);
        assert_eq!(key, "2025-03");
    }

    #[test]
    fn test_aggregate_events() {
        let events = vec![
            StampEvent {
                event_type: EventType::BatchCreated,
                batch_id: Some("0x1234".to_string()),
                block_number: 1000,
                block_timestamp: Utc.with_ymd_and_hms(2025, 3, 15, 12, 0, 0).unwrap(),
                transaction_hash: "0xabcd1".to_string(),
                log_index: 0,
                contract_source: "PostageStamp".to_string(),
                contract_address: None,
                from_address: None,
                data: EventData::BatchCreated {
                    total_amount: "1000000000000000000".to_string(),
                    normalised_balance: "500000000000000000".to_string(),
                    owner: "0x5678".to_string(),
                    depth: 20,
                    bucket_depth: 16,
                    immutable_flag: false,
                    payer: None,
                },
            },
            StampEvent {
                event_type: EventType::BatchTopUp,
                batch_id: Some("0x1234".to_string()),
                block_number: 1001,
                block_timestamp: Utc.with_ymd_and_hms(2025, 3, 15, 13, 0, 0).unwrap(),
                transaction_hash: "0xabcd2".to_string(),
                log_index: 0,
                contract_source: "PostageStamp".to_string(),
                contract_address: None,
                from_address: None,
                data: EventData::BatchTopUp {
                    topup_amount: "100000000000000000".to_string(),
                    normalised_balance: "600000000000000000".to_string(),
                    payer: None,
                },
            },
        ];

        let stats = aggregate_events(&events, &crate::cli::GroupBy::Day);

        assert_eq!(stats.len(), 1);
        assert_eq!(stats[0].batch_created_count, 1);
        assert_eq!(stats[0].batch_topup_count, 1);
        assert_eq!(stats[0].total_events, 2);
        assert_eq!(stats[0].unique_batches, 1);
    }
}
