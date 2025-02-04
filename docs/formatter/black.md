---
title: "Known Deviations from Black"
hide:
  - navigation
---

This document enumerates the known, intentional differences in code style between Black and Ruff's
formatter.

For a list of unintentional deviations, see [issue tracker](https://github.com/astral-sh/ruff/issues?q=is%3Aopen+is%3Aissue+label%3Aformatter).

### Trailing end-of-line comments

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

Ruff uses the Unicode width of a line to determine if a line fits. Black uses Unicode width for strings,
and character width for all other tokens. Ruff _also_ uses Unicode width for identifiers and comments.

### Parenthesizing long nested-expressions

Black 24 and newer parenthesizes long conditional expressions and type annotations in function parameters:

```python
# Black
[
    "____________________________",
    "foo",
    "bar",
    (
        "baz"
        if some_really_looooooooong_variable
        else "some other looooooooooooooong value"
    ),
]


def foo(
    i: int,
    x: (
        Loooooooooooooooooooooooong
        | Looooooooooooooooong
        | Looooooooooooooooooooong
        | Looooooong
    ),
    *,
    s: str,
) -> None:
    pass

# Ruff
[
    "____________________________",
    "foo",
    "bar",
    "baz"
    if some_really_looooooooong_variable
    else "some other looooooooooooooong value",
]


def foo(
    i: int,
    x: Loooooooooooooooooooooooong
    | Looooooooooooooooong
    | Looooooooooooooooooooong
    | Looooooong,
    *,
    s: str,
) -> None:
    pass
```

We agree that Ruff's formatting (that matches Black's 23) is hard to read and needs improvement. But we aren't convinced that parenthesizing long nested expressions is the best solution, especially when considering expression formatting holistically. That's why we want to defer the decision until we've explored alternative nested expression formatting styles. See [psf/Black#4123](https://github.com/psf/black/issues/4123) for an in-depth explanation of our concerns and an outline of possible alternatives.

### Call expressions with a single multiline string argument

Unlike Black, Ruff preserves the indentation of a single multiline-string argument in a call expression:

```python
# Input
call(
  """"
  A multiline
  string
  """
)

dedent(""""
    A multiline
    string
""")

# Black
call(
  """"
  A multiline
  string
  """
)

dedent(
  """"
  A multiline
  string
"""
)


# Ruff
call(
  """"
  A multiline
  string
  """
)

dedent(""""
    A multiline
    string
""")
```

Black intended to ship a similar style change as part of the 2024 style that always removes the indent. It turned out that this change was too disruptive to justify the cases where it improved formatting. Ruff introduced the new heuristic of preserving the indent. We believe it's a good compromise that improves formatting but minimizes disruption for users.

### Blank lines at the start of a block

Black 24 and newer allows blank lines at the start of a block, where Ruff always removes them:

```python
# Black
if x:

  a = 123

# Ruff
if x:
  a = 123
```

Currently, we are concerned that allowing blank lines at the start of a block leads [to unintentional blank lines when refactoring or moving code](https://github.com/astral-sh/ruff/issues/8893#issuecomment-1867259744). However, we will consider adopting Black's formatting at a later point with an improved heuristic. The style change is tracked in [#9745](https://github.com/astral-sh/ruff/issues/9745).


### F-strings

Ruff formats expression parts in f-strings whereas Black does not:

```python
# Input
f'test{inner   + "nested_string"} including math {5 ** 3 + 10}'

# Black
f'test{inner   + "nested_string"} including math {5 ** 3 + 10}'

# Ruff
f"test{inner + 'nested_string'} including math {5**3 + 10}"
```

For more details on the formatting style, refer to the [f-string
formatting](../formatter.md#f-string-formatting) section.

### Implicit concatenated strings

Ruff merges implicitly concatenated strings if the entire string fits on a single line:

```python
# Input
def test(max_history):
    raise argparse.ArgumentTypeError(
        f"The value of `--max-history {max_history}` " f"is not a positive integer."
    )

# Black
def test(max_history):
    raise argparse.ArgumentTypeError(
        f"The value of `--max-history {max_history}` " f"is not a positive integer."
    )

# Ruff
def test(max_history):
    raise argparse.ArgumentTypeError(
        f"The value of `--max-history {max_history}` is not a positive integer."
    )
```

Black's unstable style applies the same formatting.

There are few rare cases where Ruff can't merge the implicitly concatenated strings automatically.
In those cases, Ruff preserves if the implicit concatenated strings are formatted over multiple lines:

```python
# Input
a = (
    r"aaaaaaa"
    "bbbbbbbbbbbb"
)

# Black
a = r"aaaaaaa" "bbbbbbbbbbbb"

# Ruff
a = (
    r"aaaaaaa"
    "bbbbbbbbbbbb"
)
```

This ensures compatibility with `ISC001` ([#8272](https://github.com/astral-sh/ruff/issues/8272)).

### `assert` statements

Unlike Black, Ruff prefers breaking the message over breaking the assertion, similar to how both Ruff and Black prefer breaking the assignment value over breaking the assignment target:

```python
# Input
assert (
    len(policy_types) >= priority + num_duplicates
), f"This tests needs at least {priority+num_duplicates} many types."


# Black
assert (
    len(policy_types) >= priority + num_duplicates
), f"This tests needs at least {priority+num_duplicates} many types."

# Ruff
assert len(policy_types) >= priority + num_duplicates, (
    f"This tests needs at least {priority + num_duplicates} many types."
)
```

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

### Parentheses around awaited collections are not preserved

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

### Implicit string concatenations in attribute accesses

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
    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".format(bbbbbbbbbbbbbbbbbb + bbbbbbbbbbbbbbbbbb)
)
```

In general, Black splits implicit string concatenations over multiple lines more often than Ruff,
even if those concatenations _can_ fit on a single line. Ruff instead avoids splitting such
concatenations unless doing so is necessary to fit within the configured line width.

### Own-line comments on expressions don't cause the expression to expand

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

### Tuples are parenthesized when expanded

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
for a, [b, d,] in c:
    pass

# Black
for a, [
    b,
    d,
] in c:
    pass

# Ruff
for a, [
    b,
    d,
] in c:
    pass
```

### Single-element tuples are always parenthesized

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


### Parentheses around call-chain assignment values are not preserved

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

### Call chain calls break differently in some cases

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

Similarly, in some cases, Ruff will collapse composite binary expressions more aggressively than
Black, if doing so allows the expression to fit within the configured line width:

```python
# Black
assert AAAAAAAAAAAAAAAAAAAAAA.bbbbbb.fooo(
    aaaaaaaaaaaa=aaaaaaaaaaaa
).ccccc() == (len(aaaaaaaaaa) + 1) * fooooooooooo * (
    foooooo + 1
) * foooooo * len(
    list(foo(bar(4, foo), foo))
)

# Ruff
assert AAAAAAAAAAAAAAAAAAAAAA.bbbbbb.fooo(
    aaaaaaaaaaaa=aaaaaaaaaaaa
).ccccc() == (len(aaaaaaaaaa) + 1) * fooooooooooo * (
    foooooo + 1
) * foooooo * len(list(foo(bar(4, foo), foo)))
```

### Single `with` item targeting Python 3.8 or older

Unlike Black, Ruff uses the same layout for `with` statements with a single context manager as it does for `while`, `if` and other compound statements:

```python
# Input
def run(data_path, model_uri):
    with pyspark.sql.SparkSession.builder.config(
        key="spark.python.worker.reuse", value=True
    ).config(key="spark.ui.enabled", value=False).master(
        "local-cluster[2, 1, 1024]"
    ).getOrCreate():
        # ignore spark log output
        spark.sparkContext.setLogLevel("OFF")
        print(score_model(spark, data_path, model_uri))

# Black
def run(data_path, model_uri):
    with pyspark.sql.SparkSession.builder.config(
        key="spark.python.worker.reuse", value=True
    ).config(key="spark.ui.enabled", value=False).master(
        "local-cluster[2, 1, 1024]"
    ).getOrCreate():
        # ignore spark log output
        spark.sparkContext.setLogLevel("OFF")
        print(score_model(spark, data_path, model_uri))

# Ruff
def run(data_path, model_uri):
    with (
        pyspark.sql.SparkSession.builder.config(
            key="spark.python.worker.reuse", value=True
        )
        .config(key="spark.ui.enabled", value=False)
        .master("local-cluster[2, 1, 1024]")
        .getOrCreate()
    ):
        # ignore spark log output
        spark.sparkContext.setLogLevel("OFF")
        print(score_model(spark, data_path, model_uri))
```

Ruff's formatting matches the formatting of other compound statements:

```python
def test():
    if (
        pyspark.sql.SparkSession.builder.config(
            key="spark.python.worker.reuse", value=True
        )
        .config(key="spark.ui.enabled", value=False)
        .master("local-cluster[2, 1, 1024]")
        .getOrCreate()
    ):
        # ignore spark log output
        spark.sparkContext.setLogLevel("OFF")
        print(score_model(spark, data_path, model_uri))
```

### The last context manager in a `with` statement may be collapsed onto a single line

When using a `with` statement with multiple unparenthesized context managers, Ruff may collapse the
last context manager onto a single line, if doing so allows the `with` statement to fit within the
configured line width.

Black, meanwhile, tends to break the last context manager slightly differently, as in the following
example:

```python
# Black
with tempfile.TemporaryDirectory() as d1:
    symlink_path = Path(d1).joinpath("testsymlink")
    with tempfile.TemporaryDirectory(dir=d1) as d2, tempfile.TemporaryDirectory(
        dir=d1
    ) as d4, tempfile.TemporaryDirectory(dir=d2) as d3, tempfile.NamedTemporaryFile(
        dir=d4
    ) as source_file, tempfile.NamedTemporaryFile(
        dir=d3
    ) as lock_file:
        pass

# Ruff
with tempfile.TemporaryDirectory() as d1:
    symlink_path = Path(d1).joinpath("testsymlink")
    with tempfile.TemporaryDirectory(dir=d1) as d2, tempfile.TemporaryDirectory(
        dir=d1
    ) as d4, tempfile.TemporaryDirectory(dir=d2) as d3, tempfile.NamedTemporaryFile(
        dir=d4
    ) as source_file, tempfile.NamedTemporaryFile(dir=d3) as lock_file:
        pass
```

When targeting Python 3.9 or newer, parentheses will be inserted around the
context managers to allow for clearer breaks across multiple lines, as in:

```python
with tempfile.TemporaryDirectory() as d1:
    symlink_path = Path(d1).joinpath("testsymlink")
    with (
        tempfile.TemporaryDirectory(dir=d1) as d2,
        tempfile.TemporaryDirectory(dir=d1) as d4,
        tempfile.TemporaryDirectory(dir=d2) as d3,
        tempfile.NamedTemporaryFile(dir=d4) as source_file,
        tempfile.NamedTemporaryFile(dir=d3) as lock_file,
    ):
        pass
```

### Preserving parentheses around single-element lists

Ruff preserves at least one parentheses around list elements, even if the list only contains a single element. The Black 2025 or newer, on the other hand, removes the parentheses 
for single-element lists if they aren't multiline and doing so does not change semantics:

```python
# Input
items = [(True)]
items = [(((((True)))))]
items = {(123)}

# Black
items = [True]
items = [True]
items = {123}

# Ruff
items = [(True)]
items = [(True)]
items = {(123)}

```
