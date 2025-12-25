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
from typing import Literal

def list1[T](x: T) -> list[T]:
    return [x]

l1: list[Literal[1]] = list1(1)
reveal_type(l1)  # revealed: list[Literal[1]]

l2 = list1(1)
reveal_type(l2)  # revealed: list[int]

l3: list[int | str] | None = list1(1)
reveal_type(l3)  # revealed: list[int | str]

def _(l: list[int] | None = None):
    l1 = l or list()
    reveal_type(l1)  # revealed: (list[int] & ~AlwaysFalsy) | list[Unknown]

    l2: list[int] = l or list()
    # it would be better if this were `list[int]`? (https://github.com/astral-sh/ty/issues/136)
    reveal_type(l2)  # revealed: (list[int] & ~AlwaysFalsy) | list[Unknown]

def f[T](x: T, cond: bool) -> T | list[T]:
    return x if cond else [x]

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
d4: TD = dict(x=1)
d5: TD = dict(x="1")  # error: [invalid-argument-type]

reveal_type(d1)  # revealed: dict[Unknown | str, Unknown | int]
reveal_type(d2)  # revealed: TD
reveal_type(d3)  # revealed: dict[str, int]
reveal_type(d4)  # revealed: TD

def _() -> TD:
    return {"x": 1}

def _() -> TD:
    # error: [missing-typed-dict-key] "Missing required key 'x' in TypedDict `TD` constructor"
    # error: [invalid-return-type]
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
    a: list[Literal[1]]
    b: list[Literal[1]]

def _(e: E):
    e.a = [1]
    E.b = [1]
```

Function return types:

```py
def f() -> list[Literal[1]]:
    return [1]
```

## Instance attributes

```toml
[environment]
python-version = "3.12"
```

Both meta and class/instance attribute annotations are used as type context:

```py
from typing import Literal, Any

class DataDescriptor:
    def __get__(self, instance: object, owner: type | None = None) -> list[Literal[1]]:
        return []

    def __set__(self, instance: object, value: list[Literal[1]]) -> None:
        pass

def lst[T](x: T) -> list[T]:
    return [x]

def _(flag: bool):
    class Meta(type):
        if flag:
            x: DataDescriptor = DataDescriptor()

    class C(metaclass=Meta):
        x: list[int | None]

    def _(c: C):
        c.x = lst(1)

        # TODO: Use the parameter type of `__set__` as type context to avoid this error.
        # error: [invalid-assignment]
        C.x = lst(1)
```

For union targets, each element of the union is considered as a separate type context:

```py
from typing import Literal

class X:
    x: list[int | str]

class Y:
    x: list[int | None]

def lst[T](x: T) -> list[T]:
    return [x]

def _(xy: X | Y):
    xy.x = lst(1)
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

## Conditional expressions

```toml
[environment]
python-version = "3.12"
```

The type context is propagated through both branches of conditional expressions:

```py
def f[T](x: T) -> list[T]:
    raise NotImplementedError

def _(flag: bool):
    x1 = f(1) if flag else f(2)
    reveal_type(x1)  # revealed: list[int]

    x2: list[int | None] = f(1) if flag else f(2)
    reveal_type(x2)  # revealed: list[int | None]
```

## Multi-inference diagnostics

```toml
[environment]
python-version = "3.12"
```

Diagnostics unrelated to the type-context are only reported once:

`call.py`:

```py
def f[T](x: T) -> list[T]:
    return [x]

def a(x: list[bool], y: list[bool]): ...
def b(x: list[int], y: list[int]): ...
def c(x: list[int], y: list[int]): ...
def _(x: int):
    if x == 0:
        y = a
    elif x == 1:
        y = b
    else:
        y = c

    if x == 0:
        z = True

    y(f(True), [True])

    # error: [possibly-unresolved-reference] "Name `z` used when possibly not defined"
    y(f(True), [z])
```

`call_standalone_expression.py`:

```py
def f(_: str): ...
def g(_: str): ...
def _(a: object, b: object, flag: bool):
    if flag:
        x = f
    else:
        x = g

    # error: [unsupported-operator] "Operator `>` is not supported between two objects of type `object`"
    x(f"{'a' if a > b else 'b'}")
```

`attribute_assignment.py`:

```py
from typing import TypedDict

class TD(TypedDict):
    y: int

class X:
    td: TD

def _(x: X, flag: bool):
    if flag:
        y = 1

    # error: [possibly-unresolved-reference] "Name `y` used when possibly not defined"
    x.td = {"y": y}
```
