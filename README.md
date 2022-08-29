# ruff

[![Actions status](https://github.com/charliermarsh/ruff/workflows/CI/badge.svg)](https://github.com/charliermarsh/ruff/actions)
[![PyPI version](https://badge.fury.io/py/ruff.svg)](https://badge.fury.io/py/ruff)

An extremely fast Python linter, written in Rust.

<p align="center">
  <picture>
    <source srcset="https://user-images.githubusercontent.com/1309177/187221271-9db38ced-c622-406a-abf3-dec27ebc1b08.svg">
    <img alt="Bar chart with benchmark results" src="https://user-images.githubusercontent.com/1309177/187221271-9db38ced-c622-406a-abf3-dec27ebc1b08.svg">
  </picture>
</p>

<p align="center">
  <i>Linting the CPython codebase from scratch.</i>
</p>

Major features:

- 10-100x faster than your current linter (parallelized by-default).
- Installable via `pip`.
- Python 3.10 compatibility.
- [ESLint](https://eslint.org/docs/latest/user-guide/command-line-interface#caching)-inspired cache semantics.
- [TypeScript](https://www.typescriptlang.org/docs/handbook/configuring-watch.html)-inspired `--watch` semantics.
- `pyproject.toml` support.

_ruff is a proof-of-concept and not yet intended for production use. It supports only a small subset
of the Flake8 rules, and may crash on your codebase._

## Installation and usage

### Installation

Available as [ruff](https://pypi.org/project/ruff/) on PyPI:

```shell
pip install ruff
```

For now, wheels are only available for macOS (on Python 3.7, 3.8, 3.9, and 3.10). If you're using a
different operating system or Python version, you'll need to install the [Rust toolchain](https://www.rust-lang.org/tools/install)
prior to running `pip install ruff`. (This is an effort limitation on my part, not a technical
limitation.)

### Usage

To run ruff, try any of the following:

```shell
ruff path/to/code/to/check.py
ruff path/to/code/
ruff path/to/code/*.py
```

You can run ruff in `--watch` mode to automatically re-run on-change:

```shell
ruff path/to/code/ --watch
```

## Configuration

ruff is configurable both via `pyproject.toml` and the command line.

For example, you could configure ruff to only enforce a subset of rules with:

```toml
[tool.ruff]
line-length = 88
select = [
    "F401",
    "F403",
]
```

Alternatively, on the command-line:

```shell
ruff path/to/code/ --select F401 F403
```

See `ruff --help` for more:

```shell
ruff
A Python linter written in Rust

USAGE:
    ruff [OPTIONS] <FILES>...

ARGS:
    <FILES>...

OPTIONS:
    -e, --exit-zero             Exit with status code "0", even upon detecting errors
    -h, --help                  Print help information
        --ignore <IGNORE>...    Comma-separated list of error codes to ignore
    -n, --no-cache              Disable cache reads
    -q, --quiet                 Disable all logging (but still exit with status code "1" upon
                                detecting errors)
        --select <SELECT>...    Comma-separated list of error codes to enable
    -v, --verbose               Enable verbose logging
    -w, --watch                 Run in watch mode by re-running whenever files change
```

## Development

ruff is written in Rust (1.63.0). You'll need to install the [Rust toolchain](https://www.rust-lang.org/tools/install)
for development.

Assuming you have `cargo` installed, you can run:

```shell
cargo run resources/test/src
cargo fmt
cargo clippy
cargo test
```

## Deployment

ruff is distributed on [PyPI](https://pypi.org/project/ruff/), and published via [`maturin`](https://github.com/PyO3/maturin).

For now, releases are cut and published manually:

```shell
for TARGET in x86_64-apple-darwin aarch64-apple-darwin
do
  maturin publish --username crmarsh --skip-existing --target ${TARGET} -i \
    /usr/local/opt/python@3.7/libexec/bin/python \
    /usr/local/opt/python@3.8/libexec/bin/python \
    /usr/local/opt/python@3.9/libexec/bin/python \
    /usr/local/opt/python@3.10/libexec/bin/python
done
```

## Benchmarking

First, clone [CPython](https://github.com/python/cpython). It's a large and diverse Python codebase,
which makes it a good target for benchmarking.

```shell
git clone --branch 3.10 https://github.com/python/cpython.git resources/test/cpython
```

Add this `pyproject.toml` to the CPython directory:

```toml
[tool.linter]
line-length = 88
exclude = [
    "Lib/ctypes/test/test_numbers.py",
    "Lib/dataclasses.py",
    "Lib/lib2to3/tests/data/bom.py",
    "Lib/lib2to3/tests/data/crlf.py",
    "Lib/lib2to3/tests/data/different_encoding.py",
    "Lib/lib2to3/tests/data/false_encoding.py",
    "Lib/lib2to3/tests/data/py2_test_grammar.py",
    "Lib/sqlite3/test/factory.py",
    "Lib/sqlite3/test/hooks.py",
    "Lib/sqlite3/test/regression.py",
    "Lib/sqlite3/test/transactions.py",
    "Lib/sqlite3/test/types.py",
    "Lib/test/bad_coding2.py",
    "Lib/test/badsyntax_3131.py",
    "Lib/test/badsyntax_pep3120.py",
    "Lib/test/encoded_modules/module_iso_8859_1.py",
    "Lib/test/encoded_modules/module_koi8_r.py",
    "Lib/test/sortperf.py",
    "Lib/test/test_email/torture_test.py",
    "Lib/test/test_fstring.py",
    "Lib/test/test_genericpath.py",
    "Lib/test/test_getopt.py",
    "Lib/test/test_grammar.py",
    "Lib/test/test_htmlparser.py",
    "Lib/test/test_importlib/stubs.py",
    "Lib/test/test_importlib/test_files.py",
    "Lib/test/test_importlib/test_metadata_api.py",
    "Lib/test/test_importlib/test_open.py",
    "Lib/test/test_importlib/test_util.py",
    "Lib/test/test_named_expressions.py",
    "Lib/test/test_patma.py",
    "Lib/test/test_peg_generator/__main__.py",
    "Lib/test/test_pipes.py",
    "Lib/test/test_source_encoding.py",
    "Lib/test/test_weakref.py",
    "Lib/test/test_webbrowser.py",
    "Lib/tkinter/__main__.py",
    "Lib/tkinter/test/test_tkinter/test_variables.py",
    "Modules/_decimal/libmpdec/literature/fnt.py",
    "Modules/_decimal/tests/deccheck.py",
    "Tools/c-analyzer/c_parser/parser/_delim.py",
    "Tools/i18n/pygettext.py",
    "Tools/test2to3/maintest.py",
    "Tools/test2to3/setup.py",
    "Tools/test2to3/test/test_foo.py",
    "Tools/test2to3/test2to3/hello.py",
]
```

Next, to benchmark the release build:

```shell
cargo build --release

hyperfine --ignore-failure --warmup 1 \
  "./target/release/ruff ./resources/test/cpython/ --no-cache" \
  "./target/release/ruff ./resources/test/cpython/"

Benchmark 1: ./target/release/ruff ./resources/test/cpython/ --no-cache
  Time (mean ± σ):     353.6 ms ±   7.6 ms    [User: 2868.8 ms, System: 171.5 ms]
  Range (min … max):   344.4 ms … 367.3 ms    10 runs

Benchmark 2: ./target/release/ruff ./resources/test/cpython/
  Time (mean ± σ):      59.6 ms ±   2.5 ms    [User: 36.4 ms, System: 345.6 ms]
  Range (min … max):    55.9 ms …  67.0 ms    48 runs
```

To benchmark the ecosystem's existing tools:

```shell
hyperfine --ignore-failure --warmup 1 \
  "pylint --recursive=y resources/test/cpython/" \
  "pyflakes resources/test/cpython" \
  "autoflake --recursive --expand-star-imports --remove-all-unused-imports --remove-unused-variables --remove-duplicate-keys resources/test/cpython" \
  "pycodestyle resources/test/cpython" \
  "pycodestyle --select E501 resources/test/cpython" \
  "flake8 resources/test/cpython" \
  "flake8 --select=F831,F541,F634,F403,F706,F901,E501 resources/test/cpython" \
  "python -m scripts.run_flake8 resources/test/cpython" \
  "python -m scripts.run_flake8 resources/test/cpython --select=F831,F541,F634,F403,F706,F901,E501"
```

In order, these evaluate:
- Pylint
- PyFlakes
- autoflake
- pycodestyle
- pycodestyle, limited to the checks supported by ruff
- Flake8
- Flake8, limited to the checks supported by ruff
- Flake8, with a hack to enable multiprocessing on macOS
- Flake8, with a hack to enable multiprocessing on macOS, limited to the checks supported by ruff

(You can `poetry install` from `./scripts` to create a working environment for the above.)

```shell
Benchmark 1: ./target/release/ruff ./resources/test/cpython/ --no-cache
  Time (mean ± σ):     566.9 ms ±  36.6 ms    [User: 2618.0 ms, System: 992.0 ms]
  Range (min … max):   504.8 ms … 634.0 ms    10 runs

  Warning: Ignoring non-zero exit code.

Benchmark 2: ./target/release/ruff ./resources/test/cpython/
  Time (mean ± σ):      79.5 ms ±   2.3 ms    [User: 330.1 ms, System: 254.3 ms]
  Range (min … max):    75.6 ms …  85.2 ms    35 runs

  Warning: Ignoring non-zero exit code.

Benchmark 3: pylint --recursive=y resources/test/cpython/
  Time (mean ± σ):     27.532 s ±  0.207 s    [User: 26.606 s, System: 0.899 s]
  Range (min … max):   27.344 s … 28.064 s    10 runs

  Warning: Ignoring non-zero exit code.

Benchmark 4: pyflakes resources/test/cpython
  Time (mean ± σ):     28.074 s ±  0.551 s    [User: 27.845 s, System: 0.212 s]
  Range (min … max):   27.479 s … 29.467 s    10 runs

  Warning: Ignoring non-zero exit code.

Benchmark 5: autoflake --recursive --expand-star-imports --remove-all-unused-imports --remove-unused-variables --remove-duplicate-keys resources/test/cpython
  Time (mean ± σ):      4.986 s ±  0.190 s    [User: 43.257 s, System: 0.801 s]
  Range (min … max):    4.837 s …  5.462 s    10 runs

  Warning: Ignoring non-zero exit code.

Benchmark 6: pycodestyle resources/test/cpython
  Time (mean ± σ):     42.400 s ±  0.211 s    [User: 42.177 s, System: 0.213 s]
  Range (min … max):   42.106 s … 42.677 s    10 runs

  Warning: Ignoring non-zero exit code.

Benchmark 7: pycodestyle --select E501 resources/test/cpython
  Time (mean ± σ):     14.578 s ±  0.068 s    [User: 14.466 s, System: 0.108 s]
  Range (min … max):   14.475 s … 14.726 s    10 runs

  Warning: Ignoring non-zero exit code.

Benchmark 8: flake8 resources/test/cpython
  Time (mean ± σ):     76.414 s ±  0.461 s    [User: 75.611 s, System: 0.652 s]
  Range (min … max):   75.691 s … 77.180 s    10 runs

  Warning: Ignoring non-zero exit code.

Benchmark 9: flake8 --select=F831,F541,F634,F403,F706,F901,E501 resources/test/cpython
  Time (mean ± σ):     75.960 s ±  0.610 s    [User: 75.255 s, System: 0.634 s]
  Range (min … max):   75.159 s … 77.066 s    10 runs

  Warning: Ignoring non-zero exit code.

Benchmark 10: python -m scripts.run_flake8 resources/test/cpython
  Time (mean ± σ):     13.536 s ±  0.584 s    [User: 90.911 s, System: 0.934 s]
  Range (min … max):   12.831 s … 14.699 s    10 runs

Benchmark 11: python -m scripts.run_flake8 resources/test/cpython --select=F831,F541,F634,F403,F706,F901,E501
  Time (mean ± σ):     12.781 s ±  0.192 s    [User: 89.525 s, System: 0.882 s]
  Range (min … max):   12.568 s … 13.119 s    10 runs

Summary
  './target/release/ruff ./resources/test/cpython/' ran
    7.13 ± 0.50 times faster than './target/release/ruff ./resources/test/cpython/ --no-cache'
   62.69 ± 3.01 times faster than 'autoflake --recursive --expand-star-imports --remove-all-unused-imports --remove-unused-variables --remove-duplicate-keys resources/test/cpython'
  160.71 ± 5.26 times faster than 'python -m scripts.run_flake8 resources/test/cpython --select=F831,F541,F634,F403,F706,F901,E501'
  170.21 ± 8.86 times faster than 'python -m scripts.run_flake8 resources/test/cpython'
  183.30 ± 5.40 times faster than 'pycodestyle --select E501 resources/test/cpython'
  346.19 ± 10.40 times faster than 'pylint --recursive=y resources/test/cpython/'
  353.00 ± 12.39 times faster than 'pyflakes resources/test/cpython'
  533.14 ± 15.74 times faster than 'pycodestyle resources/test/cpython'
  955.13 ± 28.83 times faster than 'flake8 --select=F831,F541,F634,F403,F706,F901,E501 resources/test/cpython'
  960.82 ± 28.55 times faster than 'flake8 resources/test/cpython'
```

## License

MIT
