# Pattern matching

## With wildcard

```py
match 0:
    case 1:
        y = 2
    case _:
        y = 3

reveal_type(y)  # revealed: Literal[2, 3]
```

## Without wildcard

```py
match 0:
    case 1:
        y = 2
    case 2:
        y = 3

reveal_type(y)  # revealed: Unbound | Literal[2, 3]
```

## Basic match

```py
y = 1
y = 2
match 0:
    case 1:
        y = 3
    case 2:
        y = 4

reveal_type(y)  # revealed: Literal[2, 3, 4]
```
