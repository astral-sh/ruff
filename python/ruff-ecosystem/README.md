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

Note executable paths may be absolute, relative to the current working directory, or will be looked up in the
current Python environment and PATH.

Run `ruff check` ecosystem checks comparing your debug build to your system Ruff:

```shell
ruff-ecosystem check ruff "./target/debug/ruff"
```

Run `ruff format` ecosystem checks comparing your debug build to your system Ruff:

```shell
ruff-ecosystem format ruff "./target/debug/ruff"
```

Run `ruff format` ecosystem checks comparing with changes to code that is already formatted:

```shell
ruff-ecosystem format ruff "./target/debug/ruff" --format-comparison ruff-then-ruff
```

Run `ruff format` ecosystem checks comparing with the Black formatter:

```shell
ruff-ecosystem format black ruff -v --cache python/checkouts --format-comparison black-and-ruff
```

The default output format is markdown, which includes nice summaries of the changes. You can use `--output-format json` to display the raw data — this is
particularly useful when making changes to the ecosystem checks.

## Development

When developing, it can be useful to set the `--pdb` flag to drop into a debugger on failure:

```shell
ruff-ecosystem check ruff "./target/debug/ruff" --pdb
```

You can also provide a path to cache checkouts to speed up repeated runs:

```shell
ruff-ecosystem check ruff "./target/debug/ruff" --cache ./repos
```
