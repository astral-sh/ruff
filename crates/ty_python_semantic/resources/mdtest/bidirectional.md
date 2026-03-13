# Bidirectional type inference

ty partially supports bidirectional type inference. This is a mechanism for inferring the type of an
expression "from the outside in". Normally, type inference proceeds "from the inside out". That is,
in order to infer the type of an expression, the types of all sub-expressions must first be
inferred. There is no reverse dependency. However, when performing complex type inference, such as
when generics are involved, the type of an outer expression can sometimes be useful in inferring
inner expressions. Bidirectional type inference is a mechanism that propagates such "expected types"
to the inference of inner expressions.

```toml
[environment]
python-version = "3.12"
```

## Propagating target type annotation

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

a: list[int] = [1, 2, *(3, 4, 5)]
reveal_type(a)  # revealed: list[int]

b: list[list[int]] = [[1], [2], *([3], [4])]
reveal_type(b)  # revealed: list[list[int]]
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

reveal_type(d1)  # revealed: dict[str, int]
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

```py
from typing import overload, Callable

def list1[T](x: T) -> list[T]:
    return [x]

def f() -> list[object]:
    reveal_type(list1(1))  # revealed: list[int]
    # `list[int]` and `list[object]` are incompatible, but the return type check passes here
    # because the type of `list1(res)` is inferred by bidirectional type inference using the
    # annotated return type, and the type of `res` is not used.
    return list1(1)

def f2() -> list[object] | None:
    reveal_type(list1(1))  # revealed: list[int]
    return list1(1)

def deco[T](func: Callable[[], T]) -> Callable[[], T]:
    return func

def outer() -> Callable[[], list[object]]:
    @deco
    def inner() -> list[object]:
        reveal_type(list1(1))  # revealed: list[int]
        return list1(1)
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

Both meta and class/instance attribute annotations are used as type context:

```py
from typing import Literal, Any

class DataDescriptor:
    def __get__(self, instance: object, owner: type | None = None) -> list[Literal[1]]:
        return []

    def __set__(self, instance: object, value: list[Literal[1]]) -> None:
        pass

def _(flag: bool):
    class Meta(type):
        if flag:
            x: DataDescriptor = DataDescriptor()

    class C(metaclass=Meta):
        x: list[int | None]

    def _(c: C):
        c.x = reveal_type([1])  # revealed: list[int]

        # TODO: Use the parameter type of `__set__` as type context to avoid this error.
        # error: [invalid-assignment]
        C.x = [1]
```

For union targets, each element of the union is considered as a separate type context:

```py
from typing import Literal

class X:
    x: list[int | str]

class Y:
    x: list[int | None]

def _(xy: X | Y):
    xy.x = reveal_type([1])  # revealed: list[int]
```

## Overload evaluation

The type context of all matching overloads are considered during argument inference:

```py
from typing import overload, TypedDict

def int_or_str() -> int | str:
    raise NotImplementedError

@overload
def f1(x: list[int | None], y: int) -> int: ...
@overload
def f1(x: list[int | str], y: str) -> str: ...
def f1(x, y) -> int | str:
    raise NotImplementedError

# TODO: We should reveal `list[int]` here.
x1 = f1(reveal_type([1]), 1)  # revealed: list[int]
reveal_type(x1)  # revealed: int

x2 = f1(reveal_type([1]), int_or_str())  # revealed: list[int]
reveal_type(x2)  # revealed: int | str

@overload
def f2[T](x: T, y: int) -> T: ...
@overload
def f2(x: list[int | str], y: str) -> object: ...
def f2(x, y) -> object: ...

x3 = f2(reveal_type([1]), 1)  # revealed: list[int]
reveal_type(x3)  # revealed: list[int]

class TD(TypedDict):
    x: list[int | str]

class TD2(TypedDict):
    x: list[int | None]

@overload
def f3(x: TD, y: int) -> int: ...
@overload
def f3(x: TD2, y: str) -> str: ...
def f3(x, y) -> object: ...

# TODO: We should reveal `TD2` here.
x4 = f3(reveal_type({"x": [1]}), "1")  # revealed: dict[str, list[int]]
reveal_type(x4)  # revealed: str

x5 = f3(reveal_type({"x": [1]}), int_or_str())  # revealed: dict[str, list[int]]
reveal_type(x5)  # revealed: int | str

@overload
def f4[T](_: list[T]) -> list[T]: ...
@overload
def f4(_: list[str]) -> list[str]: ...
def f4(_: object): ...

x6 = f4(reveal_type([]))  # revealed: list[Unknown]
reveal_type(x6)  # revealed: list[Unknown]

@overload
def f5(_: list[int | str]) -> int: ...
@overload
def f5(_: set[int | str]) -> str: ...
def f5(_) -> object:
    raise NotImplementedError

def list_or_set[T](x: T) -> list[T] | set[T]:
    raise NotImplementedError

# TODO: We should reveal `list[int | str] | set[int | str]` here.
x7 = f5(reveal_type(list_or_set(1)))  # revealed: list[int] | set[int]
reveal_type(x7)  # revealed: int | str

@overload
def f6(_: list[int | None]) -> int: ...
@overload
def f6(_: set[int | str]) -> str: ...
def f6(_) -> object:
    raise NotImplementedError

def list_or_set2[T, U](x: T, y: U) -> list[T] | set[U]:
    raise NotImplementedError

# TODO: We should not error here.
# error: [no-matching-overload]
x8 = f6(reveal_type(list_or_set2(1, 1)))  # revealed: list[int] | set[int]
reveal_type(x8)  # revealed: Unknown
```

## Class constructor parameters

The parameters of both `__init__` and `__new__` are used as type context sources for constructor
calls:

```py
def f[T](x: T) -> list[T]:
    return [x]

class A:
    def __new__(cls, value: list[int | str]):
        return super().__new__(cls)

    def __init__(self, value: list[int | None]): ...

A(f(1))

# error: [invalid-argument-type] "Argument to function `__new__` is incorrect: Expected `list[int | str]`, found `list[list[Unknown]]`"
# error: [invalid-argument-type] "Argument to bound method `__init__` is incorrect: Expected `list[int | None]`, found `list[list[Unknown]]`"
A(f([]))
```

## Conditional expressions

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

## Dunder Calls

The key and value parameters types are used as type context for `__setitem__` dunder calls:

```py
from typing import TypedDict

class Bar(TypedDict):
    bar: list[int | str]

class Baz(TypedDict):
    bar: list[int | None]

def _(x: dict[str, Bar]):
    x["foo"] = reveal_type({"bar": [2]})  # revealed: Bar

class X:
    def __setitem__(self, key: Bar, value: Bar): ...

def _(x: X):
    # revealed: Bar
    x[reveal_type({"bar": [1]})] = reveal_type({"bar": [2]})  # revealed: Bar
```

If the target is a union or intersection type, the key and value expressions may be inferred
multiple times for each applicable type context:

```py
from ty_extensions import Intersection

def _(x: X | dict[Baz, Baz]):
    # revealed: dict[str, list[int]]
    x[reveal_type({"bar": [1]})] = reveal_type({"bar": [2]})  # revealed: dict[str, list[int]]

class Y:
    def __setitem__(self, key: Baz, value: Baz): ...

def _(x: Intersection[X, Y]):
    # revealed: Bar
    x[reveal_type({"bar": [1, "2"]})] = reveal_type({"bar": [3, "4"]})  # revealed: Bar

    # revealed: Baz
    x[reveal_type({"bar": [1, None]})] = reveal_type({"bar": [2, None]})  # revealed: Baz
```

Similarly, the declared type of a `TypedDict` key is used as type context:

```py
from typing import Literal

class TD(TypedDict):
    foo: list[int | None]
    bar: list[int | str]

def _(x: TD, foo_or_bar: Literal["foo", "bar"]):
    x["foo"] = reveal_type([1])  # revealed: list[int | None]
    x["bar"] = reveal_type([2])  # revealed: list[int | str]
    x[foo_or_bar] = reveal_type([3])  # revealed: list[int]

def _(x: TD | dict[str, list[int | float]]):
    x["foo"] = reveal_type([4])  # revealed: list[int]

def _(x: Bar | Baz | dict[str, list[int | float]]):
    x["bar"] = reveal_type([4])  # revealed: list[int]
```

As well as the value parameter type of augmented assignment dunder calls:

```py
from typing import TypedDict

def _(bar: Bar):
    bar |= reveal_type({"bar": [1]})  # revealed: Bar

class X:
    def __ior__(self, other: Baz): ...

def _(x: X):
    x |= reveal_type({"bar": [1]})  # revealed: Baz

def _(x: X | Bar):
    x |= reveal_type({"bar": [1]})  # revealed: dict[str, list[int]]

class Y:
    def __ior__(self, other: Bar): ...

def _(x: Intersection[X, Y]):
    # TODO: Reveal `Bar` and `Baz` here.
    x |= reveal_type({"bar": [1, "2"]})  # revealed: dict[str, list[int | str]]
    x |= reveal_type({"bar": [1, None]})  # revealed: dict[str, list[int | None]]
```

## Multi-inference diagnostics

Diagnostics unrelated to the type-context are only reported once:

```py
from typing import TypedDict

def lst[T](x: T) -> list[T]:
    return [x]

def takes_list_of_bool(x: list[bool], y: list[bool]): ...
def takes_list_of_int(x: list[int], y: list[int]): ...
def takes_list_of_int2(x: list[int], y: list[int]): ...
def _(x: int):
    if x == 0:
        y = takes_list_of_bool
    elif x == 1:
        y = takes_list_of_int
    else:
        y = takes_list_of_int2

    if x == 0:
        z = True

    y(lst(True), [True])

    # error: [possibly-unresolved-reference] "Name `z` used when possibly not defined"
    y(lst(True), [z])
```

```py
def g[T](x: T, y: list[T | None]) -> T:
    return x

def _(flag: bool):
    if flag:
        x = 1

    # error: [possibly-unresolved-reference]
    x1: int | str = g(x, [1])
    reveal_type(x1)  # revealed: int

    if flag:
        y = "1"

    # error: [possibly-unresolved-reference]
    x2: list[int | None] | list[str | None] = [y]
    reveal_type(x2)  # revealed: list[str | None]
```

```py
class Bar(TypedDict):
    bar: int

class Bar2(TypedDict):
    bar: int

class Bar3(TypedDict):
    bar: int

def _(flag: bool, bar: Bar | Bar2 | Bar3):
    if flag:
        y = 1

    # error: [possibly-unresolved-reference]
    bar |= {"bar": y}

def _(flag: bool, x: dict[Bar, Bar] | dict[Bar2, Bar2] | dict[Bar3, Bar3]):
    if flag:
        y = 1

    # error: [possibly-unresolved-reference]
    x[{"bar": y}] = {"bar": 1}
    # error: [possibly-unresolved-reference]
    x[{"bar": 1}] = {"bar": y}

class TD(TypedDict):
    foo: Bar

def _(flag: bool, x: TD | dict[str, Bar2] | dict[str, Bar3]):
    if flag:
        y = 1

    # error: [possibly-unresolved-reference]
    x["foo"] = {"bar": y}
```

```py
def takes_str(_: str): ...
def takes_str2(_: str): ...
def _(a: object, b: object, flag: bool):
    if flag:
        x = takes_str
    else:
        x = takes_str2

    # error: [unsupported-operator] "Operator `>` is not supported between two objects of type `object`"
    x(f"{'a' if a > b else 'b'}")
```

```py
from typing import TypedDict

class HasTD:
    td: Bar

def _(has_td: HasTD, flag: bool):
    if flag:
        y = 1

    # error: [possibly-unresolved-reference] "Name `y` used when possibly not defined"
    has_td.td = {"bar": y}
```
