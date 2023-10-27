# ruff-ecosystem

Compare lint and format results for two different ruff versions (e.g. main and a PR) on real world projects.

## Installation

From the Ruff project root, install with `pip`:

```shell
pip install -e ./python/ruff-ecosystem
```

## Usage

```shell
ruff-ecosystem <check | format> <baseline executable> <comparison executable>
```

Note executable paths must be absolute or relative to the current working directory.

Run `ruff check` ecosystem checks comparing your debug build to your system Ruff:

```shell
ruff-ecosystem check  "$(which ruff)" "./target/debug/ruff"
```

Run `ruff format` ecosystem checks comparing your debug build to your system Ruff:

```shell
ruff-ecosystem format  "$(which ruff)" "./target/debug/ruff"
```

## Development

When developing, it can be useful to set the `--pdb` flag to drop into a debugger on failure:

```shell
ruff-ecosystem check  "$(which ruff)" "./target/debug/ruff" --pdb
```

You can also provide a path to cache checkouts to speed up repeated runs:

```shell
ruff-ecosystem check  "$(which ruff)" "./target/debug/ruff" --cache ./repos
```
