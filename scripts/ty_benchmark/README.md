## Getting started

1. [Install `uv`](https://docs.astral.sh/uv/getting-started/installation/)

- Unix: `curl -LsSf https://astral.sh/uv/install.sh | sh`
- Windows: `powershell -c "irm https://astral.sh/uv/install.ps1 | iex"`

1. Build ty: `cargo build --bin ty --release`
1. `cd` into the benchmark directory: `cd scripts/ty_benchmark`
1. Install Pyright: `npm install`
1. Run benchmarks: `uv run benchmark`

Requires hyperfine 1.20 or newer.

## LSP benchmarks

```bash
uv run pytest src/benchmark/test_lsp_diagnostics.py::test_lsp_initial_diagnostics \
    --benchmark-group-by=param:project_setup
```

## Known limitations

The tested type checkers implement Python's type system to varying degrees and
some projects only successfully pass type checking using a specific type checker.

## Updating the benchmark

The benchmark script supports snapshoting the results when running with `--snapshot` and `--accept`.
The goal of those snapshots is to catch accidental regressions. For example, if a project adds
new dependencies that we fail to install. They are not intended as a testing tool. E.g. the snapshot runner doesn't account for platform differences so that
you might see differences when running the snapshots on your machine.
