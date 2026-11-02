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

Measures how long it takes to type check a project without a pre-existing cache.
Hyperfine 2.0 also reports peak resident set size (RSS) for each measured command
in the `Memory` row. Both metrics use the same warmups and measured runs.

All five CLIs receive the same list of tracked Python files. The runner applies
each project's includes and excludes once and skips symlinks. When an
implementation has an adjacent stub, the implementation is the check target.
Standalone stubs stay in the list. If a file and a populated directory share a
module name, the directory wins. Files omitted as targets remain available for
import resolution. The runner prints the file count and SHA-256 of the list
before timing each project.

Each CLI checks unannotated function bodies. mypy uses its normal import
following with project and user configuration disabled, so per-module overrides
and plugins cannot change its workload. Checkers can still follow different
imports or infer different types from the same source files.

You can run the benchmark with `--single-threaded` to measure the check time when using a single thread only.

### Zuban

Zuban is installed from the runner's lockfile and participates in the default CLI
and LSP suites. To run only Zuban:

```shell
uv run --python 3.15 benchmark --tool zuban
uv run --python 3.15 pytest src/benchmark/test_lsp_diagnostics.py -k zuban
```

Zuban uses mypy-compatible mode with untyped-function checking enabled and untyped
returns treated as `Any`, matching the mypy and Pyrefly benchmark policy. Its
generated configuration uses the shared target Python version, source import
roots, and project virtual environment. CLI runs use the shared file manifest in
an isolated configuration file; LSP runs use the project's include/exclude lists in
`pyproject.toml` and restore the original file afterwards. Upstream mypy or Zuban
settings cannot silently narrow its workload or suppress diagnostics.

The language server checks the same open files and edits through the common LSP
client, with the project interpreter supplied explicitly. `--single-threaded`
also limits Zuban's Rayon pool through `RAYON_NUM_THREADS=1`. Checker implementations
and diagnostic coverage still differ; these settings align the benchmark inputs
and measurement boundaries.

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

With Zuban 0.10.2, allow roughly two minutes per NumPy CLI check with the shared
file manifest. Its language server returned a server-panic error for the
incremental edits in discord.py, PyTorch, and NumPy. These cases retain the
shared workload and assertions; a failed or interrupted run is not a benchmark
result.

## Updating the benchmark

To distinguish checker upgrades from changes to the benchmark projects, run the CLI
and both LSP benchmarks with the existing project revisions first. Then update the
revisions in `src/benchmark/projects.py` and rerun all three suites. Keep checker
versions, project Python versions, and the dependency cutoff in
`src/benchmark/venv.py` the same across both runs. A project's updated requirements
can still change its installed dependencies.

Record the CLI file counts and manifest hashes alongside the hardware, operating
system, tool versions, Python versions, and ty source revision with the results.
The benchmark harness uses Python 3.15. Each project's environment uses the
Python version specified in `projects.py`.

The benchmark script supports snapshotting the results when running with `--snapshot` and `--accept`.
The goal of those snapshots is to catch accidental regressions. For example, if a project adds
new dependencies that we fail to install. They are not intended as a testing tool. E.g. the snapshot runner doesn't account for platform differences so that
you might see differences when running the snapshots on your machine.

## Project coverage

The suite includes HTTPX's synchronous and asynchronous HTTP client, FastAPI's
web framework, NumPy's Python implementation and array stubs, SQLAlchemy's
typed ORM and SQL-expression APIs, and Rich's terminal rendering and formatting,
alongside the existing application and library benchmarks. NumPy excludes its
tests and does not build its C extensions; its
source requires a Python 3.12 project environment.

Each of these projects has an incremental edit that changes diagnostics in an
importing file as well as exercising the edited module. Keep their source
revisions identical across the existing- and updated-project runs when measuring
the refresh of the other projects.
