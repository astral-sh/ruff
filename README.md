# rust-python-linter

A performance-focused, [Pyflakes](https://github.com/PyCQA/pyflakes)-inspired Python linter, written
in Rust.

Features:

- Python 3.8 compatibility
- [ESLint](https://eslint.org/docs/latest/user-guide/command-line-interface#caching)-inspired
  cache semantics
- [TypeScript](https://www.typescriptlang.org/docs/handbook/configuring-watch.html)
  -inspired `--watch` semantics

## Installation

Available as [`rust-python-linter`](https://pypi.org/project/rust-python-linter/) on PyPI:

```shell
pip install rust-python-linter
```

## Usage

To run the linter, try any of the following:

```shell
rust_python_linter path/to/code/to/check.py
# ...or...
rust_python_linter path/to/code/
# ...or...
rust_python_linter path/to/code/*.py
```

You can also run in `--watch` mode to automatically re-run the linter on-change with, e.g.:

```shell
rust_python_linter path/to/code/ --watch
```

## Development

As the name suggests, `rust-python-linter` is implemented in Rust:

```shell
cargo fmt
cargo clippy
cargo run resources/test/src
```

## Deployment

`rust-python-linter` is released for Python using [`maturin`](https://github.com/PyO3/maturin):

```shell
maturin publish --skip-existing --target x86_64-apple-darwin
maturin publish --skip-existing --target aarch64-apple-darwin
```

## Benchmarking

First, clone [CPython](https://github.com/python/cpython). It's a large and diverse Python codebase,
which makes it a good target for benchmarking. Note that we clone v3.9, as `RustPython` doesn't yet
support pattern matching, which was introduced in v3.10.

```shell
git clone --branch 3.9 https://github.com/python/cpython.git resources/test/cpython
```

Next, to benchmark the release build:

```shell
cargo build --release

hyperfine --warmup 5 \
  "./target/release/rust_python_linter ./resources/test/cpython/ --no-cache" \
  "./target/release/rust_python_linter ./resources/test/cpython/"

Benchmark 1: ./target/release/rust_python_linter ./resources/test/cpython/ --no-cache
  Time (mean ± σ):     353.6 ms ±   7.6 ms    [User: 2868.8 ms, System: 171.5 ms]
  Range (min … max):   344.4 ms … 367.3 ms    10 runs

Benchmark 2: ./target/release/rust_python_linter ./resources/test/cpython/
  Time (mean ± σ):      59.6 ms ±   2.5 ms    [User: 36.4 ms, System: 345.6 ms]
  Range (min … max):    55.9 ms …  67.0 ms    48 runs
```
