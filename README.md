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
- 📦 [ESLint](https://eslint.org/docs/latest/user-guide/command-line-interface#caching)-inspired cache support
- 🔧 [ESLint](https://eslint.org/docs/latest/user-guide/command-line-interface#caching)-inspired `--fix` support
- 👀 [TypeScript](https://www.typescriptlang.org/docs/handbook/configuring-watch.html)-inspired `--watch` support

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

ruff also works with [Pre-Commit](https://pre-commit.com) (requires Cargo on system):

```yaml
repos:
- repo: https://github.com/charliermarsh/ruff
  rev: v0.0.34
  hooks:
    - id: lint
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
ruff (v0.0.34)
An extremely fast Python linter.

USAGE:
    ruff [OPTIONS] <FILES>...

ARGS:
    <FILES>...

OPTIONS:
    -e, --exit-zero               Exit with status code "0", even upon detecting errors
        --exclude <EXCLUDE>...    List of file and/or directory patterns to exclude from checks
    -f, --fix                     Attempt to automatically fix lint errors
    -h, --help                    Print help information
        --ignore <IGNORE>...      List of error codes to ignore
    -n, --no-cache                Disable cache reads
    -q, --quiet                   Disable all logging (but still exit with status code "1" upon
                                  detecting errors)
        --select <SELECT>...      List of error codes to enable
    -v, --verbose                 Enable verbose logging
    -w, --watch                   Run in watch mode by re-running whenever files change
```

### Compatibility with Black

ruff is intended to be compatible with [Black](https://github.com/psf/black), and should be
compatible out-of-the-box as long as the `line-length` setting is consistent between the two.

As a project, ruff is designed to be used alongside Black and, as such, will defer implementing
lint rules that are obviated by Black (e.g., stylistic rules).

### Parity with Flake8

ruff's goal is to achieve feature-parity with Flake8 when used (1) without any plugins,
(2) alongside Black, and (3) on Python 3 code. (Using Black obviates the need for many of Flake8's
stylistic checks; limiting to Python 3 obviates the need for certain compatibility checks.)

Under those conditions, Flake8 implements about 58 rules, give or take. At time of writing, ruff
implements 31 rules. (Note that these 31 rules likely cover a disproportionate share of errors:
unused imports, undefined variables, etc.)

Of the unimplemented rules, ruff is missing:

- 15 rules related to string `.format` calls.
- 6 rules related to misplaced `yield`, `break`, and `return` statements.
- 3 rules related to syntax errors in doctests and annotations.
- 2 rules related to redefining unused names.

...along with a variety of others that don't fit neatly into any specific category.

Beyond rule-set parity, ruff suffers from the following limitations vis-à-vis Flake8:

1. Flake8 supports a wider range of `noqa` patterns, such as per-file ignores defined in `.flake8`.
2. Flake8 has a plugin architecture and supports writing custom lint rules.
3. ruff does not yet support parenthesized context managers.

## Rules

| Code | Name | Message |
| ---- | ----- | ------- |
| E402 | ModuleImportNotAtTopOfFile | Module level import not at top of file |
| E501 | LineTooLong | Line too long |
| E711 | NoneComparison | Comparison to `None` should be `cond is None` |
| E712 | TrueFalseComparison | Comparison to `True` should be `cond is True` |
| E713 | NotInTest | Test for membership should be `not in` |
| E714 | NotIsTest | Test for object identity should be `is not` |
| E731 | DoNotAssignLambda | Do not assign a lambda expression, use a def |
| E741 | AmbiguousVariableName | ambiguous variable name '...' |
| E902 | IOError | No such file or directory: `...` |
| F401 | UnusedImport | `...` imported but unused |
| F403 | ImportStarUsage | Unable to detect undefined names |
| F404 | LateFutureImport | from __future__ imports must occur at the beginning of the file |
| F407 | FutureFeatureNotDefined | future feature '...' is not defined |
| F541 | FStringMissingPlaceholders | f-string without any placeholders |
| F601 | MultiValueRepeatedKeyLiteral | Dictionary key literal repeated |
| F602 | MultiValueRepeatedKeyVariable | Dictionary key `...` repeated |
| F621 | TooManyExpressionsInStarredAssignment | too many expressions in star-unpacking assignment |
| F622 | TwoStarredExpressions | two starred expressions in assignment |
| F631 | AssertTuple | Assert test is a non-empty tuple, which is always `True` |
| F634 | IfTuple | If test is a tuple, which is always `True` |
| F704 | YieldOutsideFunction | a `yield` or `yield from` statement outside of a function/method |
| F706 | ReturnOutsideFunction | a `return` statement outside of a function/method |
| F707 | DefaultExceptNotLast | an `except:` block as not the last exception handler |
| F821 | UndefinedName | Undefined name `...` |
| F822 | UndefinedExport | Undefined name `...` in `__all__` |
| F823 | UndefinedLocal | Local variable `...` referenced before assignment |
| F831 | DuplicateArgumentName | Duplicate argument name in function definition |
| F841 | UnusedVariable | Local variable `...` is assigned to but never used |
| F901 | RaiseNotImplemented | `raise NotImplemented` should be `raise NotImplementedError` |
| R001 | UselessObjectInheritance | Class `...` inherits from object |
| R002 | NoAssertEquals | `assertEquals` is deprecated, use `assertEqual` instead |

## Development

ruff is written in Rust (1.63.0). You'll need to install the [Rust toolchain](https://www.rust-lang.org/tools/install)
for development.

Assuming you have `cargo` installed, you can run:

```shell
cargo run resources/test/fixtures
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
[tool.ruff]
line-length = 88
exclude = [
    "Lib/lib2to3/tests/data/bom.py",
    "Lib/lib2to3/tests/data/crlf.py",
    "Lib/lib2to3/tests/data/different_encoding.py",
    "Lib/lib2to3/tests/data/false_encoding.py",
    "Lib/lib2to3/tests/data/py2_test_grammar.py",
    "Lib/test/bad_coding2.py",
    "Lib/test/badsyntax_3131.py",
    "Lib/test/badsyntax_pep3120.py",
    "Lib/test/encoded_modules/module_iso_8859_1.py",
    "Lib/test/encoded_modules/module_koi8_r.py",
    "Lib/test/test_fstring.py",
    "Lib/test/test_grammar.py",
    "Lib/test/test_importlib/test_util.py",
    "Lib/test/test_named_expressions.py",
    "Lib/test/test_patma.py",
    "Lib/test/test_source_encoding.py",
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
