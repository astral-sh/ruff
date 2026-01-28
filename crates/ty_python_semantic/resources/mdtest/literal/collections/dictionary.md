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
from typing import Mapping, KeysView

a = {"a": 1, "b": 2}
b = {"c": 3, "d": 4}
c = {**a, **b}
reveal_type(c)  # revealed: dict[Unknown | str, Unknown | int]

# revealed: list[int | str]
# revealed: list[int | str]
d: dict[str, list[int | str]] = {"a": reveal_type([1, 2]), **{"b": reveal_type([3, 4])}}
reveal_type(d)  # revealed: dict[str, list[int | str]]

class HasKeysAndGetItem:
    def keys(self) -> KeysView[str]:
        return {}.keys()

    def __getitem__(self, arg: str) -> int:
        return 42

def _(a: dict[str, int], b: Mapping[str, int], c: HasKeysAndGetItem, d: object):
    reveal_type({**a})  # revealed: dict[Unknown | str, Unknown | int]
    reveal_type({**b})  # revealed: dict[Unknown | str, Unknown | int]
    reveal_type({**c})  # revealed: dict[Unknown | str, Unknown | int]

    # error: [invalid-argument-type] "Argument expression after ** must be a mapping type: Found `object`"
    reveal_type({**d})  # revealed: dict[Unknown, Unknown]
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
# revealed: dict[Unknown | int, Unknown | int]
reveal_type({x: y for x, y in enumerate(range(42))})
```

## Key narrowing

The original assignment to each key, as well as future assignments, are used to narrow access to
individual keys:

```py
from typing import TypedDict

x1 = {"a": 1, "b": "2"}
reveal_type(x1)  # revealed: dict[Unknown | str, Unknown | int | str]
reveal_type(x1["a"])  # revealed: Literal[1]
reveal_type(x1["b"])  # revealed: Literal["2"]

x1["a"] = 2
reveal_type(x1["a"])  # revealed: Literal[2]

x2: dict[str, int | str] = {"a": 1, "b": "2"}
reveal_type(x2)  # revealed: dict[str, int | str]
reveal_type(x2["a"])  # revealed: Literal[1]
reveal_type(x2["b"])  # revealed: Literal["2"]

class TD(TypedDict):
    td: int

x3: dict[int, int | TD] = {1: 1, 2: {"td": 1}}
reveal_type(x3)  # revealed: dict[int, int | TD]
reveal_type(x3[1])  # revealed: Literal[1]
reveal_type(x3[2])  # revealed: TD

x4 = {"a": 1, "b": {"c": 2, "d": "3"}}
reveal_type(x4["a"])  # revealed: Literal[1]
reveal_type(x4["b"])  # revealed: dict[Unknown | str, Unknown | int | str]
reveal_type(x4["b"]["c"])  # revealed: Literal[2]
reveal_type(x4["b"]["d"])  # revealed: Literal["3"]

x5: dict[str, int | dict[str, int | TD]] = {"a": 1, "b": {"c": 2, "d": {"td": 1}}}
reveal_type(x5["a"])  # revealed: Literal[1]
reveal_type(x5["b"])  # revealed: dict[str, int | TD]
reveal_type(x5["b"]["c"])  # revealed: Literal[2]
reveal_type(x5["b"]["d"])  # revealed: TD
```
