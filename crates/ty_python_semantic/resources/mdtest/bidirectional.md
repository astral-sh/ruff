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
    reveal_type(l2)  # revealed: list[int]

class TextContent: ...
class TagContent: ...

def expects_content(content: list[TextContent | TagContent]) -> None: ...
def optional_content(content: list[TextContent | TagContent] | None) -> None:
    expects_content(content or [TextContent()])

def invalid_fallback(content: list[TextContent | TagContent] | None) -> None:
    expects_content(content or [object()])  # error: [invalid-argument-type]

def f[T](x: T, cond: bool) -> T | list[T]:
    return x if cond else [x]

l5: int | list[int] = f(1, True)

x: list[int] = [1, 2, *(3, 4, 5)]
reveal_type(x)  # revealed: list[int]

x: list[list[int]] = [[1], [2], *([3], [4])]
reveal_type(x)  # revealed: list[list[int]]

x: list[list[int | str]] = [[1], [2]] * 3
reveal_type(x)  # revealed: list[list[int | str]]

x: list[list[int | str]] = 3 * ([[1]] + [[2]])
reveal_type(x)  # revealed: list[list[int | str]]

x: list[int | str] = 3 * ["x" for _ in range(3)]
reveal_type(x)  # revealed: list[int | str]

# Tuple elements are inferred individually, but type context can prevent e.g. `int` widening.
x: tuple[list[Literal[1]]] = (list1(1),)
reveal_type(x)  # revealed: tuple[list[Literal[1]]]

x: tuple[list[Literal[1]], ...] = (list1(1),) * 3
reveal_type(x)  # revealed: tuple[list[Literal[1]], ...]

x: tuple[list[Literal[1]], ...] = 3 * ((list1(1),) + (list1(1),))
reveal_type(x)  # revealed: tuple[list[Literal[1]], ...]

x: set[int | str] = {1, 2} | {3, 4}
reveal_type(x)  # revealed: set[int | str]

x: set[int | str] = {42 for _ in range(3)}
reveal_type(x)  # revealed: set[int | str]

x: dict[int | str, int | str] = {1: 2} | {3: 4}
reveal_type(x)  # revealed: dict[int | str, int | str]

x: dict[int | str, int | str] = {str(i): i for i in range(3)}
reveal_type(x)  # revealed: dict[int | str, int | str]

# TODO: We currently eagerly pass type context to collection literals on either side of a binary
# operator. That makes the cases above work, but it's not generally sound. For example, it gives the
# wrong result in this case.
class X:
    def __add__(self, _: list[int]) -> list[int | str]:
        return []

# error: [unsupported-operator] "Operator `+` is not supported between objects of type `X` and `list[int | str]`"
x: list[int | str] = X() + [1]

# TODO: We also don't yet support generic function calls like this.
# error: [invalid-assignment] "Object of type `list[int]` is not assignable to `list[int | str]`"
x: list[int | str] = list1(42) * 3
```

`typed_dict.py`:

```py
from typing import Any, Callable, Hashable, Mapping, TypedDict
from typing_extensions import Never

class TD(TypedDict):
    x: int

class BadTD(TypedDict):
    x: str

d1_literal = {"x": 1}
d1_dict = dict(x=1)

reveal_type(d1_literal)  # revealed: dict[str, int]
reveal_type(d1_dict)  # revealed: dict[str, int]

d2_literal: TD = {"x": 1}
d2_dict: TD = dict(x=1)
d2_unpack: TD = dict(**d2_literal)

reveal_type(d2_literal)  # revealed: TD
reveal_type(d2_dict)  # revealed: TD
reveal_type(d2_unpack)  # revealed: TD

d3_literal: dict[str, int] = {"x": 1}
d3_dict: dict[str, int] = dict(x=1)

reveal_type(d3_literal)  # revealed: dict[str, int]
reveal_type(d3_dict)  # revealed: dict[str, int]

d4_invalid_literal: TD = {"x": "1"}  # error: [invalid-argument-type]
d4_invalid_dict: TD = dict(x="1")  # error: [invalid-argument-type]

reveal_type(d4_invalid_literal)  # revealed: TD
reveal_type(d4_invalid_dict)  # revealed: TD

def unpack_invalid_typed_dict(src: BadTD) -> TD:
    # The fast path should validate TypedDict-shaped unpacks even when they are not assignable to
    # the target. That preserves the key-level TypedDict diagnostic instead of falling back to a
    # broad `dict[str, str]` assignment error.
    # error: [invalid-argument-type] "Invalid argument to key "x" with declared type `int` on TypedDict `TD`: value of type `str`"
    return dict(**src)

def return_any_unpack(src: Any) -> TD:
    return dict(**src)

def pass_never_unpack(src: Never) -> None:
    takes_td(dict(**src))

def takes_mapping(value: Mapping[str, object]) -> None:
    pass

def keep_keyword_diagnostics(kwargs: Mapping[str, object]) -> None:
    # The TypedDict-aware `dict(...)` fast path should not lose diagnostics from named keywords
    # when unsupported `**kwargs` forces it to fall back to ordinary dict inference.
    # error: [unresolved-reference] "Name `missing` used when not defined"
    # error: [invalid-assignment]
    maybe_td: TD = dict(x=missing, **kwargs)
    takes_mapping(maybe_td)

# Note: the second variant (`d5_dict`) is not technically allowed by the `dict.__init__` overloads
# in typeshed, which require the key type to be `str` when using keyword arguments. However, we
# special-case this pattern to match the behavior of `d5_literal`.
d5_literal: dict[Hashable, Callable[..., object]] = {"x": lambda: 1}
d5_dict: dict[Hashable, Callable[..., object]] = dict(x=lambda: 1)

d6_dict: TD = {"x": 1} | {"x": 2}

def return_literal() -> TD:
    return {"x": 1}

def return_dict() -> TD:
    return dict(x=1)

def return_unpack(src: TD) -> TD:
    return dict(**src)

def takes_td(value: TD) -> None:
    pass

def pass_unpack(src: TD) -> None:
    takes_td(dict(**src))

def return_invalid_literal() -> TD:
    # TODO: ideally, this would only emit the first error, but not `invalid-return-type` (like the `return_invalid_dict` case below).
    # error: [missing-typed-dict-key] "Missing required key 'x' in TypedDict `TD` constructor"
    # error: [invalid-return-type]
    return {}

def return_invalid_dict() -> TD:
    # error: [missing-typed-dict-key] "Missing required key 'x' in TypedDict `TD` constructor"
    return dict()
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
from concurrent.futures import Future
from os.path import abspath
from typing import Awaitable, Callable, TypeVar, Union, overload, TypedDict

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

@overload
def f7(y: list[int | str]) -> list[int | str]: ...
@overload
def f7[T](y: list[T]) -> list[T]: ...
def f7(y: object) -> object:
    raise NotImplementedError

# TODO: We should reveal `list[int | str]` here.
x9 = f7(reveal_type(["Sheet1"]))  # revealed: list[str]
reveal_type(x9)  # revealed: list[int | str]

# TODO: We should not error here once call inference can conjoin constraints
# from all call arguments.
def f8(xs: tuple[str, ...]) -> tuple[str, ...]:
    # error: [invalid-return-type]
    return tuple(map(abspath, xs))

T2 = TypeVar("T2")

def sink(func: Callable[[], Union[Awaitable[T2], T2]], future: Future[T2]) -> None:
    raise NotImplementedError

# TODO: This should not error once we conjoin constraints from all call arguments.
def f9(func: Callable[[], Union[Awaitable[T2], T2]]) -> Future[T2]:
    future: Future[T2] = Future()
    # error: [invalid-argument-type]
    sink(func, future)
    return future
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

# error: [invalid-argument-type] "Argument to constructor `A.__new__` is incorrect: Expected `list[int | str]`, found `list[list[Unknown]]`"
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

## Lambda expressions

If a lambda expression is annotated as a `Callable` type, the body of the lambda is inferred with
the annotated return type as type context, and the annotated parameter types are respected:

```py
from typing import Callable, TypedDict

class Bar(TypedDict):
    bar: int

def id[T](x: T) -> T:
    return x

f1 = lambda x: {"bar": 1}
reveal_type(f1)  # revealed: (x) -> dict[str, int]

f2: Callable[[int], Bar] = lambda x: {"bar": 1}
reveal_type(f2)  # revealed: (x: int) -> Bar

# error: [missing-typed-dict-key] "Missing required key 'bar' in TypedDict `Bar` constructor"
# error: [invalid-assignment] "Object of type `(x: int) -> dict[Unknown, Unknown]` is not assignable to `(int, /) -> Bar`"
f3: Callable[[int], Bar] = lambda x: {}
reveal_type(f3)  # revealed: (int, /) -> Bar

f4: Callable[[str], str] = lambda x: reveal_type(x)  # revealed: str
reveal_type(f4)  # revealed: (x: str) -> str

f5: Callable[[str], str] = id(lambda x: reveal_type(x))  # revealed: str
reveal_type(f5)  # revealed: (x: str) -> str

# TODO: This should not error once we support `Unpack`.
# error: [invalid-assignment]
f6: Callable[[*tuple[int, ...]], None] = lambda x, y, z: None
reveal_type(f6)  # revealed: (tuple[int, ...], /) -> None

f7: Callable[[int, str], None] = lambda *args: None
reveal_type(f7)  # revealed: (*args) -> None

# N.B. `Callable` annotations only support positional parameters.
# error: [invalid-assignment]
f8: Callable[[int], None] = lambda *, x=1: None
reveal_type(f8)  # revealed: (int, /) -> None

# TODO: This should reveal `(*args: int, *, x=1) -> None` once we support `Unpack`.
f9: Callable[[*tuple[int, ...], int], None] = lambda *args, x=1: None
reveal_type(f9)  # revealed: (*args, *, x=1) -> None

f10: Callable[[str, int, str], tuple[str, int, str]] = lambda x, y, z: reveal_type((x, y, z))  # revealed: tuple[str, int, str]
reveal_type(f10)  # revealed: (x: str, y: int, z: str) -> tuple[str, int, str]

# TODO: This should reveal `tuple[int, ...]` once we support `Unpack`.
f11: Callable[[*tuple[int, ...]], tuple[int, ...]] = lambda *args: reveal_type(args)  # revealed: tuple[Unknown, ...]
reveal_type(f11)  # revealed: (*args) -> tuple[Unknown, ...]

# TODO: Better generic call inference.
def _(x: list[int]):
    f12 = list(map(lambda y: y + 1, x))
    reveal_type(f12)  # revealed: list[Unknown]

def _() -> Callable[[int], int]:
    return id(lambda x: reveal_type(x))  # revealed: int

def _():
    def takes_callable(_: Callable[[int], int]): ...

    takes_callable(lambda x: reveal_type(x))  # revealed: int
    takes_callable(id(id(lambda x: reveal_type(x))))  # revealed: int

def _(x: bool):
    signatures = {
        "upper": str.upper,
        "lower": str.lower,
        "title": str.title,
    }

    # revealed: (x) -> Unknown
    f = signatures.get("", reveal_type(lambda x: x))
```

We do not currently account for type annotations present later in the scope:

```py
f12 = lambda: [1]
# TODO: This should not error.
_: list[int | str] = f12()  # error: [invalid-assignment]
reveal_type(f12)  # revealed: () -> list[int]
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

## `await` expressions

Type context is also propagated through `await` expressions:

```py
from typing import Literal

async def make_lst[T](x: T) -> list[T]:
    return [x]

async def _():
    x1 = await make_lst(1)
    reveal_type(x1)  # revealed: list[int]

    x2: list[Literal[1]] = await make_lst(1)
    reveal_type(x2)  # revealed: list[Literal[1]]

    x3: list[int | None] = await make_lst(1)
    reveal_type(x3)  # revealed: list[int | None]
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
