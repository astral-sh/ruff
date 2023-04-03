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
  rev: 'v0.0.260'
  hooks:
    - id: ruff
```

Or, to enable autofix:

```yaml
- repo: https://github.com/charliermarsh/ruff-pre-commit
  # Ruff version.
  rev: 'v0.0.260'
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

Ruff can also be used as a [GitHub Action](https://github.com/features/actions).  Refer to the [Ruff-Action Project](https://github.com/chartboost/ruff-action) for more details.  The action is commonly used as a pass/fail test to ensure your repository stays clean, abiding the [Rules](https://beta.ruff.rs/docs/rules/) specified in your configuration.  Though it runs `ruff` so the action can do anything `ruff` can (ex: fix)

Compatibility
This action is known to support all GitHub-hosted runner OSes. In addition, only published versions of Ruff are supported (i.e. whatever is available on PyPI).

Usage
Create a file (ex: `.github/workflows/ruff.yml`) inside your repository with:

```yaml
name: Ruff
on: [push, pull_request]
jobs:
  ruff:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - uses: chartboost/ruff-action@v1
```

Alternatively,

```yaml
      - uses: chartboost/ruff-action@v1
```

can be included as a step in any other workflow file.

The Ruff action can be customized via optional configuration parameters passed to Ruff (using `with:`):

- version: Must be release available on PyPI. default, latest release of ruff. You can pin a version, or use any valid version specifier.
- options: default,`check`
- src: default, '.'

```yaml
- uses: chartboost/ruff-action@v1
  with:
    src: "./src" 
    version: 0.0.259
    options: --select B
```

See [Configuring Ruff](https://github.com/charliermarsh/ruff/blob/main/docs/configuration.md) for details
