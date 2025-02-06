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
subparsers = parser.add_subparsers(dest="command")

counter_parser = subparsers.add_parser("counter")
counter_parser.add_argument("key", help="the counter metric to render")
counter_parser.add_argument("--group-by", required=False)

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
    plt.show()

if args.command == "counter":
    cmd_counter()
else:
    print("Missing command")
    parser.print_usage()
