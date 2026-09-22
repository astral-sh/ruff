# Dummy variable names

```toml
[lint]
select = ["ARG001", "ARG005", "F841"]
```

## ASCII identifiers

By default, unused names prefixed with an underscore are ignored unless they also end with an
underscore. Names consisting entirely of underscores are also ignored.

```py
def f(_, __, _a, __a, _a_b, _1):
    return 0

lambda _, __, _a, __a, _a_b, _1: 0
```

## Unicode identifiers

The same convention applies to Unicode identifiers, including letters, digits, and combining marks.
The name `_x́` ends with a combining acute accent (U+0301).

```py
def f(_次, __次, _次_次, _é, _١, _x́):
    return 0

lambda _次, __次, _次_次, _é, _١, _x́: 0
```

Unused local variables follow the same convention.

```py
def f():
    _次 = 0
    _x́ = 0
    return 0
```

## Trailing underscores

Names ending with an underscore are not treated as dummy names, unless they consist entirely of
underscores.

```py
def f(
    _a_,  # error: [unused-function-argument]
    _次_,  # error: [unused-function-argument]
    __init__,  # error: [unused-function-argument]
):
    return 0

lambda _a_: 0  # error: [unused-lambda-argument]
lambda _次_: 0  # error: [unused-lambda-argument]
lambda __init__: 0  # error: [unused-lambda-argument]
```

## No leading underscore

Names without a leading underscore are not treated as dummy names.

```py
def f(
    a,  # error: [unused-function-argument]
    次,  # error: [unused-function-argument]
):
    return 0

lambda a: 0  # error: [unused-lambda-argument]
lambda 次: 0  # error: [unused-lambda-argument]
```
