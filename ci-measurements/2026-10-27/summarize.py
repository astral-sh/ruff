# /// script
# requires-python = ">=3.11"
# dependencies = ["numpy==2.5.4"]
# ///
"""Recompute source-refresh measurements from the published individual samples."""

from __future__ import annotations

import argparse
import csv
import fnmatch
import json
import math
import statistics
from collections import defaultdict
from pathlib import Path

import numpy as np

VARIANTS = ("before", "after", "refresh")
PAIRS = (("after", "refresh"), ("before", "refresh"), ("before", "after"))
METRICS = (
    "job_seconds",
    "restore_seconds",
    "main_build_seconds",
    "nextest_seconds",
    "mdtest_step_seconds",
    "apt_seconds",
    "other_seconds",
    "cache_post_seconds",
    "cache_fetch_seconds",
    "cache_extract_seconds",
)


def can_write_pr(repository, commit):
    if repository != "astral-sh/uv-dev":
        return False
    exact = {
        "Cargo.toml",
        "Cargo.lock",
        "rust-toolchain.toml",
        "crates/uv-build/rust-toolchain.toml",
    }
    return any(
        path in exact
        or path.startswith(".cargo/")
        or fnmatch.fnmatchcase(path, "crates/*/Cargo.toml")
        or fnmatch.fnmatchcase(path, ".github/workflows/*.yml")
        for path in commit["changed_files"]
    )


def estimate(values, length, count, seed):
    values = np.asarray(values, dtype=float)
    assert values.ndim == 2 and values.shape[1] == 2 and np.isfinite(values).all()
    before, after = values.mean(axis=0)
    result = {
        "commits": len(values),
        "before_mean": float(before),
        "after_mean": float(after),
        "saved": float(before - after),
        "speedup": float(before / after),
    }
    if len(values) < max(5, 2 * length):
        return result
    rng = np.random.default_rng(seed)
    starts = rng.integers(len(values), size=(count, math.ceil(len(values) / length)))
    draw = ((starts[:, :, None] + np.arange(length)) % len(values)).reshape(count, -1)[
        :, : len(values)
    ]
    boot = values[draw].mean(axis=1)
    result.update(
        block_length=length,
        resamples=count,
        before_ci90=np.quantile(boot[:, 0], [0.05, 0.95]).tolist(),
        after_ci90=np.quantile(boot[:, 1], [0.05, 0.95]).tolist(),
        saved_ci90=np.quantile(boot[:, 0] - boot[:, 1], [0.05, 0.95]).tolist(),
        speedup_ci90=np.quantile(boot[:, 0] / boot[:, 1], [0.05, 0.95]).tolist(),
    )
    return result


def summarize(metadata, all_rows):
    assert len({row["job_id"] for row in all_rows}) == len(all_rows)
    groups = defaultdict(lambda: defaultdict(lambda: defaultdict(list)))
    cache_keys = {variant: {} for variant in VARIANTS}
    uploads = dict.fromkeys(VARIANTS, 0)
    uploaded_bytes = dict.fromkeys(VARIANTS, 0)
    writer_post = dict.fromkeys(VARIANTS, 0.0)
    for row in sorted(
        all_rows, key=lambda row: (int(row["record_sequence"]), row["variant"])
    ):
        index, variant, phase = int(row["history_index"]), row["variant"], row["phase"]
        assert variant in VARIANTS and phase in {"seed", "warm", "producer", "main"}
        assert row["exact_hit"] in {"True", "False"}
        assert row["cache_saved"] in {"True", "False"}
        assert (row["exact_hit"] == "True") == bool(
            row["restored_key"] and row["restored_key"] == row["primary_key"]
        )
        assert row["workflow_head"] == metadata["workflow_head"]
        source = (
            metadata["initial_sources"][variant]
            if phase == "seed"
            else metadata["source_history"][index]["sha"]
        )
        assert row["source_sha"] == source
        original = (
            metadata["initial_source_history_indices"][variant]
            if phase == "seed"
            else metadata["source_history"][index]["original_history_index"]
        )
        if row["restored_key"]:
            origin = cache_keys[variant][row["restored_key"]]
            assert int(row["restored_bytes"]) == origin["bytes"]
            assert row["cache_source_sha"] == origin["source"]
            assert int(row["cache_source_history_index"]) == origin["index"]
            assert int(row["cache_age_commits"]) == original - origin["index"] >= 0
            if variant == "refresh":
                expected_index = index if phase == "main" else index - 1
                assert (
                    origin["source"]
                    == metadata["source_history"][expected_index]["sha"]
                )
        else:
            assert phase == "seed"
        if row["cache_saved"] == "True":
            assert phase in {"seed", "producer"}
            assert row["primary_key"] not in cache_keys[variant]
            assert row["exact_hit"] == "False" and int(row["saved_bytes"]) > 0
            cache_keys[variant][row["primary_key"]] = {
                "source": source,
                "index": original,
                "bytes": int(row["saved_bytes"]),
            }
            if phase == "producer":
                uploads[variant] += 1
                uploaded_bytes[variant] += int(row["saved_bytes"])
        else:
            assert int(row["saved_bytes"]) == 0
        if phase == "producer":
            assert row["exact_hit"] == "True" or row["cache_saved"] == "True"
            writer_post[variant] += float(row["cache_post_seconds"])
        if phase == "main":
            assert row["exact_hit"] == "True" and row["cache_saved"] == "False"
        if phase == "warm":
            assert row["cache_saved"] == "False"
        groups[index][phase][variant].append(row)
    scopes = {
        name: [] for name in ("read_only", "normal_pr_policy", "main", "combined")
    }
    selected = {name: [] for name in scopes}
    for commit in metadata["source_history"][1:]:
        index = commit["index"]
        eligible = commit.get("pr_eligible", True)
        writable = can_write_pr(metadata["repository"], commit)
        assert writable == commit["pr_can_save"]
        phases = groups[index]
        expected_phases = (
            {"producer"}
            | ({"warm"} if eligible else set())
            | ({"main"} if writable else set())
        )
        assert set(phases) == expected_phases, (index, set(phases), expected_phases)
        for phase, variants in phases.items():
            assert set(variants) == set(VARIANTS)
            for rows in variants.values():
                expected_count = (
                    metadata["replicas_per_commit"] if phase == "warm" else 1
                )
                assert sorted(int(row["replica"]) for row in rows) == list(
                    range(1, expected_count + 1)
                )
                assert len({row["primary_key"] for row in rows}) == 1
                assert len({row["restored_key"] for row in rows}) == 1

        def take(phase, phases=phases):
            return {
                variant: {
                    metric: statistics.mean(
                        float(row[metric]) for row in phases[phase][variant]
                    )
                    for metric in METRICS
                }
                for variant in VARIANTS
            }

        main_phase = "main" if writable else "producer"
        main_values = take(main_phase)
        scopes["main"].append(main_values)
        selected["main"].extend(
            int(row["job_id"]) for rows in phases[main_phase].values() for row in rows
        )
        if eligible:
            normal_phase = "producer" if writable else "warm"
            normal_values = take(normal_phase)
            for scope, phase in (
                ("read_only", "warm"),
                ("normal_pr_policy", normal_phase),
            ):
                scopes[scope].append(take(phase))
                selected[scope].extend(
                    int(row["job_id"])
                    for rows in phases[phase].values()
                    for row in rows
                )
        else:
            normal_values = {
                variant: dict.fromkeys(METRICS, 0.0) for variant in VARIANTS
            }
        scopes["combined"].append(
            {
                variant: {
                    metric: main_values[variant][metric]
                    + normal_values[variant][metric]
                    for metric in METRICS
                }
                for variant in VARIANTS
            }
        )
    selected["combined"] = sorted(
        set(selected["main"]) | set(selected["normal_pr_policy"])
    )
    result = {}
    for scope, commits in scopes.items():
        comparisons = {}
        for left, right in PAIRS:
            values = [
                [commit[left]["job_seconds"], commit[right]["job_seconds"]]
                for commit in commits
            ]
            value = estimate(
                values,
                metadata["bootstrap_block_length"],
                metadata["bootstrap_resamples"],
                metadata["bootstrap_seed"],
            )
            value["sensitivity"] = {
                str(length): estimate(
                    values,
                    length,
                    metadata["bootstrap_resamples"],
                    metadata["bootstrap_seed"],
                )
                for length in metadata["bootstrap_sensitivity_lengths"]
            }
            comparisons[f"{left}_vs_{right}"] = value
        result[scope] = {
            "comparisons": comparisons,
            "selected_job_ids": sorted(selected[scope]),
            "means": {
                variant: {
                    metric: statistics.mean(
                        commit[variant][metric] for commit in commits
                    )
                    for metric in METRICS
                }
                for variant in VARIANTS
            },
        }
    result["cache"] = {
        variant: {
            "uploads": uploads[variant],
            "uploaded_bytes": uploaded_bytes[variant],
            "writer_post_seconds": writer_post[variant],
        }
        for variant in VARIANTS
    }
    return result


def check(expected, actual, path="summary"):
    if isinstance(expected, dict):
        for key, value in expected.items():
            check(value, actual[key], path + "." + key)
    elif isinstance(expected, (float, int, list)):
        assert np.allclose(expected, actual, atol=1e-9, rtol=0), (
            path,
            expected,
            actual,
        )
    else:
        assert expected == actual, (path, expected, actual)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--directory", type=Path, default=Path(__file__).resolve().parent
    )
    args = parser.parse_args()
    metadata = json.loads((args.directory / "metadata.json").read_text())
    with (args.directory / "samples.csv").open() as handle:
        rows = list(csv.DictReader(handle))
    result = summarize(metadata, rows)
    check(metadata["expected"], result)
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
