# Narrowing for `is not` conditionals

## `is not None`

The type guard removes `None` from the union type:

```py
def _(flag: bool):
    x = None if flag else 1

    if x is not None:
        reveal_type(x)  # revealed: Literal[1]
    else:
        reveal_type(x)  # revealed: None

    reveal_type(x)  # revealed: None | Literal[1]
```

## `is not` for other singleton types

```py
def _(flag: bool):
    x = True if flag else False
    reveal_type(x)  # revealed: bool

    if x is not False:
        reveal_type(x)  # revealed: Literal[True]
    else:
        reveal_type(x)  # revealed: Literal[False]
```

## `is not` for non-singleton types

Non-singleton types should *not* narrow the type: two instances of a non-singleton class may occupy
different addresses in memory even if they compare equal.

```py
x = 345
y = 345

if x is not y:
    reveal_type(x)  # revealed: Literal[345]
else:
    reveal_type(x)  # revealed: Literal[345]
```

## `is not` for other types

```py
def _(flag: bool):
    class A: ...
    x = A()
    y = x if flag else None

    if y is not x:
        reveal_type(y)  # revealed: A | None
    else:
        reveal_type(y)  # revealed: A

    reveal_type(y)  # revealed: A | None
```

## `is not` in chained comparisons

The type guard removes `False` from the union type of the tested value only.

```py
def _(x_flag: bool, y_flag: bool):
    x = True if x_flag else False
    y = True if y_flag else False

    reveal_type(x)  # revealed: bool
    reveal_type(y)  # revealed: bool

    if y is not x is not False:  # Interpreted as `(y is not x) and (x is not False)`
        reveal_type(x)  # revealed: Literal[True]
        reveal_type(y)  # revealed: bool
    else:
        # The negation of the clause above is (y is x) or (x is False)
        # So we can't narrow the type of x or y here, because each arm of the `or` could be true

        reveal_type(x)  # revealed: bool
        reveal_type(y)  # revealed: bool
```
