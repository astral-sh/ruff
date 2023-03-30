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

Note that Ruff's pre-commit hook should run before Black, isort, and other formatting tools.

Ruff can also be used as a [VS Code extension](https://github.com/charliermarsh/ruff-vscode) or
alongside any other editor through the [Ruff LSP](https://github.com/charliermarsh/ruff-lsp).

Ruff can also be used as a [GitHub Action](https://github.com/features/actions).  Commonly, as a pass/fail test to ensure your repository stays clean, abiding the [Rules](https://beta.ruff.rs/docs/rules/) specified in your configuration.  

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
      - uses: charliermarsh/ruff@v0
```

You can configure the Ruff optional configuration parameters passed to Ruff (using `with:`):

- version: Must be release available on PyPI. default, latest release. You can pin a version, or use any valid version specifier.
- options: default,`check`
- src: default, '.'

```yaml
- uses: charliermarsh/ruff@v0
  with:
    src: "./src" # OPTIONAL:  PATH 
    version: 0.0.259 # SUGGESTED: VERSION YOU DESIRE TO PIN
    options: --select B # OPTIONAL: ADDITIONAL OPTIONS/ARGUMENTS
```

See [Configuring Ruff](https://github.com/charliermarsh/ruff/blob/main/docs/configuration.md) for details

