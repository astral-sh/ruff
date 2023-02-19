<!-- Begin section: Overview -->

# Ruff

[![Ruff](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/charliermarsh/ruff/main/assets/badge/v1.json)](https://github.com/charliermarsh/ruff)
[![image](https://img.shields.io/pypi/v/ruff.svg)](https://pypi.python.org/pypi/ruff)
[![image](https://img.shields.io/pypi/l/ruff.svg)](https://pypi.python.org/pypi/ruff)
[![image](https://img.shields.io/pypi/pyversions/ruff.svg)](https://pypi.python.org/pypi/ruff)
[![Actions status](https://github.com/charliermarsh/ruff/workflows/CI/badge.svg)](https://github.com/charliermarsh/ruff/actions)

[**Discord**](https://discord.gg/c9MhzV8aU5) | [**Docs**](https://beta.ruff.rs/docs/) | [**Playground**](https://play.ruff.rs/)

An extremely fast Python linter, written in Rust.

<p align="center">
  <picture align="center">
    <source media="(prefers-color-scheme: dark)" srcset="https://user-images.githubusercontent.com/1309177/212613422-7faaf278-706b-4294-ad92-236ffcab3430.svg">
    <source media="(prefers-color-scheme: light)" srcset="https://user-images.githubusercontent.com/1309177/212613257-5f4bca12-6d6b-4c79-9bac-51a4c6d08928.svg">
    <img alt="Shows a bar chart with benchmark results." src="https://user-images.githubusercontent.com/1309177/212613257-5f4bca12-6d6b-4c79-9bac-51a4c6d08928.svg">
  </picture>
</p>

<p align="center">
  <i>Linting the CPython codebase from scratch.</i>
</p>

* ⚡️  10-100x faster than existing linters
* 🐍  Installable via `pip`
* 🛠️  `pyproject.toml` support
* 📦  Built-in caching, to avoid re-analyzing unchanged files
* 🔧  Autofix support, for automatic error correction (e.g., automatically remove unused imports)
* 📏  Over [400 built-in rules](https://beta.ruff.rs/docs/rules/) (and growing)
* ⚖️  [Near-parity](https://beta.ruff.rs/docs/faq/#how-does-ruff-compare-to-flake8) with the built-in Flake8 rule set
* 🔌  Native re-implementations of dozens of Flake8 plugins, like flake8-bugbear
* ⌨️  First-party editor integrations for [VS Code](https://github.com/charliermarsh/ruff-vscode) and [more](https://github.com/charliermarsh/ruff-lsp)
* 🌎  Monorepo-friendly, with [hierarchical and cascading configuration](#pyprojecttoml-discovery)

Ruff aims to be orders of magnitude faster than alternative tools while integrating more
functionality behind a single, common interface.

Ruff can be used to replace [Flake8](https://pypi.org/project/flake8/) (plus dozens of plugins),
[isort](https://pypi.org/project/isort/), [pydocstyle](https://pypi.org/project/pydocstyle/),
[yesqa](https://github.com/asottile/yesqa), [eradicate](https://pypi.org/project/eradicate/),
[pyupgrade](https://pypi.org/project/pyupgrade/), and [autoflake](https://pypi.org/project/autoflake/),
all while executing tens or hundreds of times faster than any individual tool.

Ruff is extremely actively developed and used in major open-source projects like:

* [pandas](https://github.com/pandas-dev/pandas)
* [FastAPI](https://github.com/tiangolo/fastapi)
* [Transformers (Hugging Face)](https://github.com/huggingface/transformers)
* [Apache Airflow](https://github.com/apache/airflow)
* [SciPy](https://github.com/scipy/scipy)

...and many more.

Read the [launch blog post](https://notes.crmarsh.com/python-tooling-could-be-much-much-faster) or
the most recent [project update](https://notes.crmarsh.com/ruff-the-first-200-releases).

## Testimonials

[**Sebastián Ramírez**](https://twitter.com/tiangolo/status/1591912354882764802), creator
of [FastAPI](https://github.com/tiangolo/fastapi):

> Ruff is so fast that sometimes I add an intentional bug in the code just to confirm it's actually
> running and checking the code.

[**Nick Schrock**](https://twitter.com/schrockn/status/1612615862904827904), founder of [Elementl](https://www.elementl.com/),
co-creator of [GraphQL](https://graphql.org/):

> Why is Ruff a gamechanger? Primarily because it is nearly 1000x faster. Literally. Not a typo. On
> our largest module (dagster itself, 250k LOC) pylint takes about 2.5 minutes, parallelized across 4
> cores on my M1. Running ruff against our _entire_ codebase takes .4 seconds.

[**Bryan Van de Ven**](https://github.com/bokeh/bokeh/pull/12605), co-creator
of [Bokeh](https://github.com/bokeh/bokeh/), original author
of [Conda](https://docs.conda.io/en/latest/):

> Ruff is ~150-200x faster than flake8 on my machine, scanning the whole repo takes ~0.2s instead of
> ~20s. This is an enormous quality of life improvement for local dev. It's fast enough that I added
> it as an actual commit hook, which is terrific.

[**Timothy Crosley**](https://twitter.com/timothycrosley/status/1606420868514877440),
creator of [isort](https://github.com/PyCQA/isort):

> Just switched my first project to Ruff. Only one downside so far: it's so fast I couldn't believe it was working till I intentionally introduced some errors.

[**Tim Abbott**](https://github.com/charliermarsh/ruff/issues/465#issuecomment-1317400028), lead
developer of [Zulip](https://github.com/zulip/zulip):

> This is just ridiculously fast... `ruff` is amazing.

<!-- End section: Overview -->

## Table of Contents

For more, see the [documentation](https://beta.ruff.rs/docs/).

1. [Installation and Usage](#installation-and-usage)
2. [Configuration](#configuration)
3. [Supported Rules](#supported-rules)
4. [Contributing](#contributing)
5. [Support](#support)
6. [Acknowledgements](#acknowledgements)
7. [Who's Using Ruff?](#whos-using-ruff)
8. [License](#license)

## Installation and Usage

For more, see the [documentation](https://beta.ruff.rs/docs/).

<!-- Begin section: Installation and Usage -->

### Installation

Ruff is available as [`ruff`](https://pypi.org/project/ruff/) on PyPI:

```shell
pip install ruff
```

For **macOS Homebrew** and **Linuxbrew** users, Ruff is also available as [`ruff`](https://formulae.brew.sh/formula/ruff) on Homebrew:

```shell
brew install ruff
```

For **Conda** users, Ruff is also available as [`ruff`](https://anaconda.org/conda-forge/ruff) on `conda-forge`:

```shell
conda install -c conda-forge ruff
```

For **Arch Linux** users, Ruff is also available as [`ruff`](https://archlinux.org/packages/community/x86_64/ruff/) on the official repositories:

```shell
pacman -S ruff
```

For **Alpine** users, Ruff is also available as [`ruff`](https://pkgs.alpinelinux.org/package/edge/testing/x86_64/ruff) on the testing repositories:

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
  rev: 'v0.0.248'
  hooks:
    - id: ruff
```

Or, to enable autofix:

```yaml
- repo: https://github.com/charliermarsh/ruff-pre-commit
  # Ruff version.
  rev: 'v0.0.248'
  hooks:
    - id: ruff
      args: [--fix, --exit-non-zero-on-fix]
```

<!-- End section: Installation and Usage -->

## Configuration

<!-- Begin section: Configuration -->

Ruff can be configured via a `pyproject.toml` file, a `ruff.toml` file, or through the command line.

For a complete enumeration of the available configuration options, see the
[documentation](https://beta.ruff.rs/docs/settings/).

### Configure via `pyproject.toml`

If left unspecified, the default configuration is equivalent to:

```toml
[tool.ruff]
# Enable Pyflakes `E` and `F` codes by default.
select = ["E", "F"]
ignore = []

# Allow autofix for all enabled rules (when `--fix`) is provided.
fixable = ["A", "B", "C", "D", "E", "F", "..."]
unfixable = []

# Exclude a variety of commonly ignored directories.
exclude = [
    ".bzr",
    ".direnv",
    ".eggs",
    ".git",
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

[tool.ruff.mccabe]
# Unlike Flake8, default to a complexity level of 10.
max-complexity = 10
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

Ruff mirrors Flake8's rule code system, in which each rule code consists of a one-to-three letter
prefix, followed by three digits (e.g., `F401`). The prefix indicates that "source" of the rule
(e.g., `F` for Pyflakes, `E` for pycodestyle, `ANN` for flake8-annotations). The set of enabled
rules is determined by the `select` and `ignore` options, which support both the full code (e.g.,
`F401`) and the prefix (e.g., `F`).

As a special-case, Ruff also supports the `ALL` code, which enables all rules. Note that some of the
pydocstyle rules conflict (e.g., `D203` and `D211`) as they represent alternative docstring
formats. Enabling `ALL` without further configuration may result in suboptimal behavior, especially
for the pydocstyle plugin.

If you're wondering how to configure Ruff, here are some **recommended guidelines**:

* Prefer `select` and `ignore` over `extend-select` and `extend-ignore`, to make your rule set
  explicit.
* Use `ALL` with discretion. Enabling `ALL` will implicitly enable new rules whenever you upgrade.
* Start with a small set of rules (`select = ["E", "F"]`) and add a category at-a-time. For example,
  you might consider expanding to `select = ["E", "F", "B"]` to enable the popular flake8-bugbear
  extension.
* By default, Ruff's autofix is aggressive. If you find that it's too aggressive for your liking,
  consider turning off autofix for specific rules or categories (see: [FAQ](https://beta.ruff.rs/docs/faq/#ruff-tried-to-fix-something-but-it-broke-my-code-what-should-i-do)).

### Configure via `ruff.toml`

As an alternative to `pyproject.toml`, Ruff will also respect a `ruff.toml` file, which implements
an equivalent schema (though the `[tool.ruff]` hierarchy can be omitted). For example, the
`pyproject.toml` described above would be represented via the following `ruff.toml`:

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

For a full list of configurable options, see the [list of all options](https://beta.ruff.rs/docs/settings/).

### Command-line interface

Some configuration settings can be provided via the command-line, such as those related to
rule enablement and disablement, file discovery, logging level, and more:

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
          Avoid writing any fixed files back; instead, output a diff for each changed file to stdout
  -w, --watch
          Run in watch mode by re-running whenever files change
      --fix-only
          Fix any fixable lint violations, but don't report on leftover violations. Implies `--fix`
      --format <FORMAT>
          Output serialization format for violations [env: RUFF_FORMAT=] [possible values: text, json, junit, grouped, github, gitlab, pylint]
      --target-version <TARGET_VERSION>
          The minimum Python version that should be supported
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
          Like --select, but adds additional rule codes on top of the selected ones
      --per-file-ignores <PER_FILE_IGNORES>
          List of mappings from file pattern to code to exclude
      --fixable <RULE_CODE>
          List of rule codes to treat as eligible for autofix. Only applicable when autofix itself is enabled (e.g., via `--fix`)
      --unfixable <RULE_CODE>
          List of rule codes to treat as ineligible for autofix. Only applicable when autofix itself is enabled (e.g., via `--fix`)

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

### `pyproject.toml` discovery

Similar to [ESLint](https://eslint.org/docs/latest/user-guide/configuring/configuration-files#cascading-and-hierarchy),
Ruff supports hierarchical configuration, such that the "closest" `pyproject.toml` file in the
directory hierarchy is used for every individual file, with all paths in the `pyproject.toml` file
(e.g., `exclude` globs, `src` paths) being resolved relative to the directory containing the
`pyproject.toml` file.

There are a few exceptions to these rules:

1. In locating the "closest" `pyproject.toml` file for a given path, Ruff ignores any
   `pyproject.toml` files that lack a `[tool.ruff]` section.
2. If a configuration file is passed directly via `--config`, those settings are used for across
   files. Any relative paths in that configuration file (like `exclude` globs or `src` paths) are
   resolved relative to the _current working directory_.
3. If no `pyproject.toml` file is found in the filesystem hierarchy, Ruff will fall back to using
   a default configuration. If a user-specific configuration file exists
   at `${config_dir}/ruff/pyproject.toml`, that file will be used instead of the default
   configuration, with `${config_dir}` being determined via the [`dirs`](https://docs.rs/dirs/4.0.0/dirs/fn.config_dir.html)
   crate, and all relative paths being again resolved relative to the _current working directory_.
4. Any `pyproject.toml`-supported settings that are provided on the command-line (e.g., via
   `--select`) will override the settings in _every_ resolved configuration file.

Unlike [ESLint](https://eslint.org/docs/latest/user-guide/configuring/configuration-files#cascading-and-hierarchy),
Ruff does not merge settings across configuration files; instead, the "closest" configuration file
is used, and any parent configuration files are ignored. In lieu of this implicit cascade, Ruff
supports an [`extend`](https://beta.ruff.rs/docs/settings#extend) field, which allows you to inherit the settings from another
`pyproject.toml` file, like so:

```toml
# Extend the `pyproject.toml` file in the parent directory.
extend = "../pyproject.toml"
# But use a different line length.
line-length = 100
```

All of the above rules apply equivalently to `ruff.toml` files. If Ruff detects both a `ruff.toml`
and `pyproject.toml` file, it will defer to the `ruff.toml`.

### Python file discovery

When passed a path on the command-line, Ruff will automatically discover all Python files in that
path, taking into account the [`exclude`](https://beta.ruff.rs/docs/settings#exclude) and
[`extend-exclude`](https://beta.ruff.rs/docs/settings#extend-exclude) settings in each directory's
`pyproject.toml` file.

By default, Ruff will also skip any files that are omitted via `.ignore`, `.gitignore`,
`.git/info/exclude`, and global `gitignore` files (see: [`respect-gitignore`](https://beta.ruff.rs/docs/settings#respect-gitignore)).

Files that are passed to `ruff` directly are always linted, regardless of the above criteria.
For example, `ruff check /path/to/excluded/file.py` will always lint `file.py`.

### Rule resolution

The set of enabled rules is controlled via the [`select`](https://beta.ruff.rs/docs/settings#select)
and [`ignore`](https://beta.ruff.rs/docs/settings#ignore) settings, along with the
[`extend-select`](https://beta.ruff.rs/docs/settings#extend-select) and
[`extend-ignore`](https://beta.ruff.rs/docs/settings#extend-ignore) modifiers.

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

Running `ruff check --extend-select B` would result in Ruff enforcing the `E`, `F`, and `B` rules, with
the exception of `F401`.

### Suppressing errors

To omit a lint rule entirely, add it to the "ignore" list via [`ignore`](https://beta.ruff.rs/docs/settings#ignore)
or [`extend-ignore`](https://beta.ruff.rs/docs/settings#extend-ignore), either on the command-line
or in your `pyproject.toml` file.

To ignore a violation inline, Ruff uses a `noqa` system similar to [Flake8](https://flake8.pycqa.org/en/3.1.1/user/ignoring-errors.html).
To ignore an individual violation, add `# noqa: {code}` to the end of the line, like so:

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

Or see the [`per-file-ignores`](https://beta.ruff.rs/docs/settings#per-file-ignores) configuration
setting, which enables the same functionality via a `pyproject.toml` file.

Note that Ruff will also respect Flake8's `# flake8: noqa` directive, and will treat it as
equivalent to `# ruff: noqa`.

#### Automatic error suppression

Ruff supports several workflows to aid in `noqa` management.

First, Ruff provides a special rule code, `RUF100`, to enforce that your `noqa` directives are
"valid", in that the violations they _say_ they ignore are actually being triggered on that line (and
thus suppressed). You can run `ruff check /path/to/file.py --extend-select RUF100` to flag unused `noqa`
directives.

Second, Ruff can _automatically remove_ unused `noqa` directives via its autofix functionality.
You can run `ruff check /path/to/file.py --extend-select RUF100 --fix` to automatically remove unused
`noqa` directives.

Third, Ruff can _automatically add_ `noqa` directives to all failing lines. This is useful when
migrating a new codebase to Ruff. You can run `ruff check /path/to/file.py --add-noqa` to automatically
add `noqa` directives to all failing lines, with the appropriate rule codes.

#### Action comments

Ruff respects `isort`'s [action comments](https://pycqa.github.io/isort/docs/configuration/action_comments.html)
(`# isort: skip_file`, `# isort: on`, `# isort: off`, `# isort: skip`, and `# isort: split`), which
enable selectively enabling and disabling import sorting for blocks of code and other inline
configuration.

See the [`isort` documentation](https://pycqa.github.io/isort/docs/configuration/action_comments.html)
for more.

### Exit codes

By default, Ruff exits with the following status codes:

* `0` if no violations were found, or if all present violations were fixed automatically.
* `1` if violations were found.
* `2` if Ruff terminates abnormally due to invalid configuration, invalid CLI options, or an internal error.

This convention mirrors that of tools like ESLint, Prettier, and RuboCop.

Ruff supports two command-line flags that alter its exit code behavior:

* `--exit-zero` will cause Ruff to exit with a status code of `0` even if violations were found.
  Note that Ruff will still exit with a status code of `2` if it terminates abnormally.
* `--exit-non-zero-on-fix` will cause Ruff to exit with a status code of `1` if violations were
  found, _even if_ all such violations were fixed automatically. Note that the use of
  `--exit-non-zero-on-fix` can result in a non-zero exit code even if no violations remain after
  autofixing.

### Autocompletion

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

<!-- End section: Configuration -->

## Supported Rules

<!-- Begin section: Rules -->

Ruff supports over 400 lint rules, many of which are inspired by popular tools like Flake8, isort,
pyupgrade, and others. Regardless of the rule's origin, Ruff re-implements every rule in
Rust as a first-party feature.

By default, Ruff enables Flake8's `E` and `F` rules. Ruff supports all rules from the `F` category,
and a [subset](https://beta.ruff.rs/docs/rules/#error-e) of the `E` category, omitting those
stylistic rules made obsolete by the use of an autoformatter, like [Black](https://github.com/psf/black).

<!-- End section: Rules -->

For a complete enumeration, see the [list of rules](https://beta.ruff.rs/docs/rules/) in the
Ruff documentation.

## Contributing

Contributions are welcome and highly appreciated. To get started, check out the
[**contributing guidelines**](https://github.com/charliermarsh/ruff/blob/main/CONTRIBUTING.md). You
can also join us on [**Discord**](https://discord.gg/c9MhzV8aU5).

## Support

Having trouble? Check out the existing issues on [**GitHub**](https://github.com/charliermarsh/ruff/issues),
or feel free to [**open a new one**](https://github.com/charliermarsh/ruff/issues/new).

You can also ask for help on [**Discord**](https://discord.gg/c9MhzV8aU5).

<!-- Begin section: Acknowledgements -->

## Acknowledgements

Ruff's linter draws on both the APIs and implementation details of many other
tools in the Python ecosystem, especially [Flake8](https://github.com/PyCQA/flake8), [Pyflakes](https://github.com/PyCQA/pyflakes),
[pycodestyle](https://github.com/PyCQA/pycodestyle), [pydocstyle](https://github.com/PyCQA/pydocstyle),
[pyupgrade](https://github.com/asottile/pyupgrade), and [isort](https://github.com/PyCQA/isort).

In some cases, Ruff includes a "direct" Rust port of the corresponding tool.
We're grateful to the maintainers of these tools for their work, and for all
the value they've provided to the Python community.

Ruff's autoformatter is built on a fork of Rome's [`rome_formatter`](https://github.com/rome/tools/tree/main/crates/rome_formatter),
and again draws on both the APIs and implementation details of [Rome](https://github.com/rome/tools),
[Prettier](https://github.com/prettier/prettier), and [Black](https://github.com/psf/black).

Ruff is also influenced by a number of tools outside the Python ecosystem, like
[Clippy](https://github.com/rust-lang/rust-clippy) and [ESLint](https://github.com/eslint/eslint).

Ruff is the beneficiary of a large number of [contributors](https://github.com/charliermarsh/ruff/graphs/contributors).

Ruff is released under the MIT license.

<!-- End section: Acknowledgements -->

## Who's Using Ruff?

Ruff is used in a number of major open-source projects, including:

* [pandas](https://github.com/pandas-dev/pandas)
* [FastAPI](https://github.com/tiangolo/fastapi)
* [Transformers (Hugging Face)](https://github.com/huggingface/transformers)
* [Diffusers (Hugging Face)](https://github.com/huggingface/diffusers)
* [Apache Airflow](https://github.com/apache/airflow)
* [SciPy](https://github.com/scipy/scipy)
* [Zulip](https://github.com/zulip/zulip)
* [Bokeh](https://github.com/bokeh/bokeh)
* [Pydantic](https://github.com/pydantic/pydantic)
* [Dagster](https://github.com/dagster-io/dagster)
* [Dagger](https://github.com/dagger/dagger)
* [Sphinx](https://github.com/sphinx-doc/sphinx)
* [Hatch](https://github.com/pypa/hatch)
* [Jupyter](https://github.com/jupyter-server/jupyter_server)
* [Great Expectations](https://github.com/great-expectations/great_expectations)
* [Polars](https://github.com/pola-rs/polars)
* [Ibis](https://github.com/ibis-project/ibis)
* [Synapse (Matrix)](https://github.com/matrix-org/synapse)
* [SnowCLI (Snowflake)](https://github.com/Snowflake-Labs/snowcli)
* [Dispatch (Netflix)](https://github.com/Netflix/dispatch)
* [Saleor](https://github.com/saleor/saleor)
* [Pynecone](https://github.com/pynecone-io/pynecone)
* [OpenBB](https://github.com/OpenBB-finance/OpenBBTerminal)
* [Home Assistant](https://github.com/home-assistant/core)
* [Pylint](https://github.com/PyCQA/pylint)
* [Cryptography (PyCA)](https://github.com/pyca/cryptography)
* [cibuildwheel (PyPA)](https://github.com/pypa/cibuildwheel)
* [build (PyPA)](https://github.com/pypa/build)
* [Babel](https://github.com/python-babel/babel)
* [featuretools](https://github.com/alteryx/featuretools)
* [meson-python](https://github.com/mesonbuild/meson-python)

## License

MIT
