# beeport-stamp-stats

Rust CLI that pulls Swarm network events from Gnosis Chain into a local database and runs
analytics on them: postage stamp batches, storage-incentive contracts (price oracle, staking,
redistribution game) and SWAP payment-channel chequebooks.

Requests are spread round-robin across several public RPC endpoints, each with its own adaptive
rate limiter, so a multi-million-block catch-up works without any API keys. It is slow on public
endpoints (expect hours for a full year) but it gets there.

This README describes branch `feat/improve-retrieval-efficiency-with-bandwidth`. For the current
verified state of git, build and databases see [`state.md`](./state.md); for the running log see
[`plan.md`](./plan.md); for architecture and conventions see [`CLAUDE.md`](./CLAUDE.md).

## Quick start

With [`just`](https://github.com/casey/just) installed (`brew install just`), the everyday
commands are recipes in the `justfile`:

```bash
just            # list recipes
just status     # per-contract highest block vs chain head, cached price, is a sync running
just sync-bg    # catch up to chain head in the background; prints the log path and a tail command
just tail       # follow the newest sync log
just graph-all  # render plots/incentives_hero_dashboard.png (full history) and open it; `just graph` is an alias
just graph-3-month   # last 90 days → plots/incentives_hero_dashboard_3m.png; also graph-1-month, graph-1-week, graph-1-day
just graph-html # build the single-file interactive HTML dashboard (add days=90 for a window)
just graph-dev  # build it and serve it on http://localhost:8787 (Node http-server)
just test       # cargo test + clippy
just fill Redistribution 45140587 47968999   # one contract over an explicit range
just stop       # kill a running sync
```

Recipes default to `config-prod.yaml` and the `beeport_tx_prod` database; override with
`just config=config.yaml db=beeport3m status`. Without `just`:

```bash
cargo build --release
B=./target/release/beeport-stamp-stats

# Point at the database that holds the full history (see "Configuration")
$B --config config-prod.yaml sync            # catch up from the last stored block to chain head
$B --config config-prod.yaml price           # current storage price from the PriceOracle
$B --config config-prod.yaml batch-status --sort-by ttl --hide-zero-balance
$B --config config-prod.yaml expiry-analytics --period week
```

With no `--config`, the tool uses built-in defaults: a single RPC (`https://rpc.gnosis.gateway.fm`)
and a SQLite file `./stamp-cache.db`.

## What it tracks

| Contract | Address (Gnosis) | From block | Events |
|---|---|---|---|
| PostageStamp | `0x45a1502382541Cd610CC9068e88727426b696293` | 31,306,385 | BatchCreated, BatchTopUp, BatchDepthIncrease, PotWithdrawn, PriceUpdate, CopyBatchFailed |
| StampsRegistry | `0x5EBfBeFB1E88391eFb022d5d33302f50a46bF4f3` | 42,390,510 | same as PostageStamp, plus payer |
| PriceOracle | `0x47EeF336e7fE5bED98499A4696bce8f28c1B0a8b` | 37,339,168 | PriceUpdate, StampPriceUpdateFailed |
| StakeRegistry | `0xda2a16EE889E7f04980A8d597b48c8D51B9518F4` | 40,430,237 | StakeUpdated, StakeSlashed, StakeFrozen, OverlayChanged, StakeWithdrawn |
| Redistribution | `0x5069cdfB3D9E56d23B1cAeE83CE6109A7E4fd62d` | 41,105,199 | Committed, Revealed, WinnerSelected, TruthSelected, CurrentRevealAnchor, CountCommits, CountReveals, ChunkCount, PriceAdjustmentSkipped, WithdrawFailed, transformedChunkAddressFromInclusionProof |
| SimpleSwapFactory (optional) | `0xc2d5a532cf69aa9a1378737d8ccdef884b6e7420` | 20,000,000 | chequebook deployments, then per-chequebook cheque events |

The storage-incentives contracts may be listed under `contracts:` or under a separate
`storage_incentives_contracts:` key; both are read. (Until 2026-10-15 only `contracts:` was,
so a config using the separate key silently fetched none of them.)

The database also holds events from earlier, now-retired deployments (several Redistribution
and StakeRegistry versions, an older PriceOracle and PostageStamp) fetched before January 2026
under a longer contract list. They are not in the current configs and nothing fetches them now.

A batch created through StampsRegistry emits `BatchCreated` on both StampsRegistry and
PostageStamp in the same transaction. Count PostageStamp events for totals; use StampsRegistry
events to see which batches came through the UI and who paid.

Round and phase for the redistribution game: `round = block / 152`; within a round, blocks 0–37
are commit, 38–75 reveal, 76–151 claim.

## Commands

Global options go before the subcommand: `--config <file>` (env `BEEPORT_CONFIG`),
`--rpc-url <url>` (env `RPC_URL`), `--cache-db <path-or-url>`, `--verbose`.

### Collecting data

| Command | What it does | Options |
|---|---|---|
| `fetch` | Fetch events for a block range and print them | `--from-block` (default: PostageStamp deployment), `--to-block` (default: chain head), `--incremental`, `--refresh`, `--max-retries`, `--initial-delay-ms` |
| `sync` | Resume from the last stored block to chain head, quietly; also caches the current price | `--from-block`, `--to-block`, `--refresh`, `--contract <name>` (a storage-incentives name syncs that contract alone; a postage-stamp name runs the postage-stamp pass) |
| `follow` | `sync`, then poll for new blocks forever | `--poll-interval <secs>` (12), `--display` |
| `price` | Print the current storage price from the PriceOracle | |
| `migrations` | Show which schema migrations have been applied | |
| `reset` | Drop and recreate the database. **Destructive**, asks for confirmation | |

`fetch` and `sync` cover all five event contracts: postage-stamp contracts first, then the
storage-incentives contracts one by one. `sync` resumes the postage-stamp pass from the `events`
table's highest block and each storage-incentives contract from its own highest block, so a run
that dies part-way leaves no hole on the next run. (`follow` currently polls the stamp contracts
only.) Fetched block ranges are remembered per contract in `rpc_cache` so re-running over a
covered range is cheap; `--refresh` forces re-processing. To fill a known gap for one contract:
`sync --contract Redistribution --from-block N --to-block M`.

### Analysing stamps

| Command | What it does | Options |
|---|---|---|
| `summary` | Activity by period with filters | `--group-by day\|week\|month`, `--months N` (12; 0 = all), `--event-type`, `--batch-id`, `--contract postage-stamp\|stamps-registry` |
| `batch-status` | Every batch with TTL, expiry date and size | `--sort-by batch-id\|depth\|ttl\|expiry\|size`, `--output table\|csv\|json`, `--price`, `--price-change`, `--refresh`, `--only-missing`, `--hide-zero-balance`, `--contract`, `--cache-validity-blocks`, `--max-retries` |
| `expiry-analytics` | When capacity expires, grouped by period | `--period hour\|day\|week\|month`, `--sort-by period\|chunks\|storage`, `--output`, `--price`, `--price-change`, `--refresh`, `--cache-validity-blocks` |
| `address-summary` | Who buys stamps, owner vs sender | `--output`, `--min-stamps`, `--show-delegated-only`, `--role owner\|sender\|owner-and-sender`, `--live-only`, `--price`, `--refresh` |
| `export` | Dump events, batches or period stats | `--data-type events\|batches\|stats`, `--output <file>`, `--format json\|csv`, `--months`, `--event-type`, `--batch-id`, `--contract` |

Balances: `batch-status`, `expiry-analytics` and `address-summary` need each batch's remaining
balance. Without `--refresh` they use the `batch_balances` cache, falling back to the balance
recorded at the batch's last event (which is stale for old batches). `--refresh` queries the chain
for every batch, which is slow on public RPCs.

### Payment channels (bandwidth incentives)

Requires a `payment_channel_factories` entry in the config (see `bandwidth-config.yaml`).

| Command | What it does | Options |
|---|---|---|
| `discover-chequebooks` | Scan factory logs for deployed chequebooks | `--from-block`, `--to-block`, `--refresh`, `--factory` |
| `sync-chequebooks` | Fetch cheque events from every discovered chequebook, in parallel | `--from-block`, `--to-block`, `--refresh`, `--chequebook`, `--parallel-batch-size` (10) |
| `cheque-summary` | Cashing activity per chequebook | `--from-block`, `--to-block`, `--output` |
| `chequebook-balances` | Chequebook addresses with balances | `--output`, `--refresh` |
| `payment-channel-summary` | Activity grouped by period | `--group-by`, `--months` |

## Configuration

Priority: CLI arguments > environment variables (`BEEPORT__SECTION__KEY`) > config file (YAML,
TOML or JSON) > built-in defaults. There is no `.env` file and no API keys anywhere; every endpoint
in the shipped configs is a free public one.

Files in this repo:

| File | Database | RPC | Purpose |
|---|---|---|---|
| `config-prod.yaml` | `postgresql://localhost/beeport_tx_prod` | 4 public endpoints | the database with the full price history; use this |
| `config.yaml` | `postgresql://localhost/beeport3m` | 3 public + `localhost:8545` | older working config; the localhost endpoint needs a local node |
| `bandwidth-config.yaml` | `postgresql://localhost/beeport_bandwidth` | `localhost:8545` | payment-channel work |
| `bandwidth-test-config.yaml` | `postgresql://localhost/beeport_bandwidth_testing` | `localhost:8545` | payment-channel testing |
| `config.example.toml` | | | annotated reference |

Minimal multi-RPC config:

```yaml
database:
  path: "postgresql://localhost/beeport_tx_prod"   # or "./stamp-cache.db" for SQLite

rpc:
  endpoints:                     # one entry → single-RPC mode; several → round-robin scheduler
    - url: "https://rpc.gnosischain.com"
      rate_limit: adaptive       # adaptive | aggressive | manual: <rps>
      priority: 1
      weight: 1
    - url: "https://gnosis-rpc.publicnode.com"
      rate_limit: adaptive
  # or, single endpoint:  url: "https://rpc.gnosis.gateway.fm"

rate_limiting:
  max_concurrent_requests: 10
  adaptive: { start_rps: 2.0, max_rps: 50.0, min_rps: 0.5, ramp_up_factor: 1.1, back_off_factor: 0.5, ramp_up_threshold: 50 }
  recovery: { cool_down_period_secs: 60, min_healthy_rate: 5.0, gradual_recovery_factor: 1.05, recovery_check_interval_secs: 30 }

blockchain:
  chunk_size: 2000               # blocks per eth_getLogs; public RPCs reject large ranges
  expected_block_time_seconds: 5

retry:
  max_retries: 5
  initial_delay_ms: 100
  backoff_multiplier: 4.0
  extended_retry_wait_seconds: 300

contracts:                       # postage-stamp contracts
  - { name: PostageStamp,   contract_type: PostageStamp,   address: "0x45a1502382541Cd610CC9068e88727426b696293", deployment_block: 31306385, active: true }
  - { name: StampsRegistry, contract_type: StampsRegistry, address: "0x5EBfBeFB1E88391eFb022d5d33302f50a46bF4f3", deployment_block: 42390510, active: true }

storage_incentives_contracts:
  - { name: PriceOracle,    contract_type: PriceOracle,    address: "0x47EeF336e7fE5bED98499A4696bce8f28c1B0a8b", deployment_block: 37339168 }
  - { name: StakeRegistry,  contract_type: StakeRegistry,  address: "0xda2a16EE889E7f04980A8d597b48c8D51B9518F4", deployment_block: 40430237 }
  - { name: Redistribution, contract_type: Redistribution, address: "0x5069cdfB3D9E56d23B1cAeE83CE6109A7E4fd62d", deployment_block: 41105199 }

payment_channel_factories:       # optional
  - { name: SimpleSwapFactory-Gnosis, address: "0xc2d5a532cf69aa9a1378737d8ccdef884b6e7420", deployment_block: 20000000, network: gnosis, active: true }
```

Rate limiting: each endpoint has a token bucket. `adaptive` starts at `start_rps`, ramps up
after `ramp_up_threshold` consecutive successes, halves on HTTP 429, and recovers gradually after
`cool_down_period_secs`. Discovered rates persist in the `rpc_rate_limits` table so the next run
starts where the last one settled. Statistics are logged every `stats_log_interval_seconds`.

Public RPC notes (tested 2026-10-15): `rpc.gnosischain.com`, `gnosis-rpc.publicnode.com`,
`xdai.fairdatasociety.org` and `rpc.gnosis.gateway.fm` serve 2000-block `eth_getLogs` queries
without a key. `drpc`, `ankr`, `1rpc`, `blockpi` and `onfinality` refuse or cap at 50 blocks, and
`blastapi` is gone. See `docs/RPC.md`. A keyed endpoint (Alchemy, Infura, etc.) would make a full
catch-up much faster; add it as another `endpoints` entry.

## Database

SQLite (`./file.db`) or PostgreSQL (`postgres://…`/`postgresql://…`), chosen from the path. Schema
migrations run automatically on start: `migrations/` for SQLite, `migrations_postgres/` for
PostgreSQL. (`migrations_sqlite/` holds one stray file and is not referenced by the code.)
The `migrations` command shows what has been applied.

Main tables:

| Table | Holds |
|---|---|
| `events` | postage-stamp events from PostageStamp and StampsRegistry, with `contract_source`, `contract_address`, `from_address` (tx sender) |
| `storage_incentives_events` | all PriceOracle, StakeRegistry and Redistribution events in one wide table; filter on `event_type` |
| `batches` | one row per batch with its latest known depth and balance |
| `batch_balances` | on-chain balance cache filled by `--refresh` |
| `rpc_cache` | fetched chunk ranges per contract (skip list for re-runs) |
| `rpc_rate_limits` | discovered per-endpoint rate limits |
| `cache_metadata` | key/value, e.g. `last_price` |
| `payment_channel_factories`, `payment_channel_deployments`, `payment_channel_events` | chequebook tracking |

Handy queries:

```sql
-- price history
SELECT round_number, price, to_timestamp(block_timestamp) FROM storage_incentives_events
WHERE event_type = 'PriceUpdate' ORDER BY block_number DESC LIMIT 20;

-- rounds and their commit/reveal counts
SELECT round_number,
       max(CASE WHEN event_type='CountCommits' THEN commit_count END) AS commits,
       max(CASE WHEN event_type='CountReveals' THEN reveal_count END) AS reveals
FROM storage_incentives_events WHERE contract_source='Redistribution'
GROUP BY round_number ORDER BY round_number DESC LIMIT 20;

-- how far the database has been synced
SELECT max(block_number) FROM events;
```

## Plotting

The storage-incentives dashboard reads straight from the PostgreSQL database and plots the
following against block number, each series on its own y-axis and with its own marker shape:

| Series | Marker | What it is |
|---|---|---|
| Price | thick orange line | Storage price in PLUR per chunk per block, one `PriceUpdate` per round from the PriceOracle. It rises when fewer nodes than the target reveal in a round and falls when more do. |
| Commits | ■ square | Nodes that committed in each round of the redistribution game (`CountCommits`), for rounds that produced a winner. |
| Reveals | ● circle | Nodes that then revealed in that round (`CountReveals`). Reveals below commits mean nodes dropped out mid-round. |
| Chunks | ▼ triangle down | `ChunkCount`: the postage contract's `validChunkCount()` at claim time, i.e. the total number of paid-for chunks across all live batches. A measure of how much storage is currently bought. |
| Depth | ⬣ hexagon | Storage depth of the round's agreed truth (`TruthSelected.truth_depth`): how finely the network is split into neighbourhoods. 10 until Nov 2025, 9 since, some 10s back from Jun 2026. |
| Freeze Time | ▲ triangle up | Duration in blocks of each `StakeFrozen` penalty. |
| Frozen Events Count | ✕ cross | Number of `StakeFrozen` events between consecutive winners, a per-round penalty count. |
| Pot Withdrawn | ◆ diamond, log scale | `PotWithdrawn.pot_total_amount`: PLUR paid out of the postage pot to each round's winner. |
| Reward per Node | + white plus | Ten-round rolling `sum(pot withdrawn) / sum(commits)`: what a participating node earned per round, smoothed. |

The dashboard comes in two forms built from the same queries:

| Output | Script | Recipe | Notes |
|---|---|---|---|
| `plots/incentives_hero_dashboard.png` | `plots/plot.py` (matplotlib) | `just graph-all` (alias `just graph`) | 300 dpi, full history, committed to the repo, re-render after each sync |
| `plots/incentives_hero_dashboard_{3m,1m,1w,1d}.png` | `plots/plot.py` | `just graph-3-month`, `graph-1-month`, `graph-1-week`, `graph-1-day` | same, windowed to 90 / 30 / 7 / 1 days; gitignored |
| `plots/incentives_hero_dashboard.html` | `plots/plot_html.py` (plotly) | `just graph-html`, `just graph-dev` | single self-contained file, zoom/pan/hover, ~13 MB for full history, gitignored |

`just graph-dev` builds the HTML and serves `plots/` on `http://localhost:8787` with Node's
`http-server`, opening the browser; `just port=9000 graph-dev` changes the port.
`just graph-interactive` opens the matplotlib window instead. Every graph recipe accepts a
`days` window, e.g. `just days=30 graph-dev`; the scripts read it as `SINCE_DAYS` and look up
the cutoff block from event timestamps.

Setup and the full series list are in [`plots/README.md`](./plots/README.md); the short version:

```bash
cd plots && pip install -r requirements.txt     # psycopg2, pandas, matplotlib, numpy, scipy, python-dotenv, plotly
```

Connection settings come from `DB_NAME`, `DB_USER`, `DB_PASS`, `DB_HOST`, `DB_PORT`
(optionally via `plots/.env`, see `plots/.env.example`); the `just` recipes pass `db` through.

## TTL and price modelling

```
chunks               = 2^depth
cost_per_block       = price_per_chunk_per_block × chunks
ttl_blocks           = remaining_balance / cost_per_block
ttl_days             = ttl_blocks × 5 s / 86400 s
```

`--price <PLUR>` overrides the cached current price. `--price-change PERCENT:DAYS` models an
exponential price change (`200:10` triples the price over ten days; `-50:30` halves it over a
month). The average price over the batch's life is integrated under that curve, so a rising price
shortens the TTL more than a naive calculation suggests. Arithmetic is 128-bit.

## Development

```bash
just test                        # cargo test (89 unit tests) + cargo clippy -D warnings, both must be clean
just build                       # cargo build --release
```

Toolchain: Rust nightly, edition 2024. Key crates: alloy 0.8, sqlx 0.8 (sqlite + postgres),
tokio 1.43, clap 4.5, config 0.14, tabled 0.17.

Repository layout:

```
justfile                 task runner recipes (status, sync, fill, graph, …)
config-prod.yaml         the config to use; config.yaml, bandwidth*-config.yaml, config.example.toml are the others
src/                     the CLI (below)
migrations/              SQLite schema; migrations_postgres/ the PostgreSQL schema
plots/                   plot.py (PNG), plot_html.py (HTML), README, requirements.txt, .env.example
docs/                    design notes, RPC endpoint list, older configs
logs/                    sync logs from `just sync-bg` (gitignored)
plan.md, state.md        running log and verified current state; CLAUDE.md architecture guide
```

```
src/
├── main.rs, lib.rs        entry point, module list
├── cli.rs                 argument parsing and command orchestration
├── config.rs              config structs, defaults, merging
├── blockchain.rs          RPC client, chunked event fetching, sender lookup
├── rpc_scheduler.rs       round-robin across endpoints
├── rate_limiter_v2.rs     token bucket with recovery (rate_limiter.rs is the older v1, to be removed)
├── retry.rs               two-phase exponential backoff
├── cache.rs               database layer (SQLite/PostgreSQL)
├── contracts/             ABIs (sol!), parsers, contract registry
├── commands/              batch_status, expiry_analytics, address_summary
├── price.rs               TTL maths
├── events.rs, types.rs    data model
├── export.rs, display.rs  output
└── hooks.rs               per-event hook trait (stub implementation)
```

Adding a contract is config plus an ABI, parser and registry entry; the walkthrough is in
[`CLAUDE.md`](./CLAUDE.md#adding-a-new-contract).

## Known limitations

- Public RPCs throttle hard; a year of blocks takes hours. Every event also costs one
  `eth_getTransactionByHash` to record the sender.
- `follow` only polls the postage-stamp contracts; run `sync` to update storage-incentives data.
- `sync --contract PostageStamp` runs both postage-stamp contracts, not just the one named.
- `batch_balances` is only populated by `--refresh`, which is slow, so TTL figures for old
  batches are approximate until a refresh runs.
- `rate_limiter.rs` (v1) still exists alongside v2 and should be deleted.
- The HTML dashboard carries every data point (~700k) and is ~13 MB; it takes a few seconds to
  load. Downsampling the dense Redistribution series would shrink it if that matters.

## License

MIT
