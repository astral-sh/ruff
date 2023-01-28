# Breaking Changes

## 0.0.237

### `--explain`, `--clean`, and `--generate-shell-completion` are now subcommands ([#2190](https://github.com/charliermarsh/ruff/pull/2190))

`--explain`, `--clean`, and `--generate-shell-completion` are now implemented as subcommands:

    ruff .         # Still works! And will always work.
    ruff check .   # New! Also works.

    ruff --explain E402   # Still works.
    ruff rule E402        # New! Also works. (And preferred.)

    # Oops! The command has to come first.
    ruff --format json --explain E402   # No longer works.
    ruff --explain E402 --format json   # Still works!
    ruff rule E402   --format json      # Works! (And preferred.)

This change is largely backwards compatible -- most users should experience
no change in behavior. However, please note the following exceptions:

* Subcommands will now fail when invoked with unsupported arguments, instead
  of silently ignoring them. For example, the following will now fail:

      ruff --clean --respect-gitignore

  (the `clean` command doesn't support `--respect-gitignore`.)

* The semantics of `ruff <arg>` have changed slightly when `<arg>` is a valid subcommand.
  For example, prior to this release, running `ruff rule` would run `ruff` over a file or
  directory called `rule`. Now, `ruff rule` would invoke the `rule` subcommand. This should
  only impact projects with files or directories named `rule`, `check`, `explain`, `clean`,
  or `generate-shell-completion`.

* Scripts that invoke ruff should supply `--` before any positional arguments.
  (The semantics of `ruff -- <arg>` have not changed.)

* `--explain` previously treated `--format grouped` as a synonym for `--format text`.
  This is no longer supported; instead, use `--format text`.

## 0.0.226

### `misplaced-comparison-constant` (`PLC2201`) was deprecated in favor of `SIM300` ([#1980](https://github.com/charliermarsh/ruff/pull/1980))

These two rules contain (nearly) identical logic. To deduplicate the rule set, we've upgraded
`SIM300` to handle a few more cases, and deprecated `PLC2201` in favor of `SIM300`.

## 0.0.225

### `@functools.cache` rewrites have been moved to a standalone rule (`UP033`) ([#1938](https://github.com/charliermarsh/ruff/pull/1938))

Previously, `UP011` handled both `@functools.lru_cache()`-to-`@functools.lru_cache` conversions,
_and_ `@functools.lru_cache(maxsize=None)`-to-`@functools.cache` conversions. The latter has been
moved out to its own rule (`UP033`). As such, some `# noqa: UP011` comments may need to be updated
to reflect the change in rule code.

## 0.0.222

### `--max-complexity` has been removed from the CLI ([#1877](https://github.com/charliermarsh/ruff/pull/1877))

The McCabe plugin's `--max-complexity` setting has been removed from the CLI, for consistency with
the treatment of other, similar settings.

To set the maximum complexity, use the `max-complexity` property in your `pyproject.toml` file,
like so:

```toml
[tool.ruff.mccabe]
max-complexity = 10
```

## 0.0.181

### Files excluded by `.gitignore` are now ignored ([#1234](https://github.com/charliermarsh/ruff/pull/1234))

Ruff will now avoid checking files that are excluded by `.ignore`, `.gitignore`,
`.git/info/exclude`, and global `gitignore` files. This behavior is powered by the [`ignore`](https://docs.rs/ignore/latest/ignore/struct.WalkBuilder.html#ignore-rules)
crate, and is applied in addition to Ruff's built-in `exclude` system.

To disable this behavior, set `respect-gitignore = false` in your `pyproject.toml` file.

Note that hidden files (i.e., files and directories prefixed with a `.`) are _not_ ignored by
default.

## 0.0.178

### Configuration files are now resolved hierarchically ([#1190](https://github.com/charliermarsh/ruff/pull/1190))

`pyproject.toml` files are now resolved hierarchically, such that for each Python file, we find
the first `pyproject.toml` file in its path, and use that to determine its lint settings.

See the [README](https://github.com/charliermarsh/ruff#pyprojecttoml-discovery) for more.
