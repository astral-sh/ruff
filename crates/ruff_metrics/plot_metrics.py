#!/usr/bin/env uv run
# /// script
# requires-python = ">=3.10"
# dependencies = [
#     "pyqt6",
#     "matplotlib",
#     "numpy",
#     "pandas",
# ]
# ///

"""Render metrics that have been produced by the ruff_metrics crate.
"""

import argparse
import json

import matplotlib.pyplot as plt
import numpy as np
import pandas as pd

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--metrics", help="JSON file with metrics data", default="metrics.json")
parser.add_argument("-o", "--output", help="save image to the given filename")
subparsers = parser.add_subparsers(dest="command")

counter_parser = subparsers.add_parser("counter")
counter_parser.add_argument("key", help="the counter metric to render")
counter_parser.add_argument("--group-by", required=False)

histogram_parser = subparsers.add_parser("histogram")
histogram_parser.add_argument("key", help="the metric to render")
histogram_parser.add_argument("--group-by")
histogram_parser.add_argument("--bins", help="number of bins (default: auto)")

args = parser.parse_args()

with open(args.metrics) as f:
    results = []
    for line in f:
        results.append(json.loads(line))
all_data = pd.DataFrame(results)

def get_metric(d: pd.DataFrame, key: str) -> pd.DataFrame:
    return d[d["key"] == key]

def plot_counter(d: pd.DataFrame, label: str=None) -> None:
    plt.xlabel("Time [s]")
    d["total"] = d["delta"].cumsum()
    plt.plot("since_start", "total", data=d, label=label)

def show_plot():
    if args.output:
        plt.savefig(args.output, dpi=600)
    else:
        plt.show()

def cmd_counter() -> None:
    data = get_metric(all_data, args.key)
    plt.ylabel(args.key)
    if args.group_by is None:
        plot_counter(data)
    else:
        for group, gd in data.groupby(args.group_by):
            plot_counter(gd, group)
        if data[args.group_by].nunique() <= 10:
            plt.legend(loc="best")
    show_plot()

def cmd_histogram() -> None:
    data = get_metric(all_data, args.key)
    bins = int(args.bins) if args.bins else "auto"
    data = data.groupby(args.group_by).last()
    plt.xlabel(args.key)
    plt.ylabel("Count")
    plt.yscale("log")
    plt.hist(data["value"], bins=bins)
    show_plot()

if args.command == "counter":
    cmd_counter()
elif args.command == "histogram":
    cmd_histogram()
else:
    print("Missing command")
    parser.print_usage()
