# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Summarize complete, interference-free runs and paired initial-latency intervals."""

from __future__ import annotations

import csv
import json
import math
import random
import re
import statistics as st
import sys
from pathlib import Path

root = Path(__file__).parent
tag = sys.argv[1]
summary = {}
for layout in ("regular",):
    data, blocks = {}, {}
    for name in ("baseline", "original", "progressive"):
        files = [root / f"{tag}-{layout}-{name}-{block}.csv" for block in range(6)]
        assert all(file.with_suffix(".done").exists() for file in files)
        batches = [list(csv.DictReader(file.open())) for file in files]
        blocks[name] = batches
        rows = [row for batch in batches for row in batch]
        metrics = {
            key: st.median(float(row[key]) for row in rows)
            for key in rows[0]
            if key != "sample"
        }
        metrics["n"] = len(rows)
        for metric in ("warm", "update", "completion", "total"):
            metrics[f"{metric}_p95_ms"] = sorted(
                float(row[f"{metric}_ms"]) for row in rows
            )[math.ceil(0.95 * len(rows)) - 1]
        if "reclaimed_ms" in rows[0]:
            metrics["drain_after_completion_ms"] = st.median(
                float(row["reclaimed_ms"]) - float(row["total_ms"]) for row in rows
            )
        for phase in ("before", "after"):
            report = Path(str(files[0].with_suffix("")) + f"-{phase}.json")
            memory = json.loads(report.read_text())
            parsed = next(
                query for query in memory["queries"] if query["name"] == "parsed_module"
            )
            metrics[f"parsed_{phase}_bytes"] = parsed["fields_bytes"]
        rss, cpu = [], []
        for file in files:
            log = file.with_suffix(".log").read_text()
            assert "Preflight passed" in log
            match = re.search(r"(\d+)\s+maximum resident set size", log)
            assert match
            rss.append(int(match[1]) / 1024**2)
            match = re.search(r"([\d.]+) user\s+([\d.]+) sys", log)
            assert match
            cpu.append(float(match[1]) + float(match[2]))
        metrics["rss_mib"] = st.median(rss)
        metrics["cpu_s_per_process"] = st.median(cpu)
        data[name] = metrics
    for name, metrics in data.items():
        metrics["warm_change_percent"] = (
            metrics["warm_ms"] / data["baseline"]["warm_ms"] - 1
        ) * 100
        if name == "baseline":
            continue
        rng = random.Random(3909)
        ratios = []
        for _ in range(10000):
            indices = rng.choices(range(6), k=6)
            baseline = st.median(
                float(row["warm_ms"]) for i in indices for row in blocks["baseline"][i]
            )
            candidate = st.median(
                float(row["warm_ms"]) for i in indices for row in blocks[name][i]
            )
            ratios.append((candidate / baseline - 1) * 100)
        ratios.sort()
        metrics["warm_change_95ci_percent"] = [ratios[250], ratios[9750]]
    summary[layout] = data
result = json.dumps(summary, indent=2) + "\n"
print(result)
(root / f"{tag}-summary.json").write_text(result)
