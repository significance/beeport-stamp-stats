import os
import psycopg2
import pandas as pd
import matplotlib.pyplot as plt
from dotenv import load_dotenv

# Use an interactive backend for zooming/panning
# If you have issues, you can try plt.switch_backend('Qt5Agg') 
plt.ion() 

load_dotenv()

def fetch_and_plot_metrics():
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
        """
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
        
        # Increase DPI for sharper zooming
        fig, ax_main = plt.subplots(figsize=(16, 9), dpi=100)
        lines_and_scatters = []

        for i, (label, df) in enumerate(dataframes.items()):
            color = cb_colors[i % len(cb_colors)]
            if i == 0:
                ax = ax_main
            else:
                ax = ax_main.twinx()
                if i > 1:
                    ax.spines['right'].set_position(('outward', 70 * (i - 1)))

            plot_data = full_timeline.join(df).dropna()

            if label == "Price":
                item = ax.plot(plot_data.index, plot_data.iloc[:, 0], color=color, label=label, linewidth=2)
                lines_and_scatters.extend(item)
            else:
                item = ax.scatter(plot_data.index, plot_data.iloc[:, 0], color=color, label=label, s=45, edgecolors='none')
                lines_and_scatters.append(item)
            
            ax.set_ylabel(label, color=color, fontweight='bold', fontsize=9)
            ax.tick_params(axis='y', labelcolor=color)

        ax_main.set_xlabel('Block Number', fontsize=12, fontweight='bold')
        plt.title(f'Storage Incentives Metrics (Interactive Zoom Enabled)\nDatabase: {dbname}', fontsize=14, pad=25)
        
        labels = [obj.get_label() for obj in lines_and_scatters]
        ax_main.legend(lines_and_scatters, labels, loc='upper left', frameon=True, shadow=True)

        fig.tight_layout()
        plt.grid(True, which='both', linestyle=':', alpha=0.3)
        
        # This keeps the window open and interactive
        print("Graph generated. Use the magnifying glass icon in the window to zoom.")
        plt.show(block=True) 

    except Exception as e:
        print(f"Error: {e}")

if __name__ == "__main__":
    fetch_and_plot_metrics()
