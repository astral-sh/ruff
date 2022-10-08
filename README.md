# ruff

[![image](https://img.shields.io/pypi/v/ruff.svg)](https://pypi.python.org/pypi/ruff)
[![image](https://img.shields.io/pypi/l/ruff.svg)](https://pypi.python.org/pypi/ruff)
[![image](https://img.shields.io/pypi/pyversions/ruff.svg)](https://pypi.python.org/pypi/ruff)
[![Actions status](https://github.com/charliermarsh/ruff/workflows/CI/badge.svg)](https://github.com/charliermarsh/ruff/actions)

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
- 🔧 [ESLint](https://eslint.org/docs/latest/user-guide/command-line-interface#--fix)-inspired `--fix` support
- 👀 [TypeScript](https://www.typescriptlang.org/docs/handbook/configuring-watch.html)-inspired `--watch` support
- ⚖️ [Near-complete parity](#Parity-with-Flake8) with the built-in Flake8 rule set

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

ruff also works with [pre-commit](https://pre-commit.com):

```yaml
repos:
  - repo: https://github.com/charliermarsh/ruff-pre-commit
    rev: v0.0.48
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
ruff path/to/code/ --select F401 --select F403
```

See `ruff --help` for more:

```shell
ruff: An extremely fast Python linter.

Usage: ruff [OPTIONS] <FILES>...

Arguments:
  <FILES>...

Options:
  -v, --verbose
          Enable verbose logging
  -q, --quiet
          Disable all logging (but still exit with status code "1" upon detecting errors)
  -e, --exit-zero
          Exit with status code "0", even upon detecting errors
  -w, --watch
          Run in watch mode by re-running whenever files change
  -f, --fix
          Attempt to automatically fix lint errors
  -n, --no-cache
          Disable cache reads
      --select <SELECT>
          List of error codes to enable
      --extend-select <EXTEND_SELECT>
          Like --select, but adds additional error codes on top of the selected ones
      --ignore <IGNORE>
          List of error codes to ignore
      --extend-ignore <EXTEND_IGNORE>
          Like --ignore, but adds additional error codes on top of the ignored ones
      --exclude <EXCLUDE>
          List of paths, used to exclude files and/or directories from checks
      --extend-exclude <EXTEND_EXCLUDE>
          Like --exclude, but adds additional files and directories on top of the excluded ones
      --per-file-ignores <PER_FILE_IGNORES>
          List of mappings from file pattern to code to exclude
      --format <FORMAT>
          Output serialization format for error messages [default: text] [possible values: text, json]
      --show-files
          See the files ruff will be run against with the current settings
      --show-settings
          See ruff's settings
      --add-noqa
          Enable automatic additions of noqa directives to failing lines
      --dummy-variable-rgx <DUMMY_VARIABLE_RGX>
          Regular expression matching the name of dummy variables
      --target-version <TARGET_VERSION>
          The minimum Python version that should be supported
  -h, --help
          Print help information
  -V, --version
          Print version information
```

### Excluding files

Exclusions are based on globs, and can be either:

- Single-path patterns, like `.mypy_cache` (to exclude any directory named `.mypy_cache` in the
  tree), `foo.py` (to exclude any file named `foo.py`), or `foo_*.py` (to exclude any file matching
  `foo_*.py` ).
- Relative patterns, like `directory/foo.py` (to exclude that specific file) or `directory/*.py`
  (to exclude any Python files in `directory`). Note that these paths are relative to the
  project root (e.g., the directory containing your `pyproject.toml`).

### Ignoring errors

To omit a lint check entirely, add it to the "ignore" list via `--ignore` or `--extend-ignore`,
either  on the command-line or in your `project.toml` file.

To ignore an error in-line, ruff uses a `noqa` system similar to [Flake8](https://flake8.pycqa.org/en/3.1.1/user/ignoring-errors.html).
To ignore an individual error, add `# noqa: {code}` to the end of the line, like so:

```python
# Ignore F841.
x = 1  # noqa: F841

# Ignore E741 and F841.
i = 1  # noqa: E741, F841

# Ignore _all_ errors.
x = 1  # noqa
```

Note that, for multi-line strings, the `noqa` directive should come at the end of the string, and
will apply to the entire body, like so:

```python
"""Lorem ipsum dolor sit amet.

Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.
"""  # noqa: E501
```

ruff supports several (experimental) workflows to aid in `noqa` management.

First, ruff provides a special error code, `M001`, to enforce that your `noqa` directives are
"valid", in that the errors they _say_ they ignore are actually being triggered on that line (and
thus suppressed). **You can run `ruff /path/to/file.py --extend-select M001` to flag unused `noqa`
directives.**

Second, ruff can _automatically remove_ unused `noqa` directives via its autofix functionality.
**You can run `ruff /path/to/file.py --extend-select M001 --fix` to automatically remove unused
`noqa` directives.**

Third, ruff can _automatically add_ `noqa` directives to all failing lines. This is useful when
migrating a new codebase to ruff. **You can run `ruff /path/to/file.py --add-noqa` to automatically
add `noqa` directives to all failing lines, with the appropriate error codes.**

### Compatibility with Black

ruff is compatible with [Black](https://github.com/psf/black) out-of-the-box, as long as
the `line-length` setting is consistent between the two.

As a project, ruff is designed to be used alongside Black and, as such, will defer implementing
stylistic lint rules that are obviated by autoformatting.

### Parity with Flake8

ruff's goal is to achieve feature parity with Flake8 when used (1) without plugins, (2) alongside
Black, and (3) on Python 3 code.

**Under those conditions, ruff implements 44 out of 60 rules.** (ruff is missing: 14 rules related
to string `.format` calls, 1 rule related to docstring parsing, and 1 rule related to redefined
variables.)

ruff also implements some of the most popular Flake8 plugins natively, including:

- [`flake8-builtins`](https://pypi.org/project/flake8-builtins/)
- [`flake8-super`](https://pypi.org/project/flake8-super/)
- [`flake8-print`](https://pypi.org/project/flake8-print/)
- [`flake8-comprehensions`](https://pypi.org/project/flake8-comprehensions/) (partial)
- [`pyupgrade`](https://pypi.org/project/pyupgrade/) (partial)

Beyond rule-set parity, ruff suffers from the following limitations vis-à-vis Flake8:

1. ruff does not yet support a few Python 3.9 and 3.10 language features, including structural
   pattern matching and parenthesized context managers.
2. Flake8 has a plugin architecture and supports writing custom lint rules.

## Rules

The ✅ emoji indicates a rule is enabled by default.

The 🛠 emoji indicates that a rule is automatically fixable by the `--fix` command-line option.

| Code | Name | Message |     |     |
| ---- | ---- | ------- | --- | --- |
| E402 | ModuleImportNotAtTopOfFile | Module level import not at top of file | ✅ |  |
| E501 | LineTooLong | Line too long (89 > 88 characters) | ✅ |  |
| E711 | NoneComparison | Comparison to `None` should be `cond is None` | ✅ |  |
| E712 | TrueFalseComparison | Comparison to `True` should be `cond is True` | ✅ |  |
| E713 | NotInTest | Test for membership should be `not in` | ✅ |  |
| E714 | NotIsTest | Test for object identity should be `is not` | ✅ |  |
| E721 | TypeComparison | Do not compare types, use `isinstance()` | ✅ |  |
| E722 | DoNotUseBareExcept | Do not use bare `except` | ✅ |  |
| E731 | DoNotAssignLambda | Do not assign a lambda expression, use a def | ✅ |  |
| E741 | AmbiguousVariableName | Ambiguous variable name: `...` | ✅ |  |
| E742 | AmbiguousClassName | Ambiguous class name: `...` | ✅ |  |
| E743 | AmbiguousFunctionName | Ambiguous function name: `...` | ✅ |  |
| E902 | IOError | IOError: `...` | ✅ |  |
| E999 | SyntaxError | SyntaxError: `...` | ✅ |  |
| W292 | NoNewLineAtEndOfFile | No newline at end of file | ✅ |  |
| F401 | UnusedImport | `...` imported but unused | ✅ | 🛠 |
| F402 | ImportShadowedByLoopVar | Import `...` from line 1 shadowed by loop variable | ✅ |  |
| F403 | ImportStarUsed | `from ... import *` used; unable to detect undefined names | ✅ |  |
| F404 | LateFutureImport | `from __future__` imports must occur at the beginning of the file | ✅ |  |
| F405 | ImportStarUsage | `...` may be undefined, or defined from star imports: `...` | ✅ |  |
| F406 | ImportStarNotPermitted | `from ... import *` only allowed at module level | ✅ |  |
| F407 | FutureFeatureNotDefined | Future feature `...` is not defined | ✅ |  |
| F541 | FStringMissingPlaceholders | f-string without any placeholders | ✅ |  |
| F601 | MultiValueRepeatedKeyLiteral | Dictionary key literal repeated | ✅ |  |
| F602 | MultiValueRepeatedKeyVariable | Dictionary key `...` repeated | ✅ |  |
| F621 | ExpressionsInStarAssignment | Too many expressions in star-unpacking assignment | ✅ |  |
| F622 | TwoStarredExpressions | Two starred expressions in assignment | ✅ |  |
| F631 | AssertTuple | Assert test is a non-empty tuple, which is always `True` | ✅ |  |
| F632 | IsLiteral | Use `==` and `!=` to compare constant literals | ✅ |  |
| F633 | InvalidPrintSyntax | Use of `>>` is invalid with `print` function | ✅ |  |
| F634 | IfTuple | If test is a tuple, which is always `True` | ✅ |  |
| F701 | BreakOutsideLoop | `break` outside loop | ✅ |  |
| F702 | ContinueOutsideLoop | `continue` not properly in loop | ✅ |  |
| F704 | YieldOutsideFunction | `yield` or `yield from` statement outside of a function/method | ✅ |  |
| F706 | ReturnOutsideFunction | `return` statement outside of a function/method | ✅ |  |
| F707 | DefaultExceptNotLast | An `except:` block as not the last exception handler | ✅ |  |
| F722 | ForwardAnnotationSyntaxError | Syntax error in forward annotation: `...` | ✅ |  |
| F821 | UndefinedName | Undefined name `...` | ✅ |  |
| F822 | UndefinedExport | Undefined name `...` in `__all__` | ✅ |  |
| F823 | UndefinedLocal | Local variable `...` referenced before assignment | ✅ |  |
| F831 | DuplicateArgumentName | Duplicate argument name in function definition | ✅ |  |
| F841 | UnusedVariable | Local variable `...` is assigned to but never used | ✅ |  |
| F901 | RaiseNotImplemented | `raise NotImplemented` should be `raise NotImplementedError` | ✅ |  |
| A001 | BuiltinVariableShadowing | Variable `...` is shadowing a python builtin |  |  |
| A002 | BuiltinArgumentShadowing | Argument `...` is shadowing a python builtin |  |  |
| A003 | BuiltinAttributeShadowing | Class attribute `...` is shadowing a python builtin |  |  |
| C400 | UnnecessaryGeneratorList | Unnecessary generator - rewrite as a list comprehension |  |  |
| C401 | UnnecessaryGeneratorSet | Unnecessary generator - rewrite as a set comprehension |  |  |
| C402 | UnnecessaryGeneratorDict | Unnecessary generator - rewrite as a dict comprehension |  |  |
| C403 | UnnecessaryListComprehensionSet | Unnecessary list comprehension - rewrite as a set comprehension |  |  |
| C404 | UnnecessaryListComprehensionDict | Unnecessary list comprehension - rewrite as a dict comprehension |  |  |
| C405 | UnnecessaryLiteralSet | Unnecessary <list/tuple> literal - rewrite as a set literal |  |  |
| C406 | UnnecessaryLiteralDict | Unnecessary <list/tuple> literal - rewrite as a dict literal |  |  |
| C408 | UnnecessaryCollectionCall | Unnecessary <dict/list/tuple> call - rewrite as a literal |  |  |
| SPR001 | SuperCallWithParameters | Use `super()` instead of `super(__class__, self)` |  | 🛠 |
| T201 | PrintFound | `print` found |  | 🛠 |
| T203 | PPrintFound | `pprint` found |  | 🛠 |
| U001 | UselessMetaclassType | `__metaclass__ = type` is implied |  | 🛠 |
| U002 | UnnecessaryAbspath | `abspath(__file__)` is unnecessary in Python 3.9 and later |  | 🛠 |
| U003 | TypeOfPrimitive | Use `str` instead of `type(...)` |  | 🛠 |
| U004 | UselessObjectInheritance | Class `...` inherits from object |  | 🛠 |
| U005 | NoAssertEquals | `assertEquals` is deprecated, use `assertEqual` instead |  | 🛠 |
| M001 | UnusedNOQA | Unused `noqa` directive |  | 🛠 |


## Integrations

### PyCharm

ruff can be installed as an [External Tool](https://www.jetbrains.com/help/pycharm/configuring-third-party-tools.html)
in PyCharm. Open the Preferences pane, then navigate to "Tools", then "External Tools". From there,
add a new tool with the following configuration:

![Install ruff as an External Tool](https://user-images.githubusercontent.com/1309177/193155720-336e43f0-1a8d-46b4-bc12-e60f9ae01f7e.png)

ruff should then appear as a runnable action:

![ruff as a runnable action](https://user-images.githubusercontent.com/1309177/193156026-732b0aaf-3dd9-4549-9b4d-2de6d2168a33.png)

### GitHub Actions

GitHub Actions has everything you need to run ruff out-of-the-box:

```yaml
name: CI
on: push
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - name: Install Python
        uses: actions/setup-python@v4
        with:
          python-version: "3.10"
      - name: Install dependencies
        run: |
          python -m pip install --upgrade pip
          pip install ruff
      - name: Run ruff
        run: ruff .
```

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
extend-exclude = [
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

hyperfine --ignore-failure --warmup 10 --runs 100 \
  "./target/release/ruff ./resources/test/cpython/ --no-cache" \
  "./target/release/ruff ./resources/test/cpython/"

Benchmark 1: ./target/release/ruff ./resources/test/cpython/ --no-cache
  Time (mean ± σ):     297.4 ms ±   4.9 ms    [User: 2460.0 ms, System: 67.2 ms]
  Range (min … max):   287.7 ms … 312.1 ms    100 runs

  Warning: Ignoring non-zero exit code.

Benchmark 2: ./target/release/ruff ./resources/test/cpython/
  Time (mean ± σ):      79.6 ms ±   7.3 ms    [User: 59.7 ms, System: 356.1 ms]
  Range (min … max):    62.4 ms … 111.2 ms    100 runs

  Warning: Ignoring non-zero exit code.
```

To benchmark against the ecosystem's existing tools:

```shell
hyperfine --ignore-failure --warmup 5 \
  "./target/release/ruff ./resources/test/cpython/ --no-cache" \
  "pylint --recursive=y resources/test/cpython/" \
  "pyflakes resources/test/cpython" \
  "autoflake --recursive --expand-star-imports --remove-all-unused-imports --remove-unused-variables --remove-duplicate-keys resources/test/cpython" \
  "pycodestyle resources/test/cpython" \
  "flake8 resources/test/cpython" \
  "python -m scripts.run_flake8 resources/test/cpython"
```

In order, these evaluate:

- ruff
- Pylint
- PyFlakes
- autoflake
- pycodestyle
- Flake8
- Flake8, with a hack to enable multiprocessing on macOS

(You can `poetry install` from `./scripts` to create a working environment for the above.)

```shell
Benchmark 1: ./target/release/ruff ./resources/test/cpython/ --no-cache
  Time (mean ± σ):     297.9 ms ±   7.0 ms    [User: 2436.6 ms, System: 65.9 ms]
  Range (min … max):   289.9 ms … 314.6 ms    10 runs

  Warning: Ignoring non-zero exit code.

Benchmark 2: pylint --recursive=y resources/test/cpython/
  Time (mean ± σ):     37.634 s ±  0.225 s    [User: 36.728 s, System: 0.853 s]
  Range (min … max):   37.201 s … 38.106 s    10 runs

  Warning: Ignoring non-zero exit code.

Benchmark 3: pyflakes resources/test/cpython
  Time (mean ± σ):     40.950 s ±  0.449 s    [User: 40.688 s, System: 0.229 s]
  Range (min … max):   40.348 s … 41.671 s    10 runs

  Warning: Ignoring non-zero exit code.

Benchmark 4: autoflake --recursive --expand-star-imports --remove-all-unused-imports --remove-unused-variables --remove-duplicate-keys resources/test/cpython
  Time (mean ± σ):     11.562 s ±  0.160 s    [User: 107.022 s, System: 1.143 s]
  Range (min … max):   11.417 s … 11.917 s    10 runs

Benchmark 5: pycodestyle resources/test/cpython
  Time (mean ± σ):     67.428 s ±  0.985 s    [User: 67.199 s, System: 0.203 s]
  Range (min … max):   65.313 s … 68.496 s    10 runs

  Warning: Ignoring non-zero exit code.

Benchmark 6: flake8 resources/test/cpython
  Time (mean ± σ):     116.099 s ±  1.178 s    [User: 115.217 s, System: 0.845 s]
  Range (min … max):   114.180 s … 117.724 s    10 runs

  Warning: Ignoring non-zero exit code.

Benchmark 7: python -m scripts.run_flake8 resources/test/cpython
  Time (mean ± σ):     20.477 s ±  0.349 s    [User: 142.372 s, System: 1.504 s]
  Range (min … max):   20.107 s … 21.183 s    10 runs

Summary
  './target/release/ruff ./resources/test/cpython/ --no-cache' ran
   38.81 ± 1.05 times faster than 'autoflake --recursive --expand-star-imports --remove-all-unused-imports --remove-unused-variables --remove-duplicate-keys resources/test/cpython'
   68.74 ± 1.99 times faster than 'python -m scripts.run_flake8 resources/test/cpython'
  126.33 ± 3.05 times faster than 'pylint --recursive=y resources/test/cpython/'
  137.46 ± 3.55 times faster than 'pyflakes resources/test/cpython'
  226.35 ± 6.23 times faster than 'pycodestyle resources/test/cpython'
  389.73 ± 9.92 times faster than 'flake8 resources/test/cpython'
```

## License

MIT

## Contributing

Contributions are welcome and hugely appreciated. To get started, check out the
[contributing guidelines](https://github.com/charliermarsh/ruff/blob/main/CONTRIBUTING.md).
