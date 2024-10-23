# Narrowing for `isinstance` checks

Narrowing for `isinstance(object, classinfo)` expressions.

## `classinfo` is a single type

```py
x = 1 if flag else "a"

if isinstance(x, int):
    reveal_type(x)  # revealed: Literal[1]

if isinstance(x, str):
    reveal_type(x)  # revealed: Literal["a"]
    if isinstance(x, int):
        reveal_type(x)  # revealed: Never

if isinstance(x, (int, object)):
    reveal_type(x)  # revealed: Literal[1] | Literal["a"]
```

## `classinfo` is a tuple of types

Note: `isinstance(x, (int, str))` should not be confused with
`isinstance(x, tuple[(int, str)])`. The former is equivalent to
`isinstance(x, int | str)`:

```py
x = 1 if flag else "a"

if isinstance(x, (int, str)):
    reveal_type(x)  # revealed: Literal[1] | Literal["a"]

if isinstance(x, (int, bytes)):
    reveal_type(x)  # revealed: Literal[1]

if isinstance(x, (bytes, str)):
    reveal_type(x)  # revealed: Literal["a"]

# No narrowing should occur if a larger type is also
# one of the possibilities:
if isinstance(x, (int, object)):
    reveal_type(x)  # revealed: Literal[1] | Literal["a"]

y = 1 if flag1 else "a" if flag2 else b"b"
if isinstance(y, (int, str)):
    reveal_type(y)  # revealed: Literal[1] | Literal["a"]

if isinstance(y, (int, bytes)):
    reveal_type(y)  # revealed: Literal[1] | Literal[b"b"]

if isinstance(y, (str, bytes)):
    reveal_type(y)  # revealed: Literal["a"] | Literal[b"b"]
```

## `classinfo` is a nested tuple of types

```py
x = 1 if flag else "a"

if isinstance(x, (bool, (bytes, int))):
    reveal_type(x)  # revealed: Literal[1]
```
