# Narrowing for `is` conditionals

## `is None`

```py
def _(flag: bool):
    x = None if flag else 1

    if x is None:
        reveal_type(x)  # revealed: None
    else:
        reveal_type(x)  # revealed: Literal[1]

    reveal_type(x)  # revealed: None | Literal[1]
```

## `is` for other types

```py
def _(flag: bool):
    class A: ...
    x = A()
    y = x if flag else None

    if y is x:
        reveal_type(y)  # revealed: A
    else:
        reveal_type(y)  # revealed: A | None

    reveal_type(y)  # revealed: A | None
```

## `is` in chained comparisons

```py
def _(x_flag: bool, y_flag: bool):
    x = True if x_flag else False
    y = True if y_flag else False

    reveal_type(x)  # revealed: bool
    reveal_type(y)  # revealed: bool

    if y is x is False:  # Interpreted as `(y is x) and (x is False)`
        reveal_type(x)  # revealed: Literal[False]
        reveal_type(y)  # revealed: bool
    else:
        # The negation of the clause above is (y is not x) or (x is not False)
        # So we can't narrow the type of x or y here, because each arm of the `or` could be true
        reveal_type(x)  # revealed: bool
        reveal_type(y)  # revealed: bool
```

## `is` in elif clause

```py
def _(flag1: bool, flag2: bool):
    x = None if flag1 else (1 if flag2 else True)

    reveal_type(x)  # revealed: None | Literal[1, True]
    if x is None:
        reveal_type(x)  # revealed: None
    elif x is True:
        reveal_type(x)  # revealed: Literal[True]
    else:
        reveal_type(x)  # revealed: Literal[1]
```

## `is` for `EllipsisType` (Python 3.10+)

```toml
[environment]
python-version = "3.10"
```

```py
from types import EllipsisType

def _(x: int | EllipsisType):
    if x is ...:
        reveal_type(x)  # revealed: EllipsisType
    else:
        reveal_type(x)  # revealed: int
```

## `is` for `EllipsisType` (Python 3.9 and below)

```toml
[environment]
python-version = "3.9"
```

```py
def _(flag: bool):
    x = ... if flag else 42

    reveal_type(x)  # revealed: ellipsis | Literal[42]

    if x is ...:
        reveal_type(x)  # revealed: ellipsis
    else:
        reveal_type(x)  # revealed: Literal[42]
```
