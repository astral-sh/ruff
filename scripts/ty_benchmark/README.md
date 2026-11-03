## Getting started

1. [Install `uv`](https://docs.astral.sh/uv/getting-started/installation/)
    - Unix: `curl -LsSf https://astral.sh/uv/install.sh | sh`
    - Windows: `powershell -c "irm https://astral.sh/uv/install.ps1 | iex"`
1. Build ty: `cargo build --bin ty --release`
1. `cd` into the benchmark directory: `cd scripts/ty_benchmark`
1. Install npm 11.10.0 or newer, which supports the dependency cooldown in `.npmrc`
1. Install Pyright: `npm ci --ignore-scripts`
1. Run benchmarks: `uv run benchmark`

Requires hyperfine 2.0 or newer.

## Benchmarks

### Cold check time

Run with:

```shell
uv run --python 3.15 benchmark
```

Measures elapsed time and peak resident set size (RSS) when type checking a project
without a pre-existing cache.

All five CLIs receive the same list of tracked Python files. The runner applies
each project's includes and excludes once and skips symlinks. When an
implementation has an adjacent stub, the stub is the check target. Implementations
without stubs and standalone stubs stay in the list. This preserves the stub's
types when mypy resolves imports; explicitly targeting the implementation would
replace those types with the runtime module, unlike the other checkers. If a file
and a populated directory share a module name, the directory wins. The runner
prints the file count and SHA-256 of the list before timing each project.

Each CLI checks unannotated function bodies. mypy uses its normal import
following with project and user configuration disabled, so per-module overrides
and plugins cannot change its workload. Checkers can still follow different
imports or infer different types from the same source files.

You can run the benchmark with `--single-threaded` to measure the check time when using a single thread only.

### Warm check time

Run with:

```shell
uv run --python 3.15 benchmark --warm
```

Measures how long it takes to recheck a project if there were no changes.

> **Note**: Of the benchmarked type checkers, only mypy supports caching.

### LSP: Time to first diagnostic

Measures how long it takes for a newly started LSP to return the diagnostics for the files open in the editor.

Run with:

```bash
uv run --python 3.15 pytest src/benchmark/test_lsp_diagnostics.py::test_fetch_diagnostics
```

**Note**: Use `-v -s` to see the set of diagnostics returned by each type checker.

### LSP: Re-check time

Measure how long it takes to recheck all open files after making a single change in a file.

Run with:

```bash
uv run --python 3.15 pytest src/benchmark/test_lsp_diagnostics.py::test_incremental_edit
```

> **Note**: This benchmark uses [pull diagnostics](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#textDocument_pullDiagnostics) for type checkers that support this operation (ty, Zuban), and falls back to [publish diagnostics](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#textDocument_publishDiagnostics) otherwise (Pyright, Pyrefly).

## Known limitations

The tested type checkers implement Python's type system to varying degrees and
some projects only successfully pass type checking using a specific type checker.
We benchmark against the latest version of each type checker, but some projects
may use another version in practice, leading to different diagnostic results
between benchmarking and reality.

## Updating the benchmark

Run the configuration and shared-target regression tests with all five CLI tools
installed (including the compiled ty binary and npm's Pyright):

```shell
uv run --python 3.15 --group test pytest src/benchmark/test_tool.py src/benchmark/test_cli_targets.py
```

The `test` dependency group supplies mypy for the native import regression;
measured mypy commands still use each project's virtual environment.

Record the CLI file counts and manifest hashes with the results.

The benchmark script supports snapshotting the results when running with `--snapshot` and `--accept`.
The goal of those snapshots is to catch accidental regressions. For example, if a project adds
new dependencies that we fail to install. They are not intended as a testing tool. E.g. the snapshot runner doesn't account for platform differences so that
you might see differences when running the snapshots on your machine.
