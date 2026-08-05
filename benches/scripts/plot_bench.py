#!/usr/bin/env python3
import argparse
import math
import re
import sys
from pathlib import Path

import plotly.graph_objects as go
from plotly.subplots import make_subplots

BENCH_RE = re.compile(
    r"[A-Za-z0-9_]+_Performances/(Encode|Decode)/([A-Za-z0-9_]+)/(\d+)"
)
THRPT_RE = re.compile(
    r"thrpt:\s*\[[\d.]+\s*[KMG]iB/s\s+([\d.]+)\s*([KMG]iB)/s\s+[\d.]+\s*[KMG]iB/s"
)

UNIT_TO_MIB = {"KiB": 1 / 1024, "MiB": 1.0, "GiB": 1024.0}

# Fixed categorical order/colors, per the house palette (slots 1-6).
# Color follows the library identity, never its rank at a given size.
LIBRARY_ORDER = ["Turbo", "bs58", "base58", "five8", "Turbo_XMR", "base58_monero"]
LIBRARY_LABEL = {
    "Turbo": "base58-turbo",
    "Turbo_XMR": "base58-turbo (XMR)",
    "bs58": "bs58",
    "base58": "base58",
    "five8": "five8",
    "base58_monero": "base58-monero",
}
COLORS = {
    "Turbo": "#2a78d6",
    "Turbo_XMR": "#1baf7a",
    "bs58": "#eb6834",
    "base58": "#eda100",
    "five8": "#e87ba4",
    "base58_monero": "#4a3aa7",
}

# No fixed surface: the chart sits on whatever page background it's placed
# on (GitHub light or dark theme), so ink is a neutral gray readable on both.
THEME = {
    "paper": "rgba(0,0,0,0)",
    "plot": "rgba(0,0,0,0)",
    "ink": "#767671",
    "muted": "#8a8983",
    "grid": "rgba(137,135,129,0.35)",
}

NICE_MULTIPLES = [1, 2, 2.5, 5, 7.5, 10]


def nice_axis(max_value, target_ticks=6):
    """Round (step, top) to human-friendly numbers (…10, 25, 50, 75, 100…)."""
    if max_value <= 0:
        return 10, 10
    raw_step = max_value / target_ticks
    magnitude = 10 ** math.floor(math.log10(raw_step))
    residual = raw_step / magnitude
    step = next(
        (m * magnitude for m in NICE_MULTIPLES if residual <= m),
        10 * magnitude,
    )
    top = math.ceil(max_value / step) * step
    return step, top


def parse(text):
    """-> {phase: {size: {library: mib_per_s}}}"""
    data = {"Encode": {}, "Decode": {}}
    pending = None
    for line in text.splitlines():
        m = BENCH_RE.search(line)
        if m:
            pending = (m.group(1), m.group(2), int(m.group(3)))
            continue
        m = THRPT_RE.search(line)
        if m and pending:
            phase, library, size = pending
            value = float(m.group(1)) * UNIT_TO_MIB[m.group(2)]
            data[phase].setdefault(size, {})[library] = value
            pending = None
    return data


def render(data, out_path):
    theme = THEME

    sizes = sorted({s for phase in data.values() for s in phase})
    size_labels = [f"{s} B" for s in sizes]

    fig = make_subplots(
        rows=2,
        cols=1,
        subplot_titles=("Encode", "Decode"),
        vertical_spacing=0.16,
    )

    # Constant per-bar width (a share of the full roster) so a group with
    # fewer libraries present (e.g. five8, only benched at 32/64/128 B) just
    # draws a narrower cluster instead of leaving a gap for the missing bar.
    bar_w = 0.9 / len(LIBRARY_ORDER)
    half_max = len(LIBRARY_ORDER) * bar_w / 2
    x_pad = 0.03

    for row, phase in enumerate(("Encode", "Decode"), start=1):
        series = {lib: {"x": [], "y": [], "text": []} for lib in LIBRARY_ORDER}
        for xi, size in enumerate(sizes):
            present = [lib for lib in LIBRARY_ORDER if lib in data[phase].get(size, {})]
            start = xi - (len(present) * bar_w) / 2
            for j, lib in enumerate(present):
                value = data[phase][size][lib]
                series[lib]["x"].append(start + bar_w * (j + 0.5))
                series[lib]["y"].append(value)
                series[lib]["text"].append(f"{value:.0f}" if lib == "Turbo" else "")

        phase_max = max(v for s in series.values() for v in s["y"])
        _, y_top = nice_axis(phase_max)

        for lib in LIBRARY_ORDER:
            s = series[lib]
            if not s["x"]:
                continue
            fig.add_trace(
                go.Bar(
                    x=s["x"],
                    y=s["y"],
                    width=bar_w * 0.9,
                    name=LIBRARY_LABEL[lib],
                    marker_color=COLORS[lib],
                    text=s["text"],
                    textposition="outside",
                    textfont=dict(size=12, color=theme["muted"]),
                    legendgroup=lib,
                    showlegend=(row == 1),
                ),
                row=row,
                col=1,
            )

        fig.update_yaxes(
            range=[0, y_top * 1.12],
            title=dict(text="MiB/s", font=dict(color=theme["muted"])),
            gridcolor=theme["grid"],
            zerolinecolor=theme["grid"],
            tickfont=dict(color=theme["muted"]),
            automargin=True,
            row=row,
            col=1,
        )
        fig.update_xaxes(
            tickvals=list(range(len(sizes))),
            ticktext=size_labels,
            range=[-half_max - x_pad, len(sizes) - 1 + half_max + x_pad],
            title=dict(
                text="Payload size" if row == 2 else "",
                font=dict(color=theme["muted"]),
            ),
            tickfont=dict(color=theme["muted"]),
            automargin=True,
            row=row,
            col=1,
        )

    fig.update_layout(
        barmode="overlay",
        width=1150,
        height=900,
        paper_bgcolor=theme["paper"],
        plot_bgcolor=theme["plot"],
        font=dict(family="Arial, Helvetica, sans-serif", color=theme["ink"], size=15),
        legend=dict(
            orientation="h",
            yanchor="top",
            y=1.12,
            xanchor="center",
            x=0.5,
            font=dict(size=13),
        ),
        margin=dict(l=10, r=10, t=110, b=10, autoexpand=True),
    )

    for annotation in fig["layout"]["annotations"]:
        annotation["font"] = dict(size=16, color=theme["ink"])

    fig.write_image(out_path, scale=2)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "input", nargs="?", type=Path, help="raw `cargo bench` output (default: stdin)"
    )
    parser.add_argument(
        "--out",
        type=Path,
        default=Path(__file__).resolve().parent.parent / "results" / "throughput.png",
    )
    args = parser.parse_args()

    text = args.input.read_text() if args.input else sys.stdin.read()
    data = parse(text)
    if not data["Encode"] and not data["Decode"]:
        sys.exit("No benchmark lines found in input.")

    args.out.parent.mkdir(parents=True, exist_ok=True)
    render(data, args.out)
    print(f"wrote {args.out}")


if __name__ == "__main__":
    main()
