"""
LLM RECREATION PROMPT:
"Act as a Python and PostgreSQL expert. Write a script to visualize storage incentive metrics from a database named 'beeport2'.
The database has a table 'storage_incentives_events' with columns: 
[block_number (bigint), event_type (text), freeze_time (text), price (text), reveal_count (bigint), commit_count (bigint), chunk_count (bigint), log_index (bigint)].

Requirements:
1. Connection: Use psycopg2 and load credentials (DB_NAME, DB_USER, etc.) from a .env file.
2. Logic: 
    - Fetch 'Price' as a continuous line (PriceUpdate).
    - Fetch 'Reveals', 'Commits', and 'Chunks' as scatter points (only where they coincide with 'WinnerSelected' blocks).
    - Fetch 'Frozen Events Count': This must use a CTE with LAG() to count 'StakeFrozen' events occurring between consecutive 'WinnerSelected' events, using (block_number, log_index) for strict ordering.
    - Fetch 'Freeze Times': Create 4 separate series for specific freeze_time values: 77824, 155648, 311296, and 622592.
3. Visualization:
    - Use Matplotlib with an interactive backend (plt.ion()) to ensure the graph is zoomable.
    - Plot all 4 specific Freeze Times as scatter points on a SINGLE shared Y-axis.
    - Plot other metrics on their own twinx() Y-axes, offset to the right to avoid overlap.
    - Use the Okabe-Ito colorblind-friendly palette.
    - All scatter points should be solid (alpha=1.0) with no edgecolors.
    - Use square markers for Freeze durations and circles for other metrics.
    - Include a consolidated, multi-column legend and a grid."
"""

import os
import psycopg2
import pandas as pd
import matplotlib.pyplot as plt
from dotenv import load_dotenv

# Enable interactive mode for zooming and panning
plt.ion() 

load_dotenv()

def fetch_and_plot_metrics():
    # --- 1. Database Configuration ---
    dbname = os.getenv("DB_NAME", "beeport2")
    user = os.getenv("DB_USER", "sig32")
    password = os.getenv("DB_PASS", "")
    host = os.getenv("DB_HOST", "localhost")
    port = os.getenv("DB_PORT", "5432")

    # The four specific freeze durations identified in context
    freeze_times = [77824, 155648, 311296, 622592]
    
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
        "Chunks": """
            SELECT block_number, chunk_count FROM storage_incentives_events 
            WHERE event_type = 'ChunkCount' AND block_number IN (
                SELECT block_number FROM storage_incentives_events WHERE event_type = 'WinnerSelected'
            )""",
        "Frozen Events Count": """
            WITH WinnerEvents AS (
                SELECT block_number, log_index,
                LAG(block_number) OVER (ORDER BY block_number, log_index) AS prev_block,
                LAG(log_index) OVER (ORDER BY block_number, log_index) AS prev_log_idx
                FROM storage_incentives_events WHERE event_type = 'WinnerSelected'
            )
            SELECT w.block_number, 
            (SELECT COUNT(*) FROM storage_incentives_events s 
             WHERE s.event_type = 'StakeFrozen'
             AND (s.block_number, s.log_index) > (COALESCE(w.prev_block, 0), COALESCE(w.prev_log_idx, -1))
             AND (s.block_number, s.log_index) <= (w.block_number, w.log_index)) AS frozen_stake_count
            FROM WinnerEvents w """
    }

    # Append individual queries for each freeze time
    for t in freeze_times:
        queries[f"Freeze {t}"] = f"""
            SELECT block_number, CAST(freeze_time AS NUMERIC) 
            FROM storage_incentives_events 
            WHERE event_type = 'StakeFrozen' AND CAST(freeze_time AS NUMERIC) = {t}"""

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

        # Create continuous timeline for alignment
        all_indices = pd.concat([df.index.to_series() for df in dataframes.values()])
        full_timeline = pd.DataFrame(index=range(int(all_indices.min()), int(all_indices.max()) + 1))

        # Okabe-Ito Colorblind Palette
        cb_colors = ['#0072B2', '#D55E00', '#009E73', '#FFFF00', '#F0E442', '#56B4E9', '#999999', '#E69F00']
        
        fig, ax_main = plt.subplots(figsize=(16, 10), dpi=100)
        lines_and_scatters = []
        freeze_axis = None 

        for i, (label, df) in enumerate(dataframes.items()):
            color = cb_colors[i % len(cb_colors)]
            
            # --- Axis Assignment ---
            if i == 0:
                ax = ax_main
            elif "Freeze" in label:
                if freeze_axis is None:
                    freeze_axis = ax_main.twinx()
                    freeze_axis.spines['right'].set_position(('outward', 60))
                ax = freeze_axis
            else:
                ax = ax_main.twinx()
                # Dynamically offset axes to the right
                ax.spines['right'].set_position(('outward', 60 * i))

            plot_data = full_timeline.join(df).dropna()

            if label == "Price":
                item = ax.plot(plot_data.index, plot_data.iloc[:, 0], color=color, label=label, linewidth=2)
                lines_and_scatters.extend(item)
            else:
                marker = 'D' if "Freeze" in label else 'o'
                item = ax.scatter(plot_data.index, plot_data.iloc[:, 0], 
                                  color=color, label=label, s=50, marker=marker, edgecolors='none')
                lines_and_scatters.append(item)
            
            # Labeling
            y_label = "Freeze Time Duration" if "Freeze" in label else label
            ax.set_ylabel(y_label, color=color, fontweight='bold', fontsize=9)
            ax.tick_params(axis='y', labelcolor=color)

        ax_main.set_xlabel('Block Number', fontsize=12, fontweight='bold')
        plt.title(f'Storage Incentives: Multi-Metric Analysis\nDatabase: {dbname}', fontsize=14, pad=20)
        
        # Legend and layout
        labels = [obj.get_label() for obj in lines_and_scatters]
        ax_main.legend(lines_and_scatters, labels, loc='upper left', frameon=True, shadow=True, ncol=2)

        fig.tight_layout()
        plt.grid(True, which='both', linestyle=':', alpha=0.3)
        
        print("Interactive Plot Ready. Use the toolbar to zoom and pan.")
        plt.show(block=True)

    except Exception as e:
        print(f"Error: {e}")

if __name__ == "__main__":
    fetch_and_plot_metrics()




# import os
# import psycopg2
# import pandas as pd
# import matplotlib.pyplot as plt
# from dotenv import load_dotenv

# # Load environment variables from .env file
# load_dotenv()

# def fetch_and_plot_metrics():
#     # --- 1. Database Configuration via Environment Variables ---
#     dbname = os.getenv("DB_NAME", "beeport2")
#     user = os.getenv("DB_USER", "sig32")
#     password = os.getenv("DB_PASS", "")
#     host = os.getenv("DB_HOST", "localhost")
#     port = os.getenv("DB_PORT", "5432")

#     queries = {
#         "Reveals": """
#             SELECT block_number, reveal_count FROM storage_incentives_events 
#             WHERE event_type = 'CountReveals' AND block_number IN (
#                 SELECT block_number FROM storage_incentives_events WHERE event_type = 'WinnerSelected'
#             )""",
#         "Commits": """
#             SELECT block_number, commit_count FROM storage_incentives_events 
#             WHERE event_type = 'CountCommits' AND block_number IN (
#                 SELECT block_number FROM storage_incentives_events WHERE event_type = 'WinnerSelected'
#             )""",
#         "Price": "SELECT block_number, CAST(price AS NUMERIC) FROM storage_incentives_events WHERE event_type = 'PriceUpdate'",
#         "Freeze Time": "SELECT block_number, CAST(freeze_time AS NUMERIC) FROM storage_incentives_events WHERE event_type = 'StakeFrozen'",
#         "Withdraw Amount": "SELECT block_number, CAST(withdraw_amount AS NUMERIC) FROM storage_incentives_events WHERE event_type = 'PotWithdrawn'",
#         "Chunks": """
#             SELECT block_number, chunk_count FROM storage_incentives_events 
#             WHERE event_type = 'ChunkCount' AND block_number IN (
#                 SELECT block_number FROM storage_incentives_events WHERE event_type = 'WinnerSelected'
#             )""",
#         "Frozen Events Count": """
#             WITH WinnerEvents AS (
#                 SELECT 
#                     block_number,
#                     log_index,
#                     LAG(block_number) OVER (ORDER BY block_number, log_index) AS prev_block,
#                     LAG(log_index) OVER (ORDER BY block_number, log_index) AS prev_log_idx
#                 FROM storage_incentives_events
#                 WHERE event_type = 'WinnerSelected'
#             )
#             SELECT 
#                 w.block_number, 
#                 (
#                     SELECT COUNT(*) 
#                     FROM storage_incentives_events s 
#                     WHERE s.event_type = 'StakeFrozen'
#                       AND (s.block_number, s.log_index) > (COALESCE(w.prev_block, 0), COALESCE(w.prev_log_idx, -1))
#                       AND (s.block_number, s.log_index) <= (w.block_number, w.log_index)
#                 ) AS frozen_stake_count
#             FROM WinnerEvents w
#         """
#     }

#     try:
#         conn = psycopg2.connect(dbname=dbname, user=user, password=password, host=host, port=port)
        
#         dataframes = {}
#         for label, sql in queries.items():
#             df = pd.read_sql(sql, conn)
#             if not df.empty:
#                 dataframes[label] = df.set_index('block_number').sort_index()
#         conn.close()

#         if not dataframes:
#             print("No data found.")
#             return

#         all_indices = pd.concat([df.index.to_series() for df in dataframes.values()])
#         min_block, max_block = int(all_indices.min()), int(all_indices.max())
#         full_timeline = pd.DataFrame(index=range(min_block, max_block + 1))

#         # Colorblind Friendly Palette (Okabe-Ito)
#         cb_colors = ['#0072B2', '#D55E00', '#009E73', '#CC79A7', '#F0E442', '#56B4E9']
        
#         fig, ax_main = plt.subplots(figsize=(16, 10))
#         lines_and_scatters = []

#         for i, (label, df) in enumerate(dataframes.items()):
#             color = cb_colors[i % len(cb_colors)]
            
#             if i == 0:
#                 ax = ax_main
#             else:
#                 ax = ax_main.twinx()
#                 if i > 1:
#                     ax.spines['right'].set_position(('outward', 70 * (i - 1)))

#             plot_data = full_timeline.join(df).dropna()

#             if label == "Price":
#                 # Removed alpha for solid line
#                 item = ax.plot(plot_data.index, plot_data.iloc[:, 0], 
#                                color=color, label=label, linewidth=2)
#                 lines_and_scatters.extend(item)
#             else:
#                 # Removed alpha for solid scatter points
#                 item = ax.scatter(plot_data.index, plot_data.iloc[:, 0], 
#                                   color=color, label=label, s=45, edgecolors='none')
#                 lines_and_scatters.append(item)
            
#             ax.set_ylabel(label, color=color, fontweight='bold', fontsize=9)
#             ax.tick_params(axis='y', labelcolor=color)

#         ax_main.set_xlabel('Block Number', fontsize=12, fontweight='bold')
#         plt.title(f'Storage Incentives Metrics\nDatabase: {dbname}', fontsize=14, pad=25)
        
#         labels = [obj.get_label() for obj in lines_and_scatters]
#         ax_main.legend(lines_and_scatters, labels, loc='upper left', frameon=True, shadow=True)

#         fig.tight_layout()
#         plt.grid(True, which='both', linestyle=':', alpha=0.3)
#         plt.show()

#     except Exception as e:
#         print(f"Error: {e}")

# if __name__ == "__main__":
#     fetch_and_plot_metrics()

# # import os
# # import psycopg2
# # import pandas as pd
# # import matplotlib.pyplot as plt
# # from dotenv import load_dotenv

# # # Load environment variables from .env file
# # load_dotenv()

# # def fetch_and_plot_metrics():
# #     # --- 1. Database Configuration via Environment Variables ---
# #     dbname = os.getenv("DB_NAME", "beeport2")
# #     user = os.getenv("DB_USER", "sig32")
# #     password = os.getenv("DB_PASS", "")
# #     host = os.getenv("DB_HOST", "localhost")
# #     port = os.getenv("DB_PORT", "5432")

# #     queries = {
# #         "Reveals": """
# #             SELECT block_number, reveal_count FROM storage_incentives_events 
# #             WHERE event_type = 'CountReveals' AND block_number IN (
# #                 SELECT block_number FROM storage_incentives_events WHERE event_type = 'WinnerSelected'
# #             )""",
# #         "Commits": """
# #             SELECT block_number, commit_count FROM storage_incentives_events 
# #             WHERE event_type = 'CountCommits' AND block_number IN (
# #                 SELECT block_number FROM storage_incentives_events WHERE event_type = 'WinnerSelected'
# #             )""",
# #         "Price": "SELECT block_number, CAST(price AS NUMERIC) FROM storage_incentives_events WHERE event_type = 'PriceUpdate'",
# #         "Freeze Time": "SELECT block_number, CAST(freeze_time AS NUMERIC) FROM storage_incentives_events WHERE event_type = 'StakeFrozen'",
# #         "Chunks": """
# #             SELECT block_number, chunk_count FROM storage_incentives_events 
# #             WHERE event_type = 'ChunkCount' AND block_number IN (
# #                 SELECT block_number FROM storage_incentives_events WHERE event_type = 'WinnerSelected'
# #             )"""
# #     }

# #     try:
# #         conn = psycopg2.connect(dbname=dbname, user=user, password=password, host=host, port=port)
        
# #         dataframes = {}
# #         for label, sql in queries.items():
# #             df = pd.read_sql(sql, conn)
# #             if not df.empty:
# #                 dataframes[label] = df.set_index('block_number').sort_index()
# #         conn.close()

# #         if not dataframes:
# #             print("No data found.")
# #             return

# #         # Create continuous timeline
# #         all_indices = pd.concat([df.index.to_series() for df in dataframes.values()])
# #         min_block, max_block = int(all_indices.min()), int(all_indices.max())
# #         full_timeline = pd.DataFrame(index=range(min_block, max_block + 1))

# #         # --- 2. Colorblind Friendly Palette (Okabe-Ito) ---
# #         # Blue, Orange, Green, Pink, Yellow (High contrast for all vision types)
# #         cb_colors = ['#0072B2', '#D55E00', '#009E73', '#CC79A7', '#F0E442']
        
# #         fig, ax_main = plt.subplots(figsize=(16, 10))
# #         lines_and_scatters = []

# #         for i, (label, df) in enumerate(dataframes.items()):
# #             color = cb_colors[i % len(cb_colors)]
            
# #             if i == 0:
# #                 ax = ax_main
# #             else:
# #                 ax = ax_main.twinx()
# #                 if i > 1:
# #                     ax.spines['right'].set_position(('outward', 75 * (i - 1)))

# #             # Join with full timeline
# #             plot_data = full_timeline.join(df).dropna() # Dropna so scatter doesn't plot empty blocks

# #             if label == "Price":
# #                 # Price is a continuous trend, use a line plot
# #                 item = ax.plot(plot_data.index, plot_data.iloc[:, 0], 
# #                                color=color, label=label, linewidth=2, alpha=0.8)
# #                 lines_and_scatters.extend(item)
# #             else:
# #                 # All other metrics are discrete events, use a scatter plot
# #                 item = ax.scatter(plot_data.index, plot_data.iloc[:, 0], 
# #                                   color=color, label=label, s=25, alpha=0.7, edgecolors='none')
# #                 # Wrap scatter in list to handle legend consolidation
# #                 lines_and_scatters.append(item)
            
# #             ax.set_ylabel(label, color=color, fontweight='bold', fontsize=10)
# #             ax.tick_params(axis='y', labelcolor=color)

# #         ax_main.set_xlabel('Block Number', fontsize=12, fontweight='bold')
# #         plt.title(f'Storage Incentives Metrics (Discrete Events vs Price Trend)\nDatabase: {dbname}', 
# #                   fontsize=14, pad=25)
        
# #         # Consolidate legend
# #         labels = [obj.get_label() for obj in lines_and_scatters]
# #         ax_main.legend(lines_and_scatters, labels, loc='upper left', frameon=True, shadow=True)

# #         fig.tight_layout()
# #         plt.grid(True, which='both', linestyle=':', alpha=0.4)
# #         plt.show()

# #     except Exception as e:
# #         print(f"Error: {e}")

# # if __name__ == "__main__":
# #     fetch_and_plot_metrics()