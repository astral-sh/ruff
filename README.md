# Ruff

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

- ⚡️  10-100x faster than existing linters
- 🐍  Installable via `pip`
- 🤝  Python 3.10 compatibility
- 🛠️  `pyproject.toml` support
- 📦  Built-in caching, to avoid re-analyzing unchanged files
- 🔧  Autofix support, for automatic error correction (e.g., automatically remove unused imports)
- ⚖️  [Near-parity](#how-does-ruff-compare-to-flake8) with the built-in Flake8 rule set
- 🔌  Native re-implementations of popular Flake8 plugins, like [`flake8-bugbear`](https://pypi.org/project/flake8-bugbear/)

Ruff aims to be orders of magnitude faster than alternative tools while integrating more
functionality behind a single, common interface. Ruff can be used to replace Flake8 (plus a variety
of plugins), [`isort`](https://pypi.org/project/isort/), [`pydocstyle`](https://pypi.org/project/pydocstyle/),
[`yesqa`](https://github.com/asottile/yesqa), [`eradicate`](https://pypi.org/project/eradicate/),
and even a subset of [`pyupgrade`](https://pypi.org/project/pyupgrade/) and [`autoflake`](https://pypi.org/project/autoflake/)
all while executing tens or hundreds of times faster than any individual tool. Ruff goes beyond the
responsibilities of a traditional linter, instead functioning as an advanced code transformation
tool capable of upgrading type annotations, rewriting class definitions, sorting imports, and more.

Ruff is extremely actively developed and used in major open-source projects like:

- [FastAPI](https://github.com/tiangolo/fastapi)
- [Bokeh](https://github.com/bokeh/bokeh)
- [Zulip](https://github.com/zulip/zulip)
- [Pydantic](https://github.com/pydantic/pydantic)
- [Saleor](https://github.com/saleor/saleor)
- [Hatch](https://github.com/pypa/hatch)
- [Jupyter Server](https://github.com/jupyter-server/jupyter_server)

Read the [launch blog post](https://notes.crmarsh.com/python-tooling-could-be-much-much-faster).

## Testimonials

[**Sebastián Ramírez**](https://twitter.com/tiangolo/status/1591912354882764802), creator
of [FastAPI](https://github.com/tiangolo/fastapi):

> Ruff is so fast that sometimes I add an intentional bug in the code just to confirm it's actually
> running and checking the code.

[**Bryan Van de Ven**](https://github.com/bokeh/bokeh/pull/12605), co-creator
of [Bokeh](https://github.com/bokeh/bokeh/), original author
of [Conda](https://docs.conda.io/en/latest/):

> Ruff is ~150-200x faster than flake8 on my machine, scanning the whole repo takes ~0.2s instead of
> ~20s. This is an enormous quality of life improvement for local dev. It's fast enough that I added
> it as an actual commit hook, which is terrific.

[**Tim Abbott**](https://github.com/charliermarsh/ruff/issues/465#issuecomment-1317400028), lead developer of [Zulip](https://github.com/zulip/zulip):

> This is just ridiculously fast... `ruff` is amazing.

## Table of Contents

1. [Installation and Usage](#installation-and-usage)
1. [Configuration](#configuration)
1. [Supported Rules](#supported-rules) <!-- Begin auto-generated table of contents. -->
   1. [Pyflakes (F)](#pyflakes-f)
   1. [pycodestyle (E, W)](#pycodestyle-e-w)
   1. [mccabe (C90)](#mccabe-c90)
   1. [isort (I)](#isort-i)
   1. [pydocstyle (D)](#pydocstyle-d)
   1. [pyupgrade (UP)](#pyupgrade-up)
   1. [pep8-naming (N)](#pep8-naming-n)
   1. [flake8-2020 (YTT)](#flake8-2020-ytt)
   1. [flake8-annotations (ANN)](#flake8-annotations-ann)
   1. [flake8-bandit (S)](#flake8-bandit-s)
   1. [flake8-blind-except (BLE)](#flake8-blind-except-ble)
   1. [flake8-boolean-trap (FBT)](#flake8-boolean-trap-fbt)
   1. [flake8-bugbear (B)](#flake8-bugbear-b)
   1. [flake8-builtins (A)](#flake8-builtins-a)
   1. [flake8-comprehensions (C4)](#flake8-comprehensions-c4)
   1. [flake8-debugger (T10)](#flake8-debugger-t10)
   1. [flake8-errmsg (EM)](#flake8-errmsg-em)
   1. [flake8-import-conventions (ICN)](#flake8-import-conventions-icn)
   1. [flake8-print (T20)](#flake8-print-t20)
   1. [flake8-quotes (Q)](#flake8-quotes-q)
   1. [flake8-return (RET)](#flake8-return-ret)
   1. [flake8-simplify (SIM)](#flake8-simplify-sim)
   1. [flake8-tidy-imports (TID)](#flake8-tidy-imports-tid)
   1. [flake8-unused-arguments (ARG)](#flake8-unused-arguments-arg)
   1. [flake8-datetimez (DTZ)](#flake8-datetimez-dtz)
   1. [eradicate (ERA)](#eradicate-era)
   1. [pandas-vet (PD)](#pandas-vet-pd)
   1. [pygrep-hooks (PGH)](#pygrep-hooks-pgh)
   1. [Pylint (PLC, PLE, PLR, PLW)](#pylint-plc-ple-plr-plw)
   1. [Ruff-specific rules (RUF)](#ruff-specific-rules-ruf)<!-- End auto-generated table of contents. -->
1. [Editor Integrations](#editor-integrations)
1. [FAQ](#faq)
1. [Development](#development)
1. [Releases](#releases)
1. [Benchmarks](#benchmarks)
1. [Reference](#reference)
1. [License](#license)
1. [Contributing](#contributing)

## Installation and Usage

### Installation

Ruff is available as [`ruff`](https://pypi.org/project/ruff/) on PyPI:

```shell
pip install ruff
```

[![Packaging status](https://repology.org/badge/vertical-allrepos/ruff-python-linter.svg?exclude_unsupported=1)](https://repology.org/project/ruff-python-linter/versions)

For **macOS Homebrew** and **Linuxbrew** users, Ruff is also available as [`ruff`](https://formulae.brew.sh/formula/ruff) on Homebrew:

```shell
brew install ruff
```

For **Conda** users, Ruff is also available as [`ruff`](https://anaconda.org/conda-forge/ruff) on `conda-forge`:

```shell
conda install -c conda-forge ruff
```

For **Arch Linux** users, Ruff is also available as [`ruff`](https://archlinux.org/packages/community/x86_64/ruff/) on the official repositories:

```shell
pacman -S ruff
```

### Usage

To run Ruff, try any of the following:

```shell
ruff path/to/code/to/check.py
ruff path/to/code/
ruff path/to/code/*.py
```

You can run Ruff in `--watch` mode to automatically re-run on-change:

```shell
ruff path/to/code/ --watch
```

Ruff also works with [pre-commit](https://pre-commit.com):

```yaml
repos:
  - repo: https://github.com/charliermarsh/ruff-pre-commit
    rev: v0.0.191
    hooks:
      - id: ruff
```

## Configuration

Ruff is configurable both via `pyproject.toml` and the command line. For a full list of configurable
options, see the [API reference](#reference).

If left unspecified, the default configuration is equivalent to:

```toml
[tool.ruff]
line-length = 88

# Enable Pyflakes `E` and `F` codes by default.
select = ["E", "F"]
ignore = []

# Exclude a variety of commonly ignored directories.
exclude = [
    ".bzr",
    ".direnv",
    ".eggs",
    ".git",
    ".hg",
    ".mypy_cache",
    ".nox",
    ".pants.d",
    ".ruff_cache",
    ".svn",
    ".tox",
    ".venv",
    "__pypackages__",
    "_build",
    "buck-out",
    "build",
    "dist",
    "node_modules",
    "venv",
]
per-file-ignores = {}

# Allow unused variables when underscore-prefixed.
dummy-variable-rgx = "^(_+|(_+[a-zA-Z0-9_]*[a-zA-Z0-9]+?))$"

# Assume Python 3.10.
target-version = "py310"

[tool.ruff.flake8-import-conventions.aliases]
altair = "alt"
"matplotlib.pyplot" = "plt"
numpy = "np"
pandas = "pd"
seaborn = "sns"

[tool.ruff.mccabe]
# Unlike Flake8, default to a complexity level of 10.
max-complexity = 10
```

As an example, the following would configure Ruff to: (1) avoid checking for line-length
violations (`E501`); (2), always autofix, but never remove unused imports (`F401`); and (3) ignore
import-at-top-of-file errors (`E402`) in `__init__.py` files:

```toml
[tool.ruff]
# Enable Pyflakes and pycodestyle rules.
select = ["E", "F"]

# Never enforce `E501` (line length violations).
ignore = ["E501"]

# Always autofix, but never try to fix `F401` (unused imports).
fix = true
unfixable = ["F401"]

# Ignore `E402` (import violations) in all `__init__.py` files, and in `path/to/file.py`.
[tool.ruff.per-file-ignores]
"__init__.py" = ["E402"]
"path/to/file.py" = ["E402"]
```

Plugin configurations should be expressed as subsections, e.g.:

```toml
[tool.ruff]
# Add "Q" to the list of enabled codes.
select = ["E", "F", "Q"]

[tool.ruff.flake8-quotes]
docstring-quotes = "double"
```

For a full list of configurable options, see the [API reference](#reference).

Some common configuration settings can be provided via the command-line:

```shell
ruff path/to/code/ --select F401 --select F403
```

See `ruff --help` for more:

```shell
Ruff: An extremely fast Python linter.

Usage: ruff [OPTIONS] [FILES]...

Arguments:
  [FILES]...

Options:
      --config <CONFIG>
          Path to the `pyproject.toml` file to use for configuration
  -v, --verbose
          Enable verbose logging
  -q, --quiet
          Only log errors
  -s, --silent
          Disable all logging (but still exit with status code "1" upon detecting errors)
  -e, --exit-zero
          Exit with status code "0", even upon detecting errors
  -w, --watch
          Run in watch mode by re-running whenever files change
      --fix
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
      --fixable <FIXABLE>
          List of error codes to treat as eligible for autofix. Only applicable when autofix itself is enabled (e.g., via `--fix`)
      --unfixable <UNFIXABLE>
          List of error codes to treat as ineligible for autofix. Only applicable when autofix itself is enabled (e.g., via `--fix`)
      --per-file-ignores <PER_FILE_IGNORES>
          List of mappings from file pattern to code to exclude
      --format <FORMAT>
          Output serialization format for error messages [possible values: text, json, junit, grouped, github]
      --show-source
          Show violations with source code
      --respect-gitignore
          Respect file exclusions via `.gitignore` and other standard ignore files
      --show-files
          See the files Ruff will be run against with the current settings
      --show-settings
          See the settings Ruff will use to check a given Python file
      --add-noqa
          Enable automatic additions of noqa directives to failing lines
      --dummy-variable-rgx <DUMMY_VARIABLE_RGX>
          Regular expression matching the name of dummy variables
      --target-version <TARGET_VERSION>
          The minimum Python version that should be supported
      --line-length <LINE_LENGTH>
          Set the line-length for length-associated checks and automatic formatting
      --max-complexity <MAX_COMPLEXITY>
          Max McCabe complexity allowed for a function
      --stdin-filename <STDIN_FILENAME>
          The name of the file when passing it through stdin
      --explain <EXPLAIN>
          Explain a rule
  -h, --help
          Print help information
  -V, --version
          Print version information
```

### `pyproject.toml` discovery

Similar to [ESLint](https://eslint.org/docs/latest/user-guide/configuring/configuration-files#cascading-and-hierarchy),
Ruff supports hierarchical configuration, such that the "closest" `pyproject.toml` file in the
directory hierarchy is used for every individual file, with all paths in the `pyproject.toml` file
(e.g., `exclude` globs, `src` paths) being resolved relative to the directory containing the
`pyproject.toml` file.

There are a few exceptions to these rules:

1. In locating the "closest" `pyproject.toml` file for a given path, Ruff ignore any
   `pyproject.toml` files that lack a `[tool.ruff]` section.
2. If a configuration file is passed directly via `--config`, those settings are used for across
   files. Any relative paths in that configuration file (like `exclude` globs or `src` paths) are
   resolved relative to the _current working directory_.
3. If no `pyproject.toml` file is found in the filesystem hierarchy, Ruff will fall back to using
   a default configuration. If a user-specific configuration file exists
   at `${config_dir}/ruff/pyproject.toml`,
   that file will be used instead of the default configuration, with `${config_dir}` being
   determined via the [`dirs`](https://docs.rs/dirs/4.0.0/dirs/fn.config_dir.html) crate, and all
   relative paths being again resolved relative to the _current working directory_.
4. Any `pyproject.toml`-supported settings that are provided on the command-line (e.g., via
   `--select`) will override the settings in _every_ resolved configuration file.

Unlike [ESLint](https://eslint.org/docs/latest/user-guide/configuring/configuration-files#cascading-and-hierarchy),
Ruff does not merge settings across configuration files; instead, the "closest" configuration file
is used, and any parent configuration files are ignored. In lieu of this implicit cascade, Ruff
supports an [`extend`](#extend) field, which allows you to inherit the settings from another
`pyproject.toml` file, like so:

```toml
# Extend the `pyproject.toml` file in the parent directory.
extend = "../pyproject.toml"
# But use a different line length.
line-length = 100
```

### Python file discovery

When passed a path on the command-line, Ruff will automatically discover all Python files in that
path, taking into account the [`exclude`](#exclude) and [`extend-exclude`](#extend-exclude) settings
in each directory's `pyproject.toml` file.

By default, Ruff will also skip any files that are omitted via `.ignore`, `.gitignore`,
`.git/info/exclude`, and global `gitignore` files (see: [`respect-gitignore`](#respect-gitignore)).

Files that are passed to `ruff` directly are always checked, regardless of the above criteria.
For example, `ruff /path/to/excluded/file.py` will always check `file.py`.

### Ignoring errors

To omit a lint check entirely, add it to the "ignore" list via [`ignore`](#ignore) or
[`extend-ignore`](#extend-ignore), either on the command-line or in your `project.toml` file.

To ignore an error inline, Ruff uses a `noqa` system similar to [Flake8](https://flake8.pycqa.org/en/3.1.1/user/ignoring-errors.html).
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
will apply to the entire string, like so:

```python
"""Lorem ipsum dolor sit amet.

Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.
"""  # noqa: E501
```

To ignore all errors across an entire file, Ruff supports Flake8's `# flake8: noqa` directive (or,
equivalently, `# ruff: noqa`). Adding either of those directives to any part of a file will disable
error reporting for the entire file.

For targeted exclusions across entire files (e.g., "Ignore all F841 violations in
`/path/to/file.py`"), see the [`per-file-ignores`](#per-file-ignores) configuration setting.

### "Action Comments"

Ruff respects `isort`'s ["Action Comments"](https://pycqa.github.io/isort/docs/configuration/action_comments.html)
(`# isort: skip_file`, `# isort: on`, `# isort: off`, `# isort: skip`, and `# isort: split`), which
enable selectively enabling and disabling import sorting for blocks of code and other inline
configuration.

See the [`isort` documentation](https://pycqa.github.io/isort/docs/configuration/action_comments.html)
for more.

### Automating `noqa` Directives

Ruff supports several workflows to aid in `noqa` management.

First, Ruff provides a special error code, `RUF100`, to enforce that your `noqa` directives are
"valid", in that the errors they _say_ they ignore are actually being triggered on that line (and
thus suppressed). You can run `ruff /path/to/file.py --extend-select RUF100` to flag unused `noqa`
directives.

Second, Ruff can _automatically remove_ unused `noqa` directives via its autofix functionality.
You can run `ruff /path/to/file.py --extend-select RUF100 --fix` to automatically remove unused
`noqa` directives.

Third, Ruff can _automatically add_ `noqa` directives to all failing lines. This is useful when
migrating a new codebase to Ruff. You can run `ruff /path/to/file.py --add-noqa` to automatically
add `noqa` directives to all failing lines, with the appropriate error codes.

## Supported Rules

Regardless of the rule's origin, Ruff re-implements every rule in Rust as a first-party feature.

By default, Ruff enables all `E` and `F` error codes, which correspond to those built-in to Flake8.

The 🛠 emoji indicates that a rule is automatically fixable by the `--fix` command-line option.

<!-- Sections automatically generated by `cargo dev generate-rules-table`. -->
<!-- Begin auto-generated sections. -->
### Pyflakes (F)

For more, see [Pyflakes](https://pypi.org/project/pyflakes/2.5.0/) on PyPI.

| Code | Name | Message | Fix |
| ---- | ---- | ------- | --- |
| F401 | UnusedImport | `...` imported but unused | 🛠 |
| F402 | ImportShadowedByLoopVar | Import `...` from line 1 shadowed by loop variable |  |
| F403 | ImportStarUsed | `from ... import *` used; unable to detect undefined names |  |
| F404 | LateFutureImport | `from __future__` imports must occur at the beginning of the file |  |
| F405 | ImportStarUsage | `...` may be undefined, or defined from star imports: `...` |  |
| F406 | ImportStarNotPermitted | `from ... import *` only allowed at module level |  |
| F407 | FutureFeatureNotDefined | Future feature `...` is not defined |  |
| F501 | PercentFormatInvalidFormat | '...' % ... has invalid format string: ... |  |
| F502 | PercentFormatExpectedMapping | '...' % ... expected mapping but got sequence |  |
| F503 | PercentFormatExpectedSequence | '...' % ... expected sequence but got mapping |  |
| F504 | PercentFormatExtraNamedArguments | '...' % ... has unused named argument(s): ... | 🛠 |
| F505 | PercentFormatMissingArgument | '...' % ... is missing argument(s) for placeholder(s): ... |  |
| F506 | PercentFormatMixedPositionalAndNamed | '...' % ... has mixed positional and named placeholders |  |
| F507 | PercentFormatPositionalCountMismatch | '...' % ... has 4 placeholder(s) but 2 substitution(s) |  |
| F508 | PercentFormatStarRequiresSequence | '...' % ... `*` specifier requires sequence |  |
| F509 | PercentFormatUnsupportedFormatCharacter | '...' % ... has unsupported format character 'c' |  |
| F521 | StringDotFormatInvalidFormat | '...'.format(...) has invalid format string: ... |  |
| F522 | StringDotFormatExtraNamedArguments | '...'.format(...) has unused named argument(s): ... | 🛠 |
| F523 | StringDotFormatExtraPositionalArguments | '...'.format(...) has unused arguments at position(s): ... |  |
| F524 | StringDotFormatMissingArguments | '...'.format(...) is missing argument(s) for placeholder(s): ... |  |
| F525 | StringDotFormatMixingAutomatic | '...'.format(...) mixes automatic and manual numbering |  |
| F541 | FStringMissingPlaceholders | f-string without any placeholders |  |
| F601 | MultiValueRepeatedKeyLiteral | Dictionary key literal repeated |  |
| F602 | MultiValueRepeatedKeyVariable | Dictionary key `...` repeated |  |
| F621 | ExpressionsInStarAssignment | Too many expressions in star-unpacking assignment |  |
| F622 | TwoStarredExpressions | Two starred expressions in assignment |  |
| F631 | AssertTuple | Assert test is a non-empty tuple, which is always `True` |  |
| F632 | IsLiteral | Use `==` and `!=` to compare constant literals | 🛠 |
| F633 | InvalidPrintSyntax | Use of `>>` is invalid with `print` function |  |
| F634 | IfTuple | If test is a tuple, which is always `True` |  |
| F701 | BreakOutsideLoop | `break` outside loop |  |
| F702 | ContinueOutsideLoop | `continue` not properly in loop |  |
| F704 | YieldOutsideFunction | `yield` statement outside of a function |  |
| F706 | ReturnOutsideFunction | `return` statement outside of a function/method |  |
| F707 | DefaultExceptNotLast | An `except` block as not the last exception handler |  |
| F722 | ForwardAnnotationSyntaxError | Syntax error in forward annotation: `...` |  |
| F811 | RedefinedWhileUnused | Redefinition of unused `...` from line 1 |  |
| F821 | UndefinedName | Undefined name `...` |  |
| F822 | UndefinedExport | Undefined name `...` in `__all__` |  |
| F823 | UndefinedLocal | Local variable `...` referenced before assignment |  |
| F831 | DuplicateArgumentName | Duplicate argument name in function definition |  |
| F841 | UnusedVariable | Local variable `...` is assigned to but never used |  |
| F842 | UnusedAnnotation | Local variable `...` is annotated but never used |  |
| F901 | RaiseNotImplemented | `raise NotImplemented` should be `raise NotImplementedError` | 🛠 |

### pycodestyle (E, W)

For more, see [pycodestyle](https://pypi.org/project/pycodestyle/2.9.1/) on PyPI.

| Code | Name | Message | Fix |
| ---- | ---- | ------- | --- |
| E401 | MultipleImportsOnOneLine | Multiple imports on one line |  |
| E402 | ModuleImportNotAtTopOfFile | Module level import not at top of file |  |
| E501 | LineTooLong | Line too long (89 > 88 characters) |  |
| E711 | NoneComparison | Comparison to `None` should be `cond is None` | 🛠 |
| E712 | TrueFalseComparison | Comparison to `True` should be `cond is True` | 🛠 |
| E713 | NotInTest | Test for membership should be `not in` | 🛠 |
| E714 | NotIsTest | Test for object identity should be `is not` | 🛠 |
| E721 | TypeComparison | Do not compare types, use `isinstance()` |  |
| E722 | DoNotUseBareExcept | Do not use bare `except` |  |
| E731 | DoNotAssignLambda | Do not assign a lambda expression, use a def | 🛠 |
| E741 | AmbiguousVariableName | Ambiguous variable name: `...` |  |
| E742 | AmbiguousClassName | Ambiguous class name: `...` |  |
| E743 | AmbiguousFunctionName | Ambiguous function name: `...` |  |
| E902 | IOError | IOError: `...` |  |
| E999 | SyntaxError | SyntaxError: `...` |  |
| W292 | NoNewLineAtEndOfFile | No newline at end of file |  |
| W605 | InvalidEscapeSequence | Invalid escape sequence: '\c' |  |

### mccabe (C90)

For more, see [mccabe](https://pypi.org/project/mccabe/0.7.0/) on PyPI.

| Code | Name | Message | Fix |
| ---- | ---- | ------- | --- |
| C901 | FunctionIsTooComplex | `...` is too complex (10) |  |

### isort (I)

For more, see [isort](https://pypi.org/project/isort/5.10.1/) on PyPI.

| Code | Name | Message | Fix |
| ---- | ---- | ------- | --- |
| I001 | UnsortedImports | Import block is un-sorted or un-formatted | 🛠 |

### pydocstyle (D)

For more, see [pydocstyle](https://pypi.org/project/pydocstyle/6.1.1/) on PyPI.

| Code | Name | Message | Fix |
| ---- | ---- | ------- | --- |
| D100 | PublicModule | Missing docstring in public module |  |
| D101 | PublicClass | Missing docstring in public class |  |
| D102 | PublicMethod | Missing docstring in public method |  |
| D103 | PublicFunction | Missing docstring in public function |  |
| D104 | PublicPackage | Missing docstring in public package |  |
| D105 | MagicMethod | Missing docstring in magic method |  |
| D106 | PublicNestedClass | Missing docstring in public nested class |  |
| D107 | PublicInit | Missing docstring in `__init__` |  |
| D200 | FitsOnOneLine | One-line docstring should fit on one line |  |
| D201 | NoBlankLineBeforeFunction | No blank lines allowed before function docstring (found 1) | 🛠 |
| D202 | NoBlankLineAfterFunction | No blank lines allowed after function docstring (found 1) | 🛠 |
| D203 | OneBlankLineBeforeClass | 1 blank line required before class docstring | 🛠 |
| D204 | OneBlankLineAfterClass | 1 blank line required after class docstring | 🛠 |
| D205 | BlankLineAfterSummary | 1 blank line required between summary line and description | 🛠 |
| D206 | IndentWithSpaces | Docstring should be indented with spaces, not tabs |  |
| D207 | NoUnderIndentation | Docstring is under-indented | 🛠 |
| D208 | NoOverIndentation | Docstring is over-indented | 🛠 |
| D209 | NewLineAfterLastParagraph | Multi-line docstring closing quotes should be on a separate line | 🛠 |
| D210 | NoSurroundingWhitespace | No whitespaces allowed surrounding docstring text | 🛠 |
| D211 | NoBlankLineBeforeClass | No blank lines allowed before class docstring | 🛠 |
| D212 | MultiLineSummaryFirstLine | Multi-line docstring summary should start at the first line |  |
| D213 | MultiLineSummarySecondLine | Multi-line docstring summary should start at the second line |  |
| D214 | SectionNotOverIndented | Section is over-indented ("Returns") | 🛠 |
| D215 | SectionUnderlineNotOverIndented | Section underline is over-indented ("Returns") | 🛠 |
| D300 | UsesTripleQuotes | Use """triple double quotes""" |  |
| D301 | UsesRPrefixForBackslashedContent | Use r""" if any backslashes in a docstring |  |
| D400 | EndsInPeriod | First line should end with a period | 🛠 |
| D402 | NoSignature | First line should not be the function's signature |  |
| D403 | FirstLineCapitalized | First word of the first line should be properly capitalized |  |
| D404 | NoThisPrefix | First word of the docstring should not be "This" |  |
| D405 | CapitalizeSectionName | Section name should be properly capitalized ("returns") | 🛠 |
| D406 | NewLineAfterSectionName | Section name should end with a newline ("Returns") | 🛠 |
| D407 | DashedUnderlineAfterSection | Missing dashed underline after section ("Returns") | 🛠 |
| D408 | SectionUnderlineAfterName | Section underline should be in the line following the section's name ("Returns") | 🛠 |
| D409 | SectionUnderlineMatchesSectionLength | Section underline should match the length of its name ("Returns") | 🛠 |
| D410 | BlankLineAfterSection | Missing blank line after section ("Returns") | 🛠 |
| D411 | BlankLineBeforeSection | Missing blank line before section ("Returns") | 🛠 |
| D412 | NoBlankLinesBetweenHeaderAndContent | No blank lines allowed between a section header and its content ("Returns") | 🛠 |
| D413 | BlankLineAfterLastSection | Missing blank line after last section ("Returns") | 🛠 |
| D414 | NonEmptySection | Section has no content ("Returns") |  |
| D415 | EndsInPunctuation | First line should end with a period, question mark, or exclamation point | 🛠 |
| D416 | SectionNameEndsInColon | Section name should end with a colon ("Returns") | 🛠 |
| D417 | DocumentAllArguments | Missing argument descriptions in the docstring: `x`, `y` |  |
| D418 | SkipDocstring | Function decorated with `@overload` shouldn't contain a docstring |  |
| D419 | NonEmpty | Docstring is empty |  |

### pyupgrade (UP)

For more, see [pyupgrade](https://pypi.org/project/pyupgrade/3.2.0/) on PyPI.

| Code | Name | Message | Fix |
| ---- | ---- | ------- | --- |
| UP001 | UselessMetaclassType | `__metaclass__ = type` is implied | 🛠 |
| UP003 | TypeOfPrimitive | Use `str` instead of `type(...)` | 🛠 |
| UP004 | UselessObjectInheritance | Class `...` inherits from object | 🛠 |
| UP005 | DeprecatedUnittestAlias | `assertEquals` is deprecated, use `assertEqual` instead | 🛠 |
| UP006 | UsePEP585Annotation | Use `list` instead of `List` for type annotations | 🛠 |
| UP007 | UsePEP604Annotation | Use `X \| Y` for type annotations | 🛠 |
| UP008 | SuperCallWithParameters | Use `super()` instead of `super(__class__, self)` | 🛠 |
| UP009 | PEP3120UnnecessaryCodingComment | UTF-8 encoding declaration is unnecessary | 🛠 |
| UP010 | UnnecessaryFutureImport | Unnecessary `__future__` import `...` for target Python version | 🛠 |
| UP011 | UnnecessaryLRUCacheParams | Unnecessary parameters to `functools.lru_cache` | 🛠 |
| UP012 | UnnecessaryEncodeUTF8 | Unnecessary call to `encode` as UTF-8 | 🛠 |
| UP013 | ConvertTypedDictFunctionalToClass | Convert `...` from `TypedDict` functional to class syntax | 🛠 |
| UP014 | ConvertNamedTupleFunctionalToClass | Convert `...` from `NamedTuple` functional to class syntax | 🛠 |
| UP015 | RedundantOpenModes | Unnecessary open mode parameters | 🛠 |
| UP016 | RemoveSixCompat | Unnecessary `six` compatibility usage | 🛠 |

### pep8-naming (N)

For more, see [pep8-naming](https://pypi.org/project/pep8-naming/0.13.2/) on PyPI.

| Code | Name | Message | Fix |
| ---- | ---- | ------- | --- |
| N801 | InvalidClassName | Class name `...` should use CapWords convention  |  |
| N802 | InvalidFunctionName | Function name `...` should be lowercase |  |
| N803 | InvalidArgumentName | Argument name `...` should be lowercase |  |
| N804 | InvalidFirstArgumentNameForClassMethod | First argument of a class method should be named `cls` |  |
| N805 | InvalidFirstArgumentNameForMethod | First argument of a method should be named `self` |  |
| N806 | NonLowercaseVariableInFunction | Variable `...` in function should be lowercase |  |
| N807 | DunderFunctionName | Function name should not start and end with `__` |  |
| N811 | ConstantImportedAsNonConstant | Constant `...` imported as non-constant `...` |  |
| N812 | LowercaseImportedAsNonLowercase | Lowercase `...` imported as non-lowercase `...` |  |
| N813 | CamelcaseImportedAsLowercase | Camelcase `...` imported as lowercase `...` |  |
| N814 | CamelcaseImportedAsConstant | Camelcase `...` imported as constant `...` |  |
| N815 | MixedCaseVariableInClassScope | Variable `mixedCase` in class scope should not be mixedCase |  |
| N816 | MixedCaseVariableInGlobalScope | Variable `mixedCase` in global scope should not be mixedCase |  |
| N817 | CamelcaseImportedAsAcronym | Camelcase `...` imported as acronym `...` |  |
| N818 | ErrorSuffixOnExceptionName | Exception name `...` should be named with an Error suffix |  |

### flake8-2020 (YTT)

For more, see [flake8-2020](https://pypi.org/project/flake8-2020/1.7.0/) on PyPI.

| Code | Name | Message | Fix |
| ---- | ---- | ------- | --- |
| YTT101 | SysVersionSlice3Referenced | `sys.version[:3]` referenced (python3.10), use `sys.version_info` |  |
| YTT102 | SysVersion2Referenced | `sys.version[2]` referenced (python3.10), use `sys.version_info` |  |
| YTT103 | SysVersionCmpStr3 | `sys.version` compared to string (python3.10), use `sys.version_info` |  |
| YTT201 | SysVersionInfo0Eq3Referenced | `sys.version_info[0] == 3` referenced (python4), use `>=` |  |
| YTT202 | SixPY3Referenced | `six.PY3` referenced (python4), use `not six.PY2` |  |
| YTT203 | SysVersionInfo1CmpInt | `sys.version_info[1]` compared to integer (python4), compare `sys.version_info` to tuple |  |
| YTT204 | SysVersionInfoMinorCmpInt | `sys.version_info.minor` compared to integer (python4), compare `sys.version_info` to tuple |  |
| YTT301 | SysVersion0Referenced | `sys.version[0]` referenced (python10), use `sys.version_info` |  |
| YTT302 | SysVersionCmpStr10 | `sys.version` compared to string (python10), use `sys.version_info` |  |
| YTT303 | SysVersionSlice1Referenced | `sys.version[:1]` referenced (python10), use `sys.version_info` |  |

### flake8-annotations (ANN)

For more, see [flake8-annotations](https://pypi.org/project/flake8-annotations/2.9.1/) on PyPI.

| Code | Name | Message | Fix |
| ---- | ---- | ------- | --- |
| ANN001 | MissingTypeFunctionArgument | Missing type annotation for function argument `...` |  |
| ANN002 | MissingTypeArgs | Missing type annotation for `*...` |  |
| ANN003 | MissingTypeKwargs | Missing type annotation for `**...` |  |
| ANN101 | MissingTypeSelf | Missing type annotation for `...` in method |  |
| ANN102 | MissingTypeCls | Missing type annotation for `...` in classmethod |  |
| ANN201 | MissingReturnTypePublicFunction | Missing return type annotation for public function `...` |  |
| ANN202 | MissingReturnTypePrivateFunction | Missing return type annotation for private function `...` |  |
| ANN204 | MissingReturnTypeSpecialMethod | Missing return type annotation for special method `...` | 🛠 |
| ANN205 | MissingReturnTypeStaticMethod | Missing return type annotation for staticmethod `...` |  |
| ANN206 | MissingReturnTypeClassMethod | Missing return type annotation for classmethod `...` |  |
| ANN401 | DynamicallyTypedExpression | Dynamically typed expressions (typing.Any) are disallowed in `...` |  |

### flake8-bandit (S)

For more, see [flake8-bandit](https://pypi.org/project/flake8-bandit/4.1.1/) on PyPI.

| Code | Name | Message | Fix |
| ---- | ---- | ------- | --- |
| S101 | AssertUsed | Use of `assert` detected |  |
| S102 | ExecUsed | Use of `exec` detected |  |
| S104 | HardcodedBindAllInterfaces | Possible binding to all interfaces |  |
| S105 | HardcodedPasswordString | Possible hardcoded password: `"..."` |  |
| S106 | HardcodedPasswordFuncArg | Possible hardcoded password: `"..."` |  |
| S107 | HardcodedPasswordDefault | Possible hardcoded password: `"..."` |  |

### flake8-blind-except (BLE)

For more, see [flake8-blind-except](https://pypi.org/project/flake8-blind-except/0.2.1/) on PyPI.

| Code | Name | Message | Fix |
| ---- | ---- | ------- | --- |
| BLE001 | BlindExcept | Do not catch blind exception: `Exception` |  |

### flake8-boolean-trap (FBT)

For more, see [flake8-boolean-trap](https://pypi.org/project/flake8-boolean-trap/0.1.0/) on PyPI.

| Code | Name | Message | Fix |
| ---- | ---- | ------- | --- |
| FBT001 | BooleanPositionalArgInFunctionDefinition | Boolean positional arg in function definition |  |
| FBT002 | BooleanDefaultValueInFunctionDefinition | Boolean default value in function definition |  |
| FBT003 | BooleanPositionalValueInFunctionCall | Boolean positional value in function call |  |

### flake8-bugbear (B)

For more, see [flake8-bugbear](https://pypi.org/project/flake8-bugbear/22.10.27/) on PyPI.

| Code | Name | Message | Fix |
| ---- | ---- | ------- | --- |
| B002 | UnaryPrefixIncrement | Python does not support the unary prefix increment |  |
| B003 | AssignmentToOsEnviron | Assigning to `os.environ` doesn't clear the environment |  |
| B004 | UnreliableCallableCheck |  Using `hasattr(x, '__call__')` to test if x is callable is unreliable. Use `callable(x)` for consistent results. |  |
| B005 | StripWithMultiCharacters | Using `.strip()` with multi-character strings is misleading the reader |  |
| B006 | MutableArgumentDefault | Do not use mutable data structures for argument defaults |  |
| B007 | UnusedLoopControlVariable | Loop control variable `i` not used within the loop body | 🛠 |
| B008 | FunctionCallArgumentDefault | Do not perform function call in argument defaults |  |
| B009 | GetAttrWithConstant | Do not call `getattr` with a constant attribute value. It is not any safer than normal property access. | 🛠 |
| B010 | SetAttrWithConstant | Do not call `setattr` with a constant attribute value. It is not any safer than normal property access. | 🛠 |
| B011 | DoNotAssertFalse | Do not `assert False` (`python -O` removes these calls), raise `AssertionError()` | 🛠 |
| B012 | JumpStatementInFinally | `return/continue/break` inside finally blocks cause exceptions to be silenced |  |
| B013 | RedundantTupleInExceptionHandler | A length-one tuple literal is redundant. Write `except ValueError` instead of `except (ValueError,)`. | 🛠 |
| B014 | DuplicateHandlerException | Exception handler with duplicate exception: `ValueError` | 🛠 |
| B015 | UselessComparison | Pointless comparison. This comparison does nothing but waste CPU instructions. Either prepend `assert` or remove it. |  |
| B016 | CannotRaiseLiteral | Cannot raise a literal. Did you intend to return it or raise an Exception? |  |
| B017 | NoAssertRaisesException | `assertRaises(Exception)` should be considered evil |  |
| B018 | UselessExpression | Found useless expression. Either assign it to a variable or remove it. |  |
| B019 | CachedInstanceMethod | Use of `functools.lru_cache` or `functools.cache` on methods can lead to memory leaks |  |
| B020 | LoopVariableOverridesIterator | Loop control variable `...` overrides iterable it iterates |  |
| B021 | FStringDocstring | f-string used as docstring. This will be interpreted by python as a joined string rather than a docstring. |  |
| B022 | UselessContextlibSuppress | No arguments passed to `contextlib.suppress`. No exceptions will be suppressed and therefore this context manager is redundant |  |
| B023 | FunctionUsesLoopVariable | Function definition does not bind loop variable `...` |  |
| B024 | AbstractBaseClassWithoutAbstractMethod | `...` is an abstract base class, but it has no abstract methods |  |
| B025 | DuplicateTryBlockException | try-except block with duplicate exception `Exception` |  |
| B026 | StarArgUnpackingAfterKeywordArg | Star-arg unpacking after a keyword argument is strongly discouraged |  |
| B027 | EmptyMethodWithoutAbstractDecorator | `...` is an empty method in an abstract base class, but has no abstract decorator |  |
| B904 | RaiseWithoutFromInsideExcept | Within an except clause, raise exceptions with raise ... from err or raise ... from None to distinguish them from errors in exception handling |  |
| B905 | ZipWithoutExplicitStrict | `zip()` without an explicit `strict=` parameter |  |

### flake8-builtins (A)

For more, see [flake8-builtins](https://pypi.org/project/flake8-builtins/2.0.1/) on PyPI.

| Code | Name | Message | Fix |
| ---- | ---- | ------- | --- |
| A001 | BuiltinVariableShadowing | Variable `...` is shadowing a python builtin |  |
| A002 | BuiltinArgumentShadowing | Argument `...` is shadowing a python builtin |  |
| A003 | BuiltinAttributeShadowing | Class attribute `...` is shadowing a python builtin |  |

### flake8-comprehensions (C4)

For more, see [flake8-comprehensions](https://pypi.org/project/flake8-comprehensions/3.10.1/) on PyPI.

| Code | Name | Message | Fix |
| ---- | ---- | ------- | --- |
| C400 | UnnecessaryGeneratorList | Unnecessary generator (rewrite as a `list` comprehension) | 🛠 |
| C401 | UnnecessaryGeneratorSet | Unnecessary generator (rewrite as a `set` comprehension) | 🛠 |
| C402 | UnnecessaryGeneratorDict | Unnecessary generator (rewrite as a `dict` comprehension) | 🛠 |
| C403 | UnnecessaryListComprehensionSet | Unnecessary `list` comprehension (rewrite as a `set` comprehension) | 🛠 |
| C404 | UnnecessaryListComprehensionDict | Unnecessary `list` comprehension (rewrite as a `dict` comprehension) | 🛠 |
| C405 | UnnecessaryLiteralSet | Unnecessary `(list\|tuple)` literal (rewrite as a `set` literal) | 🛠 |
| C406 | UnnecessaryLiteralDict | Unnecessary `(list\|tuple)` literal (rewrite as a `dict` literal) | 🛠 |
| C408 | UnnecessaryCollectionCall | Unnecessary `(dict\|list\|tuple)` call (rewrite as a literal) | 🛠 |
| C409 | UnnecessaryLiteralWithinTupleCall | Unnecessary `(list\|tuple)` literal passed to `tuple()` (remove the outer call to `tuple()`) | 🛠 |
| C410 | UnnecessaryLiteralWithinListCall | Unnecessary `(list\|tuple)` literal passed to `list()` (rewrite as a `list` literal) | 🛠 |
| C411 | UnnecessaryListCall | Unnecessary `list` call (remove the outer call to `list()`) | 🛠 |
| C413 | UnnecessaryCallAroundSorted | Unnecessary `(list\|reversed)` call around `sorted()` | 🛠 |
| C414 | UnnecessaryDoubleCastOrProcess | Unnecessary `(list\|reversed\|set\|sorted\|tuple)` call within `(list\|set\|sorted\|tuple)()` |  |
| C415 | UnnecessarySubscriptReversal | Unnecessary subscript reversal of iterable within `(reversed\|set\|sorted)()` |  |
| C416 | UnnecessaryComprehension | Unnecessary `(list\|set)` comprehension (rewrite using `(list\|set)()`) | 🛠 |
| C417 | UnnecessaryMap | Unnecessary `map` usage (rewrite using a `(list\|set\|dict)` comprehension) |  |

### flake8-debugger (T10)

For more, see [flake8-debugger](https://pypi.org/project/flake8-debugger/4.1.2/) on PyPI.

| Code | Name | Message | Fix |
| ---- | ---- | ------- | --- |
| T100 | Debugger | Import for `...` found |  |

### flake8-errmsg (EM)

For more, see [flake8-errmsg](https://pypi.org/project/flake8-errmsg/0.4.0/) on PyPI.

| Code | Name | Message | Fix |
| ---- | ---- | ------- | --- |
| EM101 | RawStringInException | Exception must not use a string literal, assign to variable first |  |
| EM102 | FStringInException | Exception must not use an f-string literal, assign to variable first |  |
| EM103 | DotFormatInException | Exception must not use a `.format()` string directly, assign to variable first |  |

### flake8-import-conventions (ICN)

| Code | Name | Message | Fix |
| ---- | ---- | ------- | --- |
| ICN001 | ImportAliasIsNotConventional | `...` should be imported as `...` |  |

### flake8-print (T20)

For more, see [flake8-print](https://pypi.org/project/flake8-print/5.0.0/) on PyPI.

| Code | Name | Message | Fix |
| ---- | ---- | ------- | --- |
| T201 | PrintFound | `print` found | 🛠 |
| T203 | PPrintFound | `pprint` found | 🛠 |

### flake8-quotes (Q)

For more, see [flake8-quotes](https://pypi.org/project/flake8-quotes/3.3.1/) on PyPI.

| Code | Name | Message | Fix |
| ---- | ---- | ------- | --- |
| Q000 | BadQuotesInlineString | Single quotes found but double quotes preferred |  |
| Q001 | BadQuotesMultilineString | Single quote multiline found but double quotes preferred |  |
| Q002 | BadQuotesDocstring | Single quote docstring found but double quotes preferred |  |
| Q003 | AvoidQuoteEscape | Change outer quotes to avoid escaping inner quotes |  |

### flake8-return (RET)

For more, see [flake8-return](https://pypi.org/project/flake8-return/1.2.0/) on PyPI.

| Code | Name | Message | Fix |
| ---- | ---- | ------- | --- |
| RET501 | UnnecessaryReturnNone | Do not explicitly `return None` in function if it is the only possible return value | 🛠 |
| RET502 | ImplicitReturnValue | Do not implicitly `return None` in function able to return non-`None` value | 🛠 |
| RET503 | ImplicitReturn | Missing explicit `return` at the end of function able to return non-`None` value | 🛠 |
| RET504 | UnnecessaryAssign | Unnecessary variable assignment before `return` statement |  |
| RET505 | SuperfluousElseReturn | Unnecessary `else` after `return` statement |  |
| RET506 | SuperfluousElseRaise | Unnecessary `else` after `raise` statement |  |
| RET507 | SuperfluousElseContinue | Unnecessary `else` after `continue` statement |  |
| RET508 | SuperfluousElseBreak | Unnecessary `else` after `break` statement |  |

### flake8-simplify (SIM)

For more, see [flake8-simplify](https://pypi.org/project/flake8-simplify/0.19.3/) on PyPI.

| Code | Name | Message | Fix |
| ---- | ---- | ------- | --- |
| SIM118 | KeyInDict | Use `key in dict` instead of `key in dict.keys()` | 🛠 |

### flake8-tidy-imports (TID)

For more, see [flake8-tidy-imports](https://pypi.org/project/flake8-tidy-imports/4.8.0/) on PyPI.

| Code | Name | Message | Fix |
| ---- | ---- | ------- | --- |
| TID252 | BannedRelativeImport | Relative imports are banned |  |

### flake8-unused-arguments (ARG)

For more, see [flake8-unused-arguments](https://pypi.org/project/flake8-unused-arguments/0.0.12/) on PyPI.

| Code | Name | Message | Fix |
| ---- | ---- | ------- | --- |
| ARG001 | UnusedFunctionArgument | Unused function argument: `...` |  |
| ARG002 | UnusedMethodArgument | Unused method argument: `...` |  |
| ARG003 | UnusedClassMethodArgument | Unused class method argument: `...` |  |
| ARG004 | UnusedStaticMethodArgument | Unused static method argument: `...` |  |
| ARG005 | UnusedLambdaArgument | Unused lambda argument: `...` |  |

### flake8-datetimez (DTZ)

For more, see [flake8-datetimez](https://pypi.org/project/flake8-datetimez/20.10.0/) on PyPI.

| Code | Name | Message | Fix |
| ---- | ---- | ------- | --- |
| DTZ001 | CallDatetimeWithoutTzinfo | The use of `datetime.datetime()` without `tzinfo` argument is not allowed |  |
| DTZ002 | CallDatetimeToday | The use of `datetime.datetime.today()` is not allowed |  |
| DTZ003 | CallDatetimeUtcnow | The use of `datetime.datetime.utcnow()` is not allowed |  |
| DTZ004 | CallDatetimeUtcfromtimestamp | The use of `datetime.datetime.utcfromtimestamp()` is not allowed |  |
| DTZ005 | CallDatetimeNowWithoutTzinfo | The use of `datetime.datetime.now()` without `tz` argument is not allowed |  |
| DTZ006 | CallDatetimeFromtimestamp | The use of `datetime.datetime.fromtimestamp()` without `tz` argument is not allowed |  |
| DTZ007 | CallDatetimeStrptimeWithoutZone | The use of `datetime.datetime.strptime()` without %z must be followed by `.replace(tzinfo=)` |  |
| DTZ011 | CallDateToday | The use of `datetime.date.today()` is not allowed. |  |
| DTZ012 | CallDateFromtimestamp | The use of `datetime.date.fromtimestamp()` is not allowed |  |

### eradicate (ERA)

For more, see [eradicate](https://pypi.org/project/eradicate/2.1.0/) on PyPI.

| Code | Name | Message | Fix |
| ---- | ---- | ------- | --- |
| ERA001 | CommentedOutCode | Found commented-out code | 🛠 |

### pandas-vet (PD)

For more, see [pandas-vet](https://pypi.org/project/pandas-vet/0.2.3/) on PyPI.

| Code | Name | Message | Fix |
| ---- | ---- | ------- | --- |
| PD002 | UseOfInplaceArgument | `inplace=True` should be avoided; it has inconsistent behavior |  |
| PD003 | UseOfDotIsNull | `.isna` is preferred to `.isnull`; functionality is equivalent |  |
| PD004 | UseOfDotNotNull | `.notna` is preferred to `.notnull`; functionality is equivalent |  |
| PD007 | UseOfDotIx | `.ix` is deprecated; use more explicit `.loc` or `.iloc` |  |
| PD008 | UseOfDotAt | Use `.loc` instead of `.at`.  If speed is important, use numpy. |  |
| PD009 | UseOfDotIat | Use `.iloc` instead of `.iat`.  If speed is important, use numpy. |  |
| PD010 | UseOfDotPivotOrUnstack | `.pivot_table` is preferred to `.pivot` or `.unstack`; provides same functionality |  |
| PD011 | UseOfDotValues | Use `.to_numpy()` instead of `.values` |  |
| PD012 | UseOfDotReadTable | `.read_csv` is preferred to `.read_table`; provides same functionality |  |
| PD013 | UseOfDotStack | `.melt` is preferred to `.stack`; provides same functionality |  |
| PD015 | UseOfPdMerge | Use `.merge` method instead of `pd.merge` function. They have equivalent functionality. |  |
| PD901 | DfIsABadVariableName | `df` is a bad variable name. Be kinder to your future self. |  |

### pygrep-hooks (PGH)

For more, see [pygrep-hooks](https://github.com/pre-commit/pygrep-hooks) on GitHub.

| Code | Name | Message | Fix |
| ---- | ---- | ------- | --- |
| PGH001 | NoEval | No builtin `eval()` allowed |  |
| PGH002 | DeprecatedLogWarn | `warn` is deprecated in favor of `warning` |  |
| PGH003 | BlanketTypeIgnore | Use specific error codes when ignoring type issues |  |

### Pylint (PLC, PLE, PLR, PLW)

For more, see [Pylint](https://pypi.org/project/pylint/2.15.7/) on PyPI.

| Code | Name | Message | Fix |
| ---- | ---- | ------- | --- |
| PLC0414 | UselessImportAlias | Import alias does not rename original package | 🛠 |
| PLC2201 | MisplacedComparisonConstant | Comparison should be ... | 🛠 |
| PLC3002 | UnnecessaryDirectLambdaCall | Lambda expression called directly. Execute the expression inline instead. |  |
| PLE0117 | NonlocalWithoutBinding | Nonlocal name `...` found without binding |  |
| PLE0118 | UsedPriorGlobalDeclaration | Name `...` is used prior to global declaration on line 1 |  |
| PLE1142 | AwaitOutsideAsync | `await` should be used within an async function |  |
| PLR0206 | PropertyWithParameters | Cannot have defined parameters for properties |  |
| PLR0402 | ConsiderUsingFromImport | Use `from ... import ...` in lieu of alias |  |
| PLR1701 | ConsiderMergingIsinstance | Merge these isinstance calls: `isinstance(..., (...))` |  |
| PLR1722 | UseSysExit | Use `sys.exit()` instead of `exit` | 🛠 |
| PLW0120 | UselessElseOnLoop | Else clause on loop without a break statement, remove the else and de-indent all the code inside it |  |
| PLW0602 | GlobalVariableNotAssigned | Using global for `...` but no assignment is done |  |

### Ruff-specific rules (RUF)

| Code | Name | Message | Fix |
| ---- | ---- | ------- | --- |
| RUF001 | AmbiguousUnicodeCharacterString | String contains ambiguous unicode character '𝐁' (did you mean 'B'?) | 🛠 |
| RUF002 | AmbiguousUnicodeCharacterDocstring | Docstring contains ambiguous unicode character '𝐁' (did you mean 'B'?) | 🛠 |
| RUF003 | AmbiguousUnicodeCharacterComment | Comment contains ambiguous unicode character '𝐁' (did you mean 'B'?) |  |
| RUF100 | UnusedNOQA | Unused `noqa` directive | 🛠 |

<!-- End auto-generated sections. -->

## Editor Integrations

### VS Code (Official)

Download the [Ruff VS Code extension](https://marketplace.visualstudio.com/items?itemName=charliermarsh.ruff),
which supports autofix actions, import sorting, and more.

![](https://user-images.githubusercontent.com/1309177/205175763-cf34871d-5c05-4abf-9916-440afc82dbf8.gif)

### Language Server Protocol (Official)

Ruff supports the [Language Server Protocol](https://microsoft.github.io/language-server-protocol/)
via the [`ruff-lsp`](https://github.com/charliermarsh/ruff-lsp) Python package, available on
[PyPI](https://pypi.org/project/ruff-lsp/).

[`ruff-lsp`](https://github.com/charliermarsh/ruff-lsp) enables Ruff to be used with any editor that
supports the Language Server Protocol, including [Neovim](https://github.com/charliermarsh/ruff-lsp#example-neovim),
[Sublime Text](https://github.com/charliermarsh/ruff-lsp#example-sublime-text), Emacs, and more.

For example, to use `ruff-lsp` with Neovim, install `ruff-lsp` from PyPI along with
[`nvim-lspconfig`](https://github.com/neovim/nvim-lspconfig). Then, add something like the following
to your `init.lua`:

```lua
-- See: https://github.com/neovim/nvim-lspconfig/tree/54eb2a070a4f389b1be0f98070f81d23e2b1a715#suggested-configuration
local opts = { noremap=true, silent=true }
vim.keymap.set('n', '<space>e', vim.diagnostic.open_float, opts)
vim.keymap.set('n', '[d', vim.diagnostic.goto_prev, opts)
vim.keymap.set('n', ']d', vim.diagnostic.goto_next, opts)
vim.keymap.set('n', '<space>q', vim.diagnostic.setloclist, opts)

-- Use an on_attach function to only map the following keys
-- after the language server attaches to the current buffer
local on_attach = function(client, bufnr)
  -- Enable completion triggered by <c-x><c-o>
  vim.api.nvim_buf_set_option(bufnr, 'omnifunc', 'v:lua.vim.lsp.omnifunc')

  -- Mappings.
  -- See `:help vim.lsp.*` for documentation on any of the below functions
  local bufopts = { noremap=true, silent=true, buffer=bufnr }
  vim.keymap.set('n', 'gD', vim.lsp.buf.declaration, bufopts)
  vim.keymap.set('n', 'gd', vim.lsp.buf.definition, bufopts)
  vim.keymap.set('n', 'K', vim.lsp.buf.hover, bufopts)
  vim.keymap.set('n', 'gi', vim.lsp.buf.implementation, bufopts)
  vim.keymap.set('n', '<C-k>', vim.lsp.buf.signature_help, bufopts)
  vim.keymap.set('n', '<space>wa', vim.lsp.buf.add_workspace_folder, bufopts)
  vim.keymap.set('n', '<space>wr', vim.lsp.buf.remove_workspace_folder, bufopts)
  vim.keymap.set('n', '<space>wl', function()
    print(vim.inspect(vim.lsp.buf.list_workspace_folders()))
  end, bufopts)
  vim.keymap.set('n', '<space>D', vim.lsp.buf.type_definition, bufopts)
  vim.keymap.set('n', '<space>rn', vim.lsp.buf.rename, bufopts)
  vim.keymap.set('n', '<space>ca', vim.lsp.buf.code_action, bufopts)
  vim.keymap.set('n', 'gr', vim.lsp.buf.references, bufopts)
  vim.keymap.set('n', '<space>f', function() vim.lsp.buf.format { async = true } end, bufopts)
end

-- Configure `ruff-lsp`.
local configs = require 'lspconfig.configs'
if not configs.ruff_lsp then
  configs.ruff_lsp = {
    default_config = {
    cmd = { "ruff-lsp" },
    filetypes = {'python'},
    root_dir = require('lspconfig').util.find_git_ancestor,
    settings = {
      ruff_lsp = {
        -- Any extra CLI arguments for `ruff` go here.
        args = {}
      }
    }
  }
}
end

require('lspconfig').ruff_lsp.setup {
  on_attach = on_attach,
}
```

Upon successful installation, you should see Ruff's diagnostics surfaced directly in your editor:

![Code Actions available in Neovim](https://user-images.githubusercontent.com/1309177/208278707-25fa37e4-079d-4597-ad35-b95dba066960.png)

### Language Server Protocol (Unofficial)

Ruff is also available as the [`python-lsp-ruff`](https://github.com/python-lsp/python-lsp-ruff)
plugin for [`python-lsp-server`](https://github.com/python-lsp/python-lsp-ruff), both of which are
installable from PyPI:

```shell
pip install python-lsp-server python-lsp-ruff
```

The LSP server can then be used with any editor that supports the Language Server Protocol.

For example, to use `python-lsp-ruff` with Neovim, add something like the following to your
`init.lua`:

```lua
require'lspconfig'.pylsp.setup {
  settings = {
    pylsp = {
      plugins = {
        ruff = {
          enabled = true
        },
        pycodestyle = {
          enabled = false
        },
        pyflakes = {
          enabled = false
        },
        mccabe = {
          enabled = false
        }
      }
    }
  },
}
```

### PyCharm

Ruff can be installed as an [External Tool](https://www.jetbrains.com/help/pycharm/configuring-third-party-tools.html)
in PyCharm. Open the Preferences pane, then navigate to "Tools", then "External Tools". From there,
add a new tool with the following configuration:

![Install Ruff as an External Tool](https://user-images.githubusercontent.com/1309177/193155720-336e43f0-1a8d-46b4-bc12-e60f9ae01f7e.png)

Ruff should then appear as a runnable action:

![Ruff as a runnable action](https://user-images.githubusercontent.com/1309177/193156026-732b0aaf-3dd9-4549-9b4d-2de6d2168a33.png)

### Vim & Neovim

Ruff can be integrated into any editor that supports the Language Server Protocol via [`ruff-lsp`](https://github.com/charliermarsh/ruff-lsp)
(see: [Language Server Protocol](#language-server-protocol-official)).

Ruff is also available as part of the [coc-pyright](https://github.com/fannheyward/coc-pyright)
extension for `coc.nvim`.

<details>
<summary>Ruff can also be integrated via <a href="https://github.com/neovim/nvim-lspconfig/blob/master/doc/server_configurations.md#efm"><code>efm</code></a> in just a <a href="https://github.com/JafarAbdi/myconfigs/blob/6f0b6b2450e92ec8fc50422928cd22005b919110/efm-langserver/config.yaml#L14-L20">few lines</a>.</summary>
<br>

```yaml
tools:
  python-ruff: &python-ruff
    lint-command: 'ruff --config ~/myconfigs/linters/ruff.toml --quiet ${INPUT}'
    lint-stdin: true
    lint-formats:
      - '%f:%l:%c: %m'
    format-command: 'ruff --stdin-filename ${INPUT} --config ~/myconfigs/linters/ruff.toml --fix --exit-zero --quiet -'
    format-stdin: true
```

</details>

<details>
<summary>For neovim users using <a href="https://github.com/jose-elias-alvarez/null-ls.nvim"><code>null-ls</code></a>, Ruff is already <a href="https://github.com/jose-elias-alvarez/null-ls.nvim">integrated</a>.</summary>
<br>

```lua
local null_ls = require("null-ls")
local methods = require("null-ls.methods")
local helpers = require("null-ls.helpers")

local function ruff_fix()
    return helpers.make_builtin({
        name = "ruff",
        meta = {
            url = "https://github.com/charliermarsh/ruff/",
            description = "An extremely fast Python linter, written in Rust.",
        },
        method = methods.internal.FORMATTING,
        filetypes = { "python" },
        generator_opts = {
            command = "ruff",
            args = { "--fix", "-e", "-n", "--stdin-filename", "$FILENAME", "-" },
            to_stdin = true
        },
        factory = helpers.formatter_factory
    })
end

null_ls.setup({
    sources = {
        ruff_fix(),
        null_ls.builtins.diagnostics.ruff,
    }
})
```

</details>

### GitHub Actions

GitHub Actions has everything you need to run Ruff out-of-the-box:

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
      - name: Run Ruff
        run: ruff .
```

## FAQ

### Is Ruff compatible with Black?

Yes. Ruff is compatible with [Black](https://github.com/psf/black) out-of-the-box, as long as
the `line-length` setting is consistent between the two.

As a project, Ruff is designed to be used alongside Black and, as such, will defer implementing
stylistic lint rules that are obviated by autoformatting.

### How does Ruff compare to Flake8?

(Coming from Flake8? Try [`flake8-to-ruff`](https://pypi.org/project/flake8-to-ruff/) to
automatically convert your existing configuration.)

Ruff can be used as a drop-in replacement for Flake8 when used (1) without or with a small number of
plugins, (2) alongside Black, and (3) on Python 3 code.

Under those conditions, Ruff implements every rule in Flake8.

Ruff also re-implements some of the most popular Flake8 plugins and related code quality tools
natively, including:

- [`autoflake`](https://pypi.org/project/autoflake/) (1/7)
- [`eradicate`](https://pypi.org/project/eradicate/)
- [`flake8-2020`](https://pypi.org/project/flake8-2020/)
- [`flake8-annotations`](https://pypi.org/project/flake8-annotations/)
- [`flake8-bandit`](https://pypi.org/project/flake8-bandit/) (6/40)
- [`flake8-blind-except`](https://pypi.org/project/flake8-blind-except/)
- [`flake8-boolean-trap`](https://pypi.org/project/flake8-boolean-trap/)
- [`flake8-bugbear`](https://pypi.org/project/flake8-bugbear/)
- [`flake8-builtins`](https://pypi.org/project/flake8-builtins/)
- [`flake8-comprehensions`](https://pypi.org/project/flake8-comprehensions/)
- [`flake8-datetimez`](https://pypi.org/project/flake8-datetimez/)
- [`flake8-debugger`](https://pypi.org/project/flake8-debugger/)
- [`flake8-docstrings`](https://pypi.org/project/flake8-docstrings/)
- [`flake8-eradicate`](https://pypi.org/project/flake8-eradicate/)
- [`flake8-errmsg`](https://pypi.org/project/flake8-errmsg/)
- [`flake8-import-conventions`](https://github.com/joaopalmeiro/flake8-import-conventions)
- [`flake8-print`](https://pypi.org/project/flake8-print/)
- [`flake8-quotes`](https://pypi.org/project/flake8-quotes/)
- [`flake8-return`](https://pypi.org/project/flake8-return/)
- [`flake8-super`](https://pypi.org/project/flake8-super/)
- [`flake8-tidy-imports`](https://pypi.org/project/flake8-tidy-imports/) (1/3)
- [`isort`](https://pypi.org/project/isort/)
- [`mccabe`](https://pypi.org/project/mccabe/)
- [`pep8-naming`](https://pypi.org/project/pep8-naming/)
- [`pydocstyle`](https://pypi.org/project/pydocstyle/)
- [`pygrep-hooks`](https://github.com/pre-commit/pygrep-hooks) (3/10)
- [`pyupgrade`](https://pypi.org/project/pyupgrade/) (16/33)
- [`yesqa`](https://github.com/asottile/yesqa)

Note that, in some cases, Ruff uses different error code prefixes than would be found in the
originating Flake8 plugins. For example, Ruff uses `TID252` to represent the `I252` rule from
`flake8-tidy-imports`. This helps minimize conflicts across plugins and allows any individual plugin
to be toggled on or off with a single (e.g.) `--select TID`, as opposed to `--select I2` (to avoid
conflicts with the `isort` rules, like `I001`).

Beyond the rule set, Ruff suffers from the following limitations vis-à-vis Flake8:

1. Ruff does not yet support structural pattern matching.
2. Flake8 has a plugin architecture and supports writing custom lint rules. (Instead, popular Flake8
   plugins are re-implemented in Rust as part of Ruff itself.)

### How does Ruff compare to Pylint?

At time of writing, Pylint implements 409 total rules, while Ruff implements 224, of which
at least 60 overlap with the Pylint rule set. Subjectively, Pylint tends to implement more rules
based on type inference (e.g., validating the number of arguments in a function call).

Like Flake8, Pylint supports plugins (called "checkers"), while Ruff implements all checks natively.

Unlike Pylint, Ruff is capable of automatically fixing its own lint errors.

Pylint parity is being tracked in [#689](https://github.com/charliermarsh/ruff/issues/689).

### Which tools does Ruff replace?

Today, Ruff can be used to replace Flake8 when used with any of the following plugins:

- [`flake8-2020`](https://pypi.org/project/flake8-2020/)
- [`flake8-annotations`](https://pypi.org/project/flake8-annotations/)
- [`flake8-bandit`](https://pypi.org/project/flake8-bandit/) (6/40)
- [`flake8-blind-except`](https://pypi.org/project/flake8-blind-except/)
- [`flake8-boolean-trap`](https://pypi.org/project/flake8-boolean-trap/)
- [`flake8-bugbear`](https://pypi.org/project/flake8-bugbear/)
- [`flake8-builtins`](https://pypi.org/project/flake8-builtins/)
- [`flake8-comprehensions`](https://pypi.org/project/flake8-comprehensions/)
- [`flake8-datetimez`](https://pypi.org/project/flake8-datetimez/)
- [`flake8-debugger`](https://pypi.org/project/flake8-debugger/)
- [`flake8-docstrings`](https://pypi.org/project/flake8-docstrings/)
- [`flake8-eradicate`](https://pypi.org/project/flake8-eradicate/)
- [`flake8-errmsg`](https://pypi.org/project/flake8-errmsg/)
- [`flake8-import-conventions`](https://github.com/joaopalmeiro/flake8-import-conventions)
- [`flake8-print`](https://pypi.org/project/flake8-print/)
- [`flake8-quotes`](https://pypi.org/project/flake8-quotes/)
- [`flake8-return`](https://pypi.org/project/flake8-return/)
- [`flake8-super`](https://pypi.org/project/flake8-super/)
- [`flake8-tidy-imports`](https://pypi.org/project/flake8-tidy-imports/) (1/3)
- [`mccabe`](https://pypi.org/project/mccabe/)
- [`pep8-naming`](https://pypi.org/project/pep8-naming/)
- [`pydocstyle`](https://pypi.org/project/pydocstyle/)

Ruff can also replace [`isort`](https://pypi.org/project/isort/),
[`yesqa`](https://github.com/asottile/yesqa), [`eradicate`](https://pypi.org/project/eradicate/),
[`pygrep-hooks`](https://github.com/pre-commit/pygrep-hooks) (3/10), and a subset of the rules
implemented in [`pyupgrade`](https://pypi.org/project/pyupgrade/) (16/33).

If you're looking to use Ruff, but rely on an unsupported Flake8 plugin, free to file an Issue.

### Do I need to install Rust to use Ruff?

Nope! Ruff is available as [`ruff`](https://pypi.org/project/ruff/) on PyPI:

```shell
pip install ruff
```

Ruff ships with wheels for all major platforms, which enables `pip` to install Ruff without relying
on Rust at all.

### Can I write my own plugins for Ruff?

Ruff does not yet support third-party plugins, though a plugin system is within-scope for the
project. See [#283](https://github.com/charliermarsh/ruff/issues/283) for more.

### How does Ruff's import sorting compare to [`isort`](https://pypi.org/project/isort/)?

Ruff's import sorting is intended to be nearly equivalent to `isort` when used `profile = "black"`.
(There are some minor differences in how Ruff and isort break ties between similar imports.)

Like `isort`, Ruff's import sorting is compatible with Black.

Ruff is less configurable than `isort`, but supports the `known-first-party`, `known-third-party`,
`extra-standard-library`, and `src` settings, like so:

```toml
[tool.ruff]
select = [
    # Pyflakes
    "F",
    # Pycodestyle
    "E",
    "W",
    # isort
    "I001"
]
src = ["src", "tests"]

[tool.ruff.isort]
known-first-party = ["my_module1", "my_module2"]
```

### Does Ruff support NumPy- or Google-style docstrings?

Yes! To enable a specific docstring convention, start by enabling all `pydocstyle` error codes, and
then selectively disabling based on your [preferred convention](https://www.pydocstyle.org/en/latest/error_codes.html#default-conventions).

For example, if you're coming from `flake8-docstrings`, the following configuration is equivalent to
`--docstring-convention=numpy`:

```toml
[tool.ruff]
extend-select = ["D"]
extend-ignore = [
    "D107",
    "D203",
    "D212",
    "D213",
    "D402",
    "D413",
    "D415",
    "D416",
    "D417",
]
```

Similarly, the following is equivalent to `--docstring-convention=google`:

```toml
[tool.ruff]
extend-select = ["D"]
extend-ignore = [
    "D203",
    "D204",
    "D213",
    "D215",
    "D400",
    "D404",
    "D406",
    "D407",
    "D408",
    "D409",
    "D413",
]
```

Similarly, the following is equivalent to `--docstring-convention=pep8`:

```toml
[tool.ruff]
extend-select = ["D"]
extend-ignore = [
    "D203",
    "D212",
    "D213",
    "D214",
    "D215",
    "D404",
    "D405",
    "D406",
    "D407",
    "D408",
    "D409",
    "D410",
    "D411",
    "D413",
    "D415",
    "D416",
    "D417",
]
```

## Development

Ruff is written in Rust (1.65.0). You'll need to install the [Rust toolchain](https://www.rust-lang.org/tools/install)
for development.

Assuming you have `cargo` installed, you can run:

```shell
cargo run resources/test/fixtures
```

For development, we use [nightly Rust](https://rust-lang.github.io/rustup/concepts/channels.html#working-with-nightly-rust):

```shell
cargo +nightly fmt
cargo +nightly clippy
cargo +nightly test
```

## Releases

Ruff is distributed on [PyPI](https://pypi.org/project/ruff/), and published via [`maturin`](https://github.com/PyO3/maturin).

See: `.github/workflows/release.yaml`.

## Benchmarks

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

- Ruff
- Pylint
- Pyflakes
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

## Reference

### Options

<!-- Sections automatically generated by `cargo dev generate-options`. -->
<!-- Begin auto-generated options sections. -->

#### [`allowed-confusables`](#allowed-confusables)

A list of allowed "confusable" Unicode characters to ignore when enforcing `RUF001`,
`RUF002`, and `RUF003`.

**Default value**: `[]`

**Type**: `Vec<char>`

**Example usage**:

```toml
[tool.ruff]
# Allow minus-sign (U+2212), greek-small-letter-rho (U+03C1), and the asterisk-operator (U+2217),
# which could be confused for "-", "p", and "*", respectively.
allowed-confusables = ["−", "ρ", "∗"]
```

---

#### [`dummy-variable-rgx`](#dummy-variable-rgx)

A regular expression used to identify "dummy" variables, or those which should be
ignored when evaluating (e.g.) unused-variable checks. The default expression matches
`_`, `__`, and `_var`, but not `_var_`.

**Default value**: `"^(_+|(_+[a-zA-Z0-9_]*[a-zA-Z0-9]+?))$"`

**Type**: `Regex`

**Example usage**:

```toml
[tool.ruff]
# Only ignore variables named "_".
dummy-variable-rgx = "^_$"
```

---

#### [`exclude`](#exclude)

A list of file patterns to exclude from linting.

Exclusions are based on globs, and can be either:

- Single-path patterns, like `.mypy_cache` (to exclude any directory named `.mypy_cache` in the
  tree), `foo.py` (to exclude any file named `foo.py`), or `foo_*.py` (to exclude any file matching
  `foo_*.py` ).
- Relative patterns, like `directory/foo.py` (to exclude that specific file) or `directory/*.py`
  (to exclude any Python files in `directory`). Note that these paths are relative to the
  project root (e.g., the directory containing your `pyproject.toml`).

Note that you'll typically want to use [`extend-exclude`](#extend-exclude) to modify
the excluded paths.

**Default value**: `[".bzr", ".direnv", ".eggs", ".git", ".hg", ".mypy_cache", ".nox", ".pants.d", ".ruff_cache", ".svn", ".tox", ".venv", "__pypackages__", "_build", "buck-out", "build", "dist", "node_modules", "venv"]`

**Type**: `Vec<FilePattern>`

**Example usage**:

```toml
[tool.ruff]
exclude = [".venv"]
```

---

#### [`extend`](#extend)

A path to a local `pyproject.toml` file to merge into this configuration. User home
directory and environment variables will be expanded.

To resolve the current `pyproject.toml` file, Ruff will first resolve this base
configuration file, then merge in any properties defined in the current configuration
file.

**Default value**: `None`

**Type**: `Path`

**Example usage**:

```toml
[tool.ruff]
# Extend the `pyproject.toml` file in the parent directory.
extend = "../pyproject.toml"
# But use a different line length.
line-length = 100
```

---

#### [`extend-exclude`](#extend-exclude)

A list of file patterns to omit from linting, in addition to those specified by `exclude`.

**Default value**: `[]`

**Type**: `Vec<FilePattern>`

**Example usage**:

```toml
[tool.ruff]
# In addition to the standard set of exclusions, omit all tests, plus a specific file.
extend-exclude = ["tests", "src/bad.py"]
```

---

#### [`extend-ignore`](#extend-ignore)

A list of check code prefixes to ignore, in addition to those specified by `ignore`.

**Default value**: `[]`

**Type**: `Vec<CheckCodePrefix>`

**Example usage**:

```toml
[tool.ruff]
# Skip unused variable checks (`F841`).
extend-ignore = ["F841"]
```

---

#### [`extend-select`](#extend-select)

A list of check code prefixes to enable, in addition to those specified by `select`.

**Default value**: `[]`

**Type**: `Vec<CheckCodePrefix>`

**Example usage**:

```toml
[tool.ruff]
# On top of the default `select` (`E`, `F`), enable flake8-bugbear (`B`) and flake8-quotes (`Q`).
extend-select = ["B", "Q"]
```

---

#### [`external`](#external)

A list of check codes that are unsupported by Ruff, but should be preserved when (e.g.)
validating `# noqa` directives. Useful for retaining `# noqa` directives that cover plugins not
yet implemented in Ruff.

**Default value**: `[]`

**Type**: `Vec<String>`

**Example usage**:

```toml
[tool.ruff]
# Avoiding flagging (and removing) `V101` from any `# noqa`
# directives, despite Ruff's lack of support for `vulture`.
external = ["V101"]
```

---

#### [`fix`](#fix)

Enable autofix behavior by-default when running `ruff` (overridden
by the `--fix` and `--no-fix` command-line flags).

**Default value**: `false`

**Type**: `bool`

**Example usage**:

```toml
[tool.ruff]
fix = true
```

---

#### [`fixable`](#fixable)

A list of check code prefixes to consider autofix-able.

**Default value**: `["A", "ANN", "ARG", "B", "BLE", "C", "D", "E", "ERA", "F", "FBT", "I", "ICN", "N", "PGH", "PLC", "PLE", "PLR", "PLW", "Q", "RET", "RUF", "S", "T", "TID", "UP", "W", "YTT"]`

**Type**: `Vec<CheckCodePrefix>`

**Example usage**:

```toml
[tool.ruff]
# Only allow autofix behavior for `E` and `F` checks.
fixable = ["E", "F"]
```

---

#### [`force-exclude`](#force-exclude)

Whether to enforce `exclude` and `extend-exclude` patterns, even for paths that are
passed to Ruff explicitly. Typically, Ruff will lint any paths passed in directly, even
if they would typically be excluded. Setting `force-exclude = true` will cause Ruff to
respect these exclusions unequivocally.

This is useful for [`pre-commit`](https://pre-commit.com/), which explicitly passes all
changed files to the [`ruff-pre-commit`](https://github.com/charliermarsh/ruff-pre-commit)
plugin, regardless of whether they're marked as excluded by Ruff's own settings.

**Default value**: `false`

**Type**: `bool`

**Example usage**:

```toml
[tool.ruff]
force-exclude = true
```

---

#### [`format`](#format)

The style in which violation messages should be formatted: `"text"` (default),
`"grouped"` (group messages by file), `"json"` (machine-readable), `"junit"`
(machine-readable XML), or `"github"` (GitHub Actions annotations).

**Default value**: `"text"`

**Type**: `SerializationType`

**Example usage**:

```toml
[tool.ruff]
# Group violations by containing file.
format = "grouped"
```

---

#### [`ignore`](#ignore)

A list of check code prefixes to ignore. Prefixes can specify exact checks (like
`F841`), entire categories (like `F`), or anything in between.

When breaking ties between enabled and disabled checks (via `select` and `ignore`,
respectively), more specific prefixes override less specific prefixes.

**Default value**: `[]`

**Type**: `Vec<CheckCodePrefix>`

**Example usage**:

```toml
[tool.ruff]
# Skip unused variable checks (`F841`).
ignore = ["F841"]
```

---

#### [`ignore-init-module-imports`](#ignore-init-module-imports)

Avoid automatically removing unused imports in `__init__.py` files. Such imports will
still be +flagged, but with a dedicated message suggesting that the import is either
added to the module' +`__all__` symbol, or re-exported with a redundant alias (e.g.,
`import os as os`).

**Default value**: `false`

**Type**: `bool`

**Example usage**:

```toml
[tool.ruff]
ignore-init-module-imports = true
```

---

#### [`line-length`](#line-length)

The line length to use when enforcing long-lines violations (like E501).

**Default value**: `88`

**Type**: `usize`

**Example usage**:

```toml
[tool.ruff]
# Allow lines to be as long as 120 characters.
line-length = 120
```

---

#### [`per-file-ignores`](#per-file-ignores)

A list of mappings from file pattern to check code prefixes to exclude, when considering
any matching files.

**Default value**: `{}`

**Type**: `HashMap<String, Vec<CheckCodePrefix>>`

**Example usage**:

```toml
[tool.ruff]
# Ignore `E402` (import violations) in all `__init__.py` files, and in `path/to/file.py`.
[tool.ruff.per-file-ignores]
"__init__.py" = ["E402"]
"path/to/file.py" = ["E402"]
```

---

#### [`respect-gitignore`](#respect-gitignore)

Whether to automatically exclude files that are ignored by `.ignore`, `.gitignore`,
`.git/info/exclude`, and global `gitignore` files. Enabled by default.

**Default value**: `true`

**Type**: `bool`

**Example usage**:

```toml
[tool.ruff]
respect_gitignore = false
```

---

#### [`select`](#select)

A list of check code prefixes to enable. Prefixes can specify exact checks (like
`F841`), entire categories (like `F`), or anything in between.

When breaking ties between enabled and disabled checks (via `select` and `ignore`,
respectively), more specific prefixes override less specific prefixes.

**Default value**: `["E", "F"]`

**Type**: `Vec<CheckCodePrefix>`

**Example usage**:

```toml
[tool.ruff]
# On top of the defaults (`E`, `F`), enable flake8-bugbear (`B`) and flake8-quotes (`Q`).
select = ["E", "F", "B", "Q"]
```

---

#### [`show-source`](#show-source)

Whether to show source code snippets when reporting lint error violations (overridden by
the `--show-source` command-line flag).

**Default value**: `false`

**Type**: `bool`

**Example usage**:

```toml
[tool.ruff]
# By default, always show source code snippets.
show-source = true
```

---

#### [`src`](#src)

The source code paths to consider, e.g., when resolving first- vs. third-party imports.

As an example: given a Python package structure like:

```text
my_package/
  pyproject.toml
  src/
    my_package/
      __init__.py
      foo.py
      bar.py
```

The `src` directory should be included in `source` (e.g., `source = ["src"]`), such that
when resolving imports, `my_package.foo` is considered a first-party import.

This field supports globs. For example, if you have a series of Python packages in
a `python_modules` directory, `src = ["python_modules/*"]` would expand to incorporate
all of the packages in that directory. User home directory and environment variables
will also be expanded.

**Default value**: `["."]`

**Type**: `Vec<PathBuf>`

**Example usage**:

```toml
[tool.ruff]
# Allow imports relative to the "src" and "test" directories.
src = ["src", "test"]
```

---

#### [`target-version`](#target-version)

The Python version to target, e.g., when considering automatic code upgrades, like
rewriting type annotations. Note that the target version will _not_ be inferred from the
_current_ Python version, and instead must be specified explicitly (as seen below).

**Default value**: `"py310"`

**Type**: `PythonVersion`

**Example usage**:

```toml
[tool.ruff]
# Always generate Python 3.7-compatible code.
target-version = "py37"
```

---

#### [`unfixable`](#unfixable)

A list of check code prefixes to consider un-autofix-able.

**Default value**: `[]`

**Type**: `Vec<CheckCodePrefix>`

**Example usage**:

```toml
[tool.ruff]
# Disable autofix for unused imports (`F401`).
unfixable = ["F401"]
```

---

### `flake8-annotations`

#### [`allow-star-arg-any`](#allow-star-arg-any)

Whether to suppress `ANN401` for dynamically typed `*args` and `**kwargs` arguments.

**Default value**: `false`

**Type**: `bool`

**Example usage**:

```toml
[tool.ruff.flake8-annotations]
allow-star-arg-any = true
```

---

#### [`mypy-init-return`](#mypy-init-return)

Whether to allow the omission of a return type hint for `__init__` if at least one
argument is annotated.

**Default value**: `false`

**Type**: `bool`

**Example usage**:

```toml
[tool.ruff.flake8-annotations]
mypy-init-return = true
```

---

#### [`suppress-dummy-args`](#suppress-dummy-args)

Whether to suppress `ANN000`-level errors for arguments matching the "dummy" variable
regex (like `_`).

**Default value**: `false`

**Type**: `bool`

**Example usage**:

```toml
[tool.ruff.flake8-annotations]
suppress-dummy-args = true
```

---

#### [`suppress-none-returning`](#suppress-none-returning)

Whether to suppress `ANN200`-level errors for functions that meet either of the
following criteria:

- Contain no `return` statement.
- Explicit `return` statement(s) all return `None` (explicitly or implicitly).

**Default value**: `false`

**Type**: `bool`

**Example usage**:

```toml
[tool.ruff.flake8-annotations]
suppress-none-returning = true
```

---

### `flake8-bugbear`

#### [`extend-immutable-calls`](#extend-immutable-calls)

Additional callable functions to consider "immutable" when evaluating, e.g.,
`no-mutable-default-argument` checks (`B006`).

**Default value**: `[]`

**Type**: `Vec<String>`

**Example usage**:

```toml
[tool.ruff.flake8-bugbear]
# Allow default arguments like, e.g., `data: List[str] = fastapi.Query(None)`.
extend-immutable-calls = ["fastapi.Depends", "fastapi.Query"]
```

---

### `flake8-errmsg`

#### [`max-string-length`](#max-string-length)

Maximum string length for string literals in exception messages.

**Default value**: `0`

**Type**: `usize`

**Example usage**:

```toml
[tool.ruff.flake8-errmsg]
max-string-length = 20
```

---

### `flake8-import-conventions`

#### [`aliases`](#aliases)

The conventional aliases for imports. These aliases can be extended by the `extend_aliases` option.

**Default value**: `{"altair": "alt", "matplotlib.pyplot": "plt", "numpy": "np", "pandas": "pd", "seaborn": "sns"}`

**Type**: `FxHashMap<String, String>`

**Example usage**:

```toml
[tool.ruff.flake8-import-conventions]
# Declare the default aliases.
altair = "alt"
matplotlib.pyplot = "plt"
numpy = "np"
pandas = "pd"
seaborn = "sns"
```

---

#### [`extend-aliases`](#extend-aliases)

A mapping of modules to their conventional import aliases. These aliases will be added to the `aliases` mapping.

**Default value**: `{}`

**Type**: `FxHashMap<String, String>`

**Example usage**:

```toml
[tool.ruff.flake8-import-conventions]
# Declare a custom alias for the `matplotlib` module.
"dask.dataframe" = "dd"
```

---

### `flake8-quotes`

#### [`avoid-escape`](#avoid-escape)

Whether to avoid using single quotes if a string contains single quotes, or vice-versa
with double quotes, as per [PEP8](https://peps.python.org/pep-0008/#string-quotes).
This minimizes the need to escape quotation marks within strings.

**Default value**: `true`

**Type**: `bool`

**Example usage**:

```toml
[tool.ruff.flake8-quotes]
# Don't bother trying to avoid escapes.
avoid-escape = false
```

---

#### [`docstring-quotes`](#docstring-quotes)

Quote style to prefer for docstrings (either "single" (`'`) or "double" (`"`)).

**Default value**: `"double"`

**Type**: `Quote`

**Example usage**:

```toml
[tool.ruff.flake8-quotes]
docstring-quotes = "single"
```

---

#### [`inline-quotes`](#inline-quotes)

Quote style to prefer for inline strings (either "single" (`'`) or "double" (`"`)).

**Default value**: `"double"`

**Type**: `Quote`

**Example usage**:

```toml
[tool.ruff.flake8-quotes]
inline-quotes = "single"
```

---

#### [`multiline-quotes`](#multiline-quotes)

Quote style to prefer for multiline strings (either "single" (`'`) or "double" (`"`)).

**Default value**: `"double"`

**Type**: `Quote`

**Example usage**:

```toml
[tool.ruff.flake8-quotes]
multiline-quotes = "single"
```

---

### `flake8-tidy-imports`

#### [`ban-relative-imports`](#ban-relative-imports)

Whether to ban all relative imports (`"all"`), or only those imports that extend into
the parent module and beyond (`"parents"`).

**Default value**: `"parents"`

**Type**: `Strictness`

**Example usage**:

```toml
[tool.ruff.flake8-tidy-imports]
# Disallow all relative imports.
ban-relative-imports = "all"
```

---

### `flake8-unused-arguments`

#### [`ignore-variadic-names`](#ignore-variadic-names)

Whether to allow unused variadic arguments, like `*args` and `**kwargs`.

**Default value**: `false`

**Type**: `bool`

**Example usage**:

```toml
[tool.ruff.flake8-unused-arguments]
ignore-variadic-names = true
```

---

### `isort`

#### [`combine-as-imports`](#combine-as-imports)

Combines as imports on the same line. See isort's [`combine-as-imports`](https://pycqa.github.io/isort/docs/configuration/options.html#combine-as-imports)
option.

**Default value**: `false`

**Type**: `bool`

**Example usage**:

```toml
[tool.ruff.isort]
combine-as-imports = true
```

---

#### [`extra-standard-library`](#extra-standard-library)

A list of modules to consider standard-library, in addition to those known to Ruff in
advance.

**Default value**: `[]`

**Type**: `Vec<String>`

**Example usage**:

```toml
[tool.ruff.isort]
extra-standard-library = ["path"]
```

---

#### [`force-wrap-aliases`](#force-wrap-aliases)

Force `import from` statements with multiple members and at least one alias (e.g.,
`import A as B`) to wrap such that every line contains exactly one member. For example,
this formatting would be retained, rather than condensing to a single line:

```py
from .utils import (
    test_directory as test_directory,
    test_id as test_id
)
```

Note that this setting is only effective when combined with `combine-as-imports = true`.
When `combine-as-imports` isn't enabled, every aliased `import from` will be given its
own line, in which case, wrapping is not necessary.

**Default value**: `false`

**Type**: `bool`

**Example usage**:

```toml
[tool.ruff.isort]
force-wrap-aliases = true
combine-as-imports = true
```

---

#### [`known-first-party`](#known-first-party)

A list of modules to consider first-party, regardless of whether they can be identified
as such via introspection of the local filesystem.

**Default value**: `[]`

**Type**: `Vec<String>`

**Example usage**:

```toml
[tool.ruff.isort]
known-first-party = ["src"]
```

---

#### [`known-third-party`](#known-third-party)

A list of modules to consider third-party, regardless of whether they can be identified
as such via introspection of the local filesystem.

**Default value**: `[]`

**Type**: `Vec<String>`

**Example usage**:

```toml
[tool.ruff.isort]
known-third-party = ["src"]
```

---

### `mccabe`

#### [`max-complexity`](#max-complexity)

The maximum McCabe complexity to allow before triggering `C901` errors.

**Default value**: `10`

**Type**: `usize`

**Example usage**:

```toml
[tool.ruff.mccabe]
# Flag errors (`C901`) whenever the complexity level exceeds 5.
max-complexity = 5
```

---

### `pep8-naming`

#### [`classmethod-decorators`](#classmethod-decorators)

A list of decorators that, when applied to a method, indicate that the method should be
treated as a class method. For example, Ruff will expect that any method decorated by a
decorator in this list takes a `cls` argument as its first argument.

**Default value**: `["classmethod"]`

**Type**: `Vec<String>`

**Example usage**:

```toml
[tool.ruff.pep8-naming]
# Allow Pydantic's `@validator` decorator to trigger class method treatment.
classmethod-decorators = ["classmethod", "pydantic.validator"]
```

---

#### [`ignore-names`](#ignore-names)

A list of names to ignore when considering `pep8-naming` violations.

**Default value**: `["setUp", "tearDown", "setUpClass", "tearDownClass", "setUpModule", "tearDownModule", "asyncSetUp", "asyncTearDown", "setUpTestData", "failureException", "longMessage", "maxDiff"]`

**Type**: `Vec<String>`

**Example usage**:

```toml
[tool.ruff.pep8-naming]
ignore-names = ["callMethod"]
```

---

#### [`staticmethod-decorators`](#staticmethod-decorators)

A list of decorators that, when applied to a method, indicate that the method should be
treated as a static method. For example, Ruff will expect that any method decorated by a
decorator in this list has no `self` or `cls` argument.

**Default value**: `["staticmethod"]`

**Type**: `Vec<String>`

**Example usage**:

```toml
[tool.ruff.pep8-naming]
# Allow a shorthand alias, `@stcmthd`, to trigger static method treatment.
staticmethod-decorators = ["staticmethod", "stcmthd"]
```

---

### `pyupgrade`

#### [`keep-runtime-typing`](#keep-runtime-typing)

Whether to avoid PEP 585 (`List[int]` -> `list[int]`) and PEP 604 (`Optional[str]` -> `str | None`) rewrites even if a file imports `from __future__ import annotations`. Note that this setting is only applicable when the target Python version is below 3.9 and 3.10 respectively.

**Default value**: `false`

**Type**: `bool`

**Example usage**:

```toml
[tool.ruff.pyupgrade]
# Preserve types, even if a file imports `from __future__ import annotations`.
keep-runtime-typing = true
```

---

<!-- End auto-generated options sections. -->

## License

MIT

## Contributing

Contributions are welcome and hugely appreciated. To get started, check out the
[contributing guidelines](https://github.com/charliermarsh/ruff/blob/main/CONTRIBUTING.md).
