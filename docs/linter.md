# The Ruff Linter

The Ruff Linter is an extremely fast Python linter designed as a drop-in replacement for [Flake8](https://pypi.org/project/flake8/)
(plus dozens of plugins), [isort](https://pypi.org/project/isort/), [pydocstyle](https://pypi.org/project/pydocstyle/),
[pyupgrade](https://pypi.org/project/pyupgrade/), [autoflake](https://pypi.org/project/autoflake/),
and more.

## `ruff check`

`ruff check` is the primary entrypoint to the Ruff linter. It accepts a list of files or
directories, and lints all discovered Python files, optionally fixing any fixable errors:

```shell
ruff check                  # Lint all files in the current directory.
ruff check --fix            # Lint all files in the current directory, and fix any fixable errors.
ruff check --watch          # Lint all files in the current directory, and re-lint on change.
ruff check path/to/code/    # Lint all files in `path/to/code` (and any subdirectories).
```

For the full list of supported options, run `ruff check --help`.

!!! note
    As of Ruff v0.1.7 the `ruff check` command uses the current working directory (`.`) as the default path to check.
    On older versions, you must provide this manually e.g. `ruff check .`.
    See [the file discovery documentation](configuration.md#python-file-discovery) for details.

## Rule selection

The set of enabled rules is controlled via the [`lint.select`](settings.md#lint_select),
[`lint.extend-select`](settings.md#lint_extend-select), and [`lint.ignore`](settings.md#lint_ignore) settings.

Ruff's linter mirrors Flake8's rule code system, in which each rule code consists of a one-to-three
letter prefix, followed by three digits (e.g., `F401`). The prefix indicates that "source" of the rule
(e.g., `F` for Pyflakes, `E` for pycodestyle, `ANN` for flake8-annotations).

Rule selectors like [`lint.select`](settings.md#lint_select) and [`lint.ignore`](settings.md#lint_ignore) accept either
a full rule code (e.g., `F401`) or any valid prefix (e.g., `F`). For example, given the following
configuration file:

=== "pyproject.toml"

    ```toml
    [tool.ruff.lint]
    select = ["E", "F"]
    ignore = ["F401"]
    ```

=== "ruff.toml"

    ```toml
    [lint]
    select = ["E", "F"]
    ignore = ["F401"]
    ```

Ruff would enable all rules with the `E` (pycodestyle) or `F` (Pyflakes) prefix, with the exception
of `F401`. For more on configuring Ruff via `pyproject.toml`, see [_Configuring Ruff_](configuration.md).

As a special-case, Ruff also supports the `ALL` code, which enables all rules. Note that some
pydocstyle rules conflict (e.g., `D203` and `D211`) as they represent alternative docstring
formats. Ruff will automatically disable any conflicting rules when `ALL` is enabled.

If you're wondering how to configure Ruff, here are some **recommended guidelines**:

- Prefer [`lint.select`](settings.md#lint_select) over [`lint.extend-select`](settings.md#lint_extend-select) to make your rule set explicit.
- Use `ALL` with discretion. Enabling `ALL` will implicitly enable new rules whenever you upgrade.
- Start with a small set of rules (`select = ["E", "F"]`) and add a category at-a-time. For example,
    you might consider expanding to `select = ["E", "F", "B"]` to enable the popular flake8-bugbear
    extension.

For example, a configuration that enables some of the most popular rules (without being too
pedantic) might look like the following:

=== "pyproject.toml"

    ```toml
    [tool.ruff.lint]
    select = [
        # pycodestyle
        "E",
        # Pyflakes
        "F",
        # pyupgrade
        "UP",
        # flake8-bugbear
        "B",
        # flake8-simplify
        "SIM",
        # isort
        "I",
    ]
    ```

=== "ruff.toml"

    ```toml
    [lint]
    select = [
        # pycodestyle
        "E",
        # Pyflakes
        "F",
        # pyupgrade
        "UP",
        # flake8-bugbear
        "B",
        # flake8-simplify
        "SIM",
        # isort
        "I",
    ]
    ```

To resolve the enabled rule set, Ruff may need to reconcile [`lint.select`](settings.md#lint_select) and
[`lint.ignore`](settings.md#lint_ignore) from a variety of sources, including the current `pyproject.toml`,
any inherited `pyproject.toml` files, and the CLI (e.g., [`--select`](settings.md#lint_select)).

In those scenarios, Ruff uses the "highest-priority" [`select`](settings.md#lint_select) as the basis for
the rule set, and then applies [`extend-select`](settings.md#lint_extend-select) and
[`ignore`](settings.md#lint_ignore) adjustments. CLI options are given higher priority than
`pyproject.toml` options, and the current `pyproject.toml` file is given higher priority than any
inherited `pyproject.toml` files.

For example, given the following configuration file:

=== "pyproject.toml"

    ```toml
    [tool.ruff.lint]
    select = ["E", "F"]
    ignore = ["F401"]
    ```

=== "ruff.toml"

    ```toml
    [lint]
    select = ["E", "F"]
    ignore = ["F401"]
    ```

Running `ruff check --select F401` would result in Ruff enforcing `F401`, and no other rules.

Running `ruff check --extend-select B` would result in Ruff enforcing the `E`, `F`, and `B` rules,
with the exception of `F401`.

## Fixes

Ruff supports automatic fixes for a variety of lint errors. For example, Ruff can remove unused
imports, reformat docstrings, rewrite type annotations to use newer Python syntax, and more.

To enable fixes, pass the `--fix` flag to `ruff check`:

```shell
ruff check --fix
```

By default, Ruff will fix all violations for which safe fixes are available; to determine
whether a rule supports fixing, see [_Rules_](rules.md).

### Fix safety

Ruff labels fixes as "safe" and "unsafe". The meaning and intent of your code will be retained when
applying safe fixes, but the meaning could be changed when applying unsafe fixes.

For example, [`unnecessary-iterable-allocation-for-first-element`](rules/unnecessary-iterable-allocation-for-first-element.md)
(`RUF015`) is a rule which checks for potentially unperformant use of `list(...)[0]`. The fix
replaces this pattern with `next(iter(...))` which can result in a drastic speedup:

```shell
$ python -m timeit "head = list(range(99999999))[0]"
1 loop, best of 5: 1.69 sec per loop
```

```shell
$ python -m timeit "head = next(iter(range(99999999)))"
5000000 loops, best of 5: 70.8 nsec per loop
```

However, when the collection is empty, this changes the raised exception from an `IndexError` to `StopIteration`:

```shell
$ python -c 'list(range(0))[0]'
Traceback (most recent call last):
  File "<string>", line 1, in <module>
IndexError: list index out of range
```

```shell
$ python -c 'next(iter(range(0)))[0]'
Traceback (most recent call last):
  File "<string>", line 1, in <module>
StopIteration
```

Since this could break error handling, this fix is categorized as unsafe.

Ruff only enables safe fixes by default. Unsafe fixes can be enabled by settings [`unsafe-fixes`](settings.md#unsafe-fixes) in your configuration file or passing the `--unsafe-fixes` flag to `ruff check`:

```shell
# Show unsafe fixes
ruff check --unsafe-fixes

# Apply unsafe fixes
ruff check --fix --unsafe-fixes
```

By default, Ruff will display a hint when unsafe fixes are available but not enabled. The suggestion can be silenced
by setting the [`unsafe-fixes`](settings.md#unsafe-fixes) setting to `false` or using the `--no-unsafe-fixes` flag.

The safety of fixes can be adjusted per rule using the [`lint.extend-safe-fixes`](settings.md#lint_extend-safe-fixes) and [`lint.extend-unsafe-fixes`](settings.md#lint_extend-unsafe-fixes) settings.

For example, the following configuration would promote unsafe fixes for `F601` to safe fixes and demote safe fixes for `UP034` to unsafe fixes:

=== "pyproject.toml"

    ```toml
    [tool.ruff.lint]
    extend-safe-fixes = ["F601"]
    extend-unsafe-fixes = ["UP034"]
    ```

=== "ruff.toml"

    ```toml
    [lint]
    extend-safe-fixes = ["F601"]
    extend-unsafe-fixes = ["UP034"]
    ```

You may use prefixes to select rules as well, e.g., `F` can be used to promote fixes for all rules in Pyflakes to safe.

!!! note
    All fixes will always be displayed by Ruff when using the `json` output format. The safety of each fix is available under the `applicability` field.

### Disabling fixes

To limit the set of rules that Ruff should fix, use the [`lint.fixable`](settings.md#lint_fixable) and
[`lint.unfixable`](settings.md#lint_unfixable) settings, along with their [`lint.extend-fixable`](settings.md#lint_extend-fixable)
and [`lint.extend-unfixable`](settings.md#lint_extend-unfixable) variants.

For example, the following configuration would enable fixes for all rules except
[`unused-imports`](rules/unused-import.md) (`F401`):

=== "pyproject.toml"

    ```toml
    [tool.ruff.lint]
    fixable = ["ALL"]
    unfixable = ["F401"]
    ```

=== "ruff.toml"

    ```toml
    [lint]
    fixable = ["ALL"]
    unfixable = ["F401"]
    ```

Conversely, the following configuration would only enable fixes for `F401`:

=== "pyproject.toml"

    ```toml
    [tool.ruff.lint]
    fixable = ["F401"]
    ```

=== "ruff.toml"

    ```toml
    [lint]
    fixable = ["F401"]
    ```

## Error suppression

Ruff supports several mechanisms for suppressing lint errors, be they false positives or
permissible violations.

To omit a lint rule entirely, add it to the "ignore" list via the [`lint.ignore`](settings.md#lint_ignore)
setting, either on the command-line or in your `pyproject.toml` or `ruff.toml` file.

To suppress a violation inline, Ruff uses a `noqa` system similar to [Flake8](https://flake8.pycqa.org/en/3.1.1/user/ignoring-errors.html).
To ignore an individual violation, add `# noqa: {code}` to the end of the line, like so:

```python
# Ignore F841.
x = 1  # noqa: F841

# Ignore E741 and F841.
i = 1  # noqa: E741, F841

# Ignore _all_ violations.
x = 1  # noqa
```

For multi-line strings (like docstrings), the `noqa` directive should come at the end of the string
(after the closing triple quote), and will apply to the entire string, like so:

```python
"""Lorem ipsum dolor sit amet.

Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor.
"""  # noqa: E501
```

For import sorting, the `noqa` should come at the end of the first line in the import block, and
will apply to all imports in the block, like so:

```python
import os  # noqa: I001
import abc
```

To ignore all violations across an entire file, add the line `# ruff: noqa` anywhere in the file,
preferably towards the top, like so:

```python
# ruff: noqa
```

To ignore a specific rule across an entire file, add the line `# ruff: noqa: {code}` anywhere in the
file, preferably towards the top, like so:

```python
# ruff: noqa: F841
```

Or see the [`lint.per-file-ignores`](settings.md#lint_per-file-ignores) setting, which enables the same
functionality from within your `pyproject.toml` or `ruff.toml` file.

Global `noqa` comments must be on their own line to disambiguate from comments which ignore
violations on a single line.

Note that Ruff will also respect Flake8's `# flake8: noqa` directive, and will treat it as
equivalent to `# ruff: noqa`.

### Detecting unused suppression comments

Ruff implements a special rule, [`unused-noqa`](https://docs.astral.sh/ruff/rules/unused-noqa/),
under the `RUF100` code, to enforce that your `noqa` directives are "valid", in that the violations
they _say_ they ignore are actually being triggered on that line (and thus suppressed). To flag
unused `noqa` directives, run: `ruff check /path/to/file.py --extend-select RUF100`.

Ruff can also _remove_ any unused `noqa` directives via its fix functionality. To remove any
unused `noqa` directives, run: `ruff check /path/to/file.py --extend-select RUF100 --fix`.

### Inserting necessary suppression comments

Ruff can _automatically add_ `noqa` directives to all lines that contain violations, which is
useful when migrating a new codebase to Ruff. To automatically add `noqa` directives to all
relevant lines (with the appropriate rule codes), run: `ruff check /path/to/file.py --add-noqa`.

### Action comments

Ruff respects isort's [action comments](https://pycqa.github.io/isort/docs/configuration/action_comments.html)
(`# isort: skip_file`, `# isort: on`, `# isort: off`, `# isort: skip`, and `# isort: split`), which
enable selectively enabling and disabling import sorting for blocks of code and other inline
configuration.

Ruff will also respect variants of these action comments with a `# ruff:` prefix
(e.g., `# ruff: isort: skip_file`, `# ruff: isort: on`, and so on). These variants more clearly
convey that the action comment is intended for Ruff, but are functionally equivalent to the
isort variants.

Unlike isort, Ruff does not respect action comments within docstrings.

See the [isort documentation](https://pycqa.github.io/isort/docs/configuration/action_comments.html)
for more.

## Exit codes

By default, `ruff check` exits with the following status codes:

- `0` if no violations were found, or if all present violations were fixed automatically.
- `1` if violations were found.
- `2` if Ruff terminates abnormally due to invalid configuration, invalid CLI options, or an
    internal error.

This convention mirrors that of tools like ESLint, Prettier, and RuboCop.

`ruff check` supports two command-line flags that alter its exit code behavior:

- `--exit-zero` will cause Ruff to exit with a status code of `0` even if violations were found.
    Note that Ruff will still exit with a status code of `2` if it terminates abnormally.
- `--exit-non-zero-on-fix` will cause Ruff to exit with a status code of `1` if violations were
    found, _even if_ all such violations were fixed automatically. Note that the use of
    `--exit-non-zero-on-fix` can result in a non-zero exit code even if no violations remain after
    fixing.
