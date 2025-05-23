# Breaking Changes

## 0.11.0

This is a follow-up to release 0.10.0. Because of a mistake in the release process, the `requires-python` inference changes were not included in that release. Ruff 0.11.0 now includes this change as well as the stabilization of the preview behavior for `PGH004`.

- **Changes to how the Python version is inferred when a `target-version` is not specified** ([#16319](https://github.com/astral-sh/ruff/pull/16319))

    In previous versions of Ruff, you could specify your Python version with:

    - The `target-version` option in a `ruff.toml` file or the `[tool.ruff]` section of a pyproject.toml file.
    - The `project.requires-python` field in a `pyproject.toml` file with a `[tool.ruff]` section.

    These options worked well in most cases, and are still recommended for fine control of the Python version. However, because of the way Ruff discovers config files, `pyproject.toml` files without a `[tool.ruff]` section would be ignored, including the `requires-python` setting. Ruff would then use the default Python version (3.9 as of this writing) instead, which is surprising when you've attempted to request another version.

    In v0.10, config discovery has been updated to address this issue:

    - If Ruff finds a `ruff.toml` file without a `target-version`, it will check
        for a `pyproject.toml` file in the same directory and respect its
        `requires-python` version, even if it does not contain a `[tool.ruff]`
        section.
    - If Ruff finds a user-level configuration, the `requires-python` field of the closest `pyproject.toml` in a parent directory will take precedence.
    - If there is no config file (`ruff.toml`or `pyproject.toml` with a
        `[tool.ruff]` section) in the directory of the file being checked, Ruff will
        search for the closest `pyproject.toml` in the parent directories and use its
        `requires-python` setting.

## 0.10.0

- **Changes to how the Python version is inferred when a `target-version` is not specified** ([#16319](https://github.com/astral-sh/ruff/pull/16319))

    Because of a mistake in the release process, the `requires-python` inference changes are not included in this release and instead shipped as part of 0.11.0.
    You can find a description of this change in the 0.11.0 section.

- **Updated `TYPE_CHECKING` behavior** ([#16669](https://github.com/astral-sh/ruff/pull/16669))

    Previously, Ruff only recognized typechecking blocks that tested the `typing.TYPE_CHECKING` symbol. Now, Ruff recognizes any local variable named `TYPE_CHECKING`. This release also removes support for the legacy `if 0:` and `if False:` typechecking checks. Use a local `TYPE_CHECKING` variable instead.

- **More robust noqa parsing** ([#16483](https://github.com/astral-sh/ruff/pull/16483))

    The syntax for both file-level and in-line suppression comments has been unified and made more robust to certain errors. In most cases, this will result in more suppression comments being read by Ruff, but there are a few instances where previously read comments will now log an error to the user instead. Please refer to the documentation on [_Error suppression_](https://docs.astral.sh/ruff/linter/#error-suppression) for the full specification.

- **Avoid unnecessary parentheses around with statements with a single context manager and a trailing comment** ([#14005](https://github.com/astral-sh/ruff/pull/14005))

    This change fixes a bug in the formatter where it introduced unnecessary parentheses around with statements with a single context manager and a trailing comment. This change may result in a change in formatting for some users.

- **Bump alpine default tag to 3.21 for derived Docker images** ([#16456](https://github.com/astral-sh/ruff/pull/16456))

    Alpine 3.21 was released in Dec 2024 and is used in the official Alpine-based Python images. Now the ruff:alpine image will use 3.21 instead of 3.20 and ruff:alpine3.20 will no longer be updated.

- **\[`unsafe-markup-use`\]: `RUF035` has been recoded to `S704`** ([#15957](https://github.com/astral-sh/ruff/pull/15957))

## 0.9.0

Ruff now formats your code according to the 2025 style guide. As a result, your code might now get formatted differently. See the [changelog](./CHANGELOG.md#090) for a detailed list of changes.

## 0.8.0

- **Default to Python 3.9**

    Ruff now defaults to Python 3.9 instead of 3.8 if no explicit Python version is configured using [`ruff.target-version`](https://docs.astral.sh/ruff/settings/#target-version) or [`project.requires-python`](https://packaging.python.org/en/latest/guides/writing-pyproject-toml/#python-requires) ([#13896](https://github.com/astral-sh/ruff/pull/13896))

- **Changed location of `pydoclint` diagnostics**

    [`pydoclint`](https://docs.astral.sh/ruff/rules/#pydoclint-doc) diagnostics now point to the first-line of the problematic docstring. Previously, this was not the case.

    If you've opted into these preview rules but have them suppressed using
    [`noqa`](https://docs.astral.sh/ruff/linter/#error-suppression) comments in
    some places, this change may mean that you need to move the `noqa` suppression
    comments. Most users should be unaffected by this change.

- **Use XDG (i.e. `~/.local/bin`) instead of the Cargo home directory in the standalone installer**

    Previously, Ruff's installer used `$CARGO_HOME` or `~/.cargo/bin` for its target install directory. Now, Ruff will be installed into `$XDG_BIN_HOME`, `$XDG_DATA_HOME/../bin`, or `~/.local/bin` (in that order).

    This change is only relevant to users of the standalone Ruff installer (using the shell or PowerShell script). If you installed Ruff using uv or pip, you should be unaffected.

- **Changes to the line width calculation**

    Ruff now uses a new version of the [unicode-width](https://github.com/unicode-rs/unicode-width) Rust crate to calculate the line width. In very rare cases, this may lead to lines containing Unicode characters being reformatted, or being considered too long when they were not before ([`E501`](https://docs.astral.sh/ruff/rules/line-too-long/)).

## 0.7.0

- The pytest rules `PT001` and `PT023` now default to omitting the decorator parentheses when there are no arguments
    ([#12838](https://github.com/astral-sh/ruff/pull/12838), [#13292](https://github.com/astral-sh/ruff/pull/13292)).
    This was a change that we attempted to make in Ruff v0.6.0, but only partially made due to an error on our part.
    See the [blog post](https://astral.sh/blog/ruff-v0.7.0) for more details.
- The `useless-try-except` rule (in our `tryceratops` category) has been recoded from `TRY302` to
    `TRY203` ([#13502](https://github.com/astral-sh/ruff/pull/13502)). This ensures Ruff's code is consistent with
    the same rule in the [`tryceratops`](https://github.com/guilatrova/tryceratops) linter.
- The `lint.allow-unused-imports` setting has been removed ([#13677](https://github.com/astral-sh/ruff/pull/13677)). Use
    [`lint.pyflakes.allow-unused-imports`](https://docs.astral.sh/ruff/settings/#lint_pyflakes_allowed-unused-imports)
    instead.

## 0.6.0

- Detect imports in `src` layouts by default for `isort` rules ([#12848](https://github.com/astral-sh/ruff/pull/12848))

- The pytest rules `PT001` and `PT023` now default to omitting the decorator parentheses when there are no arguments ([#12838](https://github.com/astral-sh/ruff/pull/12838)).

- Lint and format Jupyter Notebook by default ([#12878](https://github.com/astral-sh/ruff/pull/12878)).

    You can disable specific rules for notebooks using [`per-file-ignores`](https://docs.astral.sh/ruff/settings/#lint_per-file-ignores):

    ```toml
    [tool.ruff.lint.per-file-ignores]
    "*.ipynb" = ["E501"] # disable line-too-long in notebooks
    ```

    If you'd prefer to either only lint or only format Jupyter Notebook files, you can use the
    section-specific `exclude` option to do so. For example, the following would only lint Jupyter
    Notebook files and not format them:

    ```toml
    [tool.ruff.format]
    exclude = ["*.ipynb"]
    ```

    And, conversely, the following would only format Jupyter Notebook files and not lint them:

    ```toml
    [tool.ruff.lint]
    exclude = ["*.ipynb"]
    ```

    You can completely disable Jupyter Notebook support by updating the [`extend-exclude`](https://docs.astral.sh/ruff/settings/#extend-exclude) setting:

    ```toml
    [tool.ruff]
    extend-exclude = ["*.ipynb"]
    ```

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

### Remove formatter-conflicting rules from the default rule set ([#7900](https://github.com/astral-sh/ruff/pull/7900))

Previously, Ruff enabled all implemented rules in Pycodestyle (`E`) by default. Ruff now only includes the
Pycodestyle prefixes `E4`, `E7`, and `E9` to exclude rules that conflict with automatic formatters. Consequently,
the stable rule set no longer includes `line-too-long` (`E501`) and `mixed-spaces-and-tabs` (`E101`). Other
excluded Pycodestyle rules include whitespace enforcement in `E1` and `E2`; these rules are currently in preview, and are already omitted by default.

This change only affects those using Ruff under its default rule set. Users that include `E` in their `select` will experience no change in behavior.

## 0.0.288

### Remove support for emoji identifiers ([#7212](https://github.com/astral-sh/ruff/pull/7212))

Previously, Ruff supported non-standards-compliant emoji identifiers such as `📦 = 1`.
We decided to remove this non-standard language extension. Ruff now reports syntax errors for invalid emoji identifiers in your code, the same as CPython.

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
