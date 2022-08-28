# ruff

[![Actions status](https://github.com/charliermarsh/ruff/workflows/CI/badge.svg)](https://github.com/charliermarsh/ruff/actions)
[![PyPI version](https://badge.fury.io/py/ruff.svg)](https://badge.fury.io/py/ruff)

An extremely fast Python linter, written in Rust.

Major features:

- 10-100x faster than your current linter.
- Installable via `pip`.
- Python 3.10 compatibility.
- [ESLint](https://eslint.org/docs/latest/user-guide/command-line-interface#caching)-inspired cache semantics.
- [TypeScript](https://www.typescriptlang.org/docs/handbook/configuring-watch.html)-inspired `--watch` semantics.
- `pyproject.toml` support.

ruff is a proof-of-concept and not yet intended for production use. It supports only a small
subset of the Flake8 rules, and may crash on your codebase.

## Installation and usage

### Installation

Available as [ruff](https://pypi.org/project/ruff/) on PyPI:

```shell
pip install ruff
```

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
maturin publish --username crmarsh --skip-existing --target x86_64-apple-darwin && \
  maturin publish --username crmarsh --skip-existing --target aarch64-apple-darwin
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

hyperfine --ignore-failure --warmup 5 \
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
hyperfine --ignore-failure --warmup 5 \
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
∴ hyperfine --ignore-failure --warmup 5 \
→   "pycodestyle resources/test/cpython" \
→   "pyflakes resources/test/cpython" \
→   "flake8 resources/test/cpython" \
→   "autoflake --recursive --expand-star-imports --remove-all-unused-imports --remove-unused-variables --remove-duplicate-keys resources/test/cpython" \
→   "pylint --recursive=y resources/test/cpython/" \
→   "pycodestyle --select E501 resources/test/cpython" \
→   "flake8 --select=F831,F541,F634,F403,F706,F901,E501 resources/test/cpython" \
→   "python -m run_flake8 resources/test/cpython --select=F831,F541,F634,F403,F706,F901,E501" \
→   "python -m run_flake8 resources/test/cpython"
Benchmark 1: pycodestyle resources/test/cpython
  Time (mean ± σ):     41.921 s ±  1.409 s    [User: 41.451 s, System: 0.194 s]
  Range (min … max):   41.182 s … 45.894 s    10 runs

  Warning: Ignoring non-zero exit code.
  Warning: Statistical outliers were detected. Consider re-running this benchmark on a quiet PC without any interferences from other programs. It might help to use the '--warmup' or '--prepare' options.

Benchmark 2: pyflakes resources/test/cpython
  Time (mean ± σ):     27.960 s ±  1.251 s    [User: 27.491 s, System: 0.236 s]
  Range (min … max):   26.449 s … 29.899 s    10 runs

  Warning: Ignoring non-zero exit code.

Benchmark 3: flake8 resources/test/cpython
  Time (mean ± σ):     75.320 s ±  0.909 s    [User: 74.625 s, System: 0.610 s]
  Range (min … max):   74.181 s … 77.336 s    10 runs

  Warning: Ignoring non-zero exit code.

Benchmark 4: autoflake --recursive --expand-star-imports --remove-all-unused-imports --remove-unused-variables --remove-duplicate-keys resources/test/cpython
  Time (mean ± σ):     32.690 s ±  0.585 s    [User: 32.300 s, System: 0.296 s]
  Range (min … max):   31.948 s … 33.326 s    10 runs

  Warning: Ignoring non-zero exit code.

Benchmark 5: pylint --recursive=y resources/test/cpython/
  Time (mean ± σ):     27.592 s ±  0.227 s    [User: 26.627 s, System: 0.911 s]
  Range (min … max):   27.325 s … 27.955 s    10 runs

  Warning: Ignoring non-zero exit code.

Benchmark 6: pycodestyle --select E501 resources/test/cpython
  Time (mean ± σ):     14.540 s ±  0.156 s    [User: 14.397 s, System: 0.121 s]
  Range (min … max):   14.384 s … 14.920 s    10 runs

  Warning: Ignoring non-zero exit code.

Benchmark 7: flake8 --select=F831,F541,F634,F403,F706,F901,E501 resources/test/cpython
  Time (mean ± σ):     75.391 s ±  0.600 s    [User: 74.642 s, System: 0.591 s]
  Range (min … max):   74.458 s … 76.550 s    10 runs

  Warning: Ignoring non-zero exit code.

Benchmark 8: python -m run_flake8 resources/test/cpython --select=F831,F541,F634,F403,F706,F901,E501
  Time (mean ± σ):     12.542 s ±  0.467 s    [User: 87.891 s, System: 0.816 s]
  Range (min … max):   11.771 s … 13.034 s    10 runs

Benchmark 9: python -m run_flake8 resources/test/cpython
  Time (mean ± σ):     12.276 s ±  0.398 s    [User: 86.720 s, System: 0.792 s]
  Range (min … max):   11.809 s … 12.865 s    10 runs

Summary
  'python -m run_flake8 resources/test/cpython' ran
    1.02 ± 0.05 times faster than 'python -m run_flake8 resources/test/cpython --select=F831,F541,F634,F403,F706,F901,E501'
    1.18 ± 0.04 times faster than 'pycodestyle --select E501 resources/test/cpython'
    2.25 ± 0.08 times faster than 'pylint --recursive=y resources/test/cpython/'
    2.28 ± 0.13 times faster than 'pyflakes resources/test/cpython'
    2.66 ± 0.10 times faster than 'autoflake --recursive --expand-star-imports --remove-all-unused-imports --remove-unused-variables --remove-duplicate-keys resources/test/cpython'
    3.41 ± 0.16 times faster than 'pycodestyle resources/test/cpython'
    6.14 ± 0.21 times faster than 'flake8 resources/test/cpython'
    6.14 ± 0.21 times faster than 'flake8 --select=F831,F541,F634,F403,F706,F901,E501 resources/test/cpython'
```

## License

MIT
