# Writing type-checking / type-inference tests

Any Markdown file can be a test suite.

In order for it to be run as one, `red_knot_test::run` must be called with its path; see
`crates/red_knot_python_semantic/tests/mdtest.rs` for an example that treats all Markdown files
under a certain directory as test suites.

A Markdown test suite can contain any number of tests. A test consists of one or more embedded
"files", each defined by a triple-backticks fenced code block. The code block must have a tag string
specifying its language; currently only `py` (Python files) and `pyi` (type stub files) are
supported.

The simplest possible test suite consists of just a single test, with a single embedded file:

````markdown
```py
reveal_type(1)  # revealed: Literal[1]
```
````

When running this test, the mdtest framework will write a file with these contents to the default
file path (`/src/test.py`) in its in-memory file system, run a type check on that file, and then
match the resulting diagnostics with the assertions in the test. Assertions are in the form of
Python comments. If all diagnostics and all assertions are matched, the test passes; otherwise, it
fails.

<!---
(If you are reading this document in raw Markdown source rather than rendered Markdown, note that
the quadruple-backtick-fenced "markdown" language code block above is NOT itself part of the mdtest
syntax, it's just how this README embeds an example mdtest Markdown document.)
--->

See actual example mdtest suites in
[`crates/red_knot_python_semantic/resources/mdtest`](https://github.com/astral-sh/ruff/tree/main/crates/red_knot_python_semantic/resources/mdtest).

## Assertions

Two kinds of assertions are supported: `# revealed:` (shown above) and `# error:`.

### Assertion kinds

#### revealed

A `# revealed:` assertion should always be paired with a call to the `reveal_type` utility, which
reveals (via a diagnostic) the inferred type of its argument (which can be any expression). The text
after `# revealed:` must match exactly with the displayed form of the revealed type of that
expression.

The `reveal_type` function can be imported from the `typing` standard library module (or, for older
Python versions, from the `typing_extensions` pseudo-standard-library module[^extensions]):

```py
from typing import reveal_type

reveal_type("foo")  # revealed: Literal["foo"]
```

For convenience, type checkers also pretend that `reveal_type` is a built-in, so that this import is
not required. Using `reveal_type` without importing it issues a diagnostic warning that it was used
without importing it, in addition to the diagnostic revealing the type of the expression.

The `# revealed:` assertion must always match a revealed-type diagnostic, and will also match the
undefined-reveal diagnostic, if present, so it's safe to use `reveal_type` in tests either with or
without importing it. (Style preference is to not import it in tests, unless specifically testing
something about the behavior of importing it.)

#### error

A comment beginning with `# error:` is an assertion that a type checker diagnostic will be emitted,
with text span starting on that line. The matching can be narrowed in three ways:

- `# error: [invalid-assignment]` requires that the matched diagnostic have the rule code
    `invalid-assignment`. (The square brackets are required.)
- `# error: "Some text"` requires that the diagnostic's full message contain the text `Some text`.
    (The double quotes are required in the assertion comment; they are not part of the matched text.)
- `# error: 8 [rule-code]` or `# error: 8 "Some text"` additionally requires that the matched
    diagnostic's text span begins on column 8 (one-indexed) of this line.

Assertions must contain either a rule code or a contains-text, or both, and may optionally also
include a column number. They must come in order: first column, if present; then rule code, if
present; then contains-text, if present. For example, an assertion using all three would look like
`# error: 8 [invalid-assignment] "Some text"`.

Error assertions in tests intended to test type checker semantics should primarily use rule-code
assertions, with occasional contains-text assertions where needed to disambiguate or validate some
details of the diagnostic message.

### Assertion locations

An assertion comment may be a line-trailing comment, in which case it applies to the line it is on:

```py
x: str = 1  # error: [invalid-assignment]
```

Or it may be a comment on its own line, in which case it applies to the next line that does not
contain an assertion comment:

```py
# error: [invalid-assignment]
x: str = 1
```

Multiple assertions applying to the same line may be stacked:

```py
# error: [invalid-assignment]
# revealed: Literal[1]
x: str = reveal_type(1)
```

Intervening empty lines or non-assertion comments are not allowed; an assertion stack must be one
assertion per line, immediately following each other, with the line immediately following the last
assertion as the line of source code on which the matched diagnostics are emitted.

## Multi-file tests

Some tests require multiple files, with imports from one file into another. Multiple fenced code
blocks represent multiple embedded files. Since files must have unique names, at most one file can
use the default name of `/src/test.py`. Other files must explicitly specify their file name:

````markdown
```py
from b import C
reveal_type(C)  # revealed: Literal[C]
```

```py path=b.py
class C: pass
```
````

Relative file names are always relative to the "workspace root", which is also an import root (that
is, the equivalent of a runtime entry on `sys.path`).

The default workspace root is `/src/`. Currently it is not possible to customize this in a test, but
this is a feature we will want to add in the future.

So the above test creates two files, `/src/test.py` and `/src/b.py`, and sets the workspace root to
`/src/`, allowing `test.py` to import from `b.py` using the module name `b`.

## Multi-test suites

A single test suite (Markdown file) can contain multiple tests, by demarcating them using Markdown
header lines:

````markdown
# Same-file invalid assignment

```py
x: int = "foo"  # error: [invalid-assignment]
```

# Cross-file invalid assignment

```py
from b import y
x: int = y  # error: [invalid-assignment]
```

```py path=b.py
y = "foo"
```
````

This test suite contains two tests, one named "Same-file invalid assignment" and the other named
"Cross-file invalid assignment". The first test involves only a single embedded file, and the second
test involves two embedded files.

The tests are run independently, in independent in-memory file systems and with new red-knot
[Salsa](https://github.com/salsa-rs/salsa) databases. This means that each is a from-scratch run of
the type checker, with no data persisting from any previous test.

Due to `cargo test` limitations, an entire test suite (Markdown file) is run as a single Rust test,
so it's not possible to select individual tests within it to run.

## Structured test suites

Markdown headers can also be used to group related tests within a suite:

````markdown
# Literals

## Numbers

### Integer

```py
reveal_type(1)  # revealed: Literal[1]
```

### Float

```py
reveal_type(1.0)  # revealed: float
```

## Strings

```py
reveal_type("foo")  # revealed: Literal["foo"]
```
````

This test suite contains three tests, named "Literals - Numbers - Integer", "Literals - Numbers -
Float", and "Literals - Strings".

A header-demarcated section must either be a test or a grouping header; it cannot be both. That is,
a header section can either contain embedded files (making it a test), or it can contain more
deeply-nested headers (headers with more `#`), but it cannot contain both.

## Documentation of tests

Arbitrary Markdown syntax (including of course normal prose paragraphs) is permitted (and ignored by
the test framework) between fenced code blocks. This permits natural documentation of
why a test exists, and what it intends to assert:

````markdown
Assigning a string to a variable annotated as `int` is not permitted:

```py
x: int = "foo"  # error: [invalid-assignment]
```
````

## Planned features

There are some designed features that we intend for the test framework to have, but have not yet
implemented:

### Multi-line diagnostic assertions

We may want to be able to assert that a diagnostic spans multiple lines, and to assert the columns it
begins and/or ends on. The planned syntax for this will use `<<<` and `>>>` to mark the start and end lines for
an assertion:

```py
(3  # error: 2 [unsupported-operands] <<<
  +
 "foo")  # error: 6 >>>
```

The column assertion `6` on the ending line should be optional.

In cases of overlapping such assertions, resolve ambiguity using more angle brackets: `<<<<` begins
an assertion ended by `>>>>`, etc.

### Non-Python files

Some tests may need to specify non-Python embedded files: typeshed `stdlib/VERSIONS`, `pth` files,
`py.typed` files, `pyvenv.cfg` files...

We will allow specifying any of these using the `text` language in the code block tag string:

````markdown
```text path=/third-party/foo/py.typed
partial
```
````

We may want to also support testing Jupyter notebooks as embedded files; exact syntax for this is
yet to be determined.

Of course, red-knot is only run directly on `py` and `pyi` files, and assertion comments are only
possible in these files.

A fenced code block with no language will always be an error.

### Configuration

We will add the ability to specify non-default red-knot configurations to use in tests, by including
a TOML code block:

````markdown
```toml
[tool.knot]
warn-on-any = true
```

```py
from typing import Any

def f(x: Any):  # error: [use-of-any]
    pass
```
````

It should be possible to include a TOML code block in a single test (as shown), or in a grouping
section, in which case it applies to all nested tests within that grouping section. Configurations
at multiple level are allowed and merged, with the most-nested (closest to the test) taking
precedence.

### Running just a single test from a suite

Having each test in a suite always run as a distinct Rust test would require writing our own test
runner or code-generating tests in a build script; neither of these is planned.

We could still allow running just a single test from a suite, for debugging purposes, either via
some "focus" syntax that could be easily temporarily added to a test, or via an environment
variable.

### Configuring search paths and kinds

The red-knot TOML configuration format hasn't been designed yet, and we may want to implement
support in the test framework for configuring search paths before it is designed. If so, we can
define some configuration options for now under the `[tool.knot.tests]` namespace. In the future,
perhaps some of these can be replaced by real red-knot configuration options; some or all may also
be kept long-term as test-specific options.

Some configuration options we will want to provide:

- We should be able to configure the default workspace root to something other than `/src/` using a
    `workspace-root` configuration option.

- We should be able to add a third-party root using the `third-party-root` configuration option.

- We may want to add additional configuration options for setting additional search path kinds.

Paths for `workspace-root` and `third-party-root` must be absolute.

Relative embedded-file paths are relative to the workspace root, even if it is explicitly set to a
non-default value using the `workspace-root` config.

### Specifying a custom typeshed

Some tests will need to override the default typeshed with custom files. The `[tool.knot.tests]`
configuration option `typeshed-root` should be usable for this:

````markdown
```toml
[tool.knot.tests]
typeshed-root = "/typeshed"
```

This file is importable as part of our custom typeshed, because it is within `/typeshed`, which we
configured above as our custom typeshed root:

```py path=/typeshed/stdlib/builtins.pyi
I_AM_THE_ONLY_BUILTIN = 1
```

This file is written to `/src/test.py`, because the default workspace root is `/src/ and the default
file path is `test.py`:

```py
reveal_type(I_AM_THE_ONLY_BUILTIN)  # revealed: Literal[1]
```

````

A fenced code block with language `text` can be used to provide a `stdlib/VERSIONS` file in the
custom typeshed root. If no such file is created explicitly, one should be created implicitly
including entries enabling all specified `<typeshed-root>/stdlib` files for all supported Python
versions.

### I/O errors

We could use an `error=` configuration option in the tag string to make an embedded file cause an
I/O error on read.

### Asserting on full diagnostic output

The inline comment diagnostic assertions are useful for making quick, readable assertions about
diagnostics in a particular location. But sometimes we will want to assert on the full diagnostic
output of checking an embedded Python file. Or sometimes (see “incremental tests” below) we will
want to assert on diagnostics in a file, without impacting the contents of that file by changing a
comment in it. In these cases, a Python code block in a test could be followed by a fenced code
block with language `output`; this would contain the full diagnostic output for the preceding test
file:

````markdown
# full output

```py
x = 1
reveal_type(x)
```

This is just an example, not a proposal that red-knot would ever actually output diagnostics in
precisely this format:

```output
test.py, line 1, col 1: revealed type is 'Literal[1]'
```
````

We will want to build tooling to automatically capture and update these “full diagnostic output”
blocks, when tests are run in an update-output mode (probably specified by an environment variable.)

By default, an `output` block will specify diagnostic output for the file `<workspace-root>/test.py`.
An `output` block can have a `path=` option, to explicitly specify the Python file for which it
asserts diagnostic output, and a `stage=` option, to specify which stage of an incremental test it
specifies diagnostic output at. (See “incremental tests” below.)

It is an error for an `output` block to exist, if there is no `py` or `python` block in the same
test for the same file path.

### Incremental tests

Some tests should validate incremental checking, by initially creating some files, checking them,
and then modifying/adding/deleting files and checking again.

We should add the capability to create an incremental test by using the `stage=` option on some
fenced code blocks in the test:

````markdown
# Incremental

## modify a file

Initial version of `test.py` and `b.py`:

```py
from b import x
reveal_type(x)
```

```py path=b.py
x = 1
```

Initial expected output for `test.py`:

```output
/src/test.py, line 1, col 1: revealed type is 'Literal[1]'
```

Now in our first incremental stage, modify the contents of `b.py`:

```py path=b.py stage=1
# b.py
x = 2
```

And this is our updated expected output for `test.py` at stage 1:

```output stage=1
/src/test.py, line 1, col 1: revealed type is 'Literal[2]'
```

(One reason to use full-diagnostic-output blocks in this test is that updating
inline-comment diagnostic assertions for `test.py` would require specifying new
contents for `test.py` in stage 1, which we don't want to do in this test.)
````

It will be possible to provide any number of stages in an incremental test. If a stage re-specifies
a filename that was specified in a previous stage (or the initial stage), that file is modified. A
new filename appearing for the first time in a new stage will create a new file. To delete a
previously created file, specify that file with the tag `delete` in its tag string (in this case, it
is an error to provide non-empty contents). Any previously-created files that are not re-specified
in a later stage continue to exist with their previously-specified contents, and are not "touched".

All stages should be run in order, incrementally, and then the final state should also be re-checked
cold, to validate equivalence of cold and incremental check results.

[^extensions]: `typing-extensions` is a third-party module, but typeshed, and thus type checkers
    also, treat it as part of the standard library.
