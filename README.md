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

- ⚡️ 10-100x faster than existing linters
- 🐍 Installable via `pip`
- 🤝 Python 3.10 compatibility
- 🛠️ `pyproject.toml` support
- 📦 [ESLint](https://eslint.org/docs/latest/user-guide/command-line-interface#caching)-inspired cache support
- 🔧 [ESLint](https://eslint.org/docs/latest/user-guide/command-line-interface#--fix)-inspired autofix support (e.g., automatically remove unused imports)
- 👀 [TypeScript](https://www.typescriptlang.org/docs/handbook/configuring-watch.html)-inspired `--watch` support, for continuous file monitoring
- ⚖️ [Near-parity](#how-does-ruff-compare-to-flake8) with the built-in Flake8 rule set
- 🔌 Native re-implementations of popular Flake8 plugins, like [`flake8-docstrings`](https://pypi.org/project/flake8-docstrings/) ([`pydocstyle`](https://pypi.org/project/pydocstyle/))

Ruff aims to be orders of magnitude faster than alternative tools while integrating more
functionality behind a single, common interface. Ruff can be used to replace Flake8 (plus a variety
of plugins), [`pydocstyle`](https://pypi.org/project/pydocstyle/), [`yesqa`](https://github.com/asottile/yesqa),
and even a subset of [`pyupgrade`](https://pypi.org/project/pyupgrade/) and [`autoflake`](https://pypi.org/project/autoflake/)
all while executing tens or hundreds of times faster than any individual tool.

Read the [launch blog post](https://notes.crmarsh.com/python-tooling-could-be-much-much-faster).

## Table of Contents

1. [Installation and Usage](#installation-and-usage)
2. [Configuration](#configuration)
3. [Supported Rules](#supported-rules)
   1. [Pyflakes](#pyflakes)
   2. [pycodestyle (error)](#pycodestyle-error)
   3. [pycodestyle (warning)](#pycodestyle-warning)
   4. [pydocstyle](#pydocstyle)
   5. [pyupgrade](#pyupgrade)
   6. [pep8-naming](#pep8-naming)
   7. [flake8-comprehensions](#flake8-comprehensions)
   8. [flake8-bugbear](#flake8-bugbear)
   9. [flake8-builtins](#flake8-builtins)
   10. [flake8-print](#flake8-print)
   11. [flake8-quotes](#flake8-quotes)
   12. [Meta rules](#meta-rules)
5. [Editor Integrations](#editor-integrations)
6. [FAQ](#faq)
7. [Development](#development)
8. [Releases](#releases)
9. [Benchmarks](#benchmarks)
10. [License](#license)
11. [Contributing](#contributing)

## Installation and Usage

### Installation

Available as [`ruff`](https://pypi.org/project/ruff/) on PyPI:

```shell
pip install ruff
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
    rev: v0.0.94
    hooks:
      - id: ruff
```

<!-- TODO(charlie): Remove this message a few versions after v0.0.86. -->
_Note: prior to `v0.0.86`, `ruff-pre-commit` used `lint` (rather than `ruff`) as the hook ID._

## Configuration

Ruff is configurable both via `pyproject.toml` and the command line.

For example, you could configure Ruff to only enforce a subset of rules with:

```toml
[tool.ruff]
line-length = 88
select = ["E", "F"]
ignore = ["E501"]
per-file-ignores = [
    "__init__.py:F401",
    "path/to/file.py:F401"
]
```

Plugin configurations should be expressed as subsections, e.g.:

```toml
[tool.ruff]
line-length = 88

[tool.ruff.flake8-quotes]
docstring-quotes = "double"
```

Alternatively, common configuration settings can be provided via the command-line:

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
      --config <CONFIG>
          Path to the `pyproject.toml` file to use for configuration
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
      --stdin-filename <STDIN_FILENAME>
          The name of the file when passing it through stdin
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

To ignore an error in-line, Ruff uses a `noqa` system similar to [Flake8](https://flake8.pycqa.org/en/3.1.1/user/ignoring-errors.html).
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

Ruff supports several workflows to aid in `noqa` management.

First, Ruff provides a special error code, `M001`, to enforce that your `noqa` directives are
"valid", in that the errors they _say_ they ignore are actually being triggered on that line (and
thus suppressed). **You can run `ruff /path/to/file.py --extend-select M001` to flag unused `noqa`
directives.**

Second, Ruff can _automatically remove_ unused `noqa` directives via its autofix functionality.
**You can run `ruff /path/to/file.py --extend-select M001 --fix` to automatically remove unused
`noqa` directives.**

Third, Ruff can _automatically add_ `noqa` directives to all failing lines. This is useful when
migrating a new codebase to Ruff. **You can run `ruff /path/to/file.py --add-noqa` to automatically
add `noqa` directives to all failing lines, with the appropriate error codes.**

## Supported Rules

By default, Ruff enables all `E` and `F` error codes, which correspond to those built-in to Flake8.

The 🛠 emoji indicates that a rule is automatically fixable by the `--fix` command-line option.

### Pyflakes

| Code | Name | Message | Fix |
| ---- | ---- | ------- | --- |
| F401 | UnusedImport | `...` imported but unused | 🛠 |
| F402 | ImportShadowedByLoopVar | Import `...` from line 1 shadowed by loop variable |  |
| F403 | ImportStarUsed | `from ... import *` used; unable to detect undefined names |  |
| F404 | LateFutureImport | `from __future__` imports must occur at the beginning of the file |  |
| F405 | ImportStarUsage | `...` may be undefined, or defined from star imports: `...` |  |
| F406 | ImportStarNotPermitted | `from ... import *` only allowed at module level |  |
| F407 | FutureFeatureNotDefined | Future feature `...` is not defined |  |
| F541 | FStringMissingPlaceholders | f-string without any placeholders |  |
| F601 | MultiValueRepeatedKeyLiteral | Dictionary key literal repeated |  |
| F602 | MultiValueRepeatedKeyVariable | Dictionary key `...` repeated |  |
| F621 | ExpressionsInStarAssignment | Too many expressions in star-unpacking assignment |  |
| F622 | TwoStarredExpressions | Two starred expressions in assignment |  |
| F631 | AssertTuple | Assert test is a non-empty tuple, which is always `True` |  |
| F632 | IsLiteral | Use `==` and `!=` to compare constant literals |  |
| F633 | InvalidPrintSyntax | Use of `>>` is invalid with `print` function |  |
| F634 | IfTuple | If test is a tuple, which is always `True` |  |
| F701 | BreakOutsideLoop | `break` outside loop |  |
| F702 | ContinueOutsideLoop | `continue` not properly in loop |  |
| F704 | YieldOutsideFunction | `yield` or `yield from` statement outside of a function |  |
| F706 | ReturnOutsideFunction | `return` statement outside of a function/method |  |
| F707 | DefaultExceptNotLast | An `except:` block as not the last exception handler |  |
| F722 | ForwardAnnotationSyntaxError | Syntax error in forward annotation: `...` |  |
| F821 | UndefinedName | Undefined name `...` |  |
| F822 | UndefinedExport | Undefined name `...` in `__all__` |  |
| F823 | UndefinedLocal | Local variable `...` referenced before assignment |  |
| F831 | DuplicateArgumentName | Duplicate argument name in function definition |  |
| F841 | UnusedVariable | Local variable `...` is assigned to but never used |  |
| F901 | RaiseNotImplemented | `raise NotImplemented` should be `raise NotImplementedError` |  |

### pycodestyle (error)

| Code | Name | Message | Fix |
| ---- | ---- | ------- | --- |
| E402 | ModuleImportNotAtTopOfFile | Module level import not at top of file |  |
| E501 | LineTooLong | Line too long (89 > 88 characters) |  |
| E711 | NoneComparison | Comparison to `None` should be `cond is None` |  |
| E712 | TrueFalseComparison | Comparison to `True` should be `cond is True` |  |
| E713 | NotInTest | Test for membership should be `not in` |  |
| E714 | NotIsTest | Test for object identity should be `is not` |  |
| E721 | TypeComparison | Do not compare types, use `isinstance()` |  |
| E722 | DoNotUseBareExcept | Do not use bare `except` |  |
| E731 | DoNotAssignLambda | Do not assign a lambda expression, use a def |  |
| E741 | AmbiguousVariableName | Ambiguous variable name: `...` |  |
| E742 | AmbiguousClassName | Ambiguous class name: `...` |  |
| E743 | AmbiguousFunctionName | Ambiguous function name: `...` |  |
| E902 | IOError | IOError: `...` |  |
| E999 | SyntaxError | SyntaxError: `...` |  |

### pycodestyle (warning)

| Code | Name | Message | Fix |
| ---- | ---- | ------- | --- |
| W292 | NoNewLineAtEndOfFile | No newline at end of file |  |
| W605 | InvalidEscapeSequence | Invalid escape sequence: '\c' |  |

### pydocstyle

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
| D400 | EndsInPeriod | First line should end with a period |  |
| D402 | NoSignature | First line should not be the function's signature |  |
| D403 | FirstLineCapitalized | First word of the first line should be properly capitalized |  |
| D404 | NoThisPrefix | First word of the docstring should not be 'This' |  |
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
| D415 | EndsInPunctuation | First line should end with a period, question mark, or exclamation point |  |
| D416 | SectionNameEndsInColon | Section name should end with a colon ("Returns") | 🛠 |
| D417 | DocumentAllArguments | Missing argument descriptions in the docstring: `x`, `y` |  |
| D418 | SkipDocstring | Function decorated with `@overload` shouldn't contain a docstring |  |
| D419 | NonEmpty | Docstring is empty |  |

### pyupgrade

| Code | Name | Message | Fix |
| ---- | ---- | ------- | --- |
| U001 | UselessMetaclassType | `__metaclass__ = type` is implied | 🛠 |
| U002 | UnnecessaryAbspath | `abspath(__file__)` is unnecessary in Python 3.9 and later | 🛠 |
| U003 | TypeOfPrimitive | Use `str` instead of `type(...)` | 🛠 |
| U004 | UselessObjectInheritance | Class `...` inherits from object | 🛠 |
| U005 | DeprecatedUnittestAlias | `assertEquals` is deprecated, use `assertEqual` instead | 🛠 |
| U006 | UsePEP585Annotation | Use `list` instead of `List` for type annotations | 🛠 |
| U007 | UsePEP604Annotation | Use `X \| Y` for type annotations | 🛠 |
| U008 | SuperCallWithParameters | Use `super()` instead of `super(__class__, self)` | 🛠 |

### pep8-naming

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

### flake8-comprehensions

| Code | Name | Message | Fix |
| ---- | ---- | ------- | --- |
| C400 | UnnecessaryGeneratorList | Unnecessary generator (rewrite as a `list` comprehension) |  |
| C401 | UnnecessaryGeneratorSet | Unnecessary generator (rewrite as a `set` comprehension) |  |
| C402 | UnnecessaryGeneratorDict | Unnecessary generator (rewrite as a `dict` comprehension) |  |
| C403 | UnnecessaryListComprehensionSet | Unnecessary `list` comprehension (rewrite as a `set` comprehension) |  |
| C404 | UnnecessaryListComprehensionDict | Unnecessary `list` comprehension (rewrite as a `dict` comprehension) |  |
| C405 | UnnecessaryLiteralSet | Unnecessary `(list\|tuple)` literal (rewrite as a `set` literal) |  |
| C406 | UnnecessaryLiteralDict | Unnecessary `(list\|tuple)` literal (rewrite as a `dict` literal) |  |
| C408 | UnnecessaryCollectionCall | Unnecessary `(dict\|list\|tuple)` call (rewrite as a literal) |  |
| C409 | UnnecessaryLiteralWithinTupleCall | Unnecessary `(list\|tuple)` literal passed to `tuple()` (remove the outer call to `tuple()`) |  |
| C410 | UnnecessaryLiteralWithinListCall | Unnecessary `(list\|tuple)` literal passed to `list()` (rewrite as a `list` literal) |  |
| C411 | UnnecessaryListCall | Unnecessary `list` call (remove the outer call to `list()`) |  |
| C413 | UnnecessaryCallAroundSorted | Unnecessary `(list\|reversed)` call around `sorted()` |  |
| C414 | UnnecessaryDoubleCastOrProcess | Unnecessary `(list\|reversed\|set\|sorted\|tuple)` call within `(list\|set\|sorted\|tuple)()` |  |
| C415 | UnnecessarySubscriptReversal | Unnecessary subscript reversal of iterable within `(reversed\|set\|sorted)()` |  |
| C416 | UnnecessaryComprehension | Unnecessary `(list\|set)` comprehension (rewrite using `(list\|set)()`) |  |
| C417 | UnnecessaryMap | Unnecessary `map` usage (rewrite using a `(list\|set\|dict)` comprehension) |  |

### flake8-bugbear

| Code | Name | Message | Fix |
| ---- | ---- | ------- | --- |
| B002 | UnaryPrefixIncrement | Python does not support the unary prefix increment. |  |
| B006 | MutableArgumentDefault | Do not use mutable data structures for argument defaults. |  |
| B007 | UnusedLoopControlVariable | Loop control variable `i` not used within the loop body. | 🛠 |
| B011 | DoNotAssertFalse | Do not `assert False` (`python -O` removes these calls), raise `AssertionError()` | 🛠 |
| B013 | RedundantTupleInExceptionHandler | A length-one tuple literal is redundant. Write `except ValueError:` instead of `except (ValueError,):`. |  |
| B014 | DuplicateHandlerException | Exception handler with duplicate exception: `ValueError` | 🛠 |
| B017 | NoAssertRaisesException | `assertRaises(Exception):` should be considered evil. |  |
| B025 | DuplicateTryBlockException | try-except block with duplicate exception `Exception` |  |

### flake8-builtins

| Code | Name | Message | Fix |
| ---- | ---- | ------- | --- |
| A001 | BuiltinVariableShadowing | Variable `...` is shadowing a python builtin |  |
| A002 | BuiltinArgumentShadowing | Argument `...` is shadowing a python builtin |  |
| A003 | BuiltinAttributeShadowing | Class attribute `...` is shadowing a python builtin |  |

### flake8-print

| Code | Name | Message | Fix |
| ---- | ---- | ------- | --- |
| T201 | PrintFound | `print` found | 🛠 |
| T203 | PPrintFound | `pprint` found | 🛠 |

### flake8-quotes

| Code | Name | Message | Fix |
| ---- | ---- | ------- | --- |
| Q000 | BadQuotesInlineString | Single quotes found but double quotes preferred |  |
| Q001 | BadQuotesMultilineString | Single quote multiline found but double quotes preferred |  |
| Q002 | BadQuotesDocstring | Single quote docstring found but double quotes preferred |  |
| Q003 | AvoidQuoteEscape | Change outer quotes to avoid escaping inner quotes |  |

### Meta rules

| Code | Name | Message | Fix |
| ---- | ---- | ------- | --- |
| M001 | UnusedNOQA | Unused `noqa` directive | 🛠 |

## Editor Integrations

### PyCharm

Ruff can be installed as an [External Tool](https://www.jetbrains.com/help/pycharm/configuring-third-party-tools.html)
in PyCharm. Open the Preferences pane, then navigate to "Tools", then "External Tools". From there,
add a new tool with the following configuration:

![Install Ruff as an External Tool](https://user-images.githubusercontent.com/1309177/193155720-336e43f0-1a8d-46b4-bc12-e60f9ae01f7e.png)

Ruff should then appear as a runnable action:

![Ruff as a runnable action](https://user-images.githubusercontent.com/1309177/193156026-732b0aaf-3dd9-4549-9b4d-2de6d2168a33.png)

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

Ruff can be used as a (near) drop-in replacement for Flake8 when used (1) without or with a small
number of plugins, (2) alongside Black, and (3) on Python 3 code.

Under those conditions Ruff is missing 14 rules related to string `.format` calls, 1 rule related
to docstring parsing, and 1 rule related to redefined variables.

Ruff re-implements some of the most popular Flake8 plugins and related code quality tools natively,
including:

- [`pydocstyle`](https://pypi.org/project/pydocstyle/)
- [`pep8-naming`](https://pypi.org/project/pep8-naming/)
- [`yesqa`](https://github.com/asottile/yesqa)
- [`flake8-docstrings`](https://pypi.org/project/flake8-docstrings/)
- [`flake8-builtins`](https://pypi.org/project/flake8-builtins/)
- [`flake8-super`](https://pypi.org/project/flake8-super/)
- [`flake8-print`](https://pypi.org/project/flake8-print/)
- [`flake8-quotes`](https://pypi.org/project/flake8-quotes/)
- [`flake8-comprehensions`](https://pypi.org/project/flake8-comprehensions/)
- [`flake8-bugbear`](https://pypi.org/project/flake8-bugbear/) (10/32)
- [`pyupgrade`](https://pypi.org/project/pyupgrade/) (8/34)
- [`autoflake`](https://pypi.org/project/autoflake/) (1/7)

Beyond rule-set parity, Ruff suffers from the following limitations vis-à-vis Flake8:

1. Ruff does not yet support a few Python 3.9 and 3.10 language features, including structural
   pattern matching and parenthesized context managers.
2. Flake8 has a plugin architecture and supports writing custom lint rules. (To date, popular Flake8
   plugins have been re-implemented within Ruff directly.)

### Which tools does Ruff replace?

Today, Ruff can be used to replace Flake8 when used with any of the following plugins:

- [`pep8-naming`](https://pypi.org/project/pep8-naming/)
- [`flake8-docstrings`](https://pypi.org/project/flake8-docstrings/)
- [`flake8-builtins`](https://pypi.org/project/flake8-builtins/)
- [`flake8-super`](https://pypi.org/project/flake8-super/)
- [`flake8-print`](https://pypi.org/project/flake8-print/)
- [`flake8-quotes`](https://pypi.org/project/flake8-quotes/)
- [`flake8-comprehensions`](https://pypi.org/project/flake8-comprehensions/)
- [`flake8-bugbear`](https://pypi.org/project/flake8-bugbear/) (10/32)

Ruff also implements the functionality that you get from [`yesqa`](https://github.com/asottile/yesqa),
and a subset of the rules implemented in [`pyupgrade`](https://pypi.org/project/pyupgrade/) (8/34).

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

Ruff is written in Rust (1.64.0). You'll need to install the [Rust toolchain](https://www.rust-lang.org/tools/install)
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

## License

MIT

## Contributing

Contributions are welcome and hugely appreciated. To get started, check out the
[contributing guidelines](https://github.com/charliermarsh/ruff/blob/main/CONTRIBUTING.md).
