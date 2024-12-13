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
