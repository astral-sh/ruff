# Using Ruff

To run Ruff, try any of the following:

```shell
ruff check .                        # Lint all files in the current directory (and any subdirectories)
ruff check path/to/code/            # Lint all files in `/path/to/code` (and any subdirectories)
ruff check path/to/code/*.py        # Lint all `.py` files in `/path/to/code`
ruff check path/to/code/to/file.py  # Lint `file.py`
```

You can run Ruff in `--watch` mode to automatically re-run on-change:

```shell
ruff check path/to/code/ --watch
```

## pre-commit

Ruff can also be used as a [pre-commit](https://pre-commit.com) hook:

```yaml
- repo: https://github.com/astral-sh/ruff-pre-commit
  # Ruff version.
  rev: v0.0.276
  hooks:
    - id: ruff
```

Or, to enable autofix:

```yaml
- repo: https://github.com/astral-sh/ruff-pre-commit
  # Ruff version.
  rev: v0.0.276
  hooks:
    - id: ruff
      args: [ --fix, --exit-non-zero-on-fix ]
```

Ruff's pre-commit hook should be placed after other formatting tools, such as Black and isort,
_unless_ you enable autofix, in which case, Ruff's pre-commit hook should run _before_ Black, isort,
and other formatting tools, as Ruff's autofix behavior can output code changes that require
reformatting.

## VS Code

Ruff can also be used as a [VS Code extension](https://github.com/astral-sh/ruff-vscode) or
alongside any other editor through the [Ruff LSP](https://github.com/astral-sh/ruff-lsp).

## GitHub Action

Ruff can also be used as a GitHub Action via [`ruff-action`](https://github.com/chartboost/ruff-action).

By default, `ruff-action` runs as a pass-fail test to ensure that a given repository doesn't contain
any lint rule violations as per its [configuration](https://github.com/astral-sh/ruff/blob/main/docs/configuration.md).
However, under-the-hood, `ruff-action` installs and runs `ruff` directly, so it can be used to
execute any supported `ruff` command (e.g., `ruff check --fix`).

`ruff-action` supports all GitHub-hosted runners, and can be used with any published Ruff version
(i.e., any version available on [PyPI](https://pypi.org/project/ruff/)).

To use `ruff-action`, create a file (e.g., `.github/workflows/ruff.yml`) inside your repository
with:

```yaml
name: Ruff
on: [ push, pull_request ]
jobs:
  ruff:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - uses: chartboost/ruff-action@v1
```

Alternatively, you can include `ruff-action` as a step in any other workflow file:

```yaml
      - uses: chartboost/ruff-action@v1
```

`ruff-action` accepts optional configuration parameters via `with:`, including:

- `version`: The Ruff version to install (default: latest).
- `options`: The command-line arguments to pass to Ruff (default: `"check"`).
- `src`: The source paths to pass to Ruff (default: `"."`).

For example, to run `ruff check --select B ./src` using Ruff version `0.0.259`:

```yaml
- uses: chartboost/ruff-action@v1
  with:
    src: "./src"
    version: 0.0.259
    args: --select B
```
