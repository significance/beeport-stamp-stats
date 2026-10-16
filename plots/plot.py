import os
import psycopg2
import pandas as pd
import matplotlib.pyplot as plt
import numpy as np
from scipy.interpolate import make_interp_spline
from dotenv import load_dotenv

# Use an interactive backend for zooming/panning
plt.ion() 
plt.style.use('dark_background')

load_dotenv()

TITLE = "Storage Incentives Metrics"

# One marker shape per series so they can be told apart without relying on colour.
# matplotlib code → plotly symbol
MARKERS = {
    "Reveals":             ("o", "circle"),
    "Commits":             ("s", "square"),
    "Freeze Time":         ("^", "triangle-up"),
    "Chunks":              ("v", "triangle-down"),
    "Frozen Events Count": ("X", "x"),
    "Pot Withdrawn (log)": ("D", "diamond"),
    "Reward per Node":     ("+", "cross-thin"),
    "Depth":               ("h", "hexagon"),
}

# Colour-blind-safe palette, indexed by series order; Price and Reward per Node override it.
CB_COLORS = ['#0072B2', '#D55E00', '#009E73', '#FFFFAA', '#F0E442', '#56B4E9', '#CC79A7', '#E69F00', '#999999']
NEON_ORANGE = '#FF5F1F'


def marker_for(label, backend="mpl"):
    mpl, plotly = MARKERS.get(label, ("o", "circle"))
    return mpl if backend == "mpl" else plotly


def db_settings():
    """Connection settings from the environment (optionally via .env)."""
    return dict(
        dbname=os.getenv("DB_NAME", "beeport_tx_prod"),
        user=os.getenv("DB_USER", "sig32"),
        password=os.getenv("DB_PASS", ""),
        host=os.getenv("DB_HOST", "localhost"),
        port=os.getenv("DB_PORT", "5432"),
    )


def series_queries():
    """(label, sql) pairs for every series on the dashboard. Shared with plot_html.py."""
    return [
        ("Reveals", """
            SELECT block_number, reveal_count FROM storage_incentives_events
            WHERE event_type = 'CountReveals' AND block_number IN (
                SELECT block_number FROM storage_incentives_events WHERE event_type = 'WinnerSelected'
            )"""),
        ("Commits", """
            SELECT block_number, commit_count FROM storage_incentives_events
            WHERE event_type = 'CountCommits' AND block_number IN (
                SELECT block_number FROM storage_incentives_events WHERE event_type = 'WinnerSelected'
            )"""),
        ("Price", "SELECT block_number, CAST(price AS NUMERIC) FROM storage_incentives_events WHERE event_type = 'PriceUpdate'"),
        ("Freeze Time", "SELECT block_number, CAST(freeze_time AS NUMERIC) FROM storage_incentives_events WHERE event_type = 'StakeFrozen'"),
        ("Chunks", """
            SELECT block_number, chunk_count FROM storage_incentives_events
            WHERE event_type = 'ChunkCount' AND block_number IN (
                SELECT block_number FROM storage_incentives_events WHERE event_type = 'WinnerSelected'
            )"""),
        ("Frozen Events Count", """
            WITH WinnerEvents AS (
                SELECT
                    block_number,
                    log_index,
                    LAG(block_number) OVER (ORDER BY block_number, log_index) AS prev_block,
                    LAG(log_index) OVER (ORDER BY block_number, log_index) AS prev_log_idx
                FROM storage_incentives_events
                WHERE event_type = 'WinnerSelected'
            )
            SELECT
                w.block_number,
                (
                    SELECT COUNT(*)
                    FROM storage_incentives_events s
                    WHERE s.event_type = 'StakeFrozen'
                      AND (s.block_number, s.log_index) > (COALESCE(w.prev_block, 0), COALESCE(w.prev_log_idx, -1))
                      AND (s.block_number, s.log_index) <= (w.block_number, w.log_index)
                ) AS frozen_stake_count
            FROM WinnerEvents w
        """),
        ("Pot Withdrawn (log)", "SELECT block_number, CAST(pot_total_amount AS NUMERIC) FROM events WHERE event_type = 'PotWithdrawn' AND pot_total_amount IS NOT NULL"),
        ("Reward per Node (Raw)", """
            SELECT
                pw.block_number,
                CAST(pw.pot_total_amount AS NUMERIC) as pot_amount,
                COALESCE(cc.commit_count, 0) as commit_count
            FROM events pw
            LEFT JOIN storage_incentives_events cc
                ON pw.block_number = cc.block_number
                AND cc.event_type = 'CountCommits'
            WHERE pw.event_type = 'PotWithdrawn'
                AND pw.pot_total_amount IS NOT NULL
            ORDER BY pw.block_number
        """),
        ("Depth", "SELECT block_number, truth_depth FROM storage_incentives_events WHERE event_type = 'TruthSelected' AND truth_depth IS NOT NULL"),
    ]


def since_days():
    """Window in days from SINCE_DAYS (unset or empty = full history)."""
    v = os.getenv("SINCE_DAYS", "").strip()
    return int(v) if v else None


def title_suffix(days):
    if not days:
        return ""
    return " · last 24 hours" if days == 1 else f" · last {days} days"


def load_dataframes(settings=None, days=None):
    """Run every series query and return {label: DataFrame indexed by block_number}.

    The derived 'Reward per Node' series (10-round rolling pot / commits) replaces the raw
    query result. With `days`, only blocks from the last that many days are kept (the cutoff
    block is looked up from block timestamps). Returns an empty dict if there is no data.
    """
    settings = settings or db_settings()
    conn = psycopg2.connect(**settings)
    dataframes = {}
    cutoff = None
    try:
        if days:
            cur = conn.cursor()
            cur.execute(
                "SELECT min(block_number) FROM storage_incentives_events "
                "WHERE block_timestamp >= extract(epoch FROM now() - make_interval(days => %s))",
                (days,),
            )
            cutoff = cur.fetchone()[0]
            cur.close()
        for label, sql in series_queries():
            df = pd.read_sql(sql, conn)
            if not df.empty:
                if label == "Reward per Node (Raw)":
                    dataframes[label] = df
                else:
                    dataframes[label] = df.set_index('block_number').sort_index()
    finally:
        conn.close()

    if "Reward per Node (Raw)" in dataframes:
        raw_df = dataframes.pop("Reward per Node (Raw)")
        window = 10
        raw_df['pot_sum'] = raw_df['pot_amount'].rolling(window=window, min_periods=1).sum()
        raw_df['commit_sum'] = raw_df['commit_count'].rolling(window=window, min_periods=1).sum()
        raw_df['reward_per_node'] = raw_df['pot_sum'] / raw_df['commit_sum'].replace(0, np.nan)
        dataframes['Reward per Node'] = raw_df.set_index('block_number')[['reward_per_node']].dropna()

    if cutoff is not None:
        dataframes = {k: v[v.index >= cutoff] for k, v in dataframes.items()}
        dataframes = {k: v for k, v in dataframes.items() if not v.empty}

    return dataframes


def fetch_and_plot_metrics(export_filename=None):
    dbname = db_settings()["dbname"]
    days = since_days()

    try:
        dataframes = load_dataframes(days=days)
        if not dataframes:
            print("No data found.")
            return

        all_indices = pd.concat([df.index.to_series() for df in dataframes.values()])
        min_block, max_block = int(all_indices.min()), int(all_indices.max())
        full_timeline = pd.DataFrame(index=range(min_block, max_block + 1))

        cb_colors = CB_COLORS
        neon_orange = NEON_ORANGE
        
        fig, ax_main = plt.subplots(figsize=(19.2, 10.8), dpi=100, facecolor='#0B0B0B')
        ax_main.set_facecolor('#0B0B0B')
        
        lines_and_scatters = []

        for i, (label, df) in enumerate(dataframes.items()):
            color = cb_colors[i % len(cb_colors)]
            
            # Create the axis
            if i == 0:
                ax = ax_main
            else:
                ax = ax_main.twinx()
                if i > 1:
                    ax.spines['right'].set_position(('outward', 75 * (i - 1)))

            plot_data = full_timeline.join(df).dropna()
            x = plot_data.index.values
            y = plot_data.iloc[:, 0].values

            if label == "Price":
                if len(x) > 3:
                    x_smooth = np.linspace(x.min(), x.max(), 500)
                    spl = make_interp_spline(x, y, k=3)
                    y_smooth = spl(x_smooth)
                    
                    # LAYER 1: Subtle Outer Glow (wider, lower alpha)
                    ax.plot(x_smooth, y_smooth, color=neon_orange, linewidth=8, alpha=0.15, zorder=9)
                    
                    # LAYER 2: Main Smoothed Line (The Wide Neon Line)
                    # zorder=10 ensures it is on top of all other plots
                    item = ax.plot(x_smooth, y_smooth, color=neon_orange, label=label, 
                                   linewidth=4.5, zorder=10, alpha=1.0)
                    lines_and_scatters.extend(item)
                    
                    ax.set_ylabel(label, color=neon_orange, fontweight='bold', fontsize=11)
                    ax.tick_params(axis='y', labelcolor=neon_orange)
                else:
                    item = ax.plot(x, y, color=neon_orange, label=label, linewidth=4.5, zorder=10)
                    lines_and_scatters.extend(item)
            else:
                # Set zorder lower (default is ~2) so they sit behind Price
                if label == "Pot Withdrawn (log)":
                    # Diamond marker, half size, logarithmic scale
                    item = ax.scatter(x, y, color=color, label=label, s=25, marker='D', edgecolors='none', alpha=0.6, zorder=3)
                    ax.set_yscale('log')
                elif label == "Reward per Node":
                    # White cross marker
                    item = ax.scatter(x, y, color='white', label=label, s=50, marker='+', edgecolors='none', alpha=0.8, zorder=3)
                else:
                    item = ax.scatter(x, y, color=color, label=label, s=50, marker=marker_for(label),
                                      edgecolors='none', alpha=0.6, zorder=3)
                lines_and_scatters.append(item)
                ax.set_ylabel(label, color=color, fontweight='bold', fontsize=9)
                ax.tick_params(axis='y', labelcolor=color)
            
            ax.tick_params(axis='y', colors='#888888')
            ax.spines['left'].set_color('#333333')
            ax.spines['right'].set_color('#333333')

        ax_main.set_xlabel('Block Number', fontsize=12, fontweight='bold', color='#E0E0E0')
        # Title large, subtitle small (set_title can only do one size, so the subtitle is a text line below it)
        ax_main.set_title(TITLE, fontsize=18, pad=30, color='white', fontweight='bold')
        ax_main.text(0.5, 1.012, f'Database: {dbname}{title_suffix(days)}',
                     transform=ax_main.transAxes, ha='center', va='bottom',
                     fontsize=10, color='#AAAAAA')
        
        # Legend styling
        labels = [obj.get_label() for obj in lines_and_scatters]
        legend = ax_main.legend(lines_and_scatters, labels, loc='upper left', frameon=True)
        legend.get_frame().set_facecolor('#151515')
        legend.get_frame().set_edgecolor('#444444')

        fig.tight_layout()
        ax_main.grid(True, which='both', linestyle='--', linewidth=0.5, color='#222222', alpha=0.3)
        
        if export_filename:
            print(f"Exporting high-res dashboard to {export_filename}...")
            plt.savefig(export_filename, dpi=300, bbox_inches='tight', facecolor=fig.get_facecolor())
            print("Export successful.")

        plt.show(block=True) 

    except Exception as e:
        print(f"Error: {e}")

if __name__ == "__main__":
    EXPORT_PATH = "incentives_hero_dashboard.png" 
    fetch_and_plot_metrics(export_filename=EXPORT_PATH)
