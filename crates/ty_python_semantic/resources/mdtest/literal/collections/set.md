# Sets

## Basic set

```py
reveal_type({1, 2})  # revealed: set[int]
```

## Set of tuples

```py
reveal_type({(1, 2), (3, 4)})  # revealed: set[tuple[int, int]]
```

## Set of functions

```py
def a(_: int) -> int:
    return 0

def b(_: int) -> int:
    return 1

x = {a, b}
reveal_type(x)  # revealed: set[(_: int) -> int]
```

## Mixed set

```py
reveal_type({1, "a"})  # revealed: set[int | str]
# revealed: set[int | tuple[int, ...]]
reveal_type({1, (1, 2), (1, 2, 3)})
```

## Set comprehensions

```py
reveal_type({x for x in range(42)})  # revealed: set[int]
```

## Gradual set inference

Disabling strict set inference uses `Unknown` for heterogeneous elements, while preserving
homogeneous elements and leaving list and dictionary inference unchanged.

```toml
[analysis]
strict-set-inference = false
```

```py
reveal_type({1, "a"})  # revealed: set[Unknown]
reveal_type({"a", 1})  # revealed: set[Unknown]
reveal_type({1, 2})  # revealed: set[int]
reveal_type({*[1, 2], "a"})  # revealed: set[Unknown]
reveal_type([1, "a"])  # revealed: list[int | str]
reveal_type({"a": 1, "b": "two"})  # revealed: dict[str, int | str]
```

A single element or comprehension with a union type retains that type.

```py
def unions(value: int | str, values: set[int | str]):
    reveal_type({value})  # revealed: set[int | str]
    reveal_type({value, value})  # revealed: set[int | str]
    reveal_type({*values})  # revealed: set[int | str]
    reveal_type({*values, 1})  # revealed: set[Unknown]
    reveal_type({item for item in values})  # revealed: set[int | str]
```

Annotations and function parameters still constrain the elements. Constructors are unaffected.

```py
items: set[int | str] = {1, "a"}
reveal_type(items)  # revealed: set[int | str]
invalid: set[int] = {1, "a"}  # error: [invalid-assignment]

def accepts_ints(items: set[int]) -> None: ...

accepts_ints({1, "a"})  # error: [invalid-argument-type]
reveal_type(set((1, "a")))  # revealed: set[int | str]
```
