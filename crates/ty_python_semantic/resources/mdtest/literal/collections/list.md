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

The inferred `Callable` type is function-like, i.e. we can still access attributes like `__name__`:

```py
reveal_type(x[0].__name__)  # revealed: Unknown | str
```

## Mixed list

```py
# revealed: list[Unknown | int | tuple[int, int] | tuple[int, int, int]]
reveal_type([1, (1, 2), (1, 2, 3)])
```

## Invariant generic elements

We take care not to promote invariant generics:

```py
from typing import Literal

def _(a: list[Literal[1]], b: Literal[2]):
    c = [a]
    reveal_type(c)  # revealed: list[Unknown | list[Literal[1]]]

    d = [(a, b)]
    reveal_type(d)  # revealed: list[Unknown | tuple[list[Literal[1]], int]]
```

## List comprehensions

```py
reveal_type([x for x in range(42)])  # revealed: list[@Todo(list comprehension element type)]
```
