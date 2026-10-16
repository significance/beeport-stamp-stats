"""Interactive HTML version of the storage-incentives dashboard.

Same queries and derived series as plot.py (imported from there), rendered with plotly so the
result can be zoomed, panned and hovered in a browser. Each series gets its own y-axis, as in
the PNG. Output is a single self-contained HTML file (plotly.js inlined, ~4 MB).

    python3 plot_html.py                       # writes incentives_hero_dashboard.html here
    python3 plot_html.py path/to/out.html
"""
import sys

import plotly.graph_objects as go

from plot import db_settings, load_dataframes

# Same palette as plot.py
CB_COLORS = ['#0072B2', '#D55E00', '#009E73', '#FFFFAA', '#F0E442', '#56B4E9']
NEON_ORANGE = '#FF5F1F'


def build_figure(dataframes, dbname):
    fig = go.Figure()
    layout_axes = {}
    labels = list(dataframes.keys())

    for i, label in enumerate(labels):
        df = dataframes[label]
        x = df.index.values
        y = df.iloc[:, 0].values
        axis_id = "y" if i == 0 else f"y{i + 1}"
        axis_key = "yaxis" if i == 0 else f"yaxis{i + 1}"
        color = CB_COLORS[i % len(CB_COLORS)]

        if label == "Price":
            fig.add_trace(go.Scatter(
                x=x, y=y, name=label, yaxis=axis_id, mode="lines",
                line=dict(color=NEON_ORANGE, width=4),
                hovertemplate="block %{x}<br>price %{y:,} PLUR<extra>Price</extra>",
            ))
            color = NEON_ORANGE
        elif label == "Reward per Node":
            fig.add_trace(go.Scatter(
                x=x, y=y, name=label, yaxis=axis_id, mode="markers",
                marker=dict(symbol="cross-thin", size=7, color="white", line=dict(color="white", width=1)),
                hovertemplate="block %{x}<br>%{y:.3g} PLUR/node<extra>Reward per Node</extra>",
            ))
            color = "white"
        elif label == "Pot Withdrawn (log)":
            fig.add_trace(go.Scatter(
                x=x, y=y, name=label, yaxis=axis_id, mode="markers",
                marker=dict(symbol="diamond", size=5, color=color, opacity=0.6),
                hovertemplate="block %{x}<br>%{y:.3g} PLUR<extra>Pot Withdrawn</extra>",
            ))
        else:
            fig.add_trace(go.Scatter(
                x=x, y=y, name=label, yaxis=axis_id, mode="markers",
                marker=dict(size=6, color=color, opacity=0.6),
                hovertemplate="block %{x}<br>%{y}<extra>" + label + "</extra>",
            ))

        axis = dict(
            title=dict(text=label, font=dict(color=color, size=11)),
            tickfont=dict(color=color, size=10),
            showgrid=(i == 0),
            gridcolor="#222222",
            zeroline=False,
        )
        if label == "Pot Withdrawn (log)":
            axis["type"] = "log"
        if i > 0:
            axis.update(overlaying="y", side="right", anchor="free",
                        autoshift=True, shift=0 if i == 1 else 8)
        layout_axes[axis_key] = axis

    fig.update_layout(
        title=dict(text=f"Storage Incentives Metrics<br><sup>Database: {dbname}</sup>",
                   font=dict(color="white", size=20), x=0.5),
        template="plotly_dark",
        paper_bgcolor="#0B0B0B",
        plot_bgcolor="#0B0B0B",
        xaxis=dict(title="Block Number", gridcolor="#222222", domain=[0.0, 0.62]),
        legend=dict(x=0.01, y=0.99, bgcolor="#151515", bordercolor="#444444", borderwidth=1),
        hovermode="x unified",
        margin=dict(l=60, r=40, t=90, b=60),
        height=900,
        **layout_axes,
    )
    return fig


def main(out="incentives_hero_dashboard.html"):
    settings = db_settings()
    dataframes = load_dataframes(settings)
    if not dataframes:
        print("No data found.")
        return 1
    fig = build_figure(dataframes, settings["dbname"])
    fig.write_html(out, include_plotlyjs=True, full_html=True)
    print(f"Wrote {out}")
    return 0


if __name__ == "__main__":
    sys.exit(main(*sys.argv[1:]))
