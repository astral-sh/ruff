# Suppressing errors with `type: ignore`

Type check errors can be suppressed by a `type: ignore` comment on the same line as the violation.

## Simple `type: ignore`

```py
a = 4 + test  # type: ignore
```

## In parenthesized expression

```py
a = (
    4 + test  # type: ignore
)  # fmt: skip
```

## Before opening parentheses

A suppression that applies to all errors before the openign parentheses.

```py
a: Test = (  # type: ignore
  5
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

## Nested comments

TODO: We should support this for better interopability with other suppression comments.

```py
# fmt: off
# error: [unresolved-reference]
a = test \
  + 2  # fmt: skip # type: ignore

a = test \
  + 2  # type: ignore # fmt: skip
```

## Misspelled `type: ignore`

```py
# error: [unresolved-reference]
a = test + 2  # type: ignoree
```

## Invalid - ignore on opening parentheses

`type: ignore` comments after an opening parentheses suppress any type errors inside the parentheses
in Pyright. Neither Ruff, nor mypy support this and neither does Red Knot.

```py
# fmt: off
a = (  # type: ignore
    test + 4  # error: [unresolved-reference]
)
```
