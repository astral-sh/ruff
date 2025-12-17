# Narrowing for `len(..)` checks

When `len(x)` is used in a boolean context, we can narrow the type of `x` based on whether `len(x)`
is truthy (non-zero) or falsy (zero).

## Basic narrowing

```py
def _(x: str):
    if len(x):
        reveal_type(x)  # revealed: str & ~AlwaysFalsy
    else:
        reveal_type(x)  # revealed: str & ~AlwaysTruthy
```

## With negation

```py
def _(x: str):
    if not len(x):
        reveal_type(x)  # revealed: str & ~AlwaysTruthy
    else:
        reveal_type(x)  # revealed: str & ~AlwaysFalsy
```

## In boolean expressions

```py
def _(x: str, y: list[int]):
    if len(x) and len(y):
        reveal_type(x)  # revealed: str & ~AlwaysFalsy
        reveal_type(y)  # revealed: list[int] & ~AlwaysFalsy
```

## Combined with other conditions

```py
def _(x: str | None):
    if x is not None and len(x):
        reveal_type(x)  # revealed: str & ~AlwaysFalsy

    if x and len(x):
        reveal_type(x)  # revealed: str & ~AlwaysFalsy
```

## With literal strings

This is the case from issue #1983: when `value` can be an empty literal string, `len(value)` should
narrow away the empty string case.

```py
def _(line: str):
    value = line if len(line) < 3 else ""
    reveal_type(value)  # revealed: str

    if len(value):
        # After checking len(value), we know value is non-empty
        reveal_type(value)  # revealed: str & ~AlwaysFalsy
        # Accessing value[0] should be safe here
        _ = value[0]
```
