# state.md — beeport-stamp-stats

Verified snapshot of where this project is. `plan.md` says what we are trying to do and keeps the
running log; this file says what is true right now. Update the timestamp whenever you change it.

**Snapshot taken:** 2026-10-15 (everything below was checked against the filesystem, git, and Postgres)

---

## What this is

Rust CLI (`beeport-stamp-stats`, tokio + alloy + sqlx) that pulls events from five Swarm contracts on
Gnosis Chain into Postgres/SQLite and runs analytics on them: postage stamp batches, storage
incentives (PriceOracle `PriceUpdate`, StakeRegistry, Redistribution), and payment-channel
chequebooks. Requests are spread round-robin across many public RPC endpoints, each with its own
adaptive rate limiter. Started 2025-11-30.

Directory: `~/Code/swarm2/beeport-tx-stats-multi2`. The sibling folders `beeport-tx-stats` and
`beeport-tx-stats-dev` are older copies; do not work in them.

Claude Code transcripts for this project were deleted. `plan.md`, `CLAUDE.md`,
`ARCHITECTURE_REVIEW.md`, `docs/` and this file are the only history.

## Git

| Item | State |
|---|---|
| Remote | `git@github.com:significance/beeport-stamp-stats.git` |
| Branch | `feat/improve-retrieval-efficiency-with-bandwidth` |
| Upstream | `origin/feat/improve-retrieval-efficiency-with-bandwidth`, pushed 2026-10-15, in sync |
| Working tree | clean, apart from 4 untracked junk files (`Reports Dashboard.xlsx`, `history.txt`, `note.txt`, `rpc-api-keys.txt`) deliberately not committed |
| Latest commits | 2026-10-15: `fix: expiry-analytics balance fallback, Hour period, clippy cleanups` then `docs: add state.md, update plan`; before that `138dd2b docs: add architecture review` (2026-02-12) |
| Ahead of `main` | 27 commits; `main` last moved 2026-01-10 |
| PR for this branch | none |
| Open PRs | #4 investigate-addresses, #5 address-tracking-phase2, #6 improve-retrieval-efficiency. All superseded by this branch. |
| Diverged branch | `feat/improve-retrieval-efficiency`: 8 local unpushed, 3 remote unpulled. Inspect before merging anything. |
| Stale branch | `feat/github-ci-workflows`: merged in PR #1, 23 behind, deletable |

## Build

| Check | Result (2026-10-12) |
|---|---|
| `cargo test` | 87 passed |
| `cargo clippy -- -D warnings` | clean |
| `cargo build --release` | ok, binary at `target/release/beeport-stamp-stats` |
| `cargo clippy --all-targets` | 1 pre-existing lint: `mod tests` mid-file in `src/contracts/impls.rs:185` |
| Toolchain | rustc 1.96.0-nightly (2026-03-15), no `rust-toolchain` file |

## Configuration and secrets

- Config is YAML. Env overrides use `BEEPORT__SECTION__KEY`. There is no `.env` and never was.
- **There are no RPC API keys anywhere.** All endpoints are free public ones. `rpc-api-keys.txt` is a
  pasted Claude message, not keys.
- Three configs point at three databases:

| Config | Database | RPC |
|---|---|---|
| `config.yaml` | `beeport3m` | 4 public endpoints, round-robin |
| `bandwidth-config.yaml` | `beeport_bandwidth` | `localhost:8545` |
| `bandwidth-test-config.yaml` | `beeport_bandwidth_testing` | `localhost:8545` |

- Contract addresses live in `config.yaml` (PostageStamp, StampsRegistry, PriceOracle
  `0x47EeF336e7fE5bED98499A4696bce8f28c1B0a8b`, StakeRegistry, Redistribution).

## Data (Postgres, local, running)

Real table names: `events` (stamps) and `storage_incentives_events`. Older plan text saying
`stamp_events` is wrong.

| Database | Size | Stamp events | Storage-incentives events | Other | Freshest block |
|---|---|---|---|---|---|
| `beeport_tx_prod` | 861 MB | 184k | **1.23M incl. 80,396 PriceUpdate** | | 44,267,041 (2026-01-21) |
| `beeport3m` | 166 MB | 187k | 0 | | 44,412,633 (2026-01-29) |
| `beeport_bandwidth` | 165 MB | 186k | 0 | 713 payment-channel deployments | 44,344,711 (2026-01-25) |
| `beeport_bandwidth_testing` | 1 GB | 0 | 0 | 35k deployments, 6k channel events, 2.25M rpc_cache rows | |
| `beeport5`, `beeport6` | tiny | stubs | | | ignore |

- **Price history exists only in `beeport_tx_prod`**, which no config points at.
- **All data ends late January 2026**, roughly 4.4M blocks behind the chain head.
- `batch_balances` is empty in every database. This is why `expiry-analytics` showed nothing.

Loose files in the project root (gitignored): `price-updates.csv` 16 MB, `events.csv`,
`balances.csv`, `stamp-cache.db`, `beeport_test_2.db` (24–25 Jan 2026), and 1.6 GB of `.sql` dumps
also duplicated in `../beeport-tx-stats`.

## Known problems

1. `expiry-analytics` fix is written and tested but uncommitted. Phase 3 (parallel balance refresh or a
   `refresh-balances` command) not started.
2. Two rate limiters coexist; `rate_limiter.rs` (v1) should go, keep `rate_limiter_v2.rs`.
3. Temp files and the misleading `rpc-api-keys.txt` are not gitignored.
4. Config drift: three databases, none is the one with price history.
5. Out-of-disk during a build corrupts `target/debug/incremental`; delete that dir and rebuild.

## Sensible next steps

1. Commit the pending work (expiry fix, lint fixes, docs) on this branch.
2. Point `config.yaml` at `beeport_tx_prod` and run `sync` to catch up ~8 months.
3. Open one PR from this branch to `main`; close #4–#6; delete merged/stale branches.
4. Then the cleanup list in `ARCHITECTURE_REVIEW.md`.
