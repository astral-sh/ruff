# /// script
# requires-python = ">=3.11"
# dependencies = ["numpy==2.5.4"]
# ///
"""Recompute paired history timing estimates from the adjacent CSV file."""

from __future__ import annotations

import argparse
import csv
import json
import math
import re
from collections import defaultdict
from pathlib import Path

import numpy as np

ROOT = Path(__file__).resolve().parent


def estimate(values, block_length=5):
    data = np.asarray(values, dtype=float)
    assert data.ndim == 2 and data.shape[1] == 2 and np.isfinite(data).all()
    before, after = data.mean(axis=0)
    rng = np.random.default_rng(20261025)
    starts = rng.integers(len(data), size=(50000, math.ceil(len(data) / block_length)))
    indices = ((starts[..., None] + np.arange(block_length)) % len(data)).reshape(
        50000, -1
    )[:, : len(data)]
    means = data[indices].mean(axis=1)
    return {
        "before_mean": float(before),
        "after_mean": float(after),
        "saved": float(before - after),
        "speedup": float(before / after),
        "saved_ci90": np.quantile(means[:, 0] - means[:, 1], [0.05, 0.95]).tolist(),
        "speedup_ci90": np.quantile(means[:, 0] / means[:, 1], [0.05, 0.95]).tolist(),
    }


def main():
    parser = argparse.ArgumentParser()
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--read-only", action="store_true")
    mode.add_argument("--fresh-initial-cache", action="store_true")
    args = parser.parse_args()
    metadata = json.loads((ROOT / "metadata.json").read_text())
    if (
        args.fresh_initial_cache
        and "read_only_fresh_initial_cache_sensitivity" not in metadata
    ):
        parser.error("This report does not include a fresh-initial-cache sensitivity.")
    filename = (
        "fresh-initial-samples.csv" if args.fresh_initial_cache else "samples.csv"
    )
    with (ROOT / filename).open() as handle:
        all_rows = list(csv.DictReader(handle))
    assert len({row["job_id"] for row in all_rows}) == len(all_rows)
    read_only = args.read_only or args.fresh_initial_cache
    if read_only:
        selected_ids = {
            int(row["job_id"]) for row in all_rows if row["phase"] == "warm"
        }
        writer_indices = set()
        expected_timing = metadata[
            "read_only_fresh_initial_cache_sensitivity"
            if args.fresh_initial_cache
            else "read_only_timing"
        ]
    else:
        selected_ids = set(metadata["sample_selection"]["selected_job_ids"])
        writer_indices = set(metadata["sample_selection"]["writer_indices"])
        expected_timing = metadata["timing"]
    groups = defaultdict(lambda: defaultdict(list))
    for row in all_rows:
        if int(row["job_id"]) in selected_ids:
            groups[int(row["history_index"])][row["variant"]].append(row)
    expected_writers = set()
    for commit in metadata["source_history"][1:]:
        if not commit["pr_eligible"]:
            continue
        can_save = (
            not read_only
            and metadata["repository"] == "astral-sh/uv-dev"
            and any(
                path
                in {
                    "Cargo.toml",
                    "Cargo.lock",
                    "rust-toolchain.toml",
                    "crates/uv-build/rust-toolchain.toml",
                }
                or re.fullmatch(r"crates/.*/Cargo\.toml", path)
                or path.startswith(".cargo/")
                or re.fullmatch(r"\.github/workflows/.*\.yml", path)
                for path in commit["changed_files"]
            )
        )
        warm = [
            row
            for row in all_rows
            if int(row["history_index"]) == commit["index"] and row["phase"] == "warm"
        ]
        assert len(warm) == 2 * metadata["replicas_per_commit"]
        if can_save and any(row["exact_hit"] == "False" for row in warm):
            expected_writers.add(commit["index"])
    assert writer_indices == expected_writers
    expected_ids = {
        int(row["job_id"])
        for row in all_rows
        if (int(row["history_index"]) in writer_indices and row["phase"] == "producer")
        or (int(row["history_index"]) not in writer_indices and row["phase"] == "warm")
    }
    assert selected_ids == expected_ids
    values = []
    for index, variants in sorted(groups.items()):
        assert set(variants) == {"before", "after"}, index
        assert (
            len({row["source_sha"] for rows in variants.values() for row in rows}) == 1
        )
        for rows in variants.values():
            expected = 1 if index in writer_indices else metadata["replicas_per_commit"]
            assert sorted(int(row["replica"]) for row in rows) == list(
                range(1, expected + 1)
            )
            assert all(
                row["cache_saved"] == ("True" if index in writer_indices else "False")
                for row in rows
            )
            assert all(
                row["phase"] == ("producer" if index in writer_indices else "warm")
                for row in rows
            )
        values.append(
            [
                np.mean([float(row["job_seconds"]) for row in variants[variant]])
                for variant in ["before", "after"]
            ]
        )
    assert selected_ids == {
        int(row["job_id"])
        for variants in groups.values()
        for rows in variants.values()
        for row in rows
    }
    assert len(values) == expected_timing["commits"]
    assert len(selected_ids) // 2 == expected_timing["paired_runs"]
    result = estimate(values, metadata.get("bootstrap_block_length", 5))
    for key, value in result.items():
        assert np.allclose(value, expected_timing[key], rtol=0, atol=1e-9), key
    if "other_work_breakdown" in metadata and not args.fresh_initial_cache:
        with (ROOT / "step-samples.csv").open() as handle:
            step_rows = list(csv.DictReader(handle))
        assert len({(row["job_id"], row["step"]) for row in step_rows}) == len(
            step_rows
        )
        assert {int(row["job_id"]) for row in step_rows} == selected_ids
        assert {row["group"] for row in step_rows} == set(
            metadata["other_work_breakdown"]
        )
        breakdown = {}
        for group, expected in metadata["other_work_breakdown"].items():
            by_commit = defaultdict(lambda: defaultdict(lambda: defaultdict(float)))
            for row in step_rows:
                if row["group"] == group:
                    by_commit[int(row["history_index"])][row["variant"]][
                        row["job_id"]
                    ] += float(row["seconds"])
            assert set(by_commit) == set(groups)
            assert {
                int(job)
                for variants in by_commit.values()
                for jobs in variants.values()
                for job in jobs
            } == selected_ids
            breakdown[group] = {
                variant: float(
                    np.mean(
                        [
                            np.mean(list(variants[variant].values()))
                            for variants in by_commit.values()
                        ]
                    )
                )
                for variant in ["before", "after"]
            }
            for variant in ["before", "after"]:
                assert np.isclose(
                    breakdown[group][variant], expected[variant], rtol=0, atol=1e-9
                )
        result["other_work_breakdown"] = breakdown
    label = (
        "fresh-initial-cache"
        if args.fresh_initial_cache
        else "read-only"
        if args.read_only
        else "normal-policy"
    )
    print(
        json.dumps(
            {
                "mode": label,
                "commits": len(values),
                "paired_runs": len(selected_ids) // 2,
                **result,
            },
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
