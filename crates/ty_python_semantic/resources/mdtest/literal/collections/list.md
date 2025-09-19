# Lists

## Empty list

```py
reveal_type([])  # revealed: list[Unknown]
```

## List of tuples

```py
reveal_type([(1, 2), (3, 4)])  # revealed: list[Unknown | tuple[int, int]]
```

## List of functions

```py
def a(_: int) -> int:
    return 0

def b(_: int) -> int:
    return 1

x = [a, b]
reveal_type(x)  # revealed: list[Unknown | ((_: int) -> int)]
```

## Mixed list

```py
# revealed: list[Unknown | int | tuple[int, int] | tuple[int, int, int]]
reveal_type([1, (1, 2), (1, 2, 3)])
```

## List comprehensions

```py
reveal_type([x for x in range(42)])  # revealed: list[@Todo(list comprehension element type)]
```
