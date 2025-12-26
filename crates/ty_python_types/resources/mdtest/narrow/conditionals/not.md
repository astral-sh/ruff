# Narrowing for `not` conditionals

The `not` operator negates a constraint.

## `not is None`

```py
def _(flag: bool):
    x = None if flag else 1

    if not x is None:
        reveal_type(x)  # revealed: Literal[1]
    else:
        reveal_type(x)  # revealed: None

    reveal_type(x)  # revealed: None | Literal[1]
```

## `not isinstance`

```py
def _(flag: bool):
    x = 1 if flag else "a"

    if not isinstance(x, (int)):
        reveal_type(x)  # revealed: Literal["a"]
    else:
        reveal_type(x)  # revealed: Literal[1]
```
