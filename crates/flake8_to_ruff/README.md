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

## License

MIT

## Contributing

Contributions are welcome and hugely appreciated. To get started, check out the
[contributing guidelines](https://github.com/charliermarsh/ruff/blob/main/CONTRIBUTING.md).
