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

Ruff can also be used as a [pre-commit](https://pre-commit.com) hook:

```yaml
- repo: https://github.com/charliermarsh/ruff-pre-commit
  # Ruff version.
  rev: 'v0.0.259'
  hooks:
    - id: ruff
```

Or, to enable autofix:

```yaml
- repo: https://github.com/charliermarsh/ruff-pre-commit
  # Ruff version.
  rev: 'v0.0.259'
  hooks:
    - id: ruff
      args: [--fix, --exit-non-zero-on-fix]
```

Ruff's pre-commit hook should be placed after other formatting tools, such as Black and isort,
_unless_ you enable autofix, in which case, Ruff's pre-commit hook should run _before_ Black, isort,
and other formatting tools, as Ruff's autofix behavior can output code changes that require
reformatting.

Ruff can also be used as a [VS Code extension](https://github.com/charliermarsh/ruff-vscode) or
alongside any other editor through the [Ruff LSP](https://github.com/charliermarsh/ruff-lsp).
