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

def fetch_and_plot_metrics(export_filename=None):
    dbname = os.getenv("DB_NAME", "beeport2")
    user = os.getenv("DB_USER", "sig32")
    password = os.getenv("DB_PASS", "")
    host = os.getenv("DB_HOST", "localhost")
    port = os.getenv("DB_PORT", "5432")

    queries = {
        "Reveals": """
            SELECT block_number, reveal_count FROM storage_incentives_events
            WHERE event_type = 'CountReveals' AND block_number IN (
                SELECT block_number FROM storage_incentives_events WHERE event_type = 'WinnerSelected'
            )""",
        "Commits": """
            SELECT block_number, commit_count FROM storage_incentives_events
            WHERE event_type = 'CountCommits' AND block_number IN (
                SELECT block_number FROM storage_incentives_events WHERE event_type = 'WinnerSelected'
            )""",
        "Price": "SELECT block_number, CAST(price AS NUMERIC) FROM storage_incentives_events WHERE event_type = 'PriceUpdate'",
        "Freeze Time": "SELECT block_number, CAST(freeze_time AS NUMERIC) FROM storage_incentives_events WHERE event_type = 'StakeFrozen'",
        "Chunks": """
            SELECT block_number, chunk_count FROM storage_incentives_events
            WHERE event_type = 'ChunkCount' AND block_number IN (
                SELECT block_number FROM storage_incentives_events WHERE event_type = 'WinnerSelected'
            )""",
        "Frozen Events Count": """
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
        """,
        "Pot Withdrawn (log)": "SELECT block_number, CAST(pot_total_amount AS NUMERIC) FROM events WHERE event_type = 'PotWithdrawn' AND pot_total_amount IS NOT NULL"
    }

    try:
        conn = psycopg2.connect(dbname=dbname, user=user, password=password, host=host, port=port)
        dataframes = {}
        for label, sql in queries.items():
            df = pd.read_sql(sql, conn)
            if not df.empty:
                dataframes[label] = df.set_index('block_number').sort_index()
        conn.close()

        if not dataframes:
            print("No data found.")
            return

        all_indices = pd.concat([df.index.to_series() for df in dataframes.values()])
        min_block, max_block = int(all_indices.min()), int(all_indices.max())
        full_timeline = pd.DataFrame(index=range(min_block, max_block + 1))

        cb_colors = ['#0072B2', '#D55E00', '#009E73', '#FFFFAA', '#F0E442', '#56B4E9']
        neon_orange = '#FF5F1F'
        
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
                else:
                    item = ax.scatter(x, y, color=color, label=label, s=50, edgecolors='none', alpha=0.6, zorder=3)
                lines_and_scatters.append(item)
                ax.set_ylabel(label, color=color, fontweight='bold', fontsize=9)
                ax.tick_params(axis='y', labelcolor=color)
            
            ax.tick_params(axis='y', colors='#888888')
            ax.spines['left'].set_color('#333333')
            ax.spines['right'].set_color('#333333')

        ax_main.set_xlabel('Block Number', fontsize=12, fontweight='bold', color='#E0E0E0')
        plt.title(f'Storage Incentives Metrics\nDatabase: {dbname}', 
                  fontsize=18, pad=35, color='white', fontweight='bold')
        
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
