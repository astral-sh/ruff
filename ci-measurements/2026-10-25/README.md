# Ruff Linux Rust cache replay

Replay under the repository’s PR cache-save policy across 30 source commits, using 90 paired runs. Both variants use [`rust-workspace-cache@8139eb47`](https://github.com/astral-sh/github-actions/tree/8139eb474ac22fe39bb1e33b50afa426dcdaf455/rust-workspace-cache), matching active compilers, Cargo freshness settings, and job commands within each pair. The variants switch `cache-workspace-crates` and use isolated cache keys and artifact names.

| Dependencies only (s) | Full workspace (s) |   Speedup (90% CI) | Saved (s; 90% CI) |
| --------------------: | -----------------: | -----------------: | ----------------: |
|                 262.8 |              266.9 | 0.98× (0.95–1.03×) |  -4.1 (-14.6–7.8) |

Times cover the first through last runner-log timestamps, including setup and post steps. Queue time is excluded. Speedup is dependency-only time divided by full-cache time; values below 1× mean a slowdown.

Ruff PR jobs do not save caches. The aggregate uses the mean of three read-only pairs per source change, with each source change receiving equal weight.

## Source and cache history

The measurement window is the 30 first-parent commits after [`9e9ad51c`](https://github.com/astral-sh/ruff/commit/9e9ad51c272cb8606ee4678b8fa8981b3c133871) through [`7a8f63f0`](https://github.com/astral-sh/ruff/commit/7a8f63f0d71a1d381f99652a5fee3b043b32e8e6). Each eligible source commit has three paired read-only repetitions. The headline applies the PR save policy described above. The job definition is held fixed while source, dependencies, and the pinned toolchain follow each source revision.

The initial caches are populated from [`3e3f22dc`](https://github.com/astral-sh/ruff/commit/3e3f22dcc47a534d5bb6668817464b02c6e6e133), the preceding cache-key change, 19 commits before the window’s parent. Source-only changes keep the immutable entry because source contents are absent from its key. An exact cache hit can therefore still require workspace recompilation. A non-exact read is followed by a successful cache-producing run before advancing to the next source commit.

The initial cohort covers commits 1–9. After the first refresh, the later cohort supplies the remaining measurements. Both cohorts include commit 10 to compare the restored cache source, keys, compiler, test counts, archive sizes, and workspace compilations. Only one copy of that overlapping commit enters the aggregate. `metadata.json` retains this comparison.

At the overlapping commit, both cohorts restore the same cache source and cache keys apart from isolation prefixes, use the same compiler, run the same test count, and compile the same workspace crates. Archive sizes differ by less than 0.001% in both modes. Full-cache compilation averages 55.23 s and 55.24 s in the two cohorts. The later cohort therefore supplies the remaining history after this cache refresh.

These isolated caches are reconstructed from historical source. The replay does not measure long-term artifact accumulation, cache eviction, or queueing in production.

The separate `ty` annotation test pass runs for PR source changes matching the production path filter. Cache-producing runs skip it, matching `main`, where the change detector compares the checked-out revision with itself. Both cache modes retain the PR annotation pass; [#29710](https://github.com/astral-sh/ruff/pull/29710) removes that duplicate work separately.

## Initial-cache sensitivity

A read-only sensitivity seeded at the start of the window gives 262.0 → 247.2 s, 1.06× (0.99–1.15×). It uses a separate cohort for the first 9 commits and the same later runs. This is an alternate reconstruction, not an isolated causal estimate of cache age, because the initial runner cohort also differs.

The headline uses the preceding cache-key change to reconstruct an older immutable entry. `fresh-initial-samples.csv` retains the alternate reconstruction; those initial samples are excluded from the headline.

## Uncertainty

90% percentile intervals from 50,000 circular moving-block bootstrap resamples of paired per-commit means, block length 5 commits. Repetitions are averaged within each commit; they are not treated as independent source changes. Intervals describe the observed histories, not arbitrary future workloads.

The source commits, rather than the individual runner repetitions, are the sampling units. The intervals do not estimate variation across other runners, other CI jobs, or arbitrary future source changes. `metadata.json` also contains one-, three-, and ten-commit block sensitivities.

## Job phases

| Phase                    | Dependencies only (s) | Full workspace (s) |
| ------------------------ | --------------------: | -----------------: |
| Rust cache restore       |                   6.8 |               31.0 |
| Main test compilation    |                  55.8 |               48.5 |
| Test execution           |                  68.5 |               68.1 |
| Separate annotation pass |                  27.3 |               27.3 |
| Other job work           |                 104.3 |               92.0 |

Other work includes tool setup, documentation builds, dogfooding, and post steps. Only the main test compilation is separated from its test execution.

The following step times are included in “Other job work” above. They include compilation and execution where applicable.

| Other work           | Dependencies only (s) | Full workspace (s) |
| -------------------- | --------------------: | -----------------: |
| Documentation builds |                  37.8 |               32.0 |
| Dogfooding           |                  32.7 |               28.1 |
| Doctests             |                  10.2 |               10.1 |
| Parser feature check |                   2.1 |                0.9 |

`step-samples.csv` retains these step durations and job links. Each source commit has equal weight.

| Read-only cache             | Dependencies only | Full workspace |
| --------------------------- | ----------------: | -------------: |
| Compressed archive (MiB)    |               920 |           2509 |
| Cache age (source commits)  |              1–28 |           1–28 |
| Fetch after cache match (s) |               1.4 |            2.7 |
| Unpack (s)                  |               2.5 |           24.6 |

## Cache-producing jobs

There are 7 paired refreshes after the initial seed, with one paired run per refresh. Ruff PRs are read-only; cache-producing jobs run separately on main. These means describe the cache-producing jobs themselves.

| Cost                       | Dependencies only (s) | Full workspace (s) |
| -------------------------- | --------------------: | -----------------: |
| Entire cache-producing job |                 254.3 |              279.7 |
| Cache post step            |                  12.7 |               32.1 |

Full-cache refresh logs report removing 0–245 superseded workspace fingerprints and 0–780 artifact paths per refresh. Active workspace artifacts are intentionally retained. These logs show that pruning runs; they do not establish that every retained artifact is necessary. `metadata.json` includes the counts and direct job links.

## Reproduce the summary

`samples.csv` includes source and workflow revisions, cache keys and ages, individual durations, and direct job links. `metadata.json` records the source selection, cache-state audit, and expected summary.

```sh
uv run --script summarize.py
uv run --script summarize.py --read-only
uv run --script summarize.py --fresh-initial-cache
```

The script selects the appropriate read or write samples, averages repetitions within each source commit, and calculates the paired effect and 90% intervals. It verifies the recomputed results against `metadata.json`.
