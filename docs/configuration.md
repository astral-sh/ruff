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

    # Assume Python 3.8
    target-version = "py38"

    [tool.ruff.lint]
    # Enable Pyflakes (`F`) and a subset of the pycodestyle (`E`)  codes by default.
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

    # Assume Python 3.8
    target-version = "py38"

    [lint]
    # Enable Pyflakes (`F`) and a subset of the pycodestyle (`E`)  codes by default.
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

    # 4. Ignore `E402` (import violations) in all `__init__.py` files, and in select subdirectories.
    [tool.ruff.lint.per-file-ignores]
    "__init__.py" = ["E402"]
    "**/{tests,docs,tools}/*" = ["E402"]

    [tool.ruff.format]
    # 5. Use single quotes for non-triple-quoted strings.
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

    # 4. Ignore `E402` (import violations) in all `__init__.py` files, and in select subdirectories.
    [lint.per-file-ignores]
    "__init__.py" = ["E402"]
    "**/{tests,docs,tools}/*" = ["E402"]

    [format]
    # 5. Use single quotes for non-triple-quoted strings.
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

Similar to [ESLint](https://eslint.org/docs/latest/user-guide/configuring/configuration-files#cascading-and-hierarchy),
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
    configuration, with `${config_dir}` being determined via the [`dirs`](https://docs.rs/dirs/4.0.0/dirs/fn.config_dir.html)
    crate, and all relative paths being again resolved relative to the _current working directory_.
1. Any config-file-supported settings that are provided on the command-line (e.g., via
    `--select`) will override the settings in _every_ resolved configuration file.

Unlike [ESLint](https://eslint.org/docs/latest/user-guide/configuring/configuration-files#cascading-and-hierarchy),
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

Files that are passed to `ruff` directly are always analyzed, regardless of the above criteria.
For example, `ruff check /path/to/excluded/file.py` will always lint `file.py`.

### Default inclusions

By default, Ruff will discover files matching `*.py`, `*.ipy`, or `pyproject.toml`.

To lint or format files with additional file extensions, use the [`extend-include`](settings.md#extend-include) setting.

=== "pyproject.toml"

    ```toml
    [tool.ruff]
    extend-include = ["*.ipynb"]
    ```

=== "ruff.toml"

    ```toml
    extend-include = ["*.ipynb"]
    ```

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

Ruff has built-in support for [Jupyter Notebooks](https://jupyter.org/).

To opt in to linting and formatting Jupyter Notebook (`.ipynb`) files, add the `*.ipynb` pattern to
your [`extend-include`](settings.md#extend-include) setting, like so:

=== "pyproject.toml"

    ```toml
    [tool.ruff]
    extend-include = ["*.ipynb"]
    ```

=== "ruff.toml"

    ```toml
    extend-include = ["*.ipynb"]
    ```

This will prompt Ruff to discover Jupyter Notebook (`.ipynb`) files in any specified
directories, then lint and format them accordingly.

If you'd prefer to either only lint or only format Jupyter Notebook files, you can use the
section specific `exclude` option to do so. For example, the following would only lint Jupyter
Notebook files and not format them:

=== "pyproject.toml"

    ```toml
    [tool.ruff]
    extend-include = ["*.ipynb"]

    [tool.ruff.format]
    exclude = ["*.ipynb"]
    ```

=== "ruff.toml"

    ```toml
    extend-include = ["*.ipynb"]

    [format]
    exclude = ["*.ipynb"]
    ```

And, conversely, the following would only format Jupyter Notebook files and not lint them:

=== "pyproject.toml"

    ```toml
    [tool.ruff]
    extend-include = ["*.ipynb"]

    [tool.ruff.lint]
    exclude = ["*.ipynb"]
    ```

=== "ruff.toml"

    ```toml
    extend-include = ["*.ipynb"]

    [lint]
    exclude = ["*.ipynb"]
    ```

Alternatively, pass the notebook file(s) to `ruff` on the command-line directly. For example,
`ruff check /path/to/notebook.ipynb` will always lint `notebook.ipynb`. Similarly,
`ruff format /path/to/notebook.ipynb` will always format `notebook.ipynb`.

## Command-line interface

Some configuration options can be provided via the command-line, such as those related to rule
enablement and disablement, file discovery, logging level, and more:

```shell
ruff check path/to/code/ --select F401 --select F403 --quiet
```

See `ruff help` for more on Ruff's top-level commands:

<!-- Begin auto-generated command help. -->

```text
Ruff: An extremely fast Python linter.

Usage: ruff [OPTIONS] <COMMAND>

Commands:
  check    Run Ruff on the given files or directories (default)
  rule     Explain a rule (or all rules)
  config   List or describe the available configuration options
  linter   List all supported upstream linters
  clean    Clear any caches in the current directory and any subdirectories
  format   Run the Ruff formatter on the given files or directories
  version  Display Ruff's version
  help     Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version

Log levels:
  -v, --verbose  Enable verbose logging
  -q, --quiet    Print diagnostics, but nothing else
  -s, --silent   Disable all logging (but still exit with status code "1" upon
                 detecting diagnostics)

For help with a specific command, see: `ruff help <command>`.
```

<!-- End auto-generated command help. -->

Or `ruff help check` for more on the linting command:

<!-- Begin auto-generated check help. -->

```text
Run Ruff on the given files or directories (default)

Usage: ruff check [OPTIONS] [FILES]...

Arguments:
  [FILES]...  List of files or directories to check [default: .]

Options:
      --fix
          Apply fixes to resolve lint violations. Use `--no-fix` to disable or
          `--unsafe-fixes` to include unsafe fixes
      --unsafe-fixes
          Include fixes that may not retain the original intent of the code.
          Use `--no-unsafe-fixes` to disable
      --show-source
          Show violations with source code. Use `--no-show-source` to disable.
          (Deprecated: use `--output-format=full` or `--output-format=concise`
          instead of `--show-source` and `--no-show-source`, respectively)
      --show-fixes
          Show an enumeration of all fixed lint violations. Use
          `--no-show-fixes` to disable
      --diff
          Avoid writing any fixed files back; instead, output a diff for each
          changed file to stdout. Implies `--fix-only`
  -w, --watch
          Run in watch mode by re-running whenever files change
      --fix-only
          Apply fixes to resolve lint violations, but don't report on leftover
          violations. Implies `--fix`. Use `--no-fix-only` to disable or
          `--unsafe-fixes` to include unsafe fixes
      --ignore-noqa
          Ignore any `# noqa` comments
      --output-format <OUTPUT_FORMAT>
          Output serialization format for violations. The default serialization
          format is "concise". In preview mode, the default serialization
          format is "full" [env: RUFF_OUTPUT_FORMAT=] [possible values: text,
          concise, full, json, json-lines, junit, grouped, github, gitlab,
          pylint, azure, sarif]
  -o, --output-file <OUTPUT_FILE>
          Specify file to write the linter output to (default: stdout)
      --target-version <TARGET_VERSION>
          The minimum Python version that should be supported [possible values:
          py37, py38, py39, py310, py311, py312]
      --preview
          Enable preview mode; checks will include unstable rules and fixes.
          Use `--no-preview` to disable
      --config <CONFIG>
          Path to the `pyproject.toml` or `ruff.toml` file to use for
          configuration
      --extension <EXTENSION>
          List of mappings from file extension to language (one of ["python",
          "ipynb", "pyi"]). For example, to treat `.ipy` files as IPython
          notebooks, use `--extension ipy:ipynb`
      --statistics
          Show counts for every rule with at least one violation
      --add-noqa
          Enable automatic additions of `noqa` directives to failing lines
      --show-files
          See the files Ruff will be run against with the current settings
      --show-settings
          See the settings Ruff will use to lint a given Python file
  -h, --help
          Print help

Rule selection:
      --select <RULE_CODE>
          Comma-separated list of rule codes to enable (or ALL, to enable all
          rules)
      --ignore <RULE_CODE>
          Comma-separated list of rule codes to disable
      --extend-select <RULE_CODE>
          Like --select, but adds additional rule codes on top of those already
          specified
      --per-file-ignores <PER_FILE_IGNORES>
          List of mappings from file pattern to code to exclude
      --extend-per-file-ignores <EXTEND_PER_FILE_IGNORES>
          Like `--per-file-ignores`, but adds additional ignores on top of
          those already specified
      --fixable <RULE_CODE>
          List of rule codes to treat as eligible for fix. Only applicable when
          fix itself is enabled (e.g., via `--fix`)
      --unfixable <RULE_CODE>
          List of rule codes to treat as ineligible for fix. Only applicable
          when fix itself is enabled (e.g., via `--fix`)
      --extend-fixable <RULE_CODE>
          Like --fixable, but adds additional rule codes on top of those
          already specified

File selection:
      --exclude <FILE_PATTERN>
          List of paths, used to omit files and/or directories from analysis
      --extend-exclude <FILE_PATTERN>
          Like --exclude, but adds additional files and directories on top of
          those already excluded
      --respect-gitignore
          Respect file exclusions via `.gitignore` and other standard ignore
          files. Use `--no-respect-gitignore` to disable
      --force-exclude
          Enforce exclusions, even for paths passed to Ruff directly on the
          command-line. Use `--no-force-exclude` to disable

Miscellaneous:
  -n, --no-cache
          Disable cache reads [env: RUFF_NO_CACHE=]
      --isolated
          Ignore all configuration files
      --cache-dir <CACHE_DIR>
          Path to the cache directory [env: RUFF_CACHE_DIR=]
      --stdin-filename <STDIN_FILENAME>
          The name of the file when passing it through stdin
  -e, --exit-zero
          Exit with status code "0", even upon detecting lint violations
      --exit-non-zero-on-fix
          Exit with a non-zero status code if any files were modified via fix,
          even if no lint violations remain

Log levels:
  -v, --verbose  Enable verbose logging
  -q, --quiet    Print diagnostics, but nothing else
  -s, --silent   Disable all logging (but still exit with status code "1" upon
                 detecting diagnostics)
```

<!-- End auto-generated check help. -->

Or `ruff help format` for more on the formatting command:

<!-- Begin auto-generated format help. -->

```text
Run the Ruff formatter on the given files or directories

Usage: ruff format [OPTIONS] [FILES]...

Arguments:
  [FILES]...  List of files or directories to format [default: .]

Options:
      --check
          Avoid writing any formatted files back; instead, exit with a non-zero
          status code if any files would have been modified, and zero otherwise
      --diff
          Avoid writing any formatted files back; instead, exit with a non-zero
          status code and the difference between the current file and how the
          formatted file would look like
      --config <CONFIG>
          Path to the `pyproject.toml` or `ruff.toml` file to use for
          configuration
      --extension <EXTENSION>
          List of mappings from file extension to language (one of ["python",
          "ipynb", "pyi"]). For example, to treat `.ipy` files as IPython
          notebooks, use `--extension ipy:ipynb`
      --target-version <TARGET_VERSION>
          The minimum Python version that should be supported [possible values:
          py37, py38, py39, py310, py311, py312]
      --preview
          Enable preview mode; enables unstable formatting. Use `--no-preview`
          to disable
  -h, --help
          Print help (see more with '--help')

Miscellaneous:
  -n, --no-cache
          Disable cache reads [env: RUFF_NO_CACHE=]
      --cache-dir <CACHE_DIR>
          Path to the cache directory [env: RUFF_CACHE_DIR=]
      --isolated
          Ignore all configuration files
      --stdin-filename <STDIN_FILENAME>
          The name of the file when passing it through stdin

File selection:
      --respect-gitignore
          Respect file exclusions via `.gitignore` and other standard ignore
          files. Use `--no-respect-gitignore` to disable
      --exclude <FILE_PATTERN>
          List of paths, used to omit files and/or directories from analysis
      --force-exclude
          Enforce exclusions, even for paths passed to Ruff directly on the
          command-line. Use `--no-force-exclude` to disable

Format configuration:
      --line-length <LINE_LENGTH>  Set the line-length

Editor options:
      --range <RANGE>  When specified, Ruff will try to only format the code in
                       the given range.
                       It might be necessary to extend the start backwards or
                       the end forwards, to fully enclose a logical line.
                       The `<RANGE>` uses the format
                       `<start_line>:<start_column>-<end_line><end_column>`.

Log levels:
  -v, --verbose  Enable verbose logging
  -q, --quiet    Print diagnostics, but nothing else
  -s, --silent   Disable all logging (but still exit with status code "1" upon
                 detecting diagnostics)
```

<!-- End auto-generated format help. -->

## Shell autocompletion

Ruff supports autocompletion for most shells. A shell-specific completion script can be generated
by `ruff generate-shell-completion <SHELL>`, where `<SHELL>` is one of `bash`, `elvish`, `fig`, `fish`,
`powershell`, or `zsh`.

The exact steps required to enable autocompletion will vary by shell. For example instructions,
see the [Poetry](https://python-poetry.org/docs/#enable-tab-completion-for-bash-fish-or-zsh) or
[ripgrep](https://github.com/BurntSushi/ripgrep/blob/master/FAQ.md#complete) documentation.

As an example: to enable autocompletion for Zsh, run
`ruff generate-shell-completion zsh > ~/.zfunc/_ruff`. Then add the following line to your
`~/.zshrc` file, if they're not already present:

```zsh
fpath+=~/.zfunc
autoload -Uz compinit && compinit
```
