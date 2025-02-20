# Pattern matching

## With wildcard

```py
def _(target: int):
    match target:
        case 1:
            y = 2
        case _:
            y = 3

    reveal_type(y)  # revealed: Literal[2, 3]
```

## Without wildcard

```py
def _(target: int):
    match target:
        case 1:
            y = 2
        case 2:
            y = 3

    # revealed: Literal[2, 3]
    # error: [possibly-unresolved-reference]
    reveal_type(y)
```

## Basic match

```py
def _(target: int):
    y = 1
    y = 2

    match target:
        case 1:
            y = 3
        case 2:
            y = 4

    reveal_type(y)  # revealed: Literal[2, 3, 4]
```

## not-boolable guard

```py
class NotBoolable:
    __bool__ = 3

def _(target: int, flag: NotBoolable):
    y = 1
    match target:
        # error: [not-boolable] "Object of type `NotBoolable` has an invalid `__bool__` method"
        case 1 if flag:
            y = 2
        case 2:
            y = 3

    reveal_type(y)  # revealed: Literal[1, 2, 3]
```
