# Narrowing for `is not` conditionals

## `is not None`

The type guard removes `None` from the union type:

```py
x = None if flag else 1

if x is not None:
    reveal_type(x)  # revealed: Literal[1]

reveal_type(x)  # revealed: None | Literal[1]
```

## `is not` for other singleton types

```py
x = True if flag else False
reveal_type(x)  # revealed: bool

if x is not False:
    reveal_type(x)  # revealed: Literal[True]
```

## `is not` for non-singleton types

Non-singleton types should *not* narrow the type: two instances of a
non-singleton class may occupy different addresses in memory even if
they compare equal.

```py
x = 345
y = 345

if x is not y:
    reveal_type(x)  # revealed: Literal[345]
```

## `is not` in chained comparisons

The type guard removes `False` from the union type of the tested value only.

```py
x = True if x_flag else False
y = True if y_flag else False

reveal_type(x)  # revealed: bool
reveal_type(y)  # revealed: bool

if y is not x is not False:  # Interpreted as `(y is not x) and (x is not False)`
    reveal_type(x)  # revealed: Literal[True]
    reveal_type(y)  # revealed: bool
```
