# plots

`plot.py` draws the "storage incentives hero dashboard": one chart, block number on the x-axis,
eight series on stacked y-axes, dark theme. It reads straight from the PostgreSQL database that
`beeport-stamp-stats` fills; it does not go to the chain.

| Series | Source |
|---|---|
| Reveals, Commits, Chunks | `CountReveals` / `CountCommits` / `ChunkCount` events, only for rounds that produced a `WinnerSelected` |
| Price (neon line, spline-smoothed) | `PriceUpdate` events from the PriceOracle |
| Freeze Time | `StakeFrozen.freeze_time` |
| Frozen Events Count | number of `StakeFrozen` events between consecutive `WinnerSelected` events |
| Pot Withdrawn (log scale) | `PotWithdrawn.pot_total_amount` from the `events` table |
| Reward per Node | 10-round rolling `sum(pot withdrawn) / sum(commits)` |

Last verified working: 2026-10-15 (Python 3.13, matplotlib 3.10, pandas 2.3, against `beeport_tx_prod`).

## Run

```bash
cd plots
pip install -r requirements.txt        # psycopg2-binary pandas matplotlib numpy scipy python-dotenv
python3 plot.py                        # opens an interactive window AND writes incentives_hero_dashboard.png here
```

Connection settings come from environment variables, optionally via a `.env` file in this
directory (see `.env.example`). Defaults: `DB_NAME=beeport_tx_prod`, `DB_USER=sig32`,
`DB_PASS=` (empty), `DB_HOST=localhost`, `DB_PORT=5432`.

Headless (no window, just the PNG), or to choose the output path:

```bash
MPLBACKEND=Agg python3 -c "import plot; plot.fetch_and_plot_metrics('out.png')"
```

Point it at another database for a one-off:

```bash
DB_NAME=beeport3m python3 plot.py
```

## Notes

- The PNG is 300 dpi, about 1.4 MB; the chart takes ~15 s to build.
- pandas prints a `UserWarning` about SQLAlchemy because the script passes a raw psycopg2
  connection. It is harmless.
- The checked-in `incentives_hero_dashboard.png` was rendered on 2026-01-24 from a database called
  `beeport2` that no longer exists. Re-render it after the next full sync.
- `bk/` holds the earlier iterations (`plot.py` through `plot5.py`) kept for reference; `plot.py`
  in this directory is the current one.
- Each series gets its own y-axis, pushed outward 75 px per axis, so the figure is deliberately
  wide (19.2 × 10.8 in).
