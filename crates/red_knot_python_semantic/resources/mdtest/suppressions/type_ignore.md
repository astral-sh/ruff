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

## Codes

Mypy supports `type: ignore[code]`. Red Knot doesn't understand mypy's rule names. Therefore, ignore
the codes and suppress all errors.

```py
a = test  # type: ignore[name-defined]
```

## Nested comments

```py
# fmt: off
a = test \
  + 2  # fmt: skip # type: ignore

a = test \
  + 2  # type: ignore # fmt: skip
```

## Misspelled `type: ignore`

```py
# error: [unresolved-reference]
# error: [invalid-ignore-comment]
a = test + 2  # type: ignoree
```

## Invalid - ignore on opening parentheses

`type: ignore` comments after an opening parentheses suppress any type errors inside the parentheses
in Pyright. Neither Ruff, nor mypy support this and neither does Red Knot.

```py
# fmt: off
# error: [unused-ignore-comment]
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

# error: [unused-ignore-comment] "Unused blanket `type: ignore` directive"
# type: ignore

a = 10 / 0  # error: [division-by-zero]
b = a / 0  # error: [division-by-zero]
```
