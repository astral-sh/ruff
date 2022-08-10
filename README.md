# rust-python-linter

A performance-focused, [Pyflakes](https://github.com/PyCQA/pyflakes)-inspired Python linter, written
in Rust.

Features:

- Python 3.10 compatibility
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
maturin publish
```
