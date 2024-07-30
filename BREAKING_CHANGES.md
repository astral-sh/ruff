# Breaking Changes

## 0.5.0

- Follow the XDG specification to discover user-level configurations on macOS (same as on other Unix platforms)
- Selecting `ALL` now excludes deprecated rules
- The released archives now include an extra level of nesting, which can be removed with `--strip-components=1` when untarring.
- The release artifact's file name no longer includes the version tag. This enables users to install via `/latest` URLs on GitHub.

## 0.3.0

### Ruff 2024.2 style

The formatter now formats code according to the Ruff 2024.2 style guide. Read the [changelog](./CHANGELOG.md#030) for a detailed list of stabilized style changes.

### `isort`: Use one blank line after imports in typing stub files ([#9971](https://github.com/astral-sh/ruff/pull/9971))

Previously, Ruff used one or two blank lines (or the number configured by `isort.lines-after-imports`) after imports in typing stub files (`.pyi` files).
The [typing style guide for stubs](https://typing.readthedocs.io/en/latest/source/stubs.html#style-guide) recommends using at most 1 blank line for grouping.
As of this release, `isort` now always uses one blank line after imports in stub files, the same as the formatter.

### `build` is no longer excluded by default ([#10093](https://github.com/astral-sh/ruff/pull/10093))

Ruff maintains a list of directories and files that are excluded by default. This list now consists of the following patterns:

- `.bzr`
- `.direnv`
- `.eggs`
- `.git`
- `.git-rewrite`
- `.hg`
- `.ipynb_checkpoints`
- `.mypy_cache`
- `.nox`
- `.pants.d`
- `.pyenv`
- `.pytest_cache`
- `.pytype`
- `.ruff_cache`
- `.svn`
- `.tox`
- `.venv`
- `.vscode`
- `__pypackages__`
- `_build`
- `buck-out`
- `dist`
- `node_modules`
- `site-packages`
- `venv`

Previously, the `build` directory was included in this list. However, the `build` directory tends to be a not-unpopular directory
name, and excluding it by default caused confusion. Ruff now no longer excludes `build` except if it is excluded by a `.gitignore` file
or because it is listed in `extend-exclude`.

### `--format` is no longer a valid `rule` or `linter` command option

Previously, `ruff rule` and `ruff linter` accepted the `--format <FORMAT>` option as an alias for `--output-format`. Ruff no longer
supports this alias. Please use `ruff rule --output-format <FORMAT>` and `ruff linter --output-format <FORMAT>` instead.

## 0.1.9

### `site-packages` is now excluded by default ([#5513](https://github.com/astral-sh/ruff/pull/5513))

Ruff maintains a list of default exclusions, which now consists of the following patterns:

- `.bzr`
- `.direnv`
- `.eggs`
- `.git-rewrite`
- `.git`
- `.hg`
- `.ipynb_checkpoints`
- `.mypy_cache`
- `.nox`
- `.pants.d`
- `.pyenv`
- `.pytest_cache`
- `.pytype`
- `.ruff_cache`
- `.svn`
- `.tox`
- `.venv`
- `.vscode`
- `__pypackages__`
- `_build`
- `buck-out`
- `build`
- `dist`
- `node_modules`
- `site-packages`
- `venv`

Previously, the `site-packages` directory was not excluded by default. While `site-packages` tends
to be excluded anyway by virtue of the `.venv` exclusion, this may not be the case when using Ruff
from VS Code outside a virtual environment.

## 0.1.0

### The deprecated `format` setting has been removed

Ruff previously used the `format` setting, `--format` CLI option, and `RUFF_FORMAT` environment variable to
configure the output format of the CLI. This usage was deprecated in `v0.0.291` — the `format` setting is now used
to control Ruff's code formatting. As of this release:

- The `format` setting cannot be used to configure the output format, use `output-format` instead
- The `RUFF_FORMAT` environment variable is ignored, use `RUFF_OUTPUT_FORMAT` instead
- The `--format` option has been removed from `ruff check`, use `--output-format` instead

### Unsafe fixes are not applied by default ([#7769](https://github.com/astral-sh/ruff/pull/7769))

Ruff labels fixes as "safe" and "unsafe". The meaning and intent of your code will be retained when applying safe
fixes, but the meaning could be changed when applying unsafe fixes. Previously, unsafe fixes were always displayed
and applied when fixing was enabled. Now, unsafe fixes are hidden by default and not applied. The `--unsafe-fixes`
flag or `unsafe-fixes` configuration option can be used to enable unsafe fixes.

See the [docs](https://docs.astral.sh/ruff/configuration/#fix-safety) for details.

### Remove formatter-conflicting rules from the default rule set  ([#7900](https://github.com/astral-sh/ruff/pull/7900))

Previously, Ruff enabled all implemented rules in Pycodestyle (`E`) by default. Ruff now only includes the
Pycodestyle prefixes `E4`, `E7`, and `E9` to exclude rules that conflict with automatic formatters. Consequently,
the stable rule set no longer includes `line-too-long` (`E501`) and `mixed-spaces-and-tabs` (`E101`). Other
excluded Pycodestyle rules include whitespace enforcement in `E1` and `E2`; these rules are currently in preview, and are already omitted by default.

This change only affects those using Ruff under its default rule set. Users that include `E` in their `select` will experience no change in behavior.

## 0.0.288

### Remove support for emoji identifiers ([#7212](https://github.com/astral-sh/ruff/pull/7212))

Previously, Ruff supported the non-standard compliant emoji identifiers e.g. `📦 = 1`.
We decided to remove this non-standard language extension, and Ruff now reports syntax errors for emoji identifiers in your code, the same as CPython.

### Improved GitLab fingerprints ([#7203](https://github.com/astral-sh/ruff/pull/7203))

GitLab uses fingerprints to identify new, existing, or fixed violations. Previously, Ruff included the violation's position in the fingerprint. Using the location has the downside that changing any code before the violation causes the fingerprint to change, resulting in GitLab reporting one fixed and one new violation even though it is a pre-existing violation.

Ruff now uses a more stable location-agnostic fingerprint to minimize that existing violations incorrectly get marked as fixed and re-reported as new violations.

Expect GitLab to report each pre-existing violation in your project as fixed and a new violation in your Ruff upgrade PR.

## 0.0.283 / 0.284

### The target Python version now defaults to 3.8 instead of 3.10 ([#6397](https://github.com/astral-sh/ruff/pull/6397))

Previously, when a target Python version was not specified, Ruff would use a default of Python 3.10. However, it is safer to default to an _older_ Python version to avoid assuming the availability of new features. We now default to the oldest supported Python version which is currently Python 3.8.

(We still support Python 3.7 but since [it has reached EOL](https://devguide.python.org/versions/#unsupported-versions) we've decided not to make it the default here.)

Note this change was announced in 0.0.283 but not active until 0.0.284.

## 0.0.277

### `.ipynb_checkpoints`, `.pyenv`, `.pytest_cache`, and `.vscode` are now excluded by default ([#5513](https://github.com/astral-sh/ruff/pull/5513))

Ruff maintains a list of default exclusions, which now consists of the following patterns:

- `.bzr`
- `.direnv`
- `.eggs`
- `.git`
- `.git-rewrite`
- `.hg`
- `.ipynb_checkpoints`
- `.mypy_cache`
- `.nox`
- `.pants.d`
- `.pyenv`
- `.pytest_cache`
- `.pytype`
- `.ruff_cache`
- `.svn`
- `.tox`
- `.venv`
- `.vscode`
- `__pypackages__`
- `_build`
- `buck-out`
- `build`
- `dist`
- `node_modules`
- `venv`

Previously, the `.ipynb_checkpoints`, `.pyenv`, `.pytest_cache`, and `.vscode` directories were not
excluded by default. This change brings Ruff's default exclusions in line with other tools like
Black.

## 0.0.276

### The `keep-runtime-typing` setting has been reinstated ([#5470](https://github.com/astral-sh/ruff/pull/5470))

The `keep-runtime-typing` setting has been reinstated with revised semantics. This setting was
removed in [#4427](https://github.com/astral-sh/ruff/pull/4427), as it was equivalent to ignoring
the `UP006` and `UP007` rules via Ruff's standard `ignore` mechanism.

Taking `UP006` (rewrite `List[int]` to `list[int]`) as an example, the setting now behaves as
follows:

- On Python 3.7 and Python 3.8, setting `keep-runtime-typing = true` will cause Ruff to ignore
    `UP006` violations, even if `from __future__ import annotations` is present in the file.
    While such annotations are valid in Python 3.7 and Python 3.8 when combined with
    `from __future__ import annotations`, they aren't supported by libraries like Pydantic and
    FastAPI, which rely on runtime type checking.
- On Python 3.9 and above, the setting has no effect, as `list[int]` is a valid type annotation,
    and libraries like Pydantic and FastAPI support it without issue.

In short: `keep-runtime-typing` can be used to ensure that Ruff doesn't introduce type annotations
that are not supported at runtime by the current Python version, which are unsupported by libraries
like Pydantic and FastAPI.

Note that this is not a breaking change, but is included here to complement the previous removal
of `keep-runtime-typing`.

## 0.0.268

### The `keep-runtime-typing` setting has been removed ([#4427](https://github.com/astral-sh/ruff/pull/4427))

Enabling the `keep-runtime-typing` option, located under the `pyupgrade` section, is equivalent
to ignoring the `UP006` and `UP007` rules via Ruff's standard `ignore` mechanism. As there's no
need for a dedicated setting to disable these rules, the `keep-runtime-typing` option has been
removed.

## 0.0.267

### `update-check` is no longer a valid configuration option ([#4313](https://github.com/astral-sh/ruff/pull/4313))

The `update-check` functionality was deprecated in [#2530](https://github.com/astral-sh/ruff/pull/2530),
in that the behavior itself was removed, and Ruff was changed to warn when that option was enabled.

Now, Ruff will throw an error when `update-check` is provided via a configuration file (e.g.,
`update-check = false`) or through the command-line, since it has no effect. Users should remove
this option from their configuration.

## 0.0.265

### `--fix-only` now exits with a zero exit code, unless `--exit-non-zero-on-fix` is specified ([#4146](https://github.com/astral-sh/ruff/pull/4146))

Previously, `--fix-only` would exit with a non-zero exit code if any fixes were applied. This
behavior was inconsistent with `--fix`, and further, meant that `--exit-non-zero-on-fix` was
effectively ignored when `--fix-only` was specified.

Now, `--fix-only` will exit with a zero exit code, unless `--exit-non-zero-on-fix` is specified,
in which case it will exit with a non-zero exit code if any fixes were applied.

## 0.0.260

### Fixes are now represented as a list of edits ([#3709](https://github.com/astral-sh/ruff/pull/3709))

Previously, Ruff represented each fix as a single edit, which prohibited Ruff from automatically
fixing violations that required multiple edits across a file. As such, Ruff now represents each
fix as a list of edits.

This primarily affects the JSON API. Ruff's JSON representation used to represent the `fix` field as
a single edit, like so:

```json
{
    "message": "Remove unused import: `sys`",
    "content": "",
    "location": {"row": 1, "column": 0},
    "end_location": {"row": 2, "column": 0}
}
```

The updated representation instead includes a list of edits:

```json
{
    "message": "Remove unused import: `sys`",
    "edits": [
        {
            "content": "",
            "location": {"row": 1, "column": 0},
            "end_location": {"row": 2, "column": 0},
        }
    ]
}
```

## 0.0.246

### `multiple-statements-on-one-line-def` (`E704`) was removed ([#2773](https://github.com/astral-sh/ruff/pull/2773))

This rule was introduced in v0.0.245. However, it turns out that pycodestyle and Flake8 ignore this
rule by default, as it is not part of PEP 8. As such, we've removed it from Ruff.

## 0.0.245

### Ruff's public `check` method was removed ([#2709](https://github.com/astral-sh/ruff/pull/2709))

Previously, Ruff exposed a `check` method as a public Rust API. This method was used by few,
if any clients, and was not well documented or supported. As such, it has been removed, with
the intention of adding a stable public API in the future.

## 0.0.238

### `select`, `extend-select`, `ignore`, and `extend-ignore` have new semantics ([#2312](https://github.com/astral-sh/ruff/pull/2312))

Previously, the interplay between `select` and its related options could lead to unexpected
behavior. For example, `ruff --select E501 --ignore ALL` and `ruff --select E501 --extend-ignore ALL`
behaved differently. (See [#2312](https://github.com/astral-sh/ruff/pull/2312) for more
examples.)

When Ruff determines the enabled rule set, it has to reconcile `select` and `ignore` from a variety
of sources, including the current `pyproject.toml`, any inherited `pyproject.toml` files, and the
CLI.

The new semantics are such that Ruff uses the "highest-priority" `select` as the basis for the rule
set, and then applies any `extend-select`, `ignore`, and `extend-ignore` adjustments. CLI options
are given higher priority than `pyproject.toml` options, and the current `pyproject.toml` file is
given higher priority than any inherited `pyproject.toml` files.

`extend-select` and `extend-ignore` are no longer given "top priority"; instead, they merely append
to the `select` and `ignore` lists, as in Flake8.

This change is largely backwards compatible -- most users should experience no change in behavior.
However, as an example of a breaking change, consider the following:

```toml
[tool.ruff]
ignore = ["F401"]
```

Running `ruff --select F` would previously have enabled all `F` rules, apart from `F401`. Now, it
will enable all `F` rules, including `F401`, as the command line's `--select` resets the resolution.

### `remove-six-compat` (`UP016`) has been removed ([#2332](https://github.com/astral-sh/ruff/pull/2332))

The `remove-six-compat` rule has been removed. This rule was only useful for one-time Python 2-to-3
upgrades.

## 0.0.237

### `--explain`, `--clean`, and `--generate-shell-completion` are now subcommands ([#2190](https://github.com/astral-sh/ruff/pull/2190))

`--explain`, `--clean`, and `--generate-shell-completion` are now implemented as subcommands:

```console
ruff .         # Still works! And will always work.
ruff check .   # New! Also works.

ruff --explain E402   # Still works.
ruff rule E402        # New! Also works. (And preferred.)

# Oops! The command has to come first.
ruff --format json --explain E402   # No longer works.
ruff --explain E402 --format json   # Still works!
ruff rule E402   --format json      # Works! (And preferred.)
```

This change is largely backwards compatible -- most users should experience
no change in behavior. However, please note the following exceptions:

- Subcommands will now fail when invoked with unsupported arguments, instead
    of silently ignoring them. For example, the following will now fail:

    ```console
    ruff --clean --respect-gitignore
    ```

    (the `clean` command doesn't support `--respect-gitignore`.)

- The semantics of `ruff <arg>` have changed slightly when `<arg>` is a valid subcommand.
    For example, prior to this release, running `ruff rule` would run `ruff` over a file or
    directory called `rule`. Now, `ruff rule` would invoke the `rule` subcommand. This should
    only impact projects with files or directories named `rule`, `check`, `explain`, `clean`,
    or `generate-shell-completion`.

- Scripts that invoke ruff should supply `--` before any positional arguments.
    (The semantics of `ruff -- <arg>` have not changed.)

- `--explain` previously treated `--format grouped` as a synonym for `--format text`.
    This is no longer supported; instead, use `--format text`.

## 0.0.226

### `misplaced-comparison-constant` (`PLC2201`) was deprecated in favor of `SIM300` ([#1980](https://github.com/astral-sh/ruff/pull/1980))

These two rules contain (nearly) identical logic. To deduplicate the rule set, we've upgraded
`SIM300` to handle a few more cases, and deprecated `PLC2201` in favor of `SIM300`.

## 0.0.225

### `@functools.cache` rewrites have been moved to a standalone rule (`UP033`) ([#1938](https://github.com/astral-sh/ruff/pull/1938))

Previously, `UP011` handled both `@functools.lru_cache()`-to-`@functools.lru_cache` conversions,
_and_ `@functools.lru_cache(maxsize=None)`-to-`@functools.cache` conversions. The latter has been
moved out to its own rule (`UP033`). As such, some `# noqa: UP011` comments may need to be updated
to reflect the change in rule code.

## 0.0.222

### `--max-complexity` has been removed from the CLI ([#1877](https://github.com/astral-sh/ruff/pull/1877))

The McCabe plugin's `--max-complexity` setting has been removed from the CLI, for consistency with
the treatment of other, similar settings.

To set the maximum complexity, use the `max-complexity` property in your `pyproject.toml` file,
like so:

```toml
[tool.ruff.mccabe]
max-complexity = 10
```

## 0.0.181

### Files excluded by `.gitignore` are now ignored ([#1234](https://github.com/astral-sh/ruff/pull/1234))

Ruff will now avoid checking files that are excluded by `.ignore`, `.gitignore`,
`.git/info/exclude`, and global `gitignore` files. This behavior is powered by the [`ignore`](https://docs.rs/ignore/latest/ignore/struct.WalkBuilder.html#ignore-rules)
crate, and is applied in addition to Ruff's built-in `exclude` system.

To disable this behavior, set `respect-gitignore = false` in your `pyproject.toml` file.

Note that hidden files (i.e., files and directories prefixed with a `.`) are _not_ ignored by
default.

## 0.0.178

### Configuration files are now resolved hierarchically ([#1190](https://github.com/astral-sh/ruff/pull/1190))

`pyproject.toml` files are now resolved hierarchically, such that for each Python file, we find
the first `pyproject.toml` file in its path, and use that to determine its lint settings.

See the [documentation](https://docs.astral.sh/ruff/configuration/#python-file-discovery) for more.
