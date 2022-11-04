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

To run `flake8-to-ruff`:

```shell
flake8-to-ruff path/to/setup.cfg
flake8-to-ruff path/to/tox.ini
flake8-to-ruff path/to/.flake8
```

`flake8-to-ruff` will print the relevant `pyproject.toml` sections to standard output, like so:

```toml
[tool.ruff]
exclude = [
    '.svn',
    'CVS',
    '.bzr',
    '.hg',
    '.git',
    '__pycache__',
    '.tox',
    '.idea',
    '.mypy_cache',
    '.venv',
    'node_modules',
    '_state_machine.py',
    'test_fstring.py',
    'bad_coding2.py',
    'badsyntax_*.py',
]
select = [
    'A',
    'E',
    'F',
    'Q',
]
ignore = []

[tool.ruff.flake8-quotes]
inline-quotes = 'single'

[tool.ruff.pep8-naming]
ignore-names = [
    'foo',
    'bar',
]
```

### Plugins

`flake8-to-ruff` will attempt to infer any activated plugins based on the settings provided in your
configuration file.

For example, if your `.flake8` file includes a `docstring-convention` property, `flake8-to-ruff`
will enable the appropriate [`flake8-docstrings`](https://pypi.org/project/flake8-docstrings/)
checks.

Alternatively, you can manually specify plugins on the command-line:

```shell
flake8-to-ruff path/to/.flake8 --plugin flake8-builtins --plugin flake8-quotes
```

## Limitations

1. Ruff only supports a subset of the Flake configuration options. `flake8-to-ruff` will warn on and
   ignore unsupported options in the `.flake8` file (or equivalent). (Similarly, Ruff has a few
   configuration options that don't exist in Flake8.)
2. Ruff will omit any error codes that are unimplemented or unsupported by Ruff, including error
   codes from unsupported plugins. (See the [Ruff README](https://github.com/charliermarsh/ruff#user-content-how-does-ruff-compare-to-flake8)
   for the complete list of supported plugins.)

## License

MIT

## Contributing

Contributions are welcome and hugely appreciated. To get started, check out the
[contributing guidelines](https://github.com/charliermarsh/ruff/blob/main/CONTRIBUTING.md).
