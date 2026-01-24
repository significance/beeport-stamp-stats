  Example:
  # Weekly summary for last 12 months
  ./target/release/beeport-stamp-stats --config config.yaml payment-channel-summary

  # Daily summary for last 3 months
  ./target/release/beeport-stamp-stats --config config.yaml payment-channel-summary --group-by day --months 3

  # Monthly summary for all time
  ./target/release/beeport-stamp-stats --config config.yaml payment-channel-summary --group-by month --months 0

  The command is ready to use once you have synced chequebook events into the database!
