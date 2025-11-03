# Sets

## Basic set

```py
reveal_type({1, 2})  # revealed: set[Unknown | int]
```

## Set of tuples

```py
reveal_type({(1, 2), (3, 4)})  # revealed: set[Unknown | tuple[int, int]]
```

## Set of functions

```py
def a(_: int) -> int:
    return 0

def b(_: int) -> int:
    return 1

x = {a, b}
reveal_type(x)  # revealed: set[Unknown | ((_: int) -> int)]
```

## Mixed set

```py
# revealed: set[Unknown | int | tuple[int, int] | tuple[int, int, int]]
reveal_type({1, (1, 2), (1, 2, 3)})
```

## Set comprehensions

```py
reveal_type({x for x in range(42)})  # revealed: set[int | Unknown]
```
