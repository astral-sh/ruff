# ruff

[![Actions status](https://github.com/charliermarsh/ruff/workflows/CI/badge.svg)](https://github.com/charliermarsh/ruff/actions)
[![PyPI version](https://badge.fury.io/py/ruff.svg)](https://badge.fury.io/py/ruff)

An extremely fast Python linter, written in Rust.

<p align="center">
  <img alt="Bar chart with benchmark results" src="https://user-images.githubusercontent.com/1309177/187504482-6d9df992-a81d-4e86-9f6a-d958741c8182.svg">
</p>

<p align="center">
  <i>Linting the CPython codebase from scratch.</i>
</p>

- ⚡️ 10-100x faster than existing linters
- 🐍 Installable via `pip`
- 🤝 Python 3.10 compatibility
- 🛠️ `pyproject.toml` support
- 📦 [ESLint](https://eslint.org/docs/latest/user-guide/command-line-interface#caching)-inspired cache semantics
- 👀 [TypeScript](https://www.typescriptlang.org/docs/handbook/configuring-watch.html)-inspired `--watch` semantics

_ruff is a proof-of-concept and not yet intended for production use. It supports only a small subset
of the Flake8 rules, and may crash on your codebase._

Read the [launch blog post](https://notes.crmarsh.com/python-tooling-could-be-much-much-faster).

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
An extremely fast Python linter.

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

See: `.github/workflows/release.yaml`.

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

To benchmark against the ecosystem's existing tools:

```shell
hyperfine --ignore-failure --warmup 5 \
  "./target/release/ruff ./resources/test/cpython/ --no-cache" \
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
- ruff
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
  Time (mean ± σ):     469.3 ms ±  16.3 ms    [User: 2663.0 ms, System: 972.5 ms]
  Range (min … max):   445.2 ms … 494.8 ms    10 runs

Benchmark 2: pylint --recursive=y resources/test/cpython/
  Time (mean ± σ):     27.211 s ±  0.097 s    [User: 26.405 s, System: 0.799 s]
  Range (min … max):   27.056 s … 27.349 s    10 runs

Benchmark 3: pyflakes resources/test/cpython
  Time (mean ± σ):     27.309 s ±  0.033 s    [User: 27.137 s, System: 0.169 s]
  Range (min … max):   27.267 s … 27.372 s    10 runs

Benchmark 4: autoflake --recursive --expand-star-imports --remove-all-unused-imports --remove-unused-variables --remove-duplicate-keys resources/test/cpython
  Time (mean ± σ):      8.027 s ±  0.024 s    [User: 74.255 s, System: 0.953 s]
  Range (min … max):    7.969 s …  8.052 s    10 runs

Benchmark 5: pycodestyle resources/test/cpython
  Time (mean ± σ):     41.666 s ±  0.266 s    [User: 41.531 s, System: 0.132 s]
  Range (min … max):   41.295 s … 41.980 s    10 runs

Benchmark 6: pycodestyle --select E501 resources/test/cpython
  Time (mean ± σ):     14.547 s ±  0.077 s    [User: 14.466 s, System: 0.079 s]
  Range (min … max):   14.429 s … 14.695 s    10 runs

Benchmark 7: flake8 resources/test/cpython
  Time (mean ± σ):     75.700 s ±  0.152 s    [User: 75.254 s, System: 0.440 s]
  Range (min … max):   75.513 s … 76.014 s    10 runs

Benchmark 8: flake8 --select=F831,F541,F634,F403,F706,F901,E501 resources/test/cpython
  Time (mean ± σ):     75.122 s ±  0.532 s    [User: 74.677 s, System: 0.440 s]
  Range (min … max):   74.130 s … 75.606 s    10 runs

Benchmark 9: python -m scripts.run_flake8 resources/test/cpython
  Time (mean ± σ):     12.794 s ±  0.147 s    [User: 90.792 s, System: 0.738 s]
  Range (min … max):   12.606 s … 13.030 s    10 runs

Benchmark 10: python -m scripts.run_flake8 resources/test/cpython --select=F831,F541,F634,F403,F706,F901,E501
  Time (mean ± σ):     12.487 s ±  0.118 s    [User: 90.052 s, System: 0.714 s]
  Range (min … max):   12.265 s … 12.665 s    10 runs

Summary
  './target/release/ruff ./resources/test/cpython/ --no-cache' ran
   17.10 ± 0.60 times faster than 'autoflake --recursive --expand-star-imports --remove-all-unused-imports --remove-unused-variables --remove-duplicate-keys resources/test/cpython'
   26.60 ± 0.96 times faster than 'python -m scripts.run_flake8 resources/test/cpython --select=F831,F541,F634,F403,F706,F901,E501'
   27.26 ± 1.00 times faster than 'python -m scripts.run_flake8 resources/test/cpython'
   30.99 ± 1.09 times faster than 'pycodestyle --select E501 resources/test/cpython'
   57.98 ± 2.03 times faster than 'pylint --recursive=y resources/test/cpython/'
   58.19 ± 2.02 times faster than 'pyflakes resources/test/cpython'
   88.77 ± 3.14 times faster than 'pycodestyle resources/test/cpython'
  160.06 ± 5.68 times faster than 'flake8 --select=F831,F541,F634,F403,F706,F901,E501 resources/test/cpython'
  161.29 ± 5.61 times faster than 'flake8 resources/test/cpython'
```

## License

MIT
