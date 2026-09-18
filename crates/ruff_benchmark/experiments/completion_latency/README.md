# Completion latency experiment

This directory preserves the experiment used to investigate
[ty#3909](https://github.com/astral-sh/ty/issues/3909), following the
[proposed measurement sequence](https://github.com/astral-sh/ty/issues/3909#issuecomment-5630077724).
The instrumented entry point is `latency_experiment` in
[`benches/ty_ide.rs`](../../benches/ty_ide.rs).

## Compared revisions

All variants use baseline `ddaa74c0613971fd4cfe2a7246957da3735e4251`.
The synchronous release implementation is `21cb049e0cd4a93653a72ed076e4e2d64e2f22ca`;
the progressive implementation is `d0242bc010b0c4754c9e2c982498fee87a0c3a9e`.
Apply the benchmark change independently to each implementation to reproduce the
comparison. On the progressive implementation, also apply
[`progressive-barrier.patch`](progressive-barrier.patch). It exposes a queue-drain
helper for the benchmark and makes the reclamation barrier wait for previously
queued AST destruction. Applying it restores the `ruff_db` source used for the
reported measurements. The Rust harnesses differ only in this barrier call; the
baseline and synchronous implementation have no queue to drain.

## Build and run

Build the benchmark with its optional IDE feature:

```sh
cargo build -p ruff_benchmark --bench ty_ide --no-default-features --features ty_ide --message-format=json
```

The compiler-artifact JSON identifies the executable. Run it directly with:

```sh
/path/to/ty_ide-benchmark --latency-experiment /tmp/completion-large 8 64 regular
/path/to/ty_ide-benchmark --latency-experiment /tmp/completion-small 3 0 regular
```

The positional arguments are output prefix, sample count, payload functions per
leaf module, and package layout. CSV timings go to stdout. Parsed-module memory
reports use the output prefix with `-before.json` and `-after.json` suffixes.
The namespace fixture is unsupported on this baseline; use `regular`.

For the complete macOS comparison, copy both Python scripts into a scratch
directory outside the checkout. Save the corresponding executables there as
`main-baseline-final`, `main-original-final`, and `main-progressive-final`, then run:

```sh
uv run /tmp/completion-comparison/quiet-main.py compare main-comparison 8 64
uv run /tmp/completion-comparison/quiet-main.py compare main-small 3 0
uv run /tmp/completion-comparison/summarize-main.py main-comparison
uv run /tmp/completion-comparison/summarize-main.py main-small
```

The runner uses all six variant orders, yielding 48 large-module and 18
small-module samples per variant. It uses macOS `/usr/bin/time -l` for process CPU
and peak RSS. It waits for ten seconds without detected competing benchmarks,
builds, or tests and discards an entire timed process if a competitor appears.
The detector polls once per second and cannot exclude shorter interference.
Only accepted complete processes receive `.done` markers. Result files are
written beside the scripts; an existing `.done` marker skips that process on
reruns, so use a fresh directory or tag for a new measurement.

Use tags without periods, such as the two commands above. The preserved runner
uses file-suffix replacement when naming outputs; a period in a tag can make
different variants and blocks overwrite the same files.

Run only one copy of this comparison at a time. Competitor detection uses
executable-name heuristics and does not recognize the three renamed binaries
listed above. It therefore does not prevent two copies of this comparison from
interfering. The reported runs used the dot-free tags above and a single runner;
these scripts preserve the measurement logic rather than providing a general
benchmark scheduler.

## Measurement boundaries

Each sample creates a fresh database for 1,000 leaf modules. The large fixture
adds 64 payload functions to each module; the small fixture adds none. A separate
preflight validates the full module set, origins, completion names, and import
edits after creating another module. The client file is open; provider files are
closed. File generation and database teardown are outside the request timers.

- `warm_ms` measures the initial completion, including disposal of its results.
    The process and filesystem caches are warm; preflight has already started the
    progressive worker, so this excludes worker and server startup.
- `update_ms` measures database synchronization after a new module is written.
    The file write is outside this timer.
- `completion_ms` measures the immediately following completion and disposal of
    its results. `total_ms` covers the update and that completion together.
- `reclaimed_ms` additionally waits for previously queued AST destruction. A
    final barrier also drains destruction after the last database teardown.

Cached parsed-module memory includes fixed field storage but excludes ASTs
already transferred to the worker. Peak RSS measures the whole process. Process
CPU includes preflight, fixture creation, all samples, memory reporting, and
teardown; it is not CPU per completion. Compare CPU only within a fixture size.
The reported run used optimized debug builds on macOS arm64 with
`rustc 1.98.1-dev (a238a09a3 2026-09-10) (ohm-1.98.1-2)` and one Rayon thread.

The analysis pools samples for medians and nearest-rank p95, takes the median
process CPU and RSS across six processes, and uses 10,000 paired resamples of
order blocks with seed 3909 for initial-latency intervals. With 18 small-module
samples, p95 is the maximum observed value. The intervals describe these runs,
not a universal performance bound.
