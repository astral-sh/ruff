# Dictionaries

## Empty dictionary

```py
reveal_type({})  # revealed: dict[Unknown, Unknown]
```

## Empty dictionaries with an expected type

An empty dictionary can use the key and value types from an expected `Mapping`. If it matches more
than one union member, ty keeps a key or value type only when all matching members agree. `Any` is
preserved as a key type, and different `NewType`s remain distinct:

```py
from collections.abc import Mapping
from typing import Any, NewType

def default_to_empty(value: Mapping[str, int] | None = None) -> None:
    if value is None:
        value = {}
        reveal_type(value)  # revealed: dict[str, int]

annotated: Mapping[str, int] = {}
reveal_type(annotated)  # revealed: dict[str, int]

def same_key(value: Mapping[str, int] | Mapping[str, bytes] | None = None) -> None:
    if value is None:
        value = {}
        reveal_type(value)  # revealed: dict[str, Unknown]

def same_value(
    value: Mapping[bytes, int] | Mapping[str, int] | None = None,
) -> None:
    if value is None:
        value = {}
        reveal_type(value)  # revealed: dict[Unknown, int]

def any_key(value: Mapping[Any, int] | None = None) -> None:
    if value is None:
        value = {}
        reveal_type(value)  # revealed: dict[Any, int]

A = NewType("A", int)
B = NewType("B", int)

def distinct_newtypes(value: Mapping[A, int] | Mapping[B, bytes] | None = None) -> None:
    if value is None:
        value = {}
        reveal_type(value)  # revealed: dict[Unknown, Unknown]
```

## Basic dict

```py
reveal_type({1: 1, 2: 1})  # revealed: dict[int, int]
```

## Dict of tuples

```py
reveal_type({1: (1, 2), 2: (3, 4)})  # revealed: dict[int, tuple[int, int]]
```

## Unpacked dict

```py
from typing import Mapping, KeysView

a = {"a": 1, "b": 2}
b = {"c": 3, "d": 4}
c = {**a, **b}
reveal_type(c)  # revealed: dict[str, int]

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
    reveal_type({**a})  # revealed: dict[str, int]
    reveal_type({**b})  # revealed: dict[str, int]
    reveal_type({**c})  # revealed: dict[str, int]

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
reveal_type(x)  # revealed: dict[int, (_: int) -> int]
```

## Mixed dict

```py
# revealed: dict[str, int | tuple[int, ...]]
reveal_type({"a": 1, "b": (1, 2), "c": (1, 2, 3)})
```

## Dict comprehensions

```py
# revealed: dict[int, int]
reveal_type({x: y for x, y in enumerate(range(42))})
```

## Key narrowing

The original assignment to each key, as well as future assignments, are used to narrow access to
individual keys:

```py
from typing import TypedDict

x1 = {"a": 1, "b": "2"}
reveal_type(x1)  # revealed: dict[str, int | str]
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
reveal_type(x4["b"])  # revealed: dict[str, int | str]
reveal_type(x4["b"]["c"])  # revealed: Literal[2]
reveal_type(x4["b"]["d"])  # revealed: Literal["3"]

x5: dict[str, int | dict[str, int | TD]] = {"a": 1, "b": {"c": 2, "d": {"td": 1}}}
reveal_type(x5["a"])  # revealed: Literal[1]
reveal_type(x5["b"])  # revealed: dict[str, int | TD]
reveal_type(x5["b"]["c"])  # revealed: Literal[2]
reveal_type(x5["b"]["d"])  # revealed: TD

x6 = x7 = {"a": 1}
# TODO: This should reveal `Literal[1]`.
reveal_type(x6["a"])  # revealed: int
reveal_type(x7["a"])  # revealed: int

x8: list[dict[str, int | str]] = [{"a": 1, "b": "2"}, {"a": 3, "b": "4"}]
reveal_type(x8[0]["a"])  # revealed: Literal[1]
reveal_type(x8[1]["b"])  # revealed: Literal["4"]

x9: dict[str, list[dict[str, int | str]]] = {"a": [{"a": 1, "b": "2"}, {"a": 3, "b": "4"}]}
reveal_type(x9["a"][0]["a"])  # revealed: Literal[1]
reveal_type(x9["a"][1]["b"])  # revealed: Literal["4"]

x10: tuple[dict[str, int | str], ...] = ({"a": 1, "b": "2"}, {"a": 3, "b": "4"})
reveal_type(x10[0]["a"])  # revealed: Literal[1]
reveal_type(x10[1]["b"])  # revealed: Literal["4"]

x11: dict[str, tuple[dict[str, int | str], ...]] = {"a": ({"a": 1, "b": "2"}, {"a": 3, "b": "4"})}
reveal_type(x11["a"][0]["a"])  # revealed: Literal[1]
reveal_type(x11["a"][1]["b"])  # revealed: Literal["4"]

x12 = [({"a": 1, "b": "2"}, {"a": 3, "b": "4"}, *[{"a": 5}], {"a": 6})]
reveal_type(x12[0][0]["a"])  # revealed: Literal[1]
reveal_type(x12[0][1]["b"])  # revealed: Literal["4"]

# Starred expressions and any elements that follow them are not narrowed.
reveal_type(x12[0][2]["a"])  # revealed: int
reveal_type(x12[0][3]["b"])  # revealed: int
```

## Dict unpacking in function calls

Narrowing is also performed for dictionary unpacking expressions:

```py
def f1(a: int): ...
def f2(a: int, b: str): ...
def f3(a: int, b: str, c: float): ...
def accepts_inner(inner: dict[str, int]): ...

x1: dict[str, float | str] = {"a": 1, "b": "a"}

f2(**x1)  # ok

# N.B. We only use dictionary narrowing to narrow known keys to a more precise type, and fallback
# to the dictionary value type otherwise. We avoid making assumptions about which keys may or may
# not be present in ways that could lead to false positives.
f1(**x1)  # ok

# error: [invalid-argument-type]
f3(**x1)

x1["c"] = 1.0
f3(**x1)  # ok

def _(x: dict[str, int]):
    # error: [invalid-argument-type]
    f2(**x)

def _(x: dict[str, int | str]):
    # error: [invalid-argument-type]
    f1(**x)

    x["a"] = 1
    f1(**x)  # ok

def _(x: dict[str, int | str], flag: bool):
    if flag:
        x["a"] = 1

    # error: [invalid-argument-type]
    f1(**x)

x2: dict[str, object] = {"inner": {"a": 1}}
# error: [invalid-argument-type]
f1(**x2)

x3: dict[str, dict[str, object]] = {"inner": {"a": 1, "b": "a"}}

f2(**x3["inner"])  # ok
f1(**x3["inner"])  # ok
# error: [invalid-argument-type]
f3(**x3["inner"])

x3["inner"]["c"] = 1.0
f3(**x3["inner"])  # ok

x3["inner"] = {"inner": {"a": 1}}
# error: [invalid-argument-type]
f1(**x3["inner"])

def _(x: dict[str, object]):
    # error: [invalid-type-form]
    x["inner"]: dict[str, float | str] = {"a": 1, "b": "a"}

    f2(**x["inner"])  # ok
    f1(**x["inner"])  # ok
    # error: [invalid-argument-type]
    f3(**x["inner"])

    # The rejected annotation does not widen the assigned dictionary's value type.
    # error: [invalid-assignment]
    x["inner"]["c"] = 1.0

    x["inner"] = {"inner": {"a": 1}}
    accepts_inner(**x["inner"])  # ok
    # error: [invalid-argument-type]
    f1(**x["inner"])

def _(x: dict[str, object]):
    # An annotation-only subscript likewise cannot declare the nested dictionary's type.
    # error: [invalid-type-form]
    x["inner"]: dict[str, float | str]

    x["inner"] = {"inner": {"a": 1}}
    accepts_inner(**x["inner"])  # ok
    # error: [invalid-argument-type]
    f1(**x["inner"])

def _(x: dict[str, dict[str, float | str]]):
    # A rejected dictionary assignment does not establish known key types.
    # error: [invalid-assignment]
    x["kwargs"] = {"nested": {"a": 1}}
    reveal_type(x["kwargs"]["nested"])  # revealed: int | float | str
    # error: [invalid-argument-type]
    f1(**x["kwargs"])

def _(x: dict[str, dict[str, float | str]]):
    x["kwargs"] = {"nested": 1}
    reveal_type(x["kwargs"]["nested"])  # revealed: Literal[1]

    # A rejected replacement also invalidates a prior known-key type.
    # error: [invalid-assignment]
    x["kwargs"] = {"nested": {"a": 1}}
    reveal_type(x["kwargs"]["nested"])  # revealed: int | float | str

def accepts_value(**kwargs: object): ...
def _(x: dict[str, dict[str, float | str]]):
    # error: [invalid-assignment]
    x = {"kwargs": {"nested": {"a": object()}}}
    reveal_type(x["kwargs"]["nested"])  # revealed: int | float | str
    # error: [invalid-argument-type]
    accepts_value(**x["kwargs"]["nested"])

def _(x: list[dict[str, float | str]]):
    # error: [invalid-assignment]
    x = [{"nested": {"a": object()}}]
    # error: [invalid-argument-type]
    accepts_value(**x[0]["nested"])

def _(x: dict[str, object], y: int):
    # An invalid nested binding does not reject the dictionary assignment itself.
    # error: [invalid-assignment]
    x = {"a": (y := "bad")}
    reveal_type(x["a"])  # revealed: int

class Normalizing:
    def __getitem__(self, key: str) -> dict[str, object]:
        return {}
    def __setitem__(self, key: str, value: dict[str, object]) -> None:
        pass

def _(normalizing: Normalizing):
    # An arbitrary setter may transform the assigned value, so its children are not narrowed.
    normalizing["mapping"] = {"a": 1}
    reveal_type(normalizing["mapping"]["a"])  # revealed: object

class NormalizingDescriptor:
    def __get__(self, instance: object, owner: type | None = None) -> dict[str, object]:
        return {}
    def __set__(self, instance: object, value: object) -> None:
        pass

class WithNormalizingDescriptor:
    mapping: NormalizingDescriptor = NormalizingDescriptor()

def _(normalizing: WithNormalizingDescriptor):
    # A data descriptor may likewise transform the assigned value.
    normalizing.mapping = {"a": 1}
    reveal_type(normalizing.mapping["a"])  # revealed: object

class Y:
    inner: dict[str, object]

def _(y: Y):
    # error: [invalid-type-form]
    y.inner: dict[str, float | str] = {"a": 1, "b": "a"}

    y.inner = {"inner": {"a": 1}}
    accepts_inner(**y.inner)  # ok
    # error: [invalid-argument-type]
    f1(**y.inner)

def _(y: Y):
    y.inner = {"a": 1, "b": "a"}

    f2(**y.inner)  # ok
    f1(**y.inner)  # ok
    # error: [invalid-argument-type]
    f3(**y.inner)

    y.inner["c"] = 1.0
    f3(**y.inner)  # ok

    y.inner = {"inner": {"a": 1}}
    # error: [invalid-argument-type]
    f1(**y.inner)
```

## Rejected annotations in stubs

Annotation-only declarations in stubs are also bindings. A rejected annotation should fall back to
the type obtained by normal member lookup:

`stub.pyi`:

```pyi
x: dict[str, object]
# error: [invalid-type-form]
x["a"]: int
reveal_type(x["a"])  # revealed: object

x["b"] = ...
reveal_type(x["b"])  # revealed: Unknown

# error: [invalid-type-form]
x["c"]: int = ...
reveal_type(x["c"])  # revealed: Unknown
```
