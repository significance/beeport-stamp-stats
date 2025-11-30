use crate::events::StampEvent;

/// Event hook trait for handling new events
pub trait EventHook: Send + Sync {
    /// Called when a new event is detected
    fn on_event(&self, event: &StampEvent);
}

/// Default stub hook implementation
pub struct StubHook;

impl EventHook for StubHook {
    fn on_event(&self, event: &StampEvent) {
        tracing::debug!(
            "Hook invoked for event: {} at block {}",
            event.event_type,
            event.block_number
        );
        // Stub: In the future, this could trigger notifications, webhooks, etc.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{EventData, EventType};
    use chrono::Utc;

    #[test]
    fn test_stub_hook() {
        let hook = StubHook;
        let event = StampEvent {
            event_type: EventType::BatchCreated,
            batch_id: "0x1234".to_string(),
            block_number: 1000,
            block_timestamp: Utc::now(),
            transaction_hash: "0xabcd".to_string(),
            log_index: 0,
            data: EventData::BatchCreated {
                total_amount: "1000000000000000000".to_string(),
                normalised_balance: "500000000000000000".to_string(),
                owner: "0x5678".to_string(),
                depth: 20,
                bucket_depth: 16,
                immutable_flag: false,
            },
        };

        // Should not panic
        hook.on_event(&event);
    }
}
