use crate::events::StampEvent;

/// Event hook trait for handling new events
///
/// Implementations can filter events by contract source and provide
/// contract-specific behavior
pub trait EventHook: Send + Sync {
    /// Called when a new event is detected from any contract
    fn on_event(&self, event: &StampEvent);

    /// Called when a new event is detected from PostageStamp contract
    fn on_postage_stamp_event(&self, event: &StampEvent) {
        tracing::debug!(
            "PostageStamp event: {} at block {}",
            event.event_type,
            event.block_number
        );
    }

    /// Called when a new event is detected from StampsRegistry contract
    fn on_stamps_registry_event(&self, event: &StampEvent) {
        tracing::debug!(
            "StampsRegistry event: {} at block {}",
            event.event_type,
            event.block_number
        );
    }
}

/// Default stub hook implementation that routes events to contract-specific handlers
pub struct StubHook;

impl EventHook for StubHook {
    fn on_event(&self, event: &StampEvent) {
        tracing::debug!(
            "Hook invoked for {} event: {} at block {}",
            event.contract_source,
            event.event_type,
            event.block_number
        );

        // Route to contract-specific handlers
        match event.contract_source.as_str() {
            "PostageStamp" => self.on_postage_stamp_event(event),
            "StampsRegistry" => self.on_stamps_registry_event(event),
            _ => tracing::warn!("Unknown contract source: {}", event.contract_source),
        }
    }

    fn on_postage_stamp_event(&self, event: &StampEvent) {
        tracing::info!(
            "PostageStamp contract: {} event for batch {:?} at block {}",
            event.event_type,
            event.batch_id,
            event.block_number
        );
        // Custom logic for PostageStamp events can be added here
        // For example: webhooks, notifications, analytics, etc.
    }

    fn on_stamps_registry_event(&self, event: &StampEvent) {
        tracing::info!(
            "StampsRegistry contract: {} event for batch {:?} at block {}",
            event.event_type,
            event.batch_id,
            event.block_number
        );
        // Custom logic for StampsRegistry events can be added here
        // For example: different webhooks, notifications, analytics, etc.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{EventData, EventType};
    use chrono::Utc;

    #[test]
    fn test_stub_hook_postage_stamp() {
        let hook = StubHook;
        let event = StampEvent {
            event_type: EventType::BatchCreated,
            batch_id: Some("0x1234".to_string()),
            block_number: 1000,
            block_timestamp: Utc::now(),
            transaction_hash: "0xabcd".to_string(),
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
        };

        // Should not panic
        hook.on_event(&event);
    }

    #[test]
    fn test_stub_hook_stamps_registry() {
        let hook = StubHook;
        let event = StampEvent {
            event_type: EventType::BatchCreated,
            batch_id: Some("0x5678".to_string()),
            block_number: 2000,
            block_timestamp: Utc::now(),
            transaction_hash: "0xdef0".to_string(),
            log_index: 0,
            contract_source: "StampsRegistry".to_string(),
            contract_address: None,
            from_address: None,
            data: EventData::BatchCreated {
                total_amount: "2000000000000000000".to_string(),
                normalised_balance: "1000000000000000000".to_string(),
                owner: "0x9abc".to_string(),
                depth: 22,
                bucket_depth: 16,
                immutable_flag: true,
                payer: Some("0xpayer".to_string()),
            },
        };

        // Should not panic
        hook.on_event(&event);
    }
}
