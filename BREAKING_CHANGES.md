# Breaking Changes

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

See the [documentation](https://beta.ruff.rs/docs/configuration/#python-file-discovery) for more.
