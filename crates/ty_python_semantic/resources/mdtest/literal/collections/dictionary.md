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
