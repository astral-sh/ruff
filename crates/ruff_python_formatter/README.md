# Ruff Formatter

The Ruff formatter is an extremely fast Python code formatter that ships as part of the `ruff`
CLI (as of Ruff v0.0.287).

The formatter is currently in an **alpha** state. As such, it's not yet recommended for production
use, but it _is_ ready for experimentation and testing. _We'd love to have your feedback._

## Goals

The formatter is designed to be a drop-in replacement for [Black](https://github.com/psf/black),
but with an excessive focus on performance and direct integration with Ruff.

Specifically, the formatter is intended to emit near-identical output when run over Black-formatted
code, achieving Jaccard similarity scores of over 0.999 on extensive Black-formatted projects like
Django and Zulip. When migrating an existing project from Black to Ruff, you should expect to see
a few differences on the margins, but the vast majority of your code should be formatted
identically.

If you identify deviations in your project, spot-check them against the [intentional deviations](#intentional-deviations)
enumerated below, as well as the [unintentional deviations](https://github.com/astral-sh/ruff/issues?q=is%3Aopen+is%3Aissue+label%3Aformatter)
filed in the issue tracker. If you've identified a new deviation, feel free to [file an issue](https://github.com/astral-sh/ruff/issues/new).

When run over _non_-Black-formatted code, the formatter makes some different decisions than Black,
and so more deviations should be expected, especially around the treatment of end-of-line comments.
For details, see [Black compatibility](#black-compatibility).

## Getting started

The Ruff formatter shipped in an alpha state as part of Ruff v0.0.287.

### CLI

The Ruff formatter is available as a standalone subcommand on the `ruff` CLI:

```console
❯ ruff format --help
Run the Ruff formatter on the given files or directories

Usage: ruff format [OPTIONS] [FILES]...

Arguments:
  [FILES]...  List of files or directories to format

Options:
      --check
          Avoid writing any formatted files back; instead, exit with a non-zero status code if any files would have been modified, and zero otherwise
  -o, --output-file <OUTPUT_FILE>
          Specify file to write the formatter output to (default: stdout)
      --target-version <TARGET_VERSION>
          The minimum Python version that should be supported [possible values: py37, py38, py39, py310, py311, py312]
      --config <CONFIG>
          Path to the `pyproject.toml` or `ruff.toml` file to use for configuration
  -h, --help
          Print help

File selection:
      --respect-gitignore  Respect file exclusions via `.gitignore` and other standard ignore files
      --force-exclude      Enforce exclusions, even for paths passed to Ruff directly on the command-line

Miscellaneous:
      --isolated                         Ignore all configuration files
      --stdin-filename <STDIN_FILENAME>  The name of the file when passing it through stdin

Log levels:
  -v, --verbose  Enable verbose logging
  -q, --quiet    Print lint violations, but nothing else
  -s, --silent   Disable all logging (but still exit with status code "1" upon detecting lint violations)
```

Note: `ruff format` is currently hidden by default and will not be visible when running
`ruff --help`.

Similar to Black, running `ruff format /path/to/file.py` will format the given file or directory
in-place, while `ruff format --check /path/to/file.py` will avoid writing any formatted files back,
instead exiting with a non-zero status code if any files are not already formatted.

In future releases, the Ruff formatter will be integrated into `ruff check`.

### VS Code

As of `v2023.34.0`,
the [Ruff VS Code extension](https://marketplace.visualstudio.com/items?itemName=charliermarsh.ruff)
ships with support for the Ruff formatter. To enable formatting capabilities, set the
`ruff.enableExperimentalFormatter` setting to `true` in your `settings.json`, and mark the Ruff
extension as your default Python formatter:

```json
{
  "ruff.enableExperimentalFormatter": true,
  "[python]": {
    "editor.defaultFormatter": "charliermarsh.ruff"
  }
}
```

From there, you can format a file by running the `Format Document` command, or enable formatting
on-save by adding `"editor.formatOnSave": true` to your `settings.json`:

```json
{
  "ruff.enableExperimentalFormatter": true,
  "[python]": {
    "editor.defaultFormatter": "charliermarsh.ruff",
    "editor.formatOnSave": true
  }
}
```

### Configuration

The Ruff formatter respects Ruff's [`line-length`](https://beta.ruff.rs/docs/settings/#line-length)
setting, which can be provided via a `pyproject.toml` or `ruff.toml` file, or on the CLI, as with
`ruff check`.

In future releases, the Ruff formatter will likely support configuration of quote style (single vs.
double) and indentation (width, and spaces vs. tabs).

## Black compatibility

The formatter is designed to be a drop-in replacement for [Black](https://github.com/psf/black).

Specifically, the formatter is intended to emit near-identical output when run over Black-formatted
code. When migrating an existing project from Black to Ruff, you should expect to see a few
differences on the margins, but the vast majority of your code should be formatted identically.
Note, however, that the formatter does not yet implement or support Black's preview style.

When run over _non_-Black-formatted code, the formatter makes some different decisions than Black,
and so more deviations should be expected.

### Intentional deviations

...
