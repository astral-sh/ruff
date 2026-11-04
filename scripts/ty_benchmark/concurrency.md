# CLI concurrency calibration

Each checker uses one fixed concurrency setting across every project. The profile
was calibrated on a 16-core Apple M3 Max (12 performance and 4 efficiency cores),
128 GB RAM, and macOS 26.5.2, using ty 0.0.96, Pyrefly 1.4.0, mypy 2.4.0
(compiled), Pyright 1.1.414, and Zuban 0.10.2. Different hardware or releases can
favor other counts. LSP settings retain their separate policy.

We screened 1, 2, 4, 8, and 16 on isort, pandas, and PyTorch, also recording native
`0` defaults for mypy and Pyrefly. Leading candidates were then measured on four
projects reserved before the sweep: Black, Home Assistant, pandas-stubs, and
SymPy. Every case used one warmup and three measured runs, a deterministic
randomized count order, and serial checker processes. Source, root, and
configuration fingerprints were verified before and after each phase. Candidates
reused the same project environments and installed dependencies. mypy's cache was
cleared before every warmup and sample.

Scores are equal-weight geometric means of candidate runtime divided by fresh
16-worker runtime. Values below 1 are faster. Counts within 2% are practical
ties; significant project regressions also inform the choice.

| Checker | CLI setting             | Initial three projects | Reserved four projects |
| ------- | ----------------------- | ---------------------: | ---------------------: |
| ty      | `TY_MAX_PARALLELISM=16` |                 1.000× |                 1.000× |
| Pyrefly | `--threads 16`          |                 1.000× |                 1.000× |
| mypy    | `--num-workers 8`       |                 0.971× |                 1.000× |
| Pyright | `--threads 8`           |                 0.815× |                 0.836× |
| Zuban   | `RAYON_NUM_THREADS=4`   |                 0.899× |                 0.903× |

mypy's 8-versus-16 comparison was extended to all 18 projects after the reserved
projects exposed a tradeoff. The ten additional smaller projects used the same
three-sample protocol. NumPy used one warmup and one screening sample per count;
each invocation takes about 81 seconds. Across all 18 projects, 8 workers took
0.965× the time at 16 (3.5% less time).

Using 8 threads increased runtime by 23.3% for ty and 17.8% for Pyrefly across the seven calibration projects, so both retain 16.

A single setting has per-project tradeoffs. On Home Assistant, the selected
settings took mypy 15.37 s at 8 versus 13.71 s at 16; Pyright 33.59 s at 8 versus 29.78 s at 16. These
regressions remain visible in the benchmark report.

mypy and Pyright use worker processes. mypy's native parser also has a separate
pool capped at eight threads, including when `--num-workers 0` selects no external
checking workers. See the [pinned parser implementation](https://github.com/python/mypy/blob/v2.4.0/mypy/build.py#L1104-L1111).
The CLI's `--single-threaded` mode therefore does not force strictly single-threaded
mypy parsing. Hyperfine's macOS peak RSS does not sum simultaneous workers.

After fixing the choices, collect fresh publication measurements with three
warmups and at least ten samples per successful case, the usual root preflight,
and a cold mypy cache. Keep the exploratory timings above separate from the
publication samples.

Follow the [runner setup](README.md), then run the profile from this directory:

```shell
uv run --python 3.15 benchmark --ty-path /path/to/released/ty
```

To calibrate another host, change the selected checker's entry in
[`CLI_WORKERS`](src/benchmark/concurrency.py) for each candidate, retaining one
value across all projects. For example:

```shell
uv run --python 3.15 benchmark --tool mypy --project isort --project pandas --project pytorch
```

Compare equal-weight project ratios with fresh controls, validate on additional
projects, and rerun the published comparison after fixing the final values.
