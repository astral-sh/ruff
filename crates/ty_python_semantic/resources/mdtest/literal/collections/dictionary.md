# Dictionaries

## Empty dictionary

```py
reveal_type({})  # revealed: dict[Unknown, Unknown]
```

## Basic dict

```py
reveal_type({1: 1, 2: 1})  # revealed: dict[Unknown | int, Unknown | int]
```

## Dict of tuples

```py
reveal_type({1: (1, 2), 2: (3, 4)})  # revealed: dict[Unknown | int, Unknown | tuple[int, int]]
```

## Unpacked dict

```py
a = {"a": 1, "b": 2}
b = {"c": 3, "d": 4}

d = {**a, **b}
reveal_type(d)  # revealed: dict[Unknown | str, Unknown | int]
```

## Dict of functions

```py
def a(_: int) -> int:
    return 0

def b(_: int) -> int:
    return 1

x = {1: a, 2: b}
reveal_type(x)  # revealed: dict[Unknown | int, Unknown | ((_: int) -> int)]
```

## Mixed dict

```py
# revealed: dict[Unknown | str, Unknown | int | tuple[int, int] | tuple[int, int, int]]
reveal_type({"a": 1, "b": (1, 2), "c": (1, 2, 3)})
```

## Dict comprehensions

```py
# revealed: dict[int | Unknown, int | Unknown]
reveal_type({x: y for x, y in enumerate(range(42))})
```
