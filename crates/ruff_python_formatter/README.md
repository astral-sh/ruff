# Ruff Formatter

The Ruff formatter is an extremely fast Python code formatter that ships as part of the `ruff`
CLI (as of Ruff v0.0.289).

The formatter is currently in an **Alpha** state. The Alpha is primarily intended for
experimentation: our focus is on collecting feedback that we can address prior to a production-ready
Beta release later this year. (While we're using the formatter in production on our own projects,
the CLI, configuration options, and code style may change arbitrarily between the Alpha and Beta.)

[_We'd love to hear your feedback._](https://github.com/astral-sh/ruff/discussions/7310)

## Goals

The formatter is designed to be a drop-in replacement for [Black](https://github.com/psf/black),
but with an excessive focus on performance and direct integration with Ruff.

Specifically, the formatter is intended to emit near-identical output when run over Black-formatted
code. When run over extensive Black-formatted projects like Django and Zulip, > 99.9% of lines
are formatted identically. When migrating an existing project from Black to Ruff, you should expect
to see a few differences on the margins, but the vast majority of your code should be unchanged.

If you identify deviations in your project, spot-check them against the [intentional deviations](#intentional-deviations)
enumerated below, as well as the [unintentional deviations](https://github.com/astral-sh/ruff/issues?q=is%3Aopen+is%3Aissue+label%3Aformatter)
filed in the issue tracker. If you've identified a new deviation, please [file an issue](https://github.com/astral-sh/ruff/issues/new).

When run over _non_-Black-formatted code, the formatter makes some different decisions than Black,
and so more deviations should be expected, especially around the treatment of end-of-line comments.
For details, see [Black compatibility](#black-compatibility).

## Getting started

The Ruff formatter shipped in an Alpha state as part of Ruff v0.0.289.

### CLI

The Ruff formatter is available as a standalone subcommand on the `ruff` CLI:

```console
❯ ruff format --help
Run the Ruff formatter on the given files or directories

Usage: ruff format [OPTIONS] [FILES]...

Arguments:
  [FILES]...  List of files or directories to format

Options:
      --check            Avoid writing any formatted files back; instead, exit with a non-zero status code if any files would have been modified, and zero otherwise
      --config <CONFIG>  Path to the `pyproject.toml` or `ruff.toml` file to use for configuration
  -h, --help             Print help

File selection:
      --respect-gitignore  Respect file exclusions via `.gitignore` and other standard ignore files
      --force-exclude      Enforce exclusions, even for paths passed to Ruff directly on the command-line

Miscellaneous:
      --isolated                         Ignore all configuration files
      --stdin-filename <STDIN_FILENAME>  The name of the file when passing it through stdin

Log levels:
  -v, --verbose  Enable verbose logging
  -q, --quiet    Print diagnostics, but nothing else
  -s, --silent   Disable all logging (but still exit with status code "1" upon detecting diagnostics)
```

Note: `ruff format` is currently hidden by default and will not be visible when running
`ruff --help`.

Similar to Black, running `ruff format /path/to/file.py` will format the given file or directory
in-place, while `ruff format --check /path/to/file.py` will avoid writing any formatted files back,
instead exiting with a non-zero status code if any files are not already formatted.

### VS Code

As of `v2023.36.0`, the [Ruff VS Code extension](https://marketplace.visualstudio.com/items?itemName=charliermarsh.ruff)
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
setting, which can be provided via a `pyproject.toml` or `ruff.toml` file, or on the CLI, as in:

```console
ruff format --line-length 100 /path/to/file.py
```

In future releases, the Ruff formatter will likely support configuration of:

- Quote style (single vs. double).
- Line endings (LF vs. CRLF).
- Indentation (tabs vs. spaces).
- Tab width.

### Excluding code from formatting

Ruff supports Black's `# fmt: off`, `# fmt: on`, and `# fmt: skip` pragmas, with a few caveats.

See Ruff's [suppression comment proposal](https://github.com/astral-sh/ruff/discussions/6338) for
details.

## Black compatibility

The formatter is designed to be a drop-in replacement for [Black](https://github.com/psf/black).

Specifically, the formatter is intended to emit near-identical output when run over Black-formatted
code. When migrating an existing project from Black to Ruff, you should expect to see a few
differences on the margins, but the vast majority of your code should be formatted identically.
Note, however, that the formatter does not yet implement or support Black's preview style.

When run over _non_-Black-formatted code, the formatter makes some different decisions than Black,
and so more deviations should be expected.

### Intentional deviations

This section enumerates the known, intentional deviations between the Ruff formatter and Black's
stable style. (Unintentional deviations are tracked in the [issue tracker](https://github.com/astral-sh/ruff/issues?q=is%3Aopen+is%3Aissue+label%3Aformatter).)

#### Trailing end-of-line comments

Black's priority is to fit an entire statement on a line, even if it contains end-of-line comments.
In such cases, Black collapses the statement, and moves the comment to the end of the collapsed
statement:

```python
# Input
while (
    cond1  # almost always true
    and cond2  # almost never true
):
    print("Do something")

# Black
while cond1 and cond2:  # almost always true  # almost never true
    print("Do something")
```

Ruff, like [Prettier](https://prettier.io/), expands any statement that contains trailing
end-of-line comments. For example, Ruff would avoid collapsing the `while` test in the snippet
above. This ensures that the comments remain close to their original position and retain their
original intent, at the cost of retaining additional vertical space.

This deviation only impacts unformatted code, in that Ruff's output should not deviate for code that
has already been formatted by Black.

### Pragma comments are ignored when computing line width

Pragma comments (`# type`, `# noqa`, `# pyright`, `# pylint`, etc.) are ignored when computing the width of a line.
This prevents Ruff from moving pragma comments around, thereby modifying their meaning and behavior:

See Ruff's [pragma comment handling proposal](https://github.com/astral-sh/ruff/discussions/6670)
for details.

This is similar to [Pyink](https://github.com/google/pyink) but a deviation from Black. Black avoids
splitting any lines that contain a `# type` comment ([#997](https://github.com/psf/black/issues/997)),
but otherwise avoids special-casing pragma comments.

As Ruff expands trailing end-of-line comments, Ruff will also avoid moving pragma comments in cases
like the following, where moving the `# noqa` to the end of the line causes it to suppress errors
on both `first()` and `second()`:

```python
# Input
[
    first(),  # noqa
    second()
]

# Black
[first(), second()]  # noqa

# Ruff
[
    first(),  # noqa
    second(),
]
```

### Line width vs. line length

Ruff uses the Unicode width of a line to determine if a line fits. Black's stable style uses
character width, while Black's preview style uses Unicode width for strings ([#3445](https://github.com/psf/black/pull/3445)),
and character width for all other tokens. Ruff's behavior is closer to Black's preview style than
Black's stable style, although Ruff _also_ uses Unicode width for identifiers and comments.

### Walruses in slice expressions

Black avoids inserting space around `:=` operators within slices. For example, the following adheres
to Black stable style:

```python
# Input
x[y:=1]

# Black
x[y:=1]
```

Ruff will instead add space around the `:=` operator:

```python
# Input
x[y:=1]

# Ruff
x[y := 1]
```

This will likely be incorporated into Black's preview style ([#3823](https://github.com/psf/black/pull/3823)).

### `global` and `nonlocal` names are broken across multiple lines by continuations

If a `global` or `nonlocal` statement includes multiple names, and exceeds the configured line
width, Ruff will break them across multiple lines using continuations:

```python
# Input
global analyze_featuremap_layer, analyze_featuremapcompression_layer, analyze_latencies_post, analyze_motions_layer, analyze_size_model

# Ruff
global \
    analyze_featuremap_layer, \
    analyze_featuremapcompression_layer, \
    analyze_latencies_post, \
    analyze_motions_layer, \
    analyze_size_model
```

### Newlines are inserted after all class docstrings

Black typically enforces a single newline after a class docstring. However, it does not apply such
formatting if the docstring is single-quoted rather than triple-quoted, while Ruff enforces a
single newline in both cases:

```python
# Input
class IntFromGeom(GEOSFuncFactory):
    "Argument is a geometry, return type is an integer."
    argtypes = [GEOM_PTR]
    restype = c_int
    errcheck = staticmethod(check_minus_one)

# Black
class IntFromGeom(GEOSFuncFactory):
    "Argument is a geometry, return type is an integer."
    argtypes = [GEOM_PTR]
    restype = c_int
    errcheck = staticmethod(check_minus_one)

# Ruff
class IntFromGeom(GEOSFuncFactory):
    "Argument is a geometry, return type is an integer."

    argtypes = [GEOM_PTR]
    restype = c_int
    errcheck = staticmethod(check_minus_one)
```

### Trailing own-line comments on imports are not moved to the next line

Black enforces a single empty line between an import and a trailing own-line comment. Ruff leaves
such comments in-place:

```python
# Input
import os
# comment

import sys

# Black
import os

# comment

import sys

# Ruff
import os
# comment

import sys
```
