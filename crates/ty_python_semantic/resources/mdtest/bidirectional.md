# Bidirectional type inference

ty partially supports bidirectional type inference. This is a mechanism for inferring the type of an
expression "from the outside in". Normally, type inference proceeds "from the inside out". That is,
in order to infer the type of an expression, the types of all sub-expressions must first be
inferred. There is no reverse dependency. However, when performing complex type inference, such as
when generics are involved, the type of an outer expression can sometimes be useful in inferring
inner expressions. Bidirectional type inference is a mechanism that propagates such "expected types"
to the inference of inner expressions.

## Propagating target type annotation

```toml
[environment]
python-version = "3.12"
```

```py
def list1[T](x: T) -> list[T]:
    return [x]

l1 = list1(1)
reveal_type(l1)  # revealed: list[Literal[1]]
l2: list[int] = list1(1)
reveal_type(l2)  # revealed: list[int]

# `list[Literal[1]]` and `list[int]` are incompatible, since `list[T]` is invariant in `T`.
# error: [invalid-assignment] "Object of type `list[Literal[1]]` is not assignable to `list[int]`"
l2 = l1

intermediate = list1(1)
# TODO: the error will not occur if we can infer the type of `intermediate` to be `list[int]`
# error: [invalid-assignment] "Object of type `list[Literal[1]]` is not assignable to `list[int]`"
l3: list[int] = intermediate
# TODO: it would be nice if this were `list[int]`
reveal_type(intermediate)  # revealed: list[Literal[1]]
reveal_type(l3)  # revealed: list[int]

l4: list[int | str] | None = list1(1)
reveal_type(l4)  # revealed: list[int | str]

def _(l: list[int] | None = None):
    l1 = l or list()
    reveal_type(l1)  # revealed: (list[int] & ~AlwaysFalsy) | list[Unknown]

    l2: list[int] = l or list()
    # it would be better if this were `list[int]`? (https://github.com/astral-sh/ty/issues/136)
    reveal_type(l2)  # revealed: (list[int] & ~AlwaysFalsy) | list[Unknown]

def f[T](x: T, cond: bool) -> T | list[T]:
    return x if cond else [x]

# TODO: no error
# error: [invalid-assignment] "Object of type `Literal[1] | list[Literal[1]]` is not assignable to `int | list[int]`"
l5: int | list[int] = f(1, True)
```

`typed_dict.py`:

```py
from typing import TypedDict

class TD(TypedDict):
    x: int

d1 = {"x": 1}
d2: TD = {"x": 1}
d3: dict[str, int] = {"x": 1}

reveal_type(d1)  # revealed: dict[Unknown | str, Unknown | int]
reveal_type(d2)  # revealed: TD
reveal_type(d3)  # revealed: dict[str, int]

def _() -> TD:
    return {"x": 1}

def _() -> TD:
    # error: [missing-typed-dict-key] "Missing required key 'x' in TypedDict `TD` constructor"
    return {}
```

## Propagating return type annotation

```toml
[environment]
python-version = "3.12"
```

```py
from typing import overload, Callable

def list1[T](x: T) -> list[T]:
    return [x]

def get_data() -> dict | None:
    return {}

def wrap_data() -> list[dict]:
    if not (res := get_data()):
        return list1({})
    reveal_type(list1(res))  # revealed: list[dict[Unknown, Unknown] & ~AlwaysFalsy]
    # `list[dict[Unknown, Unknown] & ~AlwaysFalsy]` and `list[dict[Unknown, Unknown]]` are incompatible,
    # but the return type check passes here because the type of `list1(res)` is inferred
    # by bidirectional type inference using the annotated return type, and the type of `res` is not used.
    return list1(res)

def wrap_data2() -> list[dict] | None:
    if not (res := get_data()):
        return None
    reveal_type(list1(res))  # revealed: list[dict[Unknown, Unknown] & ~AlwaysFalsy]
    return list1(res)

def deco[T](func: Callable[[], T]) -> Callable[[], T]:
    return func

def outer() -> Callable[[], list[dict]]:
    @deco
    def inner() -> list[dict]:
        if not (res := get_data()):
            return list1({})
        reveal_type(list1(res))  # revealed: list[dict[Unknown, Unknown] & ~AlwaysFalsy]
        return list1(res)
    return inner

@overload
def f(x: int) -> list[int]: ...
@overload
def f(x: str) -> list[str]: ...
def f(x: int | str) -> list[int] | list[str]:
    # `list[int] | list[str]` is disjoint from `list[int | str]`.
    if isinstance(x, int):
        return list1(x)
    else:
        return list1(x)

reveal_type(f(1))  # revealed: list[int]
reveal_type(f("a"))  # revealed: list[str]

async def g() -> list[int | str]:
    return list1(1)

def h[T](x: T, cond: bool) -> T | list[T]:
    return i(x, cond)

def i[T](x: T, cond: bool) -> T | list[T]:
    return x if cond else [x]
```

## Type context sources

Type context is sourced from various places, including annotated assignments:

```py
from typing import Literal

a: list[Literal[1]] = [1]
```

Function parameter annotations:

```py
def b(x: list[Literal[1]]): ...

b([1])
```

Bound method parameter annotations:

```py
class C:
    def __init__(self, x: list[Literal[1]]): ...
    def foo(self, x: list[Literal[1]]): ...

C([1]).foo([1])
```

Declared variable types:

```py
d: list[Literal[1]]
d = [1]
```

Declared attribute types:

```py
class E:
    e: list[Literal[1]]

def _(e: E):
    # TODO: Implement attribute type context.
    # error: [invalid-assignment] "Object of type `list[Unknown | int]` is not assignable to attribute `e` of type `list[Literal[1]]`"
    e.e = [1]
```

Function return types:

```py
def f() -> list[Literal[1]]:
    return [1]
```

## Class constructor parameters

```toml
[environment]
python-version = "3.12"
```

The parameters of both `__init__` and `__new__` are used as type context sources for constructor
calls:

```py
def f[T](x: T) -> list[T]:
    return [x]

class A:
    def __new__(cls, value: list[int | str]):
        return super().__new__(cls, value)

    def __init__(self, value: list[int | None]): ...

A(f(1))

# error: [invalid-argument-type] "Argument to function `__new__` is incorrect: Expected `list[int | str]`, found `list[list[Unknown]]`"
# error: [invalid-argument-type] "Argument to bound method `__init__` is incorrect: Expected `list[int | None]`, found `list[list[Unknown]]`"
A(f([]))
```
