# If expressions

## Simple if-expression

```py
def _(flag: bool):
    x = 1 if flag else 2
    reveal_type(x)  # revealed: Literal[1, 2]
```

## If-expression with walrus operator

```py
def _(flag: bool):
    y = 0
    z = 0
    x = (y := 1) if flag else (z := 2)
    reveal_type(x)  # revealed: Literal[1, 2]
    reveal_type(y)  # revealed: Literal[0, 1]
    reveal_type(z)  # revealed: Literal[0, 2]
```

## Nested if-expression

```py
def _(flag: bool, flag2: bool):
    x = 1 if flag else 2 if flag2 else 3
    reveal_type(x)  # revealed: Literal[1, 2, 3]
```

## None

```py
def _(flag: bool):
    x = 1 if flag else None
    reveal_type(x)  # revealed: Literal[1] | None
```

## Empty collection branches

A nonempty literal provides element types for an empty literal of the same collection kind in either
branch.

```py
def _(flag: bool):
    nonempty_first = ["x"] if flag else []
    reveal_type(nonempty_first)  # revealed: list[str]

    empty_first = [] if flag else ["x"]
    reveal_type(empty_first)  # revealed: list[str]

    mapping = {"x": 1} if flag else {}
    reveal_type(mapping)  # revealed: dict[str, int]

    reversed_mapping = {} if flag else {"x": 1}
    reveal_type(reversed_mapping)  # revealed: dict[str, int]
```

## Empty collection branches with declared types

An explicit annotation supplies the context for both branches. An incompatible nonempty branch still
produces a diagnostic.

```py
def _(flag: bool):
    values: list[object] = ["x"] if flag else []
    reveal_type(values)  # revealed: list[object]

    mapping: dict[str, object] = {"x": 1} if flag else {}
    reveal_type(mapping)  # revealed: dict[str, object]

    incompatible: list[str] = [1] if flag else []  # error: [invalid-assignment]
    reversed_incompatible: list[str] = [] if flag else [1]  # error: [invalid-assignment]
    incompatible_mapping: dict[str, int] = {"x": "y"} if flag else {}  # error: [invalid-assignment]
```

## Joint collection branches

If both branches contain literals for the same collection type, the type inferred for the
conditional expression combines both literals:

```py
def _(flag: bool):
    a = ["x"] if flag else [1]
    reveal_type(a)  # revealed: list[str | int]

    b = [1] if flag else ["x"]
    reveal_type(b)  # revealed: list[int | str]

    items = {"x"} if flag else {1}
    reveal_type(items)  # revealed: set[str | int]

    mapping = {"key": 1} if flag else {2: "value"}
    reveal_type(mapping)  # revealed: dict[str | int, int | str]

    _ = ["x"] if flag else [int(missing)]  # error: [unresolved-reference]
```

Dynamic types are retained from both branches:

```py
from typing import Any

def _(flag: bool, dynamic: Any):
    a = [dynamic] if flag else [1]
    reveal_type(a)  # revealed: list[Any | int]

    b = [1] if flag else [dynamic]
    reveal_type(b)  # revealed: list[int | Any]

    nested = [[]] if flag else [[1]]
    reveal_type(nested)  # revealed: list[list[Unknown] | list[int]]
```

When both branches are empty, their element types remain unknown:

```py
def _(flag: bool):
    reveal_type([] if flag else [])  # revealed: list[Unknown]
    reveal_type({} if flag else {})  # revealed: dict[Unknown, Unknown]
```

A list in one branch and a dictionary in the other produce a union of the two types:

```py
def _(flag: bool):
    reveal_type(["x"] if flag else {})  # revealed: list[str] | dict[Unknown, Unknown]
```

If one branch contains an existing collection, the literal in the other branch is inferred against
the existing collection's declared type:

```py
def _(flag: bool, existing: list[int], gradual: list[Any]):
    reveal_type(["x"] if flag else gradual)  # revealed: list[Any]

    aliased = ["x"] if flag else existing
    reveal_type(aliased)  # revealed: list[int | str] | list[int]
```

An explicit union annotation can supply a different element type to each branch:

```py
def _(flag: bool):
    x: list[int] | list[str] = [1] if flag else ["x"]
    reveal_type(x)  # revealed: list[int] | list[str]
```

## Collection branches and evaluation order

The condition is evaluated before either branch, and a branch's assignment expression remains
conditional.

```py
def _(flag: bool):
    value = "before"
    result = [(value := "after")] if flag else []
    reveal_type(result)  # revealed: list[str]
    reveal_type(value)  # revealed: Literal["before", "after"]

    result = [selected] if (selected := flag) else []
    reveal_type(result)  # revealed: list[bool]

    skipped = "before"
    result = [(skipped := "after")] if False else []
    reveal_type(result)  # revealed: list[str]
    reveal_type(skipped)  # revealed: Literal["before"]

    reachable = ["x"] if False else [1]
    reveal_type(reachable)  # revealed: list[int]
```

## Statically known compound conditions

Short-circuit conditions can select a single branch even when an operand has mutable truthiness.
Saving the condition's value and testing it again does not provide the same guarantee.

```py
def _(value: object):
    reveal_type(1 if value and False else 2)  # revealed: Literal[2]
    reveal_type(1 if value or True else 2)  # revealed: Literal[1]

    saved = value and False
    reveal_type(1 if saved else 2)  # revealed: Literal[1, 2]
```

A comparison chain can select a single branch even when an individual comparison returns an
arbitrary object. Saving the chain's result allows that object's truthiness to be tested again.

```py
class Comparable:
    def __lt__(self, other: int) -> object:
        return object()

def _(value: Comparable):
    reveal_type(1 if value < 1 < 0 else 2)  # revealed: Literal[2]

    saved = value < 1 < 0
    reveal_type(1 if saved else 2)  # revealed: Literal[1, 2]
```

An operand narrowed to `Never` cannot produce a result. Nested conditions preserve the remaining
short-circuit outcome when selecting a branch.

```py
def _(other: object, value: bool):
    reveal_type(1 if other and (isinstance(value, str) and value) else 2)  # revealed: Literal[2]
    reveal_type(1 if other or (not isinstance(value, str) or value) else 2)  # revealed: Literal[1]
```

## Conditions with operands equivalent to `Never`

A call whose return type is an alias of `Never` cannot produce a result. The preceding short-circuit
outcome alone selects the conditional expression's branch.

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Never

type Bottom = Never

def stop() -> Bottom:
    raise RuntimeError

def _(flag: bool):
    reveal_type(1 if flag and stop() else 2)  # revealed: Literal[2]
    reveal_type(1 if flag or stop() else 2)  # revealed: Literal[1]
```

A type variable bounded by `Never` also cannot produce a result.

```py
def _[T: Never](flag: bool, value: T):
    reveal_type(1 if flag and value else 2)  # revealed: Literal[2]
```

## Condition with object that implements `__bool__` incorrectly

```py
class NotBoolable:
    __bool__: int = 3

# error: [unsupported-bool-conversion] "Boolean conversion is not supported for type `NotBoolable`"
3 if NotBoolable() else 4
```
