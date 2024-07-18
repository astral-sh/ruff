# Installing Ruff

Ruff is available as [`ruff`](https://pypi.org/project/ruff/) on PyPI:

```shell
pip install ruff
```

Once installed, you can run Ruff from the command line:

```shell
ruff check   # Lint all files in the current directory.
ruff format  # Format all files in the current directory.
```

Starting with version `0.5.0`, Ruff can be installed with our standalone installers:

```shell
# On macOS and Linux.
curl -LsSf https://astral.sh/ruff/install.sh | sh

# On Windows.
powershell -c "irm https://astral.sh/ruff/install.ps1 | iex"

# For a specific version.
curl -LsSf https://astral.sh/ruff/0.5.0/install.sh | sh
powershell -c "irm https://astral.sh/ruff/0.5.0/install.ps1 | iex"
```

For **macOS Homebrew** and **Linuxbrew** users, Ruff is also available
as [`ruff`](https://formulae.brew.sh/formula/ruff) on Homebrew:

```shell
brew install ruff
```

For **Conda** users, Ruff is also available as [`ruff`](https://anaconda.org/conda-forge/ruff) on
`conda-forge`:

```shell
conda install -c conda-forge ruff
```

For **pkgx** users, Ruff is also available as [`ruff`](https://pkgx.dev/pkgs/github.com/charliermarsh/ruff/)
on the `pkgx` registry:

```shell
pkgx install ruff
```

For **Arch Linux** users, Ruff is also available as [`ruff`](https://archlinux.org/packages/extra/x86_64/ruff/)
on the official repositories:

```shell
pacman -S ruff
```

For **Alpine** users, Ruff is also available as [`ruff`](https://pkgs.alpinelinux.org/package/edge/testing/x86_64/ruff)
on the testing repositories:

```shell
apk add ruff
```

For **openSUSE Tumbleweed** users, Ruff is also available in the distribution repository:

```shell
sudo zypper install python3-ruff
```

On **Docker**, it is published as `ghcr.io/astral-sh/ruff`, tagged for each release and `latest` for
the latest release.

```shell
docker run -v .:/io --rm ghcr.io/astral-sh/ruff check
docker run -v .:/io --rm ghcr.io/astral-sh/ruff:0.3.0 check

# Or, for Podman on SELinux.
docker run -v .:/io:Z --rm ghcr.io/astral-sh/ruff check
```

[![Packaging status](https://repology.org/badge/vertical-allrepos/ruff-python-linter.svg?exclude_unsupported=1)](https://repology.org/project/ruff-python-linter/versions)
