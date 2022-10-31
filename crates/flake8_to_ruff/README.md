# flake8-to-ruff

Convert existing Flake8 configuration files (`setup.cfg`, `tox.ini`, or `.flake8`) for use with
[Ruff](https://github.com/charliermarsh/ruff).

Generates a Ruff-compatible `pyproject.toml` section.

## Installation and Usage

### Installation

Available as [`flake8-to-ruff`](https://pypi.org/project/flake8-to-ruff/) on PyPI:

```shell
pip install flake8-to-ruff
```

### Usage

To run Ruff, try any of the following:

```shell
flake8-to-ruff path/to/setup.cfg
flake8-to-ruff path/to/tox.ini
flake8-to-ruff path/to/.flake8
```

## Limitations

1. Ruff only supports a subset of the Flake configuration options. `flake8-to-ruff` will warn on and
   ignore unsupported options in the `.flake8` file (or equivalent). (Similarly, Ruff has a few
   configuration options that don't exist in Flake8.)
2. Ruff will omit any error codes that are unimplemented or unsupported by Ruff, including error
   codes from unsupported plugins. (See the [Ruff README](https://github.com/charliermarsh/ruff#user-content-how-does-ruff-compare-to-flake8)
   for the complete list of supported plugins.)
3. `flake8-to-ruff` does not auto-detect your Flake8 plugins, so any reliance on Flake8 plugins that
   implicitly enable third-party checks will be ignored. Instead, add those error codes to your
   `select` or `extend-select` fields, so that `flake8-to-ruff` can pick them up.

## License

MIT

## Contributing

Contributions are welcome and hugely appreciated. To get started, check out the
[contributing guidelines](https://github.com/charliermarsh/ruff/blob/main/CONTRIBUTING.md).
