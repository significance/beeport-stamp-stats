//! Unit tests for price calculation module
//!
//! Tests cover:
//! - TTL calculations
//! - Block/day conversions
//! - Price change modeling
//! - Edge cases

use beeport_stamp_stats::price::{
    blocks_to_days, calculate_ttl_blocks, days_to_blocks, PriceChange, PriceConfig,
};

#[test]
fn test_price_change_parsing() {
    let change = "200:10".parse::<PriceChange>().unwrap();
    assert_eq!(change.percentage, 200.0);
    assert_eq!(change.days, 10.0);

    let change = "50:7".parse::<PriceChange>().unwrap();
    assert_eq!(change.percentage, 50.0);
    assert_eq!(change.days, 7.0);

    // Test invalid formats
    assert!("200".parse::<PriceChange>().is_err());
    assert!("200:0".parse::<PriceChange>().is_err());
    assert!("abc:def".parse::<PriceChange>().is_err());
    assert!("200:-5".parse::<PriceChange>().is_err());
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
fn test_average_price_zero_ttl() {
    let change = PriceChange {
        percentage: 100.0,
        days: 10.0,
    };

    let current_price = 1000u128;
    let avg = change.average_price(current_price, 0.0);

    // With zero TTL, should return current price
    assert_eq!(avg, current_price);
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
fn test_ttl_calculation_large_balance() {
    // Balance: 10,000,000,000 PLUR (10 billion)
    // Depth: 18 (2^18 = 262,144 chunks)
    // Price: 1000 PLUR per chunk per block
    let ttl = calculate_ttl_blocks("10000000000", 18, 1000).unwrap();

    // Expected: 10,000,000,000 / (1000 * 262,144) ≈ 38 blocks
    assert_eq!(ttl, 38);
}

#[test]
fn test_ttl_calculation_small_balance() {
    // Balance: 100,000 PLUR
    // Depth: 16 (2^16 = 65,536 chunks)
    // Price: 10 PLUR per chunk per block
    let ttl = calculate_ttl_blocks("100000", 16, 10).unwrap();

    // Expected: 100,000 / (10 * 65,536) ≈ 0 blocks
    assert_eq!(ttl, 0);
}

#[test]
fn test_ttl_calculation_zero_price_error() {
    let result = calculate_ttl_blocks("1000000", 20, 0);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Price cannot be zero"));
}

#[test]
fn test_ttl_calculation_invalid_balance() {
    let result = calculate_ttl_blocks("invalid", 20, 100);
    assert!(result.is_err());
}

#[test]
fn test_blocks_to_days() {
    // 17,280 blocks = 1 day (at 5 seconds per block)
    let days = blocks_to_days(17280, 5.0);
    assert!((days - 1.0).abs() < 0.01);

    // 172,800 blocks = 10 days
    let days = blocks_to_days(172800, 5.0);
    assert!((days - 10.0).abs() < 0.01);

    // Test with different block time (12 seconds like Ethereum)
    let days = blocks_to_days(7200, 12.0);
    assert!((days - 1.0).abs() < 0.01);
}

#[test]
fn test_blocks_to_days_zero_blocks() {
    let days = blocks_to_days(0, 5.0);
    assert_eq!(days, 0.0);
}

#[test]
fn test_blocks_to_days_large_number() {
    // 1 million blocks at 5 seconds per block
    let days = blocks_to_days(1_000_000, 5.0);
    assert!((days - 57.87).abs() < 0.01); // ~57.87 days
}

#[test]
fn test_days_to_blocks() {
    // 1 day = 17,280 blocks (at 5 seconds per block)
    let blocks = days_to_blocks(1.0, 5.0);
    assert_eq!(blocks, 17280);

    // 10 days = 172,800 blocks
    let blocks = days_to_blocks(10.0, 5.0);
    assert_eq!(blocks, 172800);

    // Test with different block time (12 seconds like Ethereum)
    let blocks = days_to_blocks(1.0, 12.0);
    assert_eq!(blocks, 7200);
}

#[test]
fn test_days_to_blocks_fractional() {
    // 0.5 days at 5 seconds per block
    let blocks = days_to_blocks(0.5, 5.0);
    assert_eq!(blocks, 8640); // Half of 17,280
}

#[test]
fn test_days_to_blocks_zero() {
    let blocks = days_to_blocks(0.0, 5.0);
    assert_eq!(blocks, 0);
}

#[test]
fn test_price_config_new() {
    let config = PriceConfig::new(1000);
    assert_eq!(config.base_price, 1000);
    assert!(config.price_change.is_none());
}

#[test]
fn test_price_config_with_price_change() {
    let change = PriceChange {
        percentage: 100.0,
        days: 10.0,
    };
    let config = PriceConfig::with_price_change(1000, change.clone());

    assert_eq!(config.base_price, 1000);
    assert!(config.price_change.is_some());

    let stored_change = config.price_change.unwrap();
    assert_eq!(stored_change.percentage, 100.0);
    assert_eq!(stored_change.days, 10.0);
}

#[test]
fn test_effective_price_without_change() {
    let config = PriceConfig::new(1000);
    assert_eq!(config.effective_price(10.0), 1000);
}

#[test]
fn test_effective_price_with_change() {
    let change = PriceChange {
        percentage: 100.0,
        days: 10.0,
    };
    let config = PriceConfig::with_price_change(1000, change);
    let effective = config.effective_price(10.0);

    // Should return average price, not base price
    assert!(effective > 1000);
}

#[test]
fn test_price_change_negative_days_error() {
    let result = "100:-5".parse::<PriceChange>();
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Days must be positive"));
}

#[test]
fn test_roundtrip_blocks_days_conversion() {
    let original_blocks = 100000u64;
    let days = blocks_to_days(original_blocks, 5.0);
    let converted_back = days_to_blocks(days, 5.0);

    // Should be very close (within rounding)
    assert!((original_blocks as i64 - converted_back as i64).abs() <= 1);
}
