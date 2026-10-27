# /// script
# requires-python = ">=3.11"
# dependencies = ["matplotlib==3.11.2", "numpy==2.5.4"]
# ///
"""Plot execution time, compilation, and cache age from the adjacent report."""

from __future__ import annotations

import argparse
import csv
import json
import statistics
from pathlib import Path

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np

VARIANTS = ("before", "after", "refresh")
COLORS = {"before": "#52657d", "after": "#bd7b29", "refresh": "#087e73"}
LABELS = {
    "before": "Dependencies only",
    "after": "Workspace (no source key)",
    "refresh": "Workspace (source key)",
}


def render(folder):
    metadata = json.loads((folder / "metadata.json").read_text())
    with (folder / "samples.csv").open() as handle:
        rows = list(csv.DictReader(handle))
    history = metadata["source_history"][1:]
    indices = [commit["index"] for commit in history]
    plt.rcParams.update(
        {
            "font.family": "DejaVu Sans",
            "font.size": 10,
            "svg.fonttype": "none",
            "svg.hashsalt": "cache-refresh-20261027",
        }
    )
    fig, axes = plt.subplots(2, 2, figsize=(11.5, 7.6), sharex=True)
    panels = [
        (axes[0, 0], "PR execution", "job_seconds", "pr"),
        (axes[0, 1], "Main execution", "job_seconds", "main"),
        (axes[1, 0], "Read-only test compilation", "main_build_seconds", "read"),
        (axes[1, 1], "Age of restored workspace cache", "cache_age_commits", "age"),
    ]
    for ax, title, metric, scope in panels:
        for variant in VARIANTS:
            if scope == "age" and variant == "before":
                continue
            values = []
            for commit in history:
                if scope in {"pr", "read", "age"} and not commit.get(
                    "pr_eligible", True
                ):
                    values.append(np.nan)
                    continue
                phase = "warm"
                if scope == "pr" and commit["pr_can_save"]:
                    phase = "producer"
                elif scope == "main":
                    phase = "main" if commit["pr_can_save"] else "producer"
                selected = [
                    float(row[metric])
                    for row in rows
                    if int(row["history_index"]) == commit["index"]
                    and row["phase"] == phase
                    and row["variant"] == variant
                ]
                values.append(statistics.mean(selected) if selected else np.nan)
            ax.plot(
                indices,
                values,
                color=COLORS[variant],
                label=LABELS[variant],
                marker="o",
                markersize=4,
                linewidth=1.8,
            )
        ax.set_title(title, loc="left", fontweight="bold", pad=10)
        ax.set_ylabel("Commits since cached source" if scope == "age" else "Seconds")
        ax.grid(axis="y", color="#e4e8ed", linewidth=0.7)
        ax.set_axisbelow(True)
        ax.spines[["top", "right"]].set_visible(False)
        ax.spines[["left", "bottom"]].set_color("#b7bfc9")
        ax.set_xticks(indices)
        ax.set_xlim(min(indices) - 0.3, max(indices) + 0.3)
        ax.set_ylim(bottom=0)
    for ax in axes[1]:
        ax.set_xlabel("Source commit in the fixed replay window")
    handles, labels = axes[0, 0].get_legend_handles_labels()
    project = "Ruff" if metadata["repository"] == "astral-sh/ruff" else "uv"
    title = f"{project}: keeping workspace caches current across source changes"
    if not metadata["complete"]:
        title += " (incomplete preview)"
    fig.suptitle(title, x=0.08, y=0.99, ha="left", fontsize=16, fontweight="bold")
    fig.legend(
        handles,
        labels,
        loc="upper left",
        bbox_to_anchor=(0.075, 0.955),
        ncol=3,
        frameon=False,
    )
    fig.text(
        0.08,
        0.015,
        "Execution includes setup and cache uploads; queue time is excluded. Read-only values average three replicas.\nMain and writable PR values use one run per source. Confidence intervals for aggregate comparisons are in the report.",
        fontsize=9,
        color="#4c5764",
    )
    fig.tight_layout(rect=(0.02, 0.07, 1, 0.90), h_pad=2.0, w_pad=2.2)
    svg_path = folder / "history.svg"
    fig.savefig(svg_path, metadata={"Date": None})
    svg_path.write_text(
        "\n".join(line.rstrip() for line in svg_path.read_text().splitlines()) + "\n"
    )
    fig.savefig(
        folder / "history.png", dpi=140, metadata={"Software": "cache-refresh-report"}
    )
    plt.close(fig)
    print(folder / "history.svg")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--directory", type=Path, default=Path(__file__).resolve().parent
    )
    args = parser.parse_args()
    render(args.directory)
