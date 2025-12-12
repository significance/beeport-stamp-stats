use crate::error::{Result, StampError};
use std::str::FromStr;

/// Price configuration for batch calculations
#[derive(Debug, Clone)]
pub struct PriceConfig {
    /// Base price per chunk per block in PLUR (smallest unit)
    pub base_price: u128,
    /// Optional price change configuration
    pub price_change: Option<PriceChange>,
}

/// Price change configuration
#[derive(Debug, Clone)]
pub struct PriceChange {
    /// Percentage change (e.g., 200 for 200% increase)
    pub percentage: f64,
    /// Time period in days over which the change occurs
    pub days: f64,
}

impl PriceChange {
    /// Parse price change from string format "percentage:days"
    /// Example: "200:10" means 200% increase over 10 days
    pub fn from_str(s: &str) -> Result<Self> {
        let parts: Vec<&str> = s.split(':').collect();
        if parts.len() != 2 {
            return Err(StampError::Parse(
                "Price change must be in format 'percentage:days' (e.g., '200:10')".to_string(),
            ));
        }

        let percentage = parts[0]
            .parse::<f64>()
            .map_err(|_| StampError::Parse("Invalid percentage value".to_string()))?;

        let days = parts[1]
            .parse::<f64>()
            .map_err(|_| StampError::Parse("Invalid days value".to_string()))?;

        if days <= 0.0 {
            return Err(StampError::Parse("Days must be positive".to_string()));
        }

        Ok(Self { percentage, days })
    }

    /// Calculate the daily growth rate
    /// Formula: r = (1 + percentage/100)^(1/days)
    pub fn daily_growth_rate(&self) -> f64 {
        (1.0 + self.percentage / 100.0).powf(1.0 / self.days)
    }

    /// Calculate the effective average price over a given TTL in days
    ///
    /// When prices are changing exponentially, the average price is not simply
    /// the arithmetic mean. We need to integrate the exponential price curve.
    ///
    /// Formula: avg_price = current_price × (r^ttl_days - 1) / (ln(r) × ttl_days)
    ///
    /// Where:
    /// - r is the daily growth rate
    /// - ttl_days is the time to live in days
    ///
    /// Special case: When r ≈ 1 (no growth), this approaches current_price
    pub fn average_price(&self, current_price: u128, ttl_days: f64) -> u128 {
        if ttl_days <= 0.0 {
            return current_price;
        }

        let r = self.daily_growth_rate();

        // Special case: if growth rate is very close to 1 (no growth), return current price
        if (r - 1.0).abs() < 1e-10 {
            return current_price;
        }

        let r_to_ttl = r.powf(ttl_days);
        let numerator = r_to_ttl - 1.0;
        let denominator = r.ln() * ttl_days;

        let multiplier = numerator / denominator;

        // Calculate average price
        let avg_price = (current_price as f64) * multiplier;

        avg_price.round() as u128
    }
}

impl PriceConfig {
    /// Create a new price configuration with just a base price
    pub fn new(base_price: u128) -> Self {
        Self {
            base_price,
            price_change: None,
        }
    }

    /// Create a price configuration with price change
    pub fn with_price_change(base_price: u128, price_change: PriceChange) -> Self {
        Self {
            base_price,
            price_change: Some(price_change),
        }
    }

    /// Parse price from string (PLUR units)
    #[allow(dead_code)]
    pub fn parse_price(s: &str) -> Result<u128> {
        u128::from_str(s)
            .map_err(|_| StampError::Parse("Invalid price value".to_string()))
    }

    /// Get the effective price for a given TTL
    /// If price change is configured, returns the average price over the TTL period
    /// Otherwise, returns the base price
    #[allow(dead_code)]
    pub fn effective_price(&self, ttl_days: f64) -> u128 {
        match &self.price_change {
            Some(change) => change.average_price(self.base_price, ttl_days),
            None => self.base_price,
        }
    }
}

/// Calculate Time To Live (TTL) in blocks for a batch
///
/// Formula: TTL = normalised_balance / (price_per_chunk_per_block × chunks)
///
/// Where:
/// - normalised_balance: The balance in PLUR (smallest unit)
/// - price_per_chunk_per_block: Price per chunk per block in PLUR
/// - chunks: Number of chunks (2^depth)
pub fn calculate_ttl_blocks(
    normalised_balance: &str,
    depth: u8,
    price_per_chunk_per_block: u128,
) -> Result<u64> {
    let balance = u128::from_str(normalised_balance)
        .map_err(|_| StampError::Parse("Invalid normalised balance".to_string()))?;

    if price_per_chunk_per_block == 0 {
        return Err(StampError::Parse("Price cannot be zero".to_string()));
    }

    let chunks: u128 = 1u128 << depth; // 2^depth
    let total_price_per_block = price_per_chunk_per_block * chunks;

    let ttl = balance / total_price_per_block;

    Ok(ttl as u64)
}

/// Calculate Time To Live in days from blocks
/// Assuming 5 second block time on Gnosis Chain
pub fn blocks_to_days(blocks: u64) -> f64 {
    const SECONDS_PER_BLOCK: f64 = 5.0;
    const SECONDS_PER_DAY: f64 = 86400.0;

    (blocks as f64) * SECONDS_PER_BLOCK / SECONDS_PER_DAY
}

/// Calculate days to blocks
#[allow(dead_code)]
pub fn days_to_blocks(days: f64) -> u64 {
    const SECONDS_PER_BLOCK: f64 = 5.0;
    const SECONDS_PER_DAY: f64 = 86400.0;

    ((days * SECONDS_PER_DAY) / SECONDS_PER_BLOCK).round() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_price_change_parsing() {
        let change = PriceChange::from_str("200:10").unwrap();
        assert_eq!(change.percentage, 200.0);
        assert_eq!(change.days, 10.0);

        let change = PriceChange::from_str("50:7").unwrap();
        assert_eq!(change.percentage, 50.0);
        assert_eq!(change.days, 7.0);

        // Test invalid formats
        assert!(PriceChange::from_str("200").is_err());
        assert!(PriceChange::from_str("200:0").is_err());
        assert!(PriceChange::from_str("abc:def").is_err());
    }

    #[test]
    fn test_daily_growth_rate() {
        // 100% increase over 1 day = 2x growth
        let change = PriceChange {
            percentage: 100.0,
            days: 1.0,
        };
        assert!((change.daily_growth_rate() - 2.0).abs() < 1e-10);

        // 100% increase over 2 days
        let change = PriceChange {
            percentage: 100.0,
            days: 2.0,
        };
        let expected = 2.0_f64.sqrt(); // ~1.414
        assert!((change.daily_growth_rate() - expected).abs() < 1e-10);
    }

    #[test]
    fn test_average_price_no_growth() {
        let change = PriceChange {
            percentage: 0.0,
            days: 10.0,
        };

        let current_price = 1000u128;
        let avg = change.average_price(current_price, 30.0);

        // With no growth, average should equal current price
        assert_eq!(avg, current_price);
    }

    #[test]
    fn test_average_price_with_growth() {
        // 100% increase over 10 days
        let change = PriceChange {
            percentage: 100.0,
            days: 10.0,
        };

        let current_price = 1000u128;
        let avg = change.average_price(current_price, 10.0);

        // Average price should be between current and final price (2000)
        // Due to exponential growth, it should be closer to geometric mean
        assert!(avg > current_price);
        assert!(avg < 2000);

        // For exponential growth, the average should be roughly 1442 for this case
        // (This is the integral of the exponential curve divided by the period)
        assert!((avg as f64 - 1442.0).abs() < 50.0);
    }

    #[test]
    fn test_ttl_calculation() {
        // Balance: 1,000,000,000 PLUR
        // Depth: 20 (2^20 = 1,048,576 chunks)
        // Price: 100 PLUR per chunk per block
        let ttl = calculate_ttl_blocks("1000000000", 20, 100).unwrap();

        // Expected: 1,000,000,000 / (100 * 1,048,576) ≈ 9 blocks
        assert_eq!(ttl, 9);
    }

    #[test]
    fn test_blocks_to_days() {
        // 17,280 blocks = 1 day (at 5 seconds per block)
        let days = blocks_to_days(17280);
        assert!((days - 1.0).abs() < 0.01);

        // 172,800 blocks = 10 days
        let days = blocks_to_days(172800);
        assert!((days - 10.0).abs() < 0.01);
    }

    #[test]
    fn test_days_to_blocks() {
        // 1 day = 17,280 blocks
        let blocks = days_to_blocks(1.0);
        assert_eq!(blocks, 17280);

        // 10 days = 172,800 blocks
        let blocks = days_to_blocks(10.0);
        assert_eq!(blocks, 172800);
    }

    #[test]
    fn test_price_config() {
        let config = PriceConfig::new(1000);
        assert_eq!(config.effective_price(10.0), 1000);

        let change = PriceChange {
            percentage: 100.0,
            days: 10.0,
        };
        let config = PriceConfig::with_price_change(1000, change);
        let effective = config.effective_price(10.0);

        // Should return average price, not base price
        assert!(effective > 1000);
    }
}
