# Suppressing errors with `type: ignore`

Type check errors can be suppressed by a `type: ignore` comment on the same line as the violation.

## Simple `type: ignore`

```py
a = 4 + test  # type: ignore
```

## Multiline ranges

A diagnostic with a multiline range can be suppressed by a comment on the same line as the
diagnostic's start or end. This is the same behavior as Mypy's.

```py
# fmt: off
y = (
    4 / 0  # type: ignore
)

y = (
    4 /  # type: ignore
    0
)

y = (
    4 /
    0  # type: ignore
)
```

Pyright diverges from this behavior and instead applies a suppression if its range intersects with
the diagnostic range. This can be problematic for nested expressions because a suppression in a
child expression now suppresses errors in the outer expression.

For example, the `type: ignore` comment in this example suppresses the error of adding `2` to
`"test"` and adding `"other"` to the result of the cast.

```py
from typing import cast

y = (
    # error: [unsupported-operator]
    cast(
        int,
        2 + "test",  # type: ignore
    )
    + "other"
)
```

Mypy flags the second usage.

## Before opening parenthesis

A suppression that applies to all errors before the opening parenthesis.

```py
a: Test = (  # type: ignore
  Test()  # error: [unresolved-reference]
)  # fmt: skip
```

## Multiline string

```py
a: int = 4
a = """
  This is a multiline string and the suppression is at its end
"""  # type: ignore
```

## Line continuations

Suppressions after a line continuation apply to all previous lines.

```py
# fmt: off
a = test \
  + 2  # type: ignore

a = test \
  + a \
  + 2  # type: ignore
```

## Interpolated strings

```toml
[environment]
python-version = "3.14"
```

Suppressions for expressions within interpolated strings can be placed after the interpolated string
if it's a single-line interpolation.

```py
a = f"""
{test}
"""  # type: ignore
```

For multiline-interpolation, put the ignore comment on the expression's start or end line:

```py
# fmt:off
a = f"""
{
  10 /  # type: ignore
  0
}
"""

a = f"""
{
  10 /
  0  # type: ignore
}
"""
```

But not at the end of the f-string:

```py
a = f"""
{
    10 / 0  # error: [division-by-zero]
}
"""  # error: [unused-type-ignore-comment]  # type: ignore
```

## Codes

Similar to mypy support `type: ignore[codes]` comments. But unlike mypy, ty only respects codes
starting with `ty:` to avoid ambiguity with suppression comments from mypy and other type checkers.

```py
a = test  # type: ignore[name-defined, ty:unresolved-reference]
```

## Unknown codes starting with `ty`

```py
# error: [unresolved-reference]
# error: [ignore-comment-unknown-rule]
a = test  # type: ignore[ty:name-defined]
```

## Nested comments

```py
# fmt: off
a = test \
  + 2  # fmt: skip # type: ignore

a = test \
  + 2  # type: ignore # fmt: skip
```

```py
a = (3
  # snapshot
  + 2)  # ty:ignore[division-by-zero] # fmt: skip
```

```snapshot
warning[unused-ignore-comment]: Unused `ty: ignore` directive
 --> src/mdtest_snippet.py:9:9
  |
9 |   + 2)  # ty:ignore[division-by-zero] # fmt: skip
  |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  |
help: Remove the unused suppression comment
6  |   + 2  # type: ignore # fmt: skip
7  | a = (3
8  |   # snapshot
   -   + 2)  # ty:ignore[division-by-zero] # fmt: skip
9  +   + 2)  # fmt: skip
10 | a = (3
11 |   # snapshot
12 |   + 2)  # fmt: skip # ty:ignore[division-by-zero]
```

```py
a = (3
  # snapshot
  + 2)  # fmt: skip # ty:ignore[division-by-zero]
```

```snapshot
warning[unused-ignore-comment]: Unused `ty: ignore` directive
  --> src/mdtest_snippet.py:12:21
   |
12 |   + 2)  # fmt: skip # ty:ignore[division-by-zero]
   |                     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
   |
help: Remove the unused suppression comment
9  |   + 2)  # ty:ignore[division-by-zero] # fmt: skip
10 | a = (3
11 |   # snapshot
   -   + 2)  # fmt: skip # ty:ignore[division-by-zero]
12 +   + 2)  # fmt: skip
```

## Misspelled `type: ignore`

```py
# error: [unresolved-reference]
# error: [invalid-ignore-comment]
a = test + 2  # type: ignoree
```

## Invalid - ignore on opening parentheses

`type: ignore` comments after an opening parentheses suppress any type errors inside the parentheses
in Pyright. Neither Ruff, nor mypy support this and neither does ty.

```py
# fmt: off
# error: [unused-type-ignore-comment]
a = (  # type: ignore
    test + 4  # error: [unresolved-reference]
)
```

## File level suppression

```py
# type: ignore

a = 10 / 0
b = a / 0
```

## File level suppression with code

```py
# type: ignore[ty:division-by-zero]

a = 10 / 0
b = a + c  # error: [unresolved-reference]
```

## File level suppression with leading shebang

```py
#!/usr/bin/env/python
# type: ignore

a = 10 / 0
b = a / 0
```

## Invalid own-line suppression

```py
"""
File level suppressions must come before any non-trivia token,
including module docstrings.
"""

# error: [unused-type-ignore-comment] "Unused blanket `type: ignore` directive"
# type: ignore

a = 10 / 0  # error: [division-by-zero]
b = a / 0  # error: [division-by-zero]
```

## `respect-type-ignore-comments=false`

ty ignore `type-ignore` comments if `respect-type-ignore-comments` is set to false.

```toml
[analysis]
respect-type-ignore-comments = false
```

`type: ignore` comments can't be used to suppress an error:

```py
# error: [unresolved-reference]
a = b + 10  # type: ignore
```

ty doesn't report or remove unused `type: ignore` comments:

```py
a = 10 + 5  # type: ignore
```

ty doesn't report invalid `type: ignore` comments:

```py
a = 10 + 4  # type: ignoreee
```

## Unused ignore comment mixed with mypy comments

```py
# snapshot
a = 10 / 2  # type: ignore[mypy-code, ty:division-by-zero]
```

```snapshot
warning[unused-type-ignore-comment]: Unused `type: ignore` directive: 'division-by-zero'
 --> src/mdtest_snippet.py:2:39
  |
2 | a = 10 / 2  # type: ignore[mypy-code, ty:division-by-zero]
  |                                       ^^^^^^^^^^^^^^^^^^^
  |
help: Remove the unused suppression code
1 | # snapshot
  - a = 10 / 2  # type: ignore[mypy-code, ty:division-by-zero]
2 + a = 10 / 2  # type: ignore[mypy-code]
```

## Unused ignore comment

```py
# snapshot
a = 10 / 2  # type: ignore[ty:division-by-zero]
```

```snapshot
warning[unused-type-ignore-comment]: Unused `type: ignore` directive
 --> src/mdtest_snippet.py:2:13
  |
2 | a = 10 / 2  # type: ignore[ty:division-by-zero]
  |             ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  |
help: Remove the unused suppression comment
1 | # snapshot
  - a = 10 / 2  # type: ignore[ty:division-by-zero]
2 + a = 10 / 2
```

## Unknown ignore code

```py
# snapshot
a = 10 / 2  # type: ignore[ty:division-by]
```

```snapshot
warning[ignore-comment-unknown-rule]: Unknown rule `division-by`. Did you mean `division-by-zero`?
 --> src/mdtest_snippet.py:2:28
  |
2 | a = 10 / 2  # type: ignore[ty:division-by]
  |                            ^^^^^^^^^^^^^^
  |
```
