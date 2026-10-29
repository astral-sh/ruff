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

> **Note**: This benchmark uses [pull diagnostics](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#textDocument_pullDiagnostics) for type checkers that support this operation (ty), and falls back to [publish diagnostics](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#textDocument_publishDiagnostics) otherwise (Pyright, Pyrefly).

## Known limitations

The tested type checkers implement Python's type system to varying degrees and
some projects only successfully pass type checking using a specific type checker.
We benchmark against the latest version of each type checker, but some projects
may use another version in practice, leading to different diagnostic results
between benchmarking and reality.

## Updating the benchmark

To distinguish checker upgrades from changes to the benchmark projects, run the CLI
and both LSP benchmarks with the existing project revisions first. Then update the
revisions in `src/benchmark/projects.py` and rerun all three suites. Keep checker
versions, project Python versions, and the dependency cutoff in
`src/benchmark/venv.py` the same across both runs. A project's updated requirements
can still change its installed dependencies.

Record the hardware, operating system, tool versions, Python versions, and ty source
revision with the results. The benchmark harness uses Python 3.15; each project's
environment uses the Python version specified in `projects.py`.

The benchmark script supports snapshotting the results when running with `--snapshot` and `--accept`.
The goal of those snapshots is to catch accidental regressions. For example, if a project adds
new dependencies that we fail to install. They are not intended as a testing tool. E.g. the snapshot runner doesn't account for platform differences so that
you might see differences when running the snapshots on your machine.

## Project coverage

The suite includes HTTPX's synchronous and asynchronous HTTP client, FastAPI's
web framework, NumPy's Python implementation and array stubs, and SQLAlchemy's
typed ORM and SQL-expression APIs, alongside the existing application and library
benchmarks. NumPy excludes its tests and does not build its C extensions; its
source requires a Python 3.12 project environment.

Each of these projects has an incremental edit that changes diagnostics in an
importing file as well as exercising the edited module. Keep their source
revisions identical across the existing- and updated-project runs when measuring
the refresh of the other projects.
