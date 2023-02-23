### Installation

Ruff is available as [`ruff`](https://pypi.org/project/ruff/) on PyPI:

```shell
pip install ruff
```

For **macOS Homebrew** and **Linuxbrew** users, Ruff is also available as [`ruff`](https://formulae.brew.sh/formula/ruff) on Homebrew:

```shell
brew install ruff
```

For **Conda** users, Ruff is also available as [`ruff`](https://anaconda.org/conda-forge/ruff) on
`conda-forge`:

```shell
conda install -c conda-forge ruff
```

For **Arch Linux** users, Ruff is also available as [`ruff`](https://archlinux.org/packages/community/x86_64/ruff/)
on the official repositories:

```shell
pacman -S ruff
```

For **Alpine** users, Ruff is also available as [`ruff`](https://pkgs.alpinelinux.org/package/edge/testing/x86_64/ruff)
on the testing repositories:

```shell
apk add ruff
```

[![Packaging status](https://repology.org/badge/vertical-allrepos/ruff-python-linter.svg?exclude_unsupported=1)](https://repology.org/project/ruff-python-linter/versions)

### Usage

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

Ruff also works with [pre-commit](https://pre-commit.com):

```yaml
- repo: https://github.com/charliermarsh/ruff-pre-commit
  # Ruff version.
  rev: 'v0.0.252'
  hooks:
    - id: ruff
```

Or, to enable autofix:

```yaml
- repo: https://github.com/charliermarsh/ruff-pre-commit
  # Ruff version.
  rev: 'v0.0.252'
  hooks:
    - id: ruff
      args: [--fix, --exit-non-zero-on-fix]
```

Note that Ruff's pre-commit hook should run before Black, isort, and other formatting tools.
