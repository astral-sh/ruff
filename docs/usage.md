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


Ruff can also be used as a [GitHub Action](https://github.com/features/actions) :


You can use Ruff within a GitHub Actions workflow without setting your own Python environment. Great for enforcing your Ruff rules.  Naturally, assumes the repo has an appropriate [config](https://beta.ruff.rs/docs/configuration/) in a `pyproject.toml`, `ruff.toml`, or `.ruff.toml`.

Compatibility
This action is known to support all GitHub-hosted runner OSes. In addition, only published versions of Ruff are supported (i.e. whatever is available on PyPI).

Usage
Create a file named .github/workflows/ruff.yml inside your repository with:

```
name: Ruff

on: [push, pull_request]

jobs:
  ruff:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - uses: charliermarsh/ruff@v0
```

The version of Ruff the action will use can be configured via version. This can be any valid version specifier or just the version number if you want an exact version. The action defaults to the latest release available on PyPI. Only versions available from PyPI are supported, so no commit SHAs or branch names.

You can also configure the arguments passed to Ruff via options (defaults to 'check) and src (default is '.'). 


```
- uses: charliermarsh/ruff@v0
  with:
    src: "./src"
    version: "0.0.259"
```



