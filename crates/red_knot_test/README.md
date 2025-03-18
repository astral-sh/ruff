# Writing type-checking / type-inference tests

Any Markdown file can be a test suite.

In order for it to be run as one, `red_knot_test::run` must be called with its path; see
`crates/red_knot_python_semantic/tests/mdtest.rs` for an example that treats all Markdown files
under a certain directory as test suites.

A Markdown test suite can contain any number of tests. A test consists of one or more embedded
"files", each defined by a triple-backticks fenced code block. The code block must have a tag string
specifying its language. We currently support `py` (Python files) and `pyi` (type stub files), as
well as [typeshed `VERSIONS`] files and `toml` for configuration.

The simplest possible test suite consists of just a single test, with a single embedded file:

````markdown
```py
reveal_type(1)  # revealed: Literal[1]
```
````

When running this test, the mdtest framework will write a file with these contents to the default
file path (`/src/mdtest_snippet.py`) in its in-memory file system, run a type check on that file,
and then match the resulting diagnostics with the assertions in the test. Assertions are in the form
of Python comments. If all diagnostics and all assertions are matched, the test passes; otherwise,
it fails.

<!---
(If you are reading this document in raw Markdown source rather than rendered Markdown, note that
the quadruple-backtick-fenced "markdown" language code block above is NOT itself part of the mdtest
syntax, it's just how this README embeds an example mdtest Markdown document.)
--->

See actual example mdtest suites in
[`crates/red_knot_python_semantic/resources/mdtest`](https://github.com/astral-sh/ruff/tree/main/crates/red_knot_python_semantic/resources/mdtest).

> [!NOTE]
> If you use `dir-test`, `rstest` or similar to generate a separate test for all Markdown files in a certain directory,
> as with the example in `crates/red_knot_python_semantic/tests/mdtest.rs`,
> you will likely want to also make sure that the crate the tests are in is rebuilt every time a
> Markdown file is added or removed from the directory. See
> [`crates/red_knot_python_semantic/build.rs`](https://github.com/astral-sh/ruff/tree/main/crates/red_knot_python_semantic/build.rs)
> for an example of how to do this.
>
> This is because these macros generate their tests at build time rather than at runtime.
> Without the `build.rs` file to force a rebuild when a Markdown file is added or removed,
> a new Markdown test suite might not be run unless some other change in the crate caused a rebuild
> following the addition of the new test file.

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

## Literate style

If multiple code blocks (without an explicit path, see below) are present in a single test, they will
be merged into a single file in the order they appear in the Markdown file. This allows for tests that
interleave code and explanations:

````markdown
# My literate test

This first snippet here:

```py
from typing import Literal

def f(x: Literal[1]):
    pass
```

will be merged with this second snippet here, i.e. `f` is defined here:

```py
f(2)  # error: [invalid-argument-type]
```
````

## Diagnostic Snapshotting

In addition to inline assertions, one can also snapshot the full diagnostic
output of a test. This is done by adding a `<!-- snapshot-diagnostics -->` directive
in the corresponding section. For example:

````markdown
## Unresolvable module import

<!-- snapshot-diagnostics -->

```py
import zqzqzqzqzqzqzq  # error: [unresolved-import] "Cannot resolve import `zqzqzqzqzqzqzq`"
```
````

The `snapshot-diagnostics` directive must appear before anything else in
the section.

This will use `insta` to manage an external file snapshot of all diagnostic
output generated.

Inline assertions, as described above, may be used in conjunction with diagnostic
snapshotting.

At present, there is no way to do inline snapshotting or to request more granular
snapshotting of specific diagnostics.

## Multi-file tests

Some tests require multiple files, with imports from one file into another. For this purpose,
tests can specify explicit file paths in a separate line before the code block (`b.py` below):

````markdown
```py
from b import C
reveal_type(C)  # revealed: Literal[C]
```

`b.py`:

```py
class C: pass
```
````

Relative file names are always relative to the "workspace root", which is also an import root (that
is, the equivalent of a runtime entry on `sys.path`).

The default workspace root is `/src/`. Currently it is not possible to customize this in a test, but
this is a feature we will want to add in the future.

So the above test creates two files, `/src/mdtest_snippet.py` and `/src/b.py`, and sets the workspace
root to `/src/`, allowing imports from `b.py` using the module name `b`.

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

`b.py`:

```py
y = "foo"
```
````

This test suite contains two tests, one named "Same-file invalid assignment" and the other named
"Cross-file invalid assignment". The first test involves only a single embedded file, and the second
test involves two embedded files.

The tests are run independently, in independent in-memory file systems and with new red-knot
[Salsa](https://github.com/salsa-rs/salsa) databases. This means that each is a from-scratch run of
the type checker, with no data persisting from any previous test.

It is possible to filter to individual tests within a single markdown file using the
`MDTEST_TEST_FILTER` environment variable. This variable will match any tests which contain the
value as a case-sensitive substring in its name. An example test name is
`unpacking.md - Unpacking - Tuple - Multiple assignment`, which contains the name of the markdown
file and its parent headers joined together with hyphens.

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

## Configuration

The test framework supports a TOML-based configuration format, which is a subset of the full red-knot
configuration format. This configuration can be specified in fenced code blocks with `toml` as the
language tag:

````markdown
```toml
[environment]
python-version = "3.10"
```
````

This configuration will apply to all tests in the same section, and all nested sections within that
section. Nested sections can override configurations from their parent sections.

See [`MarkdownTestConfig`](https://github.com/astral-sh/ruff/blob/main/crates/red_knot_test/src/config.rs) for the full list of supported configuration options.

### Specifying a custom typeshed

Some tests will need to override the default typeshed with custom files. The `[environment]`
configuration option `typeshed` can be used to do this:

````markdown
```toml
[environment]
typeshed = "/typeshed"
```
````

For more details, take a look at the [custom-typeshed Markdown test].

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

## Running the tests

All Markdown-based tests are executed in a normal `cargo test` / `cargo run nextest` run. If you want to run the Markdown tests
*only*, you can filter the tests using `mdtest__`:

```bash
cargo test -p red_knot_python_semantic -- mdtest__
```

Alternatively, you can use the `mdtest.py` runner which has a watch mode that will re-run corresponding tests when Markdown files change, and recompile automatically when Rust code changes:

```bash
uv run crates/red_knot_python_semantic/mdtest.py
```

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

### Configuring search paths and kinds

The red-knot TOML configuration format hasn't been finalized, and we may want to implement
support in the test framework for configuring search paths before it is designed. If so, we can
define some configuration options for now under the `[tests]` namespace. In the future, perhaps
some of these can be replaced by real red-knot configuration options; some or all may also be
kept long-term as test-specific options.

Some configuration options we will want to provide:

- We should be able to configure the default workspace root to something other than `/src/` using a
    `workspace-root` configuration option.

- We should be able to add a third-party root using the `third-party-root` configuration option.

- We may want to add additional configuration options for setting additional search path kinds.

Paths for `workspace-root` and `third-party-root` must be absolute.

Relative embedded-file paths are relative to the workspace root, even if it is explicitly set to a
non-default value using the `workspace-root` config.

### I/O errors

We could use an `error=` configuration option in the tag string to make an embedded file cause an
I/O error on read.

### Asserting on full diagnostic output

> [!NOTE]
> At present, one can opt into diagnostic snapshotting that is managed via external files. See
> the section above for more details. The feature outlined below, *inline* diagnostic snapshotting,
> is still desirable.

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
mdtest_snippet.py, line 1, col 1: revealed type is 'Literal[1]'
```
````

We will want to build tooling to automatically capture and update these “full diagnostic output”
blocks, when tests are run in an update-output mode (probably specified by an environment variable.)

By default, an `output` block will specify diagnostic output for the file
`<workspace-root>/mdtest_snippet.py`. An `output` block can be prefixed by a
<code>`&lt;path>`:</code> label as usual, to explicitly specify the Python file for which it asserts
diagnostic output.

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

Initial file contents:

```py
from b import x
reveal_type(x)
```

`b.py`:

```py
x = 1
```

Initial expected output for the unnamed file:

```output
/src/mdtest_snippet.py, line 1, col 1: revealed type is 'Literal[1]'
```

Now in our first incremental stage, modify the contents of `b.py`:

`b.py`:

```py stage=1
# b.py
x = 2
```

And this is our updated expected output for the unnamed file at stage 1:

```output stage=1
/src/mdtest_snippet.py, line 1, col 1: revealed type is 'Literal[2]'
```

(One reason to use full-diagnostic-output blocks in this test is that updating inline-comment
diagnostic assertions for `mdtest_snippet.py` would require specifying new contents for
`mdtest_snippet.py` in stage 1, which we don't want to do in this test.)
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

[custom-typeshed markdown test]: ../red_knot_python_semantic/resources/mdtest/mdtest_custom_typeshed.md
[typeshed `versions`]: https://github.com/python/typeshed/blob/c546278aae47de0b2b664973da4edb613400f6ce/stdlib/VERSIONS#L1-L18%3E
