# Configuring Ruff

Ruff can be configured through a `pyproject.toml`, `ruff.toml`, or `.ruff.toml` file.

Whether you're using Ruff as a linter, formatter, or both, the underlying configuration strategy and
semantics are the same.

For a complete enumeration of the available configuration options, see [_Settings_](settings.md).

If left unspecified, Ruff's default configuration is equivalent to:

=== "pyproject.toml"

    ```toml
    [tool.ruff]
    # Exclude a variety of commonly ignored directories.
    exclude = [
        ".bzr",
        ".direnv",
        ".eggs",
        ".git",
        ".git-rewrite",
        ".hg",
        ".ipynb_checkpoints",
        ".mypy_cache",
        ".nox",
        ".pants.d",
        ".pyenv",
        ".pytest_cache",
        ".pytype",
        ".ruff_cache",
        ".svn",
        ".tox",
        ".venv",
        ".vscode",
        "__pypackages__",
        "_build",
        "buck-out",
        "build",
        "dist",
        "node_modules",
        "site-packages",
        "venv",
    ]

    # Same as Black.
    line-length = 88
    indent-width = 4

    # Assume Python 3.10
    target-version = "py310"

    [tool.ruff.lint]
    # Enable Pyflakes (`F`) and a subset of the pycodestyle (`E`) codes by default.
    # Unlike Flake8, Ruff doesn't enable pycodestyle warnings (`W`) or
    # McCabe complexity (`C901`) by default.
    select = ["E4", "E7", "E9", "F"]
    ignore = []

    # Allow fix for all enabled rules (when `--fix`) is provided.
    fixable = ["ALL"]
    unfixable = []

    # Allow unused variables when underscore-prefixed.
    dummy-variable-rgx = "^(_+|(_+[a-zA-Z0-9_]*[a-zA-Z0-9]+?))$"

    [tool.ruff.format]
    # Like Black, use double quotes for strings.
    quote-style = "double"

    # Like Black, indent with spaces, rather than tabs.
    indent-style = "space"

    # Like Black, respect magic trailing commas.
    skip-magic-trailing-comma = false

    # Like Black, automatically detect the appropriate line ending.
    line-ending = "auto"

    # Enable auto-formatting of code examples in docstrings. Markdown,
    # reStructuredText code/literal blocks and doctests are all supported.
    #
    # This is currently disabled by default, but it is planned for this
    # to be opt-out in the future.
    docstring-code-format = false

    # Set the line length limit used when formatting code snippets in
    # docstrings.
    #
    # This only has an effect when the `docstring-code-format` setting is
    # enabled.
    docstring-code-line-length = "dynamic"
    ```

=== "ruff.toml"

    ```toml
    # Exclude a variety of commonly ignored directories.
    exclude = [
        ".bzr",
        ".direnv",
        ".eggs",
        ".git",
        ".git-rewrite",
        ".hg",
        ".ipynb_checkpoints",
        ".mypy_cache",
        ".nox",
        ".pants.d",
        ".pyenv",
        ".pytest_cache",
        ".pytype",
        ".ruff_cache",
        ".svn",
        ".tox",
        ".venv",
        ".vscode",
        "__pypackages__",
        "_build",
        "buck-out",
        "build",
        "dist",
        "node_modules",
        "site-packages",
        "venv",
    ]

    # Same as Black.
    line-length = 88
    indent-width = 4

    # Assume Python 3.10
    target-version = "py310"

    [lint]
    # Enable Pyflakes (`F`) and a subset of the pycodestyle (`E`) codes by default.
    # Unlike Flake8, Ruff doesn't enable pycodestyle warnings (`W`) or
    # McCabe complexity (`C901`) by default.
    select = ["E4", "E7", "E9", "F"]
    ignore = []

    # Allow fix for all enabled rules (when `--fix`) is provided.
    fixable = ["ALL"]
    unfixable = []

    # Allow unused variables when underscore-prefixed.
    dummy-variable-rgx = "^(_+|(_+[a-zA-Z0-9_]*[a-zA-Z0-9]+?))$"

    [format]
    # Like Black, use double quotes for strings.
    quote-style = "double"

    # Like Black, indent with spaces, rather than tabs.
    indent-style = "space"

    # Like Black, respect magic trailing commas.
    skip-magic-trailing-comma = false

    # Like Black, automatically detect the appropriate line ending.
    line-ending = "auto"

    # Enable auto-formatting of code examples in docstrings. Markdown,
    # reStructuredText code/literal blocks and doctests are all supported.
    #
    # This is currently disabled by default, but it is planned for this
    # to be opt-out in the future.
    docstring-code-format = false

    # Set the line length limit used when formatting code snippets in
    # docstrings.
    #
    # This only has an effect when the `docstring-code-format` setting is
    # enabled.
    docstring-code-line-length = "dynamic"
    ```

As an example, the following would configure Ruff to:

=== "pyproject.toml"

    ```toml
    [tool.ruff.lint]
    # 1. Enable flake8-bugbear (`B`) rules, in addition to the defaults.
    select = ["E4", "E7", "E9", "F", "B"]

    # 2. Avoid enforcing line-length violations (`E501`)
    ignore = ["E501"]

    # 3. Avoid trying to fix flake8-bugbear (`B`) violations.
    unfixable = ["B"]

    # 4. Ignore `E402` (import violations) in all `__init__.py` files, and in selected subdirectories.
    [tool.ruff.lint.per-file-ignores]
    "__init__.py" = ["E402"]
    "**/{tests,docs,tools}/*" = ["E402"]

    [tool.ruff.format]
    # 5. Use single quotes in `ruff format`.
    quote-style = "single"
    ```

=== "ruff.toml"

    ```toml
    [lint]
    # 1. Enable flake8-bugbear (`B`) rules, in addition to the defaults.
    select = ["E4", "E7", "E9", "F", "B"]

    # 2. Avoid enforcing line-length violations (`E501`)
    ignore = ["E501"]

    # 3. Avoid trying to fix flake8-bugbear (`B`) violations.
    unfixable = ["B"]

    # 4. Ignore `E402` (import violations) in all `__init__.py` files, and in selected subdirectories.
    [lint.per-file-ignores]
    "__init__.py" = ["E402"]
    "**/{tests,docs,tools}/*" = ["E402"]

    [format]
    # 5. Use single quotes in `ruff format`.
    quote-style = "single"
    ```

Linter plugin configurations are expressed as subsections, e.g.:

=== "pyproject.toml"

    ```toml
    [tool.ruff.lint]
    # Add "Q" to the list of enabled codes.
    select = ["E4", "E7", "E9", "F", "Q"]

    [tool.ruff.lint.flake8-quotes]
    docstring-quotes = "double"
    ```

=== "ruff.toml"

    ```toml
    [lint]
    # Add "Q" to the list of enabled codes.
    select = ["E4", "E7", "E9", "F", "Q"]

    [lint.flake8-quotes]
    docstring-quotes = "double"
    ```

Ruff respects `pyproject.toml`, `ruff.toml`, and `.ruff.toml` files. All three implement an
equivalent schema (though in the `ruff.toml` and `.ruff.toml` versions, the `[tool.ruff]` header and
`tool.ruff` section prefix is omitted).

For a complete enumeration of the available configuration options, see [_Settings_](settings.md).

## Config file discovery

Similar to [ESLint](https://eslint.org/docs/latest/use/configure/configuration-files#cascading-configuration-objects),
Ruff supports hierarchical configuration, such that the "closest" config file in the
directory hierarchy is used for every individual file, with all paths in the config file
(e.g., `exclude` globs, `src` paths) being resolved relative to the directory containing that
config file.

There are a few exceptions to these rules:

1. In locating the "closest" `pyproject.toml` file for a given path, Ruff ignores any
    `pyproject.toml` files that lack a `[tool.ruff]` section.
1. If a configuration file is passed directly via `--config`, those settings are used for _all_
    analyzed files, and any relative paths in that configuration file (like `exclude` globs or
    `src` paths) are resolved relative to the _current_ working directory.
1. If no config file is found in the filesystem hierarchy, Ruff will fall back to using
    a default configuration. If a user-specific configuration file exists
    at `${config_dir}/ruff/pyproject.toml`, that file will be used instead of the default
    configuration, with `${config_dir}` being determined via [`etcetera`'s base strategy](https://docs.rs/etcetera/latest/etcetera/#native-strategy),
    and all relative paths being again resolved relative to the _current working directory_.
1. Any config-file-supported settings that are provided on the command-line (e.g., via
    `--select`) will override the settings in _every_ resolved configuration file.

Unlike [ESLint](https://eslint.org/docs/latest/use/configure/configuration-files#cascading-configuration-objects),
Ruff does not merge settings across configuration files; instead, the "closest" configuration file
is used, and any parent configuration files are ignored. In lieu of this implicit cascade, Ruff
supports an [`extend`](settings.md#extend) field, which allows you to inherit the settings from another
config file, like so:

=== "pyproject.toml"

    ```toml
    [tool.ruff]
    # Extend the `pyproject.toml` file in the parent directory...
    extend = "../pyproject.toml"

    # ...but use a different line length.
    line-length = 100
    ```

=== "ruff.toml"

    ```toml
    # Extend the `ruff.toml` file in the parent directory...
    extend = "../ruff.toml"

    # ...but use a different line length.
    line-length = 100
    ```

All of the above rules apply equivalently to `pyproject.toml`, `ruff.toml`, and `.ruff.toml` files.
If Ruff detects multiple configuration files in the same directory, the `.ruff.toml` file will take
precedence over the `ruff.toml` file, and the `ruff.toml` file will take precedence over
the `pyproject.toml` file.

### Inferring the Python version
When no discovered configuration specifies a [`target-version`](settings.md#target-version), Ruff will attempt to fall back to the minimum version compatible with the `requires-python` field in a nearby `pyproject.toml`.
The rules for this behavior are as follows:

1. If a configuration file is passed directly, Ruff does not attempt to infer a missing `target-version`.
1. If a configuration file is found in the filesystem hierarchy, Ruff will infer a missing `target-version` from the `requires-python` field in a `pyproject.toml` file in the same directory as the found configuration.
1. If we are using a user-level configuration from `${config_dir}/ruff/pyproject.toml`, the `requires-python` field in the first `pyproject.toml` file found in an ancestor of the current working directory takes precedence over the `target-version` in the user-level configuration.
1. If no configuration files are found, Ruff will infer the `target-version` from the `requires-python` field in the first `pyproject.toml` file found in an ancestor of the current working directory.

Note that in these last two cases, the behavior of Ruff may differ depending on the working directory from which it is invoked.

## Python file discovery

When passed a path on the command-line, Ruff will automatically discover all Python files in that
path, taking into account the [`exclude`](settings.md#exclude) and [`extend-exclude`](settings.md#extend-exclude)
settings in each directory's configuration file.

Files can also be selectively excluded from linting or formatting by scoping the `exclude` setting
to the tool-specific configuration tables. For example, the following would prevent `ruff` from
formatting `.pyi` files, but would continue to include them in linting:

=== "pyproject.toml"

    ```toml
    [tool.ruff.format]
    exclude = ["*.pyi"]
    ```

=== "ruff.toml"

    ```toml
    [format]
    exclude = ["*.pyi"]
    ```

By default, Ruff will also skip any files that are omitted via `.ignore`, `.gitignore`,
`.git/info/exclude`, and global `gitignore` files (see: [`respect-gitignore`](settings.md#respect-gitignore)).

Files that are passed to `ruff` directly are always analyzed, regardless of the above criteria, 
unless [`force-exclude`](settings.md#force-exclude) is also enabled (via CLI or settings file).
For example, without `force-exclude` enabled, `ruff check /path/to/excluded/file.py` will always lint `file.py`.

### Default inclusions

By default, Ruff will discover files matching `*.py`, `*.pyi`, `*.ipynb`, or `pyproject.toml`.
In [preview](preview.md) mode, Ruff will also discover `*.pyw` by default.

To lint or format files with additional file extensions, use the [`extend-include`](settings.md#extend-include) setting.
You can also change the default selection using the [`include`](settings.md#include) setting.


=== "pyproject.toml"

    ```toml
    [tool.ruff]
    include = ["pyproject.toml", "src/**/*.py", "scripts/**/*.py"]
    ```

=== "ruff.toml"

    ```toml
    include = ["pyproject.toml", "src/**/*.py", "scripts/**/*.py"]
    ```

!!! warning
    Paths provided to `include` _must_ match files. For example, `include = ["src"]` will fail since it
    matches a directory.

## Jupyter Notebook discovery

Ruff has built-in support for linting and formatting [Jupyter Notebooks](https://jupyter.org/),
which are linted and formatted by default on version `0.6.0` and higher.

If you'd prefer to either only lint or only format Jupyter Notebook files, you can use the
section-specific `exclude` option to do so. For example, the following would only lint Jupyter
Notebook files and not format them:

=== "pyproject.toml"

    ```toml
    [tool.ruff.format]
    exclude = ["*.ipynb"]
    ```

=== "ruff.toml"

    ```toml
    [format]
    exclude = ["*.ipynb"]
    ```

And, conversely, the following would only format Jupyter Notebook files and not lint them:

=== "pyproject.toml"

    ```toml
    [tool.ruff.lint]
    exclude = ["*.ipynb"]
    ```

=== "ruff.toml"

    ```toml
    [lint]
    exclude = ["*.ipynb"]
    ```

You can completely disable Jupyter Notebook support by updating the
[`extend-exclude`](settings.md#extend-exclude) setting:

=== "pyproject.toml"

    ```toml
    [tool.ruff]
    extend-exclude = ["*.ipynb"]
    ```

=== "ruff.toml"

    ```toml
    extend-exclude = ["*.ipynb"]
    ```

If you'd like to ignore certain rules specifically for Jupyter Notebook files, you can do so by
using the [`per-file-ignores`](settings.md#per-file-ignores) setting:

=== "pyproject.toml"

    ```toml
    [tool.ruff.lint.per-file-ignores]
    "*.ipynb" = ["T20"]
    ```

=== "ruff.toml"

    ```toml
    [lint.per-file-ignores]
    "*.ipynb" = ["T20"]
    ```

Some rules have different behavior when applied to Jupyter Notebook files. For
example, when applied to `.py` files the
[`module-import-not-at-top-of-file` (`E402`)](rules/module-import-not-at-top-of-file.md)
rule detect imports at the top of a file, but for notebooks it detects imports at the top of a
**cell**. For a given rule, the rule's documentation will always specify if it has different
behavior when applied to Jupyter Notebook files.

