# plots

`plot.py` draws the "storage incentives hero dashboard": one chart, block number on the x-axis,
eight series on stacked y-axes, dark theme. It reads straight from the PostgreSQL database that
`beeport-stamp-stats` fills; it does not go to the chain.

| Series | Marker | Source |
|---|---|---|
| Reveals | ● circle | `CountReveals` events, only for rounds that produced a `WinnerSelected` |
| Commits | ■ square | `CountCommits`, same rounds |
| Chunks | ▼ triangle down | `ChunkCount` (= `PostageStamp.validChunkCount()` at claim), same rounds |
| Price | neon line, spline-smoothed | `PriceUpdate` events from the PriceOracle |
| Freeze Time | ▲ triangle up | `StakeFrozen.freeze_time` |
| Frozen Events Count | ✕ cross | number of `StakeFrozen` events between consecutive `WinnerSelected` events |
| Pot Withdrawn (log scale) | ◆ diamond | `PotWithdrawn.pot_total_amount` from the `events` table |
| Reward per Node | + plus, white | 10-round rolling `sum(pot withdrawn) / sum(commits)` |
| Depth | ⬣ hexagon | `TruthSelected.truth_depth`: the storage depth of the round's agreed truth |

Markers are defined once in `plot.py` (`MARKERS`) and shared with the HTML renderer, so the two
outputs match. The subtitle under the title names the database and any time window.

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

Restrict to a recent window with `SINCE_DAYS` (the cutoff block is looked up from event
timestamps; the title says so):

```bash
SINCE_DAYS=90 python3 plot.py          # or: just graph-3-month (writes incentives_hero_dashboard_3m.png)
just graph-1-month                     # likewise graph-1-week, graph-1-day; graph-all is the full history
just days=30 graph-dev                 # the HTML recipes take a free-form window
```

## Interactive HTML version

`plot_html.py` draws the same series with plotly into one self-contained HTML file (plotly.js
inlined, about 13 MB with the full history) that you can zoom, pan and hover:

```bash
python3 plot_html.py                    # writes incentives_hero_dashboard.html here
just graph-html                         # same, from the project root
just graph-dev                          # build it and serve plots/ on http://localhost:8787 (Node http-server), opens the browser
just port=9000 graph-dev                # different port
```

The HTML is gitignored; rebuild it after a sync. `plot_html.py` imports `series_queries`,
`load_dataframes` and `db_settings` from `plot.py`, so the two stay in step: add a series to
`series_queries()` and both outputs pick it up.

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
