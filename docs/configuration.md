# Configuring Ruff

Ruff can be configured through a `pyproject.toml`, `ruff.toml`, or `.ruff.toml` file.

For a complete enumeration of the available configuration options, see
[_Settings_](settings.md).

## Using `pyproject.toml`

If left unspecified, Ruff's default configuration is equivalent to:

```toml
[tool.ruff]
# Enable the pycodestyle (`E`) and Pyflakes (`F`) rules by default.
# Unlike Flake8, Ruff doesn't enable pycodestyle warnings (`W`) or
# McCabe complexity (`C901`) by default.
select = ["E", "F"]
ignore = []

# Allow autofix for all enabled rules (when `--fix`) is provided.
fixable = ["ALL"]
unfixable = []

# Exclude a variety of commonly ignored directories.
exclude = [
    ".bzr",
    ".direnv",
    ".eggs",
    ".git",
    ".git-rewrite",
    ".hg",
    ".mypy_cache",
    ".nox",
    ".pants.d",
    ".pytype",
    ".ruff_cache",
    ".svn",
    ".tox",
    ".venv",
    "__pypackages__",
    "_build",
    "buck-out",
    "build",
    "dist",
    "node_modules",
    "venv",
]
per-file-ignores = {}

# Same as Black.
line-length = 88

# Allow unused variables when underscore-prefixed.
dummy-variable-rgx = "^(_+|(_+[a-zA-Z0-9_]*[a-zA-Z0-9]+?))$"

# Assume Python 3.10.
target-version = "py310"
```

As an example, the following would configure Ruff to: (1) enforce flake8-bugbear rules, in addition
to the defaults; (2) avoid enforcing line-length violations (`E501`); (3) avoid attempting to fix
flake8-bugbear (`B`) violations; and (3) ignore import-at-top-of-file violations (`E402`) in
`__init__.py` files:

```toml
[tool.ruff]
# Enable flake8-bugbear (`B`) rules.
select = ["E", "F", "B"]

# Never enforce `E501` (line length violations).
ignore = ["E501"]

# Avoid trying to fix flake8-bugbear (`B`) violations.
unfixable = ["B"]

# Ignore `E402` (import violations) in all `__init__.py` files, and in `path/to/file.py`.
[tool.ruff.per-file-ignores]
"__init__.py" = ["E402"]
"path/to/file.py" = ["E402"]
```

Plugin configurations should be expressed as subsections, e.g.:

```toml
[tool.ruff]
# Add "Q" to the list of enabled codes.
select = ["E", "F", "Q"]

[tool.ruff.flake8-quotes]
docstring-quotes = "double"
```

For a complete enumeration of the available configuration options, see
[_Settings_](settings.md).

Ruff mirrors Flake8's rule code system, in which each rule code consists of a one-to-three letter
prefix, followed by three digits (e.g., `F401`). The prefix indicates that "source" of the rule
(e.g., `F` for Pyflakes, `E` for pycodestyle, `ANN` for flake8-annotations). The set of enabled
rules is determined by the `select` and `ignore` options, which accept either the full code (e.g.,
`F401`) or the prefix (e.g., `F`).

As a special-case, Ruff also supports the `ALL` code, which enables all rules. Note that some
pydocstyle rules conflict (e.g., `D203` and `D211`) as they represent alternative docstring
formats. Ruff will automatically disable any conflicting rules when `ALL` is enabled.

If you're wondering how to configure Ruff, here are some **recommended guidelines**:

- Prefer `select` and `ignore` over `extend-select` and `extend-ignore`, to make your rule set
  explicit.
- Use `ALL` with discretion. Enabling `ALL` will implicitly enable new rules whenever you upgrade.
- Start with a small set of rules (`select = ["E", "F"]`) and add a category at-a-time. For example,
  you might consider expanding to `select = ["E", "F", "B"]` to enable the popular flake8-bugbear
  extension.
- By default, Ruff's autofix is aggressive. If you find that it's too aggressive for your liking,
  consider turning off autofix for specific rules or categories (see [_FAQ_](faq.md#ruff-tried-to-fix-something--but-it-broke-my-code)).

## Using `ruff.toml`

As an alternative to `pyproject.toml`, Ruff will also respect a `ruff.toml` (or `.ruff.toml`) file,
which implements an equivalent schema (though in the `ruff.toml` and `.ruff.toml` versions, the
`[tool.ruff]` header is omitted).

For example, the `pyproject.toml` described above would be represented via the following
`ruff.toml` (or `.ruff.toml`):

```toml
# Enable flake8-bugbear (`B`) rules.
select = ["E", "F", "B"]

# Never enforce `E501` (line length violations).
ignore = ["E501"]

# Avoid trying to fix flake8-bugbear (`B`) violations.
unfixable = ["B"]

# Ignore `E402` (import violations) in all `__init__.py` files, and in `path/to/file.py`.
[per-file-ignores]
"__init__.py" = ["E402"]
"path/to/file.py" = ["E402"]
```

For a complete enumeration of the available configuration options, see [_Settings_](settings.md).

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
  check   Run Ruff on the given files or directories (default)
  rule    Explain a rule
  config  List or describe the available configuration options
  linter  List all supported upstream linters
  clean   Clear any caches in the current directory and any subdirectories
  help    Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version

Log levels:
  -v, --verbose  Enable verbose logging
  -q, --quiet    Print lint violations, but nothing else
  -s, --silent   Disable all logging (but still exit with status code "1" upon detecting lint violations)

For help with a specific command, see: `ruff help <command>`.
```

<!-- End auto-generated command help. -->

Or `ruff help check` for more on the linting command:

<!-- Begin auto-generated subcommand help. -->

```text
Run Ruff on the given files or directories (default)

Usage: ruff check [OPTIONS] [FILES]...

Arguments:
  [FILES]...  List of files or directories to check

Options:
      --fix
          Attempt to automatically fix lint violations
      --show-source
          Show violations with source code
      --show-fixes
          Show an enumeration of all autofixed lint violations
      --diff
          Avoid writing any fixed files back; instead, output a diff for each changed file to stdout. Implies `--fix-only`
  -w, --watch
          Run in watch mode by re-running whenever files change
      --fix-only
          Fix any fixable lint violations, but don't report on leftover violations. Implies `--fix`
      --ignore-noqa
          Ignore any `# noqa` comments
      --format <FORMAT>
          Output serialization format for violations [env: RUFF_FORMAT=] [possible values: text, json, json-lines, junit, grouped, github, gitlab, pylint, azure]
  -o, --output-file <OUTPUT_FILE>
          Specify file to write the linter output to (default: stdout)
      --target-version <TARGET_VERSION>
          The minimum Python version that should be supported [possible values: py37, py38, py39, py310, py311, py312]
      --config <CONFIG>
          Path to the `pyproject.toml` or `ruff.toml` file to use for configuration
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
          Comma-separated list of rule codes to enable (or ALL, to enable all rules)
      --ignore <RULE_CODE>
          Comma-separated list of rule codes to disable
      --extend-select <RULE_CODE>
          Like --select, but adds additional rule codes on top of those already specified
      --per-file-ignores <PER_FILE_IGNORES>
          List of mappings from file pattern to code to exclude
      --extend-per-file-ignores <EXTEND_PER_FILE_IGNORES>
          Like `--per-file-ignores`, but adds additional ignores on top of those already specified
      --fixable <RULE_CODE>
          List of rule codes to treat as eligible for autofix. Only applicable when autofix itself is enabled (e.g., via `--fix`)
      --unfixable <RULE_CODE>
          List of rule codes to treat as ineligible for autofix. Only applicable when autofix itself is enabled (e.g., via `--fix`)
      --extend-fixable <RULE_CODE>
          Like --fixable, but adds additional rule codes on top of those already specified

File selection:
      --exclude <FILE_PATTERN>         List of paths, used to omit files and/or directories from analysis
      --extend-exclude <FILE_PATTERN>  Like --exclude, but adds additional files and directories on top of those already excluded
      --respect-gitignore              Respect file exclusions via `.gitignore` and other standard ignore files
      --force-exclude                  Enforce exclusions, even for paths passed to Ruff directly on the command-line

Miscellaneous:
  -n, --no-cache
          Disable cache reads
      --isolated
          Ignore all configuration files
      --cache-dir <CACHE_DIR>
          Path to the cache directory [env: RUFF_CACHE_DIR=]
      --stdin-filename <STDIN_FILENAME>
          The name of the file when passing it through stdin
  -e, --exit-zero
          Exit with status code "0", even upon detecting lint violations
      --exit-non-zero-on-fix
          Exit with a non-zero status code if any files were modified via autofix, even if no lint violations remain

Log levels:
  -v, --verbose  Enable verbose logging
  -q, --quiet    Print lint violations, but nothing else
  -s, --silent   Disable all logging (but still exit with status code "1" upon detecting lint violations)
```

<!-- End auto-generated subcommand help. -->

## `pyproject.toml` discovery

Similar to [ESLint](https://eslint.org/docs/latest/user-guide/configuring/configuration-files#cascading-and-hierarchy),
Ruff supports hierarchical configuration, such that the "closest" `pyproject.toml` file in the
directory hierarchy is used for every individual file, with all paths in the `pyproject.toml` file
(e.g., `exclude` globs, `src` paths) being resolved relative to the directory containing that
`pyproject.toml` file.

There are a few exceptions to these rules:

1. In locating the "closest" `pyproject.toml` file for a given path, Ruff ignores any
   `pyproject.toml` files that lack a `[tool.ruff]` section.
1. If a configuration file is passed directly via `--config`, those settings are used for across
   files. Any relative paths in that configuration file (like `exclude` globs or `src` paths) are
   resolved relative to the _current working directory_.
1. If no `pyproject.toml` file is found in the filesystem hierarchy, Ruff will fall back to using
   a default configuration. If a user-specific configuration file exists
   at `${config_dir}/ruff/pyproject.toml`, that file will be used instead of the default
   configuration, with `${config_dir}` being determined via the [`dirs`](https://docs.rs/dirs/4.0.0/dirs/fn.config_dir.html)
   crate, and all relative paths being again resolved relative to the _current working directory_.
1. Any `pyproject.toml`-supported settings that are provided on the command-line (e.g., via
   `--select`) will override the settings in _every_ resolved configuration file.

Unlike [ESLint](https://eslint.org/docs/latest/user-guide/configuring/configuration-files#cascading-and-hierarchy),
Ruff does not merge settings across configuration files; instead, the "closest" configuration file
is used, and any parent configuration files are ignored. In lieu of this implicit cascade, Ruff
supports an [`extend`](settings.md#extend) field, which allows you to inherit the settings from another
`pyproject.toml` file, like so:

```toml
# Extend the `pyproject.toml` file in the parent directory.
extend = "../pyproject.toml"
# But use a different line length.
line-length = 100
```

All of the above rules apply equivalently to `ruff.toml` and `.ruff.toml` files. If Ruff detects
multiple configuration files in the same directory, the `.ruff.toml` file will take precedence over
the `ruff.toml` file, and the `ruff.toml` file will take precedence over the `pyproject.toml` file.

## Python file discovery

When passed a path on the command-line, Ruff will automatically discover all Python files in that
path, taking into account the [`exclude`](settings.md#exclude) and
[`extend-exclude`](settings.md#extend-exclude) settings in each directory's
`pyproject.toml` file.

By default, Ruff will also skip any files that are omitted via `.ignore`, `.gitignore`,
`.git/info/exclude`, and global `gitignore` files (see: [`respect-gitignore`](settings.md#respect-gitignore)).

Files that are passed to `ruff` directly are always linted, regardless of the above criteria.
For example, `ruff check /path/to/excluded/file.py` will always lint `file.py`.

## Jupyter Notebook discovery

Ruff has built-in experimental support for linting [Jupyter Notebooks](https://jupyter.org/).

To opt in to linting Jupyter Notebook (`.ipynb`) files, add the `*.ipynb` pattern to your
[`include`](settings.md#include) setting, like so:

```toml
[tool.ruff]
include = ["*.py", "*.pyi", "**/pyproject.toml", "*.ipynb"]
```

This will prompt Ruff to discover Jupyter Notebook (`.ipynb`) files in any specified
directories, and lint them accordingly.

Alternatively, pass the notebook file(s) to `ruff` on the command-line directly. For example,
`ruff check /path/to/notebook.ipynb` will always lint `notebook.ipynb`.

## Rule selection

The set of enabled rules is controlled via the [`select`](settings.md#select) and
[`ignore`](settings.md#ignore) settings, along with the
[`extend-select`](settings.md#extend-select) and
[`extend-ignore`](settings.md#extend-ignore) modifiers.

To resolve the enabled rule set, Ruff may need to reconcile `select` and `ignore` from a variety
of sources, including the current `pyproject.toml`, any inherited `pyproject.toml` files, and the
CLI (e.g., `--select`).

In those scenarios, Ruff uses the "highest-priority" `select` as the basis for the rule set, and
then applies any `extend-select`, `ignore`, and `extend-ignore` adjustments. CLI options are given
higher priority than `pyproject.toml` options, and the current `pyproject.toml` file is given higher
priority than any inherited `pyproject.toml` files.

For example, given the following `pyproject.toml` file:

```toml
[tool.ruff]
select = ["E", "F"]
ignore = ["F401"]
```

Running `ruff check --select F401` would result in Ruff enforcing `F401`, and no other rules.

Running `ruff check --extend-select B` would result in Ruff enforcing the `E`, `F`, and `B` rules,
with the exception of `F401`.

## Error suppression

To omit a lint rule entirely, add it to the "ignore" list via [`ignore`](settings.md#ignore)
or [`extend-ignore`](settings.md#extend-ignore), either on the command-line
or in your `pyproject.toml` file.

To ignore a violation inline, Ruff uses a `noqa` system similar to
[Flake8](https://flake8.pycqa.org/en/3.1.1/user/ignoring-errors.html). To ignore an individual
violation, add `# noqa: {code}` to the end of the line, like so:

```python
# Ignore F841.
x = 1  # noqa: F841

# Ignore E741 and F841.
i = 1  # noqa: E741, F841

# Ignore _all_ violations.
x = 1  # noqa
```

Note that, for multi-line strings, the `noqa` directive should come at the end of the string, and
will apply to the entire string, like so:

```python
"""Lorem ipsum dolor sit amet.

Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor.
"""  # noqa: E501
```

To ignore all violations across an entire file, add `# ruff: noqa` to any line in the file, like so:

```python
# ruff: noqa
```

To ignore a specific rule across an entire file, add `# ruff: noqa: {code}` to any line in the file,
like so:

```python
# ruff: noqa: F841
```

Or see the [`per-file-ignores`](settings.md#per-file-ignores) configuration
setting, which enables the same functionality via a `pyproject.toml` file.

Note that Ruff will also respect Flake8's `# flake8: noqa` directive, and will treat it as
equivalent to `# ruff: noqa`.

### Automatic `noqa` management

Ruff supports several workflows to aid in `noqa` management.

First, Ruff provides a special rule code, `RUF100`, to enforce that your `noqa` directives are
"valid", in that the violations they _say_ they ignore are actually being triggered on that line
(and thus suppressed). You can run `ruff check /path/to/file.py --extend-select RUF100` to flag
unused `noqa` directives.

Second, Ruff can _automatically remove_ unused `noqa` directives via its autofix functionality.
You can run `ruff check /path/to/file.py --extend-select RUF100 --fix` to automatically remove
unused `noqa` directives.

Third, Ruff can _automatically add_ `noqa` directives to all failing lines. This is useful when
migrating a new codebase to Ruff. You can run `ruff check /path/to/file.py --add-noqa` to
automatically add `noqa` directives to all failing lines, with the appropriate rule codes.

### Action comments

Ruff respects isort's [action comments](https://pycqa.github.io/isort/docs/configuration/action_comments.html)
(`# isort: skip_file`, `# isort: on`, `# isort: off`, `# isort: skip`, and `# isort: split`), which
enable selectively enabling and disabling import sorting for blocks of code and other inline
configuration.

Ruff will also respect variants of these action comments with a `# ruff:` prefix
(e.g., `# ruff: isort: skip_file`, `# ruff: isort: on`, and so on). These variants more clearly
convey that the action comment is intended for Ruff, but are functionally equivalent to the
isort variants.

See the [isort documentation](https://pycqa.github.io/isort/docs/configuration/action_comments.html)
for more.

## Exit codes

By default, Ruff exits with the following status codes:

- `0` if no violations were found, or if all present violations were fixed automatically.
- `1` if violations were found.
- `2` if Ruff terminates abnormally due to invalid configuration, invalid CLI options, or an
  internal error.

This convention mirrors that of tools like ESLint, Prettier, and RuboCop.

Ruff supports two command-line flags that alter its exit code behavior:

- `--exit-zero` will cause Ruff to exit with a status code of `0` even if violations were found.
  Note that Ruff will still exit with a status code of `2` if it terminates abnormally.
- `--exit-non-zero-on-fix` will cause Ruff to exit with a status code of `1` if violations were
  found, _even if_ all such violations were fixed automatically. Note that the use of
  `--exit-non-zero-on-fix` can result in a non-zero exit code even if no violations remain after
  autofixing.

## Autocompletion

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
