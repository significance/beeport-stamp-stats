# beeport-stamp-stats task runner. `just` lists recipes; `just <recipe>` runs one.
# Override defaults inline: `just config=config.yaml db=beeport3m sync`

set shell := ["bash", "-euo", "pipefail", "-c"]

config := "config-prod.yaml"
db     := "beeport_tx_prod"
bin    := "./target/release/beeport-stamp-stats"
logdir := "logs"
port   := "8787"
days   := ""        # window for the graph recipes: "" = full history, e.g. `just days=30 graph-dev`

default:
    @just --list --unsorted

# Build the release binary
build:
    cargo build --release

# Unit tests and clippy (the project standard: zero warnings)
test:
    cargo test
    cargo clippy -- -D warnings

# Catch the database up to chain head, in the foreground
sync: build
    {{bin}} --config {{config}} sync

# Same, detached: writes logs/sync-<timestamp>.log and prints how to follow it
sync-bg: build
    @mkdir -p {{logdir}}
    @log="{{logdir}}/sync-$(date -u +%Y%m%d-%H%M%S).log"; \
      nohup {{bin}} --config {{config}} sync >> "$log" 2>&1 & \
      echo "started pid $! → $log"; \
      echo "follow:  tail -F $log | grep --line-buffered -vE 'Chunk [0-9]+/'"

# Sync one contract over an explicit block range, e.g. `just fill Redistribution 45140587 47968999`
fill contract from to: build
    {{bin}} --config {{config}} sync --contract {{contract}} --from-block {{from}} --to-block {{to}}

# Stop any running sync
stop:
    -pkill -f beeport-stamp-stats && echo stopped || echo "nothing running"

# Follow the newest log in logs/
tail:
    @f="$(ls -t {{logdir}}/*.log 2>/dev/null | head -1)"; [ -n "$f" ] || { echo "no logs yet"; exit 1; }; \
      echo "$f"; tail -F "$f" | grep --line-buffered -vE 'Chunk [0-9]+/'

# Where the data is: per-contract highest block vs chain head, and the cached price
status:
    @head=$(curl -s -m 10 -X POST -H 'content-type: application/json' \
      --data '{"jsonrpc":"2.0","id":1,"method":"eth_blockNumber","params":[]}' https://gnosis-rpc.publicnode.com \
      | python3 -c 'import sys,json; print(int(json.load(sys.stdin)["result"],16))' 2>/dev/null || echo "?"); \
      echo "chain head: $head"; \
      psql -tA -F '  ' {{db}} -c "SELECT rpad(contract_source,16), max(block_number), count(*) FROM events GROUP BY 1 ORDER BY 1"; \
      psql -tA -F '  ' {{db}} -c "SELECT rpad(contract_source,16), max(block_number), count(*) FROM storage_incentives_events GROUP BY 1 ORDER BY 1"; \
      psql -tA {{db}} -c "SELECT 'cached price: '||value FROM cache_metadata WHERE key='last_price'"; \
      pgrep -f beeport-stamp-stats >/dev/null && echo "a sync is RUNNING" || echo "no sync running"

# Render the dashboard PNG for a time window and open it. window = days ("" = all), suffix = file name tag.
_graph window suffix:
    cd plots && MPLBACKEND=Agg DB_NAME={{db}} SINCE_DAYS={{window}} python3 -c "import plot; plot.fetch_and_plot_metrics('incentives_hero_dashboard{{suffix}}.png')"
    rm -rf plots/__pycache__
    open plots/incentives_hero_dashboard{{suffix}}.png

# Full history → plots/incentives_hero_dashboard.png (the committed one)
graph-all: (_graph "" "")

# Last 90 days → plots/incentives_hero_dashboard_3m.png
graph-3-month: (_graph "90" "_3m")

# Last 30 days → plots/incentives_hero_dashboard_1m.png
graph-1-month: (_graph "30" "_1m")

# Last 7 days → plots/incentives_hero_dashboard_1w.png
graph-1-week: (_graph "7" "_1w")

# Last 24 hours → plots/incentives_hero_dashboard_1d.png
graph-1-day: (_graph "1" "_1d")

# Alias for graph-all
graph: graph-all

# Render the dashboard in an interactive matplotlib window (zoom/pan)
graph-interactive:
    cd plots && DB_NAME={{db}} SINCE_DAYS={{days}} python3 plot.py

# Build the interactive HTML dashboard (plotly) at plots/incentives_hero_dashboard.html
graph-html:
    cd plots && DB_NAME={{db}} SINCE_DAYS={{days}} python3 plot_html.py incentives_hero_dashboard.html
    rm -rf plots/__pycache__

# Build the HTML dashboard and serve plots/ on http://localhost:{{port}} (Node http-server), opening the browser
graph-dev: graph-html
    @echo "serving plots/ at http://localhost:{{port}}/incentives_hero_dashboard.html  (Ctrl-C to stop)"
    npx --yes http-server plots -p {{port}} -c-1 -s -o /incentives_hero_dashboard.html

# Current storage price straight from the chain
price: build
    {{bin}} --config {{config}} price
