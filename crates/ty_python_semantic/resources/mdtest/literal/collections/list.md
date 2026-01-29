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
reveal_type(x[0].__name__)  # revealed: str
```

## Mixed list

```py
# revealed: list[Unknown | int | tuple[int, int] | tuple[int, int, int]]
reveal_type([1, (1, 2), (1, 2, 3)])
```

## List comprehensions

```py
reveal_type([x for x in range(42)])  # revealed: list[Unknown | int]
```

## Element narrowing

The original assignment to each index, as well as future assignments, are used to narrow access to
individual elements:

```py
from typing import TypedDict

x1 = [1, "2"]
reveal_type(x1)  # revealed: list[Unknown | int | str]
reveal_type(x1[0])  # revealed: Literal[1]
reveal_type(x1[1])  # revealed: Literal["2"]

x1[0] = 2
reveal_type(x1[0])  # revealed: Literal[2]

x2: list[int | str] = [1, "2"]
reveal_type(x2)  # revealed: list[int | str]
reveal_type(x2[0])  # revealed: Literal[1]
reveal_type(x2[1])  # revealed: Literal["2"]

class TD(TypedDict):
    td: int

x3: list[int | TD] = [1, {"td": 1}]
reveal_type(x3)  # revealed: list[int | TD]
reveal_type(x3[0])  # revealed: Literal[1]
reveal_type(x3[1])  # revealed: TD

x4 = [1, [2, "3"]]
reveal_type(x4[0])  # revealed: Literal[1]
reveal_type(x4[1])  # revealed: list[Unknown | int | str]
reveal_type(x4[1][0])  # revealed: Literal[2]
reveal_type(x4[1][1])  # revealed: Literal["3"]

x5: list[int | list[int | TD]] = [1, [2, {"td": 1}]]
reveal_type(x5[0])  # revealed: Literal[1]
reveal_type(x5[1])  # revealed: list[int | TD]
reveal_type(x5[1][0])  # revealed: Literal[2]
reveal_type(x5[1][1])  # revealed: TD

x6: list[int | dict[str, list[int | TD]]] = [1, {"a": [2], "b": [{"td": 1}]}]
reveal_type(x6[0])  # revealed: Literal[1]
reveal_type(x6[1])  # revealed: dict[str, list[int | TD]]
reveal_type(x6[1]["a"][0])  # revealed: Literal[2]
reveal_type(x6[1]["b"][0])  # revealed: TD
```
