# Ruff Formatter

The Ruff formatter is an extremely fast Python code formatter that ships as part of the `ruff`
CLI.

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

The Ruff formatter is available in Beta as of Ruff v0.1.2.

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
      --diff
          Avoid writing any formatted files back; instead, exit with a non-zero status code and the difference between the current file and how the formatted file would look like
      --config <CONFIG>
          Path to the `pyproject.toml` or `ruff.toml` file to use for configuration
      --target-version <TARGET_VERSION>
          The minimum Python version that should be supported [possible values: py37, py38, py39, py310, py311, py312]
      --preview
          Enable preview mode; enables unstable formatting. Use `--no-preview` to disable
  -h, --help
          Print help

Miscellaneous:
  -n, --no-cache                         Disable cache reads
      --cache-dir <CACHE_DIR>            Path to the cache directory [env: RUFF_CACHE_DIR=]
      --isolated                         Ignore all configuration files
      --stdin-filename <STDIN_FILENAME>  The name of the file when passing it through stdin

File selection:
      --respect-gitignore       Respect file exclusions via `.gitignore` and other standard ignore files. Use `--no-respect-gitignore` to disable
      --exclude <FILE_PATTERN>  List of paths, used to omit files and/or directories from analysis
      --force-exclude           Enforce exclusions, even for paths passed to Ruff directly on the command-line. Use `--no-force-exclude` to disable

Log levels:
  -v, --verbose  Enable verbose logging
  -q, --quiet    Print diagnostics, but nothing else
  -s, --silent   Disable all logging (but still exit with status code "1" upon detecting diagnostics)
```

Similar to Black, running `ruff format /path/to/file.py` will format the given file or directory
in-place, while `ruff format --check /path/to/file.py` will avoid writing any formatted files back,
instead exiting with a non-zero status code if any files are not already formatted.

### VS Code

As of `v2023.44.0`, the [Ruff VS Code extension](https://marketplace.visualstudio.com/items?itemName=charliermarsh.ruff)
ships with full support for the Ruff formatter. To enable formatting capabilities, mark the Ruff
extension as your default Python formatter:

```json
{
  "[python]": {
    "editor.defaultFormatter": "charliermarsh.ruff"
  }
}
```

From there, you can format a file by running the `Format Document` command, or enable formatting
on-save by adding `"editor.formatOnSave": true` to your `settings.json`:

```json
{
  "[python]": {
    "editor.defaultFormatter": "charliermarsh.ruff",
    "editor.formatOnSave": true
  }
}
```

### Configuration

The Ruff formatter allows configuration of [indent style](https://docs.astral.sh/ruff/settings/#format-indent-style),
[line ending](https://docs.astral.sh/ruff/settings/#format-line-ending), [quote style](https://docs.astral.sh/ruff/settings/#format-quote-style),
and [magic trailing comma behavior](https://docs.astral.sh/ruff/settings/#format-skip-magic-trailing-comma).
Like the linter, the Ruff formatter reads configuration via `pyproject.toml` or `ruff.toml` files,
as in:

```toml
[tool.ruff.format]
# Use tabs instead of 4 space indentation.
indent-style = "tab"

# Prefer single quotes over double quotes.
quote-style = "single"
```

The Ruff formatter also respects Ruff's [`line-length`](https://docs.astral.sh/ruff/settings/#line-length)
setting, which also can be provided via a `pyproject.toml` or `ruff.toml` file, or on the CLI, as
in:

```console
ruff format --line-length 100 /path/to/file.py
```

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

#### Pragma comments are ignored when computing line width

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

#### Line width vs. line length

Ruff uses the Unicode width of a line to determine if a line fits. Black's stable style uses
character width, while Black's preview style uses Unicode width for strings ([#3445](https://github.com/psf/black/pull/3445)),
and character width for all other tokens. Ruff's behavior is closer to Black's preview style than
Black's stable style, although Ruff _also_ uses Unicode width for identifiers and comments.

#### Walruses in slice expressions

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

#### `global` and `nonlocal` names are broken across multiple lines by continuations

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

#### Newlines are inserted after all class docstrings

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

#### Trailing own-line comments on imports are not moved to the next line

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

#### Parentheses around awaited collections are not preserved

Black preserves parentheses around awaited collections:

```python
await ([1, 2, 3])
```

Ruff will instead remove them:

```python
await [1, 2, 3]
```

This is more consistent to the formatting of other awaited expressions: Ruff and Black both
remove parentheses around, e.g., `await (1)`, only retaining them when syntactically required,
as in, e.g., `await (x := 1)`.

#### Implicit string concatenations in attribute accesses ([#7052](https://github.com/astral-sh/ruff/issues/7052))

Given the following unformatted code:

```python
print("aaaaaaaaaaaaaaaa" "aaaaaaaaaaaaaaaa".format(bbbbbbbbbbbbbbbbbb + bbbbbbbbbbbbbbbbbb))
```

Internally, Black's logic will first expand the outermost `print` call:

```python
print(
    "aaaaaaaaaaaaaaaa" "aaaaaaaaaaaaaaaa".format(bbbbbbbbbbbbbbbbbb + bbbbbbbbbbbbbbbbbb)
)
```

Since the argument is _still_ too long, Black will then split on the operator with the highest split
precedence. In this case, Black splits on the implicit string concatenation, to produce the
following Black-formatted code:

```python
print(
    "aaaaaaaaaaaaaaaa"
    "aaaaaaaaaaaaaaaa".format(bbbbbbbbbbbbbbbbbb + bbbbbbbbbbbbbbbbbb)
)
```

Ruff gives implicit concatenations a "lower" priority when breaking lines. As a result, Ruff
would instead format the above as:

```python
print(
    "aaaaaaaaaaaaaaaa" "aaaaaaaaaaaaaaaa".format(
        bbbbbbbbbbbbbbbbbb + bbbbbbbbbbbbbbbbbb
    )
)
```

In general, Black splits implicit string concatenations over multiple lines more often than Ruff,
even if those concatenations _can_ fit on a single line. Ruff instead avoids splitting such
concatenations unless doing so is necessary to fit within the configured line width.

#### Own-line comments on expressions don't cause the expression to expand ([#7314](https://github.com/astral-sh/ruff/issues/7314))

Given an expression like:

```python
(
    # A comment in the middle
    some_example_var and some_example_var not in some_example_var
)
```

Black associates the comment with `some_example_var`, thus splitting it over two lines:

```python
(
    # A comment in the middle
    some_example_var
    and some_example_var not in some_example_var
)
```

Ruff will instead associate the comment with the entire boolean expression, thus preserving the
initial formatting:

```python
(
    # A comment in the middle
    some_example_var and some_example_var not in some_example_var
)
```

#### Tuples are parenthesized when expanded ([#7317](https://github.com/astral-sh/ruff/issues/7317))

Ruff tends towards parenthesizing tuples (with a few exceptions), while Black tends to remove tuple
parentheses more often.

In particular, Ruff will always insert parentheses around tuples that expand over multiple lines:

```python
# Input
(a, b), (c, d,)

# Black
(a, b), (
    c,
    d,
)

# Ruff
(
    (a, b),
    (
        c,
        d,
    ),
)
```

There's one exception here. In `for` loops, both Ruff and Black will avoid inserting unnecessary
parentheses:

```python
# Input
for a, f(b,) in c:
    pass

# Black
for a, f(
    b,
) in c:
    pass

# Ruff
for a, f(
    b,
) in c:
    pass
```

#### Single-element tuples are always parenthesized

Ruff always inserts parentheses around single-element tuples, while Black will omit them in some
cases:

```python
# Input
(a, b),

# Black
(a, b),

# Ruff
((a, b),)
```

Adding parentheses around single-element tuples adds visual distinction and helps avoid "accidental"
tuples created by extraneous trailing commas (see, e.g., [#17181](https://github.com/django/django/pull/17181)).

#### Trailing commas are inserted when expanding a function definition with a single argument ([#7323](https://github.com/astral-sh/ruff/issues/7323))

When a function definition with a single argument is expanded over multiple lines, Black
will add a trailing comma in some cases, depending on whether the argument includes a type
annotation and/or a default value.

For example, Black will add a trailing comma to the first and second function definitions below,
but not the third:

```python
def func(
    aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa,
) -> None:
    ...


def func(
    aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa=1,
) -> None:
    ...


def func(
    aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa: Argument(
        "network_messages.pickle",
        help="The path of the pickle file that will contain the network messages",
    ) = 1
) -> None:
    ...
```

Ruff will instead insert a trailing comma in all such cases for consistency.

#### Parentheses around call-chain assignment values are not preserved ([#7320](https://github.com/astral-sh/ruff/issues/7320))

Given:

```python
def update_emission_strength():
    (
        get_rgbw_emission_node_tree(self)
        .nodes["Emission"]
        .inputs["Strength"]
        .default_value
    ) = (self.emission_strength * 2)
```

Black will preserve the parentheses in `(self.emission_strength * 2)`, whereas Ruff will remove
them.

Both Black and Ruff remove such parentheses in simpler assignments, like:

```python
# Input
def update_emission_strength():
    value = (self.emission_strength * 2)

# Black
def update_emission_strength():
    value = self.emission_strength * 2

# Ruff
def update_emission_strength():
    value = self.emission_strength * 2
```

#### Call chain calls break differently ([#7051](https://github.com/astral-sh/ruff/issues/7051))

Black occasionally breaks call chains differently than Ruff; in particular, Black occasionally
expands the arguments for the last call in the chain, as in:

```python
# Input
df.drop(
    columns=["aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"]
).drop_duplicates().rename(
    columns={
        "a": "a",
    }
).to_csv(path / "aaaaaa.csv", index=False)

# Black
df.drop(
    columns=["aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"]
).drop_duplicates().rename(
    columns={
        "a": "a",
    }
).to_csv(
    path / "aaaaaa.csv", index=False
)

# Ruff
df.drop(
    columns=["aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"]
).drop_duplicates().rename(
    columns={
        "a": "a",
    }
).to_csv(path / "aaaaaa.csv", index=False)
```

Ruff will only expand the arguments if doing so is necessary to fit within the configured line
width.

Note that Black does not apply this last-call argument breaking universally. For example, both
Black and Ruff will format the following identically:

```python
# Input
df.drop(
    columns=["aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"]
).drop_duplicates(a).rename(
    columns={
        "a": "a",
    }
).to_csv(
    path / "aaaaaa.csv", index=False
).other(a)

# Black
df.drop(columns=["aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"]).drop_duplicates(a).rename(
    columns={
        "a": "a",
    }
).to_csv(path / "aaaaaa.csv", index=False).other(a)

# Ruff
df.drop(columns=["aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"]).drop_duplicates(a).rename(
    columns={
        "a": "a",
    }
).to_csv(path / "aaaaaa.csv", index=False).other(a)
```

#### Expressions with (non-pragma) trailing comments are split more often ([#7823](https://github.com/astral-sh/ruff/issues/7823))

Both Ruff and Black will break the following expression over multiple lines, since it then allows
the expression to fit within the configured line width:

```python
# Input
some_long_variable_name = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"

# Black
some_long_variable_name = (
    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
)

# Ruff
some_long_variable_name = (
    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
)
```

However, if the expression ends in a trailing comment, Black will avoid wrapping the expression
in some cases, while Ruff will wrap as long as it allows the expanded lines to fit within the line
length limit:

```python
# Input
some_long_variable_name = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"  # a trailing comment

# Black
some_long_variable_name = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"  # a trailing comment

# Ruff
some_long_variable_name = (
    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
)  # a trailing comment
```

Doing so leads to fewer overlong lines while retaining the comment's intent. As pragma comments
(like `# noqa` and `# type: ignore`) are ignored when computing line width, this behavior only
applies to non-pragma comments.
