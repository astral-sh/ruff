# The Ruff Formatter

The Ruff formatter is an extremely fast Python code formatter designed as a drop-in replacement for
[Black](https://pypi.org/project/black/), available as part of the `ruff` CLI via `ruff format`.

The Ruff formatter is available as of Ruff [v0.1.2](https://astral.sh/blog/the-ruff-formatter).

## `ruff format`

`ruff format` is the primary entrypoint to the formatter. It accepts a list of files or
directories, and formats all discovered Python files:

```shell
ruff format                   # Format all files in the current directory.
ruff format path/to/code/     # Format all files in `path/to/code` (and any subdirectories).
ruff format path/to/file.py   # Format a single file.
```

Similar to Black, running `ruff format /path/to/file.py` will format the given file or directory
in-place, while `ruff format --check /path/to/file.py` will avoid writing any formatted files back,
and instead exit with a non-zero status code upon detecting any unformatted files.

For the full list of supported options, run `ruff format --help`.

!!! note
    As of Ruff v0.1.7 the `ruff format` command uses the current working directory (`.`) as the default path to format.
    See [the file discovery documentation](configuration.md#python-file-discovery) for details.

## Philosophy

The initial goal of the Ruff formatter is _not_ to innovate on code style, but rather, to innovate
on performance, and provide a unified toolchain across Ruff's linter, formatter, and any and all
future tools.

As such, the formatter is designed as a drop-in replacement for [Black](https://github.com/psf/black),
but with an excessive focus on performance and direct integration with Ruff. Given Black's
popularity within the Python ecosystem, targeting Black compatibility ensures that formatter
adoption is minimally disruptive for the vast majority of projects.

Specifically, the formatter is intended to emit near-identical output when run over existing
Black-formatted code. When run over extensive Black-formatted projects like Django and Zulip, > 99.9%
of lines are formatted identically. (See: [_Black compatibility_](#black-compatibility).)

Given this focus on Black compatibility, the formatter thus adheres to [Black's (stable) code style](https://black.readthedocs.io/en/stable/the_black_code_style/current_style.html),
which aims for "consistency, generality, readability and reducing git diffs". To give you a sense
for the enforced code style, here's an example:

```python
# Input
def _make_ssl_transport(
    rawsock, protocol, sslcontext, waiter=None,
    *, server_side=False, server_hostname=None,
    extra=None, server=None,
    ssl_handshake_timeout=None,
    call_connection_made=True):
    '''Make an SSL transport.'''
    if waiter is None:
      waiter = Future(loop=loop)

    if extra is None:
      extra = {}

    ...

# Ruff
def _make_ssl_transport(
    rawsock,
    protocol,
    sslcontext,
    waiter=None,
    *,
    server_side=False,
    server_hostname=None,
    extra=None,
    server=None,
    ssl_handshake_timeout=None,
    call_connection_made=True,
):
    """Make an SSL transport."""
    if waiter is None:
        waiter = Future(loop=loop)

    if extra is None:
        extra = {}

    ...
```

Like Black, the Ruff formatter does _not_ support extensive code style configuration; however,
unlike Black, it _does_ support configuring the desired quote style, indent style, line endings,
and more. (See: [_Configuration_](#configuration).)

While the formatter is designed to be a drop-in replacement for Black, it is not intended to be
used interchangeably with Black on an ongoing basis, as the formatter _does_ differ from
Black in a few conscious ways (see: [_Known deviations_](formatter/black.md)). In general,
deviations are limited to cases in which Ruff's behavior was deemed more consistent, or
significantly simpler to support (with negligible end-user impact) given the differences in the
underlying implementations between Black and Ruff.

Going forward, the Ruff Formatter will support Black's preview style under Ruff's own
[preview](preview.md) mode.

## Configuration

The Ruff Formatter exposes a small set of configuration options, some of which are also supported
by Black (like line width), some of which are unique to Ruff (like quote, indentation style and
formatting code examples in docstrings).

For example, to configure the formatter to use single quotes, format code
examples in docstrings, a line width of 100, and tab indentation, add the
following to your configuration file:

=== "pyproject.toml"

    ```toml
    [tool.ruff]
    line-length = 100

    [tool.ruff.format]
    quote-style = "single"
    indent-style = "tab"
    docstring-code-format = true
    ```

=== "ruff.toml"

    ```toml
    line-length = 100

    [format]
    quote-style = "single"
    indent-style = "tab"
    docstring-code-format = true
    ```


For the full list of supported settings, see [_Settings_](settings.md#format). For more on
configuring Ruff via `pyproject.toml`, see [_Configuring Ruff_](configuration.md).

Given the focus on Black compatibility (and unlike formatters like [YAPF](https://github.com/google/yapf)),
Ruff does not currently expose any other configuration options.

## Docstring formatting

The Ruff formatter provides an opt-in feature for automatically formatting
Python code examples in docstrings. The Ruff formatter currently recognizes
code examples in the following formats:

* The Python [doctest] format.
* CommonMark [fenced code blocks] with the following info strings: `python`,
`py`, `python3`, or `py3`. Fenced code blocks without an info string are
assumed to be Python code examples and also formatted.
* reStructuredText [literal blocks]. While literal blocks may contain things
other than Python, this is meant to reflect a long-standing convention in the
Python ecosystem where literal blocks often contain Python code.
* reStructuredText [`code-block` and `sourcecode` directives]. As with
Markdown, the language names recognized for Python are `python`, `py`,
`python3`, or `py3`.

If a code example is recognized and treated as Python, the Ruff formatter will
automatically skip it if the code does not parse as valid Python or if the
reformatted code would produce an invalid Python program.

Users may also configure the line length limit used for reformatting Python
code examples in docstrings. The default is a special value, `dynamic`, which
instructs the formatter to respect the line length limit setting for the
surrounding Python code. The `dynamic` setting ensures that even when code
examples are found inside indented docstrings, the line length limit configured
for the surrounding Python code will not be exceeded. Users may also configure
a fixed line length limit for code examples in docstrings.

For example, this configuration shows how to enable docstring code formatting
with a fixed line length limit:

=== "pyproject.toml"

    ```toml
    [tool.ruff.format]
    docstring-code-format = true
    docstring-code-line-length = 20
    ```

=== "ruff.toml"

    ```toml
    [format]
    docstring-code-format = true
    docstring-code-line-length = 20
    ```

With the above configuration, this code:

```python
def f(x):
    '''
    Something about `f`. And an example:

    .. code-block:: python

        foo, bar, quux = this_is_a_long_line(lion, hippo, lemur, bear)
    '''
    pass
```

... will be reformatted (assuming the rest of the options are set
to their defaults) as:

```python
def f(x):
    """
    Something about `f`. And an example:

    .. code-block:: python

        (
            foo,
            bar,
            quux,
        ) = this_is_a_long_line(
            lion,
            hippo,
            lemur,
            bear,
        )
    """
    pass
```

[doctest]: https://docs.python.org/3/library/doctest.html
[fenced code blocks]: https://spec.commonmark.org/0.30/#fenced-code-blocks
[literal blocks]: https://docutils.sourceforge.io/docs/ref/rst/restructuredtext.html#literal-blocks
[`code-block` and `sourcecode` directives]: https://www.sphinx-doc.org/en/master/usage/restructuredtext/directives.html#directive-code-block

## Format suppression

Like Black, Ruff supports `# fmt: on`, `# fmt: off`, and `# fmt: skip` pragma comments, which can
be used to temporarily disable formatting for a given code block.

`# fmt: on` and `# fmt: off` comments are enforced at the statement level:

```python
# fmt: off
not_formatted=3
also_not_formatted=4
# fmt: on
```

As such, adding `# fmt: on` and `# fmt: off` comments within expressions will have no effect. In
the following example, both list entries will be formatted, despite the `# fmt: off`:

```python
[
    # fmt: off
    '1',
    # fmt: on
    '2',
]
```

Instead, apply the `# fmt: off` comment to the entire statement:

```python
# fmt: off
[
    '1',
    '2',
]
# fmt: on
```

Like Black, Ruff will _also_ recognize [YAPF](https://github.com/google/yapf)'s `# yapf: disable` and `# yapf: enable` pragma
comments, which are treated equivalently to `# fmt: off` and `# fmt: on`, respectively.

`# fmt: skip` comments suppress formatting for a preceding statement, case header, decorator,
function definition, or class definition:

```python
if True:
    pass
elif False: # fmt: skip
    pass

@Test
@Test2 # fmt: skip
def test(): ...

a = [1, 2, 3, 4, 5] # fmt: skip

def test(a, b, c, d, e, f) -> int: # fmt: skip
    pass
```

As such, adding an `# fmt: skip` comment at the end of an expression will have no effect. In
the following example, the list entry `'1'` will be formatted, despite the `# fmt: skip`:

```python
a = call(
    [
        '1',  # fmt: skip
        '2',
    ],
    b
)
```

Instead, apply the `# fmt: skip` comment to the entire statement:

```python
a = call(
  [
    '1',
    '2',
  ],
  b
)  # fmt: skip
```

## Conflicting lint rules

Ruff's formatter is designed to be used alongside the linter. However, the linter includes
some rules that, when enabled, can cause conflicts with the formatter, leading to unexpected
behavior. When configured appropriately, the goal of Ruff's formatter-linter compatibility is
such that running the formatter should never introduce new lint errors.

When using Ruff as a formatter, we recommend avoiding the following lint rules:

- [`tab-indentation`](rules/tab-indentation.md) (`W191`)
- [`indentation-with-invalid-multiple`](rules/indentation-with-invalid-multiple.md) (`E111`)
- [`indentation-with-invalid-multiple-comment`](rules/indentation-with-invalid-multiple-comment.md) (`E114`)
- [`over-indented`](rules/over-indented.md) (`E117`)
- [`indent-with-spaces`](rules/indent-with-spaces.md) (`D206`)
- [`triple-single-quotes`](rules/triple-single-quotes.md) (`D300`)
- [`bad-quotes-inline-string`](rules/bad-quotes-inline-string.md) (`Q000`)
- [`bad-quotes-multiline-string`](rules/bad-quotes-multiline-string.md) (`Q001`)
- [`bad-quotes-docstring`](rules/bad-quotes-docstring.md) (`Q002`)
- [`avoidable-escaped-quote`](rules/avoidable-escaped-quote.md) (`Q003`)
- [`missing-trailing-comma`](rules/missing-trailing-comma.md) (`COM812`)
- [`prohibited-trailing-comma`](rules/prohibited-trailing-comma.md) (`COM819`)
- [`single-line-implicit-string-concatenation`](rules/single-line-implicit-string-concatenation.md) (`ISC001`)
- [`multi-line-implicit-string-concatenation`](rules/multi-line-implicit-string-concatenation.md) (`ISC002`)

While the [`line-too-long`](rules/line-too-long.md) (`E501`) rule _can_ be used alongside the
formatter, the formatter only makes a best-effort attempt to wrap lines at the configured
[`line-length`](settings.md#line-length). As such, formatted code _may_ exceed the line length,
leading to [`line-too-long`](rules/line-too-long.md) (`E501`) errors.

None of the above are included in Ruff's default configuration. However, if you've enabled
any of these rules or their parent categories (like `Q`), we recommend disabling them via the
linter's [`lint.ignore`](settings.md#lint_ignore) setting.

Similarly, we recommend avoiding the following isort settings, which are incompatible with the
formatter's treatment of import statements when set to non-default values:

- [`force-single-line`](settings.md#lint_isort_force-single-line)
- [`force-wrap-aliases`](settings.md#lint_isort_force-wrap-aliases)
- [`lines-after-imports`](settings.md#lint_isort_lines-after-imports)
- [`lines-between-types`](settings.md#lint_isort_lines-between-types)
- [`split-on-trailing-comma`](settings.md#lint_isort_split-on-trailing-comma)

If you've configured any of these settings to take on non-default values, we recommend removing
them from your Ruff configuration.

When an incompatible lint rule or setting is enabled, `ruff format` will emit a warning. If your
`ruff format` is free of warnings, you're good to go!

## Exit codes

`ruff format` exits with the following status codes:

- `0` if Ruff terminates successfully, regardless of whether any files were formatted.
- `2` if Ruff terminates abnormally due to invalid configuration, invalid CLI options, or an
    internal error.

Meanwhile, `ruff format --check` exits with the following status codes:

- `0` if Ruff terminates successfully, and no files would be formatted if `--check` were not
    specified.
- `1` if Ruff terminates successfully, and one or more files would be formatted if `--check` were
    not specified.
- `2` if Ruff terminates abnormally due to invalid configuration, invalid CLI options, or an
    internal error.

## Black compatibility

The formatter is designed to be a drop-in replacement for [Black](https://github.com/psf/black).

Specifically, the formatter is intended to emit near-identical output when run over Black-formatted
code. When run over extensive Black-formatted projects like Django and Zulip, > 99.9% of lines
are formatted identically. When migrating an existing project from Black to Ruff, you should expect
to see a few differences on the margins, but the vast majority of your code should be unchanged.

When run over _non_-Black-formatted code, the formatter makes some different decisions than Black,
and so more deviations should be expected, especially around the treatment of end-of-line comments.

If you identify deviations in your project, spot-check them against the [known deviations](formatter/black.md),
as well as the [unintentional deviations](https://github.com/astral-sh/ruff/issues?q=is%3Aopen+is%3Aissue+label%3Aformatter)
filed in the issue tracker. If you've identified a new deviation, please [file an issue](https://github.com/astral-sh/ruff/issues/new).

### Intentional deviations

While the Ruff formatter aims to be a drop-in replacement for Black, it does differ from Black
in a few known ways. Some of these differences emerge from conscious attempts to improve upon
Black's code style, while others fall out of differences in the underlying implementations.

For a complete enumeration of these intentional deviations, see [_Known deviations_](formatter/black.md).

Unintentional deviations from Black are tracked in the [issue tracker](https://github.com/astral-sh/ruff/issues?q=is%3Aopen+is%3Aissue+label%3Aformatter).

### Preview style

Black gates formatting changes behind a [`preview`](https://black.readthedocs.io/en/stable/the_black_code_style/future_style.html#preview-style)
flag. The formatter does not yet support Black's preview style, though the intention is to support
it within the coming months behind Ruff's own [`preview`](https://docs.astral.sh/ruff/settings/#preview)
flag.

Black promotes some of its preview styling to stable at the end of each year. Ruff will similarly
implement formatting changes under the [`preview`](https://docs.astral.sh/ruff/settings/#preview)
flag, promoting them to stable through minor releases, in accordance with our [versioning policy](https://github.com/astral-sh/ruff/discussions/6998#discussioncomment-7016766).

## Sorting imports

Currently, the Ruff formatter does not sort imports. In order to both sort imports and format,
call the Ruff linter and then the formatter:

```shell
ruff check --select I --fix
ruff format
```

A unified command for both linting and formatting is [planned](https://github.com/astral-sh/ruff/issues/8232).
