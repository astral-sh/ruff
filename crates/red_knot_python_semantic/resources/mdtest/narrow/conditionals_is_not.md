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
    # TODO the following should be `Literal[True]`
    reveal_type(x)  # revealed: bool & ~Literal[False]
```

## `is not` for non-singleton types

Non-singleton types should *not* narrow the type: two instances of a
non-singleton class may occupy different addresses in memory even if
they compare equal.

```py
x = [1]
y = [1]

if x is not y:
    reveal_type(x)  # revealed: list
```
