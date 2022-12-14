# Breaking Changes

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
