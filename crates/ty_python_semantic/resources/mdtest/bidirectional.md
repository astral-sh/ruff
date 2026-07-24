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
python-version = "3.13"
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

## Collection literals

### Basic

```py
import typing

a: list[int] = [1, 2, 3]
reveal_type(a)  # revealed: list[int]

b: list[int | str] = [1, 2, 3]
reveal_type(b)  # revealed: list[int | str]

c: typing.List[int] = [1, 2, 3]
reveal_type(c)  # revealed: list[int]

d: list[typing.Any] = []
reveal_type(d)  # revealed: list[Any]

e: set[int] = {1, 2, 3}
reveal_type(e)  # revealed: set[int]

f: set[int | str] = {1, 2, 3}
reveal_type(f)  # revealed: set[int | str]

g: typing.Set[int] = {1, 2, 3}
reveal_type(g)  # revealed: set[int]

h: list[list[int]] = [[], [42]]
reveal_type(h)  # revealed: list[list[int]]

i: list[typing.Any] = [1, 2, "3", ([4],)]
reveal_type(i)  # revealed: list[Any]

j: list[tuple[str | int, ...]] = [(1, 2), ("foo", "bar"), ()]
reveal_type(j)  # revealed: list[tuple[str | int, ...]]

k: list[tuple[list[int], ...]] = [([],), ([1, 2], [3, 4]), ([5], [6], [7])]
reveal_type(k)  # revealed: list[tuple[list[int], ...]]

l: tuple[list[int], *tuple[list[typing.Any], ...], list[str]] = ([1, 2, 3], [4, 5, 6], [7, 8, 9], ["10", "11", "12"])
reveal_type(l)  # revealed: tuple[list[int], list[Any], list[Any], list[str]]

type IntList = list[int]

m: IntList = [1, 2, 3]
reveal_type(m)  # revealed: list[int]

n: list[typing.Literal[1, 2, 3]] = [1, 2, 3]
reveal_type(n)  # revealed: list[Literal[1, 2, 3]]

o: list[typing.LiteralString] = ["a", "b", "c"]
reveal_type(o)  # revealed: list[LiteralString]

p: dict[int, int] = {}
reveal_type(p)  # revealed: dict[int, int]

q: dict[int | str, int] = {1: 1, 2: 2, 3: 3}
reveal_type(q)  # revealed: dict[int | str, int]

r: dict[int | str, int | str] = {1: 1, 2: 2, 3: 3}
reveal_type(r)  # revealed: dict[int | str, int | str]

s: dict[int | str, int | str]
s = {1: 1, 2: 2, 3: 3}
reveal_type(s)  # revealed: dict[int | str, int | str]
(s := {1: 1, 2: 2, 3: 3})
reveal_type(s)  # revealed: dict[int | str, int | str]
```

### Optional unions

```py
import typing

a: list[int] | None = [1, 2, 3]
reveal_type(a)  # revealed: list[int]

b: list[int | str] | None = [1, 2, 3]
reveal_type(b)  # revealed: list[int | str]

c: typing.List[int] | None = [1, 2, 3]
reveal_type(c)  # revealed: list[int]

d: list[typing.Any] | None = []
reveal_type(d)  # revealed: list[Any]

e: set[int] | None = {1, 2, 3}
reveal_type(e)  # revealed: set[int]

f: set[int | str] | None = {1, 2, 3}
reveal_type(f)  # revealed: set[int | str]

g: typing.Set[int] | None = {1, 2, 3}
reveal_type(g)  # revealed: set[int]

h: list[list[int]] | None = [[], [42]]
reveal_type(h)  # revealed: list[list[int]]

i: list[typing.Any] | None = [1, 2, "3", ([4],)]
reveal_type(i)  # revealed: list[Any]

j: list[tuple[str | int, ...]] | None = [(1, 2), ("foo", "bar"), ()]
reveal_type(j)  # revealed: list[tuple[str | int, ...]]

k: list[tuple[list[int], ...]] | None = [([],), ([1, 2], [3, 4]), ([5], [6], [7])]
reveal_type(k)  # revealed: list[tuple[list[int], ...]]

l: tuple[list[int], *tuple[list[typing.Any], ...], list[str]] | None = ([1, 2, 3], [4, 5, 6], [7, 8, 9], ["10", "11", "12"])
reveal_type(l)  # revealed: tuple[list[int], list[Any], list[Any], list[str]]

type IntList = list[int]

m: IntList | None = [1, 2, 3]
reveal_type(m)  # revealed: list[int]

n: list[typing.Literal[1, 2, 3]] | None = [1, 2, 3]
reveal_type(n)  # revealed: list[Literal[1, 2, 3]]

o: list[typing.LiteralString] | None = ["a", "b", "c"]
reveal_type(o)  # revealed: list[LiteralString]

p: dict[int, int] | None = {}
reveal_type(p)  # revealed: dict[int, int]

q: dict[int | str, int] | None = {1: 1, 2: 2, 3: 3}
reveal_type(q)  # revealed: dict[int | str, int]

r: dict[int | str, int | str] | None = {1: 1, 2: 2, 3: 3}
reveal_type(r)  # revealed: dict[int | str, int | str]
```

### Starred elements and dictionary unpacking

```py
from typing import Any

x1: list[int] = [1, 2, *(3, 4, 5)]
reveal_type(x1)  # revealed: list[int]

x2: list[list[int]] = [[1], [2], *([3], [4])]
reveal_type(x2)  # revealed: list[list[int]]

x3: dict[str, int] = {"a": 1, **{"b": 2}}
reveal_type(x3)  # revealed: dict[str, int]

def dynamic_mapping() -> Any: ...

x4: dict[str, int] = reveal_type({**dynamic_mapping()})  # revealed: dict[str | Any, int | Any]

# error: [invalid-argument-type] "Argument expression after ** must be a mapping type"
x5: dict[str, int] = {**42}
```

### Collection unions

```py
from collections.abc import Mapping, Sequence
from typing import Literal

x1: list[int | str] | list[int | None] = [1, 2, 3]
reveal_type(x1)  # revealed: list[int | str]

x2: Sequence[int | str] | Sequence[int | None] = [1, 2, 3]
reveal_type(x2)  # revealed: list[int]

x3: list[int] | list[int | None] | list[str | None] = ["1", "2"]
reveal_type(x3)  # revealed: list[str | None]

x4: dict[str, list[int | None]] | dict[str, list[str | None]] = {"a": ["b"]}
reveal_type(x4)  # revealed: dict[str, list[str | None]]

x5: Mapping[str, list[int | None]] | Mapping[str, list[str | None]] = {"a": ["b"]}
reveal_type(x5)  # revealed: dict[str, list[str | None]]

def _(x6: list[dict[str, list[int] | int] | dict[str, list[int]]]):
    x6.append(reveal_type({"b": 1}))  # revealed: dict[str, list[int] | int]

type EitherList = list[int | str] | list[int | None]

x7: EitherList = [None, None]
reveal_type(x7)  # revealed: list[int | None]

x8: EitherList = ["1", "2", "3"]
reveal_type(x8)  # revealed: list[int | str]

type SelfOp[T] = Mapping[Literal["$eq", "$ne"], T]
type ListOp[T] = Mapping[Literal["$in", "$nin"], Sequence[T]]
type Ops[T] = SelfOp[T] | ListOp[T]
type NestedOp[T] = T | Ops[T]

x9: NestedOp[str] = {"$in": ["a", "b"]}
reveal_type(x9)  # revealed: dict[Literal["$in", "$nin"], list[str]]
```

### Binary operations

```py
def singleton[T](x: T) -> list[T]:
    return [x]

x1: list[list[int | str]] = [[1], [2]] * 3
reveal_type(x1)  # revealed: list[list[int | str]]

x2: list[list[int | str]] = 3 * ([[1]] + [[2]])
reveal_type(x2)  # revealed: list[list[int | str]]

x3: list[int | str] = 3 * ["x" for _ in range(3)]
reveal_type(x3)  # revealed: list[int | str]

x4: set[int | str] = {1, 2} | {3, 4}
reveal_type(x4)  # revealed: set[int | str]

x5: dict[int | str, int | str] = {1: 2} | {3: 4}
reveal_type(x5)  # revealed: dict[int | str, int | str]

# TODO: We currently eagerly pass type context to collection literals on either side of a binary
# operator. That makes the cases above work, but it is not generally sound.
class X:
    def __add__(self, _: list[int]) -> list[int | str]:
        return []

# error: [unsupported-operator] "Operator `+` is not supported between objects of type `X` and `list[int | str]`"
x6: list[int | str] = X() + [1]

# TODO: We do not yet propagate type context through the generic call.
# error: [invalid-assignment] "Object of type `list[int]` is not assignable to `list[int | str]`"
x7: list[int | str] = singleton(42) * 3
```

## Comprehensions

```py
x1: set[int | str] = {42 for _ in range(3)}
reveal_type(x1)  # revealed: set[int | str]

x2: dict[int | str, int | str] = {str(i): i for i in range(3)}
reveal_type(x2)  # revealed: dict[int | str, int | str]
```

## Tuple expressions

```py
from typing import Literal

def singleton[T](x: T) -> list[T]:
    return [x]

# Tuple elements are inferred individually, but type context can prevent e.g. `int` widening.
x1: tuple[list[Literal[1]]] = (singleton(1),)
reveal_type(x1)  # revealed: tuple[list[Literal[1]]]

x2: tuple[list[Literal[1]], ...] = (singleton(1),) * 3
reveal_type(x2)  # revealed: tuple[list[Literal[1]], ...]

x3: tuple[list[Literal[1]], ...] = 3 * ((singleton(1),) + (singleton(1),))
reveal_type(x3)  # revealed: tuple[list[Literal[1]], ...]
```

## Generator expressions

```py
from collections.abc import AsyncGenerator, AsyncIterable, Generator, Iterable

class TextContent: ...
class TagContent: ...

def expects_generator_content(content: Generator[list[TextContent | TagContent], None, None]) -> None: ...
def expects_iterable_content(content: Iterable[list[TextContent | TagContent]]) -> None: ...
def expects_optional_iterable_content(content: Iterable[list[TextContent | TagContent]] | None) -> None: ...
def generator_content() -> None:
    expects_generator_content([TextContent()] for _ in range(1))
    expects_iterable_content([TextContent()] for _ in range(1))
    expects_optional_iterable_content([TextContent()] for _ in range(1))
    expects_generator_content((reveal_type([TextContent()]) for _ in range(1)))  # revealed: list[TextContent | TagContent]

def expects_int_iterable_or_str_generator(content: Generator[list[str], int, None] | Iterable[list[int]]) -> None: ...
def generator_content_with_incompatible_generator_arm() -> None:
    expects_int_iterable_or_str_generator((reveal_type([]) for _ in range(1)))  # revealed: list[int]

def invalid_generator_content() -> None:
    expects_generator_content([object()] for _ in range(1))  # error: [invalid-argument-type]
    expects_optional_iterable_content([object()] for _ in range(1))  # error: [invalid-argument-type]

async def async_texts() -> AsyncGenerator[TextContent, None]:
    yield TextContent()

def expects_async_generator_content(content: AsyncGenerator[list[TextContent | TagContent], None]) -> None: ...
def expects_async_iterable_content(content: AsyncIterable[list[TextContent | TagContent]]) -> None: ...
async def async_generator_content() -> None:
    expects_async_generator_content([TextContent()] async for _ in async_texts())
    expects_async_iterable_content([TextContent()] async for _ in async_texts())

async def invalid_async_generator_content() -> None:
    expects_async_generator_content([object()] async for _ in async_texts())  # error: [invalid-argument-type]
```

## Generic call inference

The declared type of a generic call expression is used to infer a more assignable specialization for
the callable:

```py
from typing import Literal

def f[T](x: T) -> list[T]:
    return [x]

x1 = f("a")
reveal_type(x1)  # revealed: list[str]

x2: list[int | Literal["a"]] = f("a")
reveal_type(x2)  # revealed: list[int | Literal["a"]]

x3: list[int | str] = f("a")
reveal_type(x3)  # revealed: list[int | str]

x4: list[int | tuple[int, int]] = f((1, 2))
reveal_type(x4)  # revealed: list[int | tuple[int, int]]

x5: list[int] = f(True)
reveal_type(x5)  # revealed: list[int]

# error: [invalid-assignment] "Object of type `list[str]` is not assignable to `list[int]`"
x6: list[int] = f("a")

# error: [invalid-assignment] "Object of type `list[str]` is not assignable to `tuple[int]`"
x7: tuple[int] = f("a")

def f2[T: int](x: T) -> T:
    return x

x8: int = f2(True)
reveal_type(x8)  # revealed: Literal[True]

x9: int | str = f2(True)
reveal_type(x9)  # revealed: Literal[True]
```

```py
from typing import Callable, overload

def singleton[T](x: T) -> list[T]:
    return [x]

x10: list[int | str] | None = singleton(1)
reveal_type(x10)  # revealed: list[int | str]

def value_or_list[T](x: T, cond: bool) -> T | list[T]:
    return x if cond else [x]

x11: int | list[int] = value_or_list(1, True)
reveal_type(x11)  # revealed: int | list[int]

def returns_objects() -> list[object]:
    reveal_type(singleton(1))  # revealed: list[int]
    # `list[int]` and `list[object]` are incompatible, but the return type check passes because
    # this call is inferred using the annotated return type.
    return singleton(1)

def returns_optional_objects() -> list[object] | None:
    return singleton(1)

def deco[T](func: Callable[[], T]) -> Callable[[], T]:
    return func

def outer() -> Callable[[], list[object]]:
    @deco
    def inner() -> list[object]:
        return singleton(1)

    return inner

@overload
def overloaded(x: int) -> list[int]: ...
@overload
def overloaded(x: str) -> list[str]: ...
def overloaded(x: int | str) -> list[int] | list[str]:
    # `list[int] | list[str]` is disjoint from `list[int | str]`.
    if isinstance(x, int):
        return singleton(x)
    else:
        return singleton(x)

reveal_type(overloaded(1))  # revealed: list[int]
reveal_type(overloaded("a"))  # revealed: list[str]

async def async_return() -> list[int | str]:
    return singleton(1)

def forward[T](x: T, cond: bool) -> T | list[T]:
    return forwarded(x, cond)

def forwarded[T](x: T, cond: bool) -> T | list[T]:
    return x if cond else [x]
```

## Generic constructors

The same applies to constructors of generic classes:

```py
from typing import Any

class X[T]:
    def __init__(self, value: T):
        self.value = value

x1: X[int] = X(1)
reveal_type(x1)  # revealed: X[int]

x2: X[int | None] = X(1)
reveal_type(x2)  # revealed: X[int | None]

x3: X[int | None] | None = X(1)
reveal_type(x3)  # revealed: X[int | None]

def _[T](x1: X[T]):
    x2: X[T | int] = X(x1.value)
    reveal_type(x2)  # revealed: X[T@_ | int]

x4: X[Any] = X(1)
reveal_type(x4)  # revealed: X[Any]

def _(flag: bool):
    x5: X[int | None] = X(1) if flag else X(2)
    reveal_type(x5)  # revealed: X[int | None]
```

```py
from dataclasses import dataclass

@dataclass
class Y[T]:
    value: T

y1 = Y(value=1)
reveal_type(y1)  # revealed: Y[int]

y2: Y[Any] = Y(value=1)
reveal_type(y2)  # revealed: Y[Any]
```

```py
class Z[T]:
    value: T

    def __new__(cls, value: T):
        return super().__new__(cls)

z1 = Z(1)
reveal_type(z1)  # revealed: Z[int]

z2: Z[Any] = Z(1)
reveal_type(z2)  # revealed: Z[Any]
```

The return type should preserve the independent key and value types of a generic `dict` constructor:

```py
from collections.abc import Iterable, Mapping

def dict_with_numeric_promotion(
    keys: Iterable[float],
    values: Iterable[int],
) -> Mapping[float, int]:
    return dict(zip(keys, values))
```

```py
from collections.abc import Callable, Hashable
from typing import Any

# The `dict(...)` variant is not technically allowed by the typeshed overloads, which require
# string keys for keyword arguments. We special-case it to match the literal form.
x1: dict[Hashable, Callable[..., object]] = {"x": lambda: 1}
x2: dict[Hashable, Callable[..., object]] = dict(x=lambda: 1)
```

## Generic call argument inference

A function's arguments are also inferred using the type context:

```py
from typing import Callable, TypedDict

class TD(TypedDict):
    x: int

def first[T](x: list[T]) -> T:
    return x[0]

type ObjectCallback = Callable[[object], None]
type IntCallback = Callable[[int], None]

def make_callback[T](callback: Callable[[T], None]) -> Callable[[T], None]:
    return callback

def consume(value: int) -> None:
    pass

x1: TD = first([{"x": 0}, {"x": 1}])
reveal_type(x1)  # revealed: TD

x2: TD | None = first([{"x": 0}, {"x": 1}])
reveal_type(x2)  # revealed: TD

# error: [missing-typed-dict-key] "Missing required key 'x' in TypedDict `TD` constructor"
# error: [invalid-key] "Unknown key "y" for TypedDict `TD`"
# error: [invalid-assignment] "Object of type `TD | dict[str, int]` is not assignable to `TD`"
x3: TD = first([{"y": 0}, {"x": 1}])

# error: [missing-typed-dict-key] "Missing required key 'x' in TypedDict `TD` constructor"
# error: [invalid-key] "Unknown key "y" for TypedDict `TD`"
# error: [invalid-assignment] "Object of type `TD | None | dict[str, int]` is not assignable to `TD | None`"
x4: TD | None = first([{"y": 0}, {"x": 1}])

# `ObjectCallback` is redundant in this union, so expanding the aliases collapses the narrowing
# target to `IntCallback`.
x5: ObjectCallback | IntCallback = make_callback(lambda value: consume(value.bit_length()))
```

But not in a way that leads to assignability errors:

```py
from typing import Any, Sequence, TypedDict

class TD2(TypedDict):
    x: str

def _(dt: dict[str, Any], key: str):
    x1: TD = dt.get(key, {})
    reveal_type(x1)  # revealed: TD

    x2: TD = dt.get(key, {"x": 0})
    reveal_type(x2)  # revealed: TD

    x3: TD | None = dt.get(key, {})
    reveal_type(x3)  # revealed: TD | None

    x4: TD | None = dt.get(key, {"x": 0})
    reveal_type(x4)  # revealed: TD | None

    x5: TD2 = dt.get(key, {})
    reveal_type(x5)  # revealed: TD2

    x6: TD2 = dt.get(key, {"x": 0})
    reveal_type(x6)  # revealed: TD2

    x7: TD2 | None = dt.get(key, {})
    reveal_type(x7)  # revealed: TD2 | None

    x8: TD2 | None = dt.get(key, {"x": 0})
    reveal_type(x8)  # revealed: TD2 | None

def as_sequence[T](x: T, y: list[T], z: list[T]) -> Sequence[T]:
    return [x]

def _(x: int, z: list[int]):
    x1: Sequence[int] = as_sequence(x, [x], z)

    # TODO: A covariant type context should not cause us to unnecessarily widen call arguments.
    x2: Sequence[int | str] = as_sequence(x, [x], z)  # error: [invalid-argument-type]
```

Partially specialized type context is not ignored:

```py
from typing import TypeVar

U = TypeVar("U", default=Any)

class X: ...

def lst[T](x: T) -> list[T]:
    return [x]

def two_lists[T](x: list[T | int], y: list[T | str]) -> T:
    raise NotImplementedError

def two_lists_default(x: list[U | int], y: list[U | str]) -> U:
    raise NotImplementedError

def dct[K, V](k: K, v: V) -> dict[K, V]:
    return {k: v}

def two_dicts[T](x: dict[T | int, Any], y: dict[T | str, Any]) -> T:
    raise NotImplementedError

def two_dicts_default(x: dict[U | int, Any], y: dict[U | str, Any]) -> U:
    raise NotImplementedError

def _():
    # revealed: list[X | int]
    # revealed: list[X | str]
    x1 = two_lists(reveal_type(lst(X())), reveal_type(lst(X())))
    reveal_type(x1)  # revealed: X

    # revealed: list[X | int]
    # revealed: list[X | str]
    x2 = two_lists(reveal_type([X()]), reveal_type([X()]))
    reveal_type(x2)  # revealed: X

    # revealed: list[X | int]
    # revealed: list[X | str]
    x3 = two_lists_default(reveal_type(lst(X())), reveal_type(lst(X())))
    reveal_type(x3)  # revealed: X

    # revealed: list[X | int]
    # revealed: list[X | str]
    x4 = two_lists_default(reveal_type([X()]), reveal_type([X()]))
    reveal_type(x4)  # revealed: X

    # revealed: dict[X | int, Any]
    # revealed: dict[X | str, Any]
    x5 = two_dicts(reveal_type(dct(X(), X())), reveal_type(dct(X(), X())))
    reveal_type(x5)  # revealed: X

    # revealed: dict[X | int, Any]
    # revealed: dict[X | str, Any]
    x6 = two_dicts(reveal_type({X(): X()}), reveal_type({X(): X()}))
    reveal_type(x6)  # revealed: X

    # revealed: dict[X | int, Any]
    # revealed: dict[X | str, Any]
    x7 = two_dicts_default(reveal_type(dct(X(), X())), reveal_type(dct(X(), X())))
    reveal_type(x7)  # revealed: X

    # revealed: dict[X | int, Any]
    # revealed: dict[X | str, Any]
    x8 = two_dicts_default(reveal_type({X(): X()}), reveal_type({X(): X()}))
    reveal_type(x8)  # revealed: X
```

## Prefer the declared type of generic classes and callables

When inferring a generic call, we only use the declared type as type context if it is in
non-covariant position. The final annotated assignment binding still uses the declared type if the
inferred and declared types are mutually assignable:

```py
from typing import Any

class Bivariant[T]:
    pass

class Covariant[T]:
    def pop(self) -> T:
        raise NotImplementedError

class Contravariant[T]:
    def push(self, value: T) -> None:
        pass

class Invariant[T]:
    x: T

def bivariant[T](x: T) -> Bivariant[T]:
    return Bivariant()

def covariant[T](x: T) -> Covariant[T]:
    return Covariant()

def contravariant[T](x: T) -> Contravariant[T]:
    return Contravariant()

def invariant[T](x: T) -> Invariant[T]:
    return Invariant()

x1 = bivariant(1)
x2 = covariant(1)
x3 = contravariant(1)
x4 = invariant(1)

reveal_type(x1)  # revealed: Bivariant[Literal[1]]
reveal_type(x2)  # revealed: Covariant[Literal[1]]
reveal_type(x3)  # revealed: Contravariant[int]
reveal_type(x4)  # revealed: Invariant[int]

x5: Bivariant[int | None] = bivariant(1)
x6: Covariant[int | None] = covariant(1)
x7: Contravariant[int | None] = contravariant(1)
x8: Invariant[int | None] = invariant(1)

reveal_type(x5)  # revealed: Bivariant[int | None]
reveal_type(x6)  # revealed: Covariant[Literal[1]]
reveal_type(x7)  # revealed: Contravariant[int | None]
reveal_type(x8)  # revealed: Invariant[int | None]

x9: Bivariant[Any] = bivariant(1)
x10: Covariant[Any] = covariant(1)
x11: Contravariant[Any] = contravariant(1)
x12: Invariant[Any] = invariant(1)

reveal_type(x9)  # revealed: Bivariant[Any]
reveal_type(x10)  # revealed: Covariant[Any]
reveal_type(x11)  # revealed: Contravariant[Any]
reveal_type(x12)  # revealed: Invariant[Any]
```

This behavior also applies to invariant collection types:

```py
from typing import Any

def f[T](x: T) -> list[T]:
    return [x]

def f2[T](x: T) -> list[T] | None:
    return [x]

def f3[T](x: T) -> list[T] | dict[T, T]:
    return [x]

x1 = f(1)
reveal_type(x1)  # revealed: list[int]

x2: list[Any] = f(1)
reveal_type(x2)  # revealed: list[Any]

x3: list[Any] = [1]
reveal_type(x3)  # revealed: list[Any]

x4: list[Any] | None = f(1)
reveal_type(x4)  # revealed: list[Any]

x5: list[Any] | None = [1]
reveal_type(x5)  # revealed: list[Any]

x6: list[Any] | None = f2(1)
reveal_type(x6)  # revealed: list[Any] | None

x7: list[Any] | dict[Any, Any] = f3(1)
reveal_type(x7)  # revealed: list[Any] | dict[Any, Any]
```

As well as constructors of generic classes:

```py
class X[T]:
    def __init__(self: "X[None]"): ...
    def pop(self) -> T:
        raise NotImplementedError

x1: X[int | None] = X()
reveal_type(x1)  # revealed: X[None]
```

We also prefer the declared type of `Callable` parameters, which are in contravariant position:

```py
from typing import Callable

type AnyToBool = Callable[[Any], bool]

def wrap[**P, T](f: Callable[P, T]) -> Callable[P, T]:
    return f

def make_callable[T](x: T) -> Callable[[T], bool]:
    raise NotImplementedError

def maybe_make_callable[T](x: T) -> Callable[[T], bool] | None:
    raise NotImplementedError

x1: Callable[[Any], bool] = make_callable(0)
reveal_type(x1)  # revealed: (Any, /) -> bool

x2: AnyToBool = make_callable(0)
reveal_type(x2)  # revealed: (Any, /) -> bool

x3: Callable[[list[Any]], bool] = make_callable([0])
reveal_type(x3)  # revealed: (list[Any], /) -> bool

x4: Callable[[Any], bool] = wrap(make_callable(0))
reveal_type(x4)  # revealed: (Any, /) -> bool

x5: Callable[[Any], bool] | None = maybe_make_callable(0)
reveal_type(x5)  # revealed: ((Any, /) -> bool) | None
```

## Declared type preference sees through subtyping

Additionally, if the inferred type is a subtype of the declared type, we prefer declared type
assignments that are in non-covariant position. This behavior applies to collection literals:

```py
import builtins
from collections import defaultdict
from collections.abc import Mapping
from typing import Any, Callable, Iterable, Literal, MutableSequence, overload, Sequence

x1: Sequence[Any] = [1, 2, 3]
reveal_type(x1)  # revealed: list[int]

x2: MutableSequence[Any] = [1, 2, 3]
reveal_type(x2)  # revealed: list[Any]

x3: Iterable[Any] = [1, 2, 3]
reveal_type(x3)  # revealed: list[int]

x4: Iterable[Iterable[Any]] = [[1, 2, 3]]
reveal_type(x4)  # revealed: list[list[int]]

x5: list[Iterable[Any]] = [[1, 2, 3]]
reveal_type(x5)  # revealed: list[Iterable[Any]]

x6: Iterable[list[Any]] = [[1, 2, 3]]
reveal_type(x6)  # revealed: list[list[Any]]

x7: Sequence[Any] = [i for i in [1, 2, 3]]
reveal_type(x7)  # revealed: list[int]

x8: MutableSequence[Any] = [i for i in [1, 2, 3]]
reveal_type(x8)  # revealed: list[Any]

x9: Iterable[Any] = [i for i in [1, 2, 3]]
reveal_type(x9)  # revealed: list[int]

x10: Iterable[Iterable[Any]] = [[i] for i in [1, 2, 3]]
reveal_type(x10)  # revealed: list[list[int]]

x11: list[Iterable[Any]] = [[i] for i in [1, 2, 3]]
reveal_type(x11)  # revealed: list[Iterable[Any]]

x12: Iterable[list[Any]] = [[i] for i in [1, 2, 3]]
reveal_type(x12)  # revealed: list[list[Any]]
```

As well as generic calls, and constructors of generic classes:

```py
class X[T]:
    value: T

    def __init__(self, value: T): ...

class A[T](X[T]): ...

def a[T](value: T) -> A[T]:
    return A(value)

x13: A[object] = A(1)
reveal_type(x13)  # revealed: A[object]

x14: X[object] = A(1)
reveal_type(x14)  # revealed: A[object]

x15: X[object] | None = A(1)
reveal_type(x15)  # revealed: A[object]

x16: X[object] | None = a(1)
reveal_type(x16)  # revealed: A[object]

def f[T](x: T) -> list[list[T]]:
    return [[x]]

x17: Sequence[Sequence[Any]] = f(1)
reveal_type(x17)  # revealed: list[list[int]]

x18: Sequence[list[Any]] = f(1)
reveal_type(x18)  # revealed: list[list[Any]]

x19: dict[int, dict[str, int]] = defaultdict(dict)
reveal_type(x19)  # revealed: defaultdict[int, dict[str, int]]
```

Complex subtyping relationships are solved correctly:

```py
from typing import Hashable

def variadic(*args: Any, **kwargs: Any) -> Any: ...

x20: Mapping[Hashable, list[Callable[..., Any]]] = {"x": [variadic]}
reveal_type(x20)  # revealed: dict[Hashable, list[(...) -> Any]]

x21: Mapping[Hashable, list[Callable[..., Any]]] = dict(x=[variadic])
reveal_type(x21)  # revealed: dict[Hashable, list[(...) -> Any]]

x22: Mapping[str, Literal["+", "-"]] = {
    "plus": "+",
    "minus": "-",
}
reveal_type(x22)  # revealed: dict[str, Literal["+", "-"]]

class DataFrame: ...

type Aggregate = Callable[[DataFrame], object] | str
type AggregateSpec = Aggregate | list[Aggregate]

def mean(data: DataFrame) -> float:
    return 0.0

x23: Mapping[Hashable, AggregateSpec] = {"col1": ["sum", mean], "col2": mean}
```

## Implicit generic class specialization

Callable type context is also used to inform the implicit specialization of a generic class:

```py
import builtins
from collections import defaultdict
from collections.abc import Mapping
from typing import Any, Callable, overload

x1: Mapping[str, list[str]] = reveal_type(defaultdict(list))  # revealed: defaultdict[str, list[str]]
x1["key"].append(1)  # error: [invalid-argument-type]

x2: Callable[[], list[str]] = reveal_type(list)  # revealed: <class 'list[str]'>
reveal_type(x2())  # revealed: list[str]

x3: Callable[[], list[str]] | None = reveal_type(list)  # revealed: <class 'list[str]'>

x4: Callable[[], list[str]] = reveal_type(builtins.list)  # revealed: <class 'list[str]'>
reveal_type(x4())  # revealed: list[str]

type ListFactory = Callable[[], list[str]]

x5: ListFactory = reveal_type(list)  # revealed: <class 'list[str]'>
reveal_type(x5())  # revealed: list[str]

x6: Callable[..., Any] = reveal_type(list)  # revealed: <class 'list'>
x7: Callable[[Any], Any] = reveal_type(list)  # revealed: <class 'list'>

class Wrapped[T]:
    value: T

    def __new__(cls, value: T) -> "Wrapped[tuple[T]]":
        raise NotImplementedError

x8: Callable[[str], Wrapped[tuple[str]]] = reveal_type(Wrapped)  # revealed: <class 'Wrapped[str]'>
reveal_type(x8("x"))  # revealed: Wrapped[tuple[str]]

class M[T]:
    value: T

    def __new__[S](cls, value: S) -> "M[tuple[S]]":
        raise NotImplementedError

x9: Callable[[str], M[tuple[str]]] = reveal_type(M)  # revealed: <class 'M'>
reveal_type(x9("x"))  # revealed: M[tuple[str]]

class MultiPath[T]:
    value: T

    @overload
    def __init__(self, value: T) -> None: ...
    @overload
    def __init__(self, value: list[T]) -> None: ...
    def __init__(self, value: object) -> None: ...

# fmt: off
x10: Callable[[list[int]], MultiPath[int] | MultiPath[list[int]]] = reveal_type(MultiPath)  # revealed: <class 'MultiPath'>
# fmt: on
```

## Narrow union declared type for generic calls

When a generic call is checked against a union declared type, the union is narrowed to the first
compatible element:

```py
from typing import reveal_type, Any, Callable, TypedDict

def identity[T](x: T) -> T:
    return x

type Target = Any | list[str] | dict[str, str] | Callable[[str], None] | None

def _(narrow: dict[str, str], target: Target):
    target = identity(narrow)
    reveal_type(target)  # revealed: dict[str, str]

def _(narrow: list[str], target: Target):
    target = identity(narrow)
    reveal_type(target)  # revealed: list[str]

def _(narrow: Callable[[str], None], target: Target):
    target = identity(narrow)
    reveal_type(target)  # revealed: (str, /) -> None

def _(narrow: list[str] | dict[str, str], target: Target):
    target = identity(narrow)
    reveal_type(target)  # revealed: list[str] | dict[str, str]

class TD(TypedDict):
    x: int

type TargetWithTD = Any | list[TD] | dict[str, TD] | Callable[[TD], None] | None

def _(target: TargetWithTD):
    target = identity([{"x": 1}])
    reveal_type(target)  # revealed: list[TD]

def _(target: TargetWithTD):
    target = identity({"x": {"x": 1}})
    reveal_type(target)  # revealed: dict[str, TD]

def _(target: TargetWithTD):
    def make_callable[T](x: T) -> Callable[[T], None]:
        raise NotImplementedError

    target = identity(make_callable({"x": 1}))
    reveal_type(target)  # revealed: (TD, /) -> None
```

```py
def identity[T](x: T) -> T:
    return x

def lst[T](x: T) -> list[T]:
    return [x]

def _(i: int):
    x1: int | None = i
    x2: int | None = identity(i)
    x3: int | str | None = identity(i)
    reveal_type(x1)  # revealed: int
    reveal_type(x2)  # revealed: int
    reveal_type(x3)  # revealed: int

    x1: list[int | None] | None = [i]
    x2: list[int | None] | None = identity([i])
    x3: list[int | None] | int | None = identity([i])
    reveal_type(x1)  # revealed: list[int | None]
    reveal_type(x2)  # revealed: list[int | None]
    reveal_type(x3)  # revealed: list[int | None]

    x1: list[int | None] | None = [i]
    x2: list[int | None] | None = lst(i)
    x3: list[int | None] | int | None = lst(i)
    reveal_type(x1)  # revealed: list[int | None]
    reveal_type(x2)  # revealed: list[int | None]
    reveal_type(x3)  # revealed: list[int | None]

    x1: list | None = []  # error: [missing-type-argument]
    x2: list | None = identity([])  # error: [missing-type-argument]
    x3: list | int | None = identity([])  # error: [missing-type-argument]
    reveal_type(x1)  # revealed: list[Unknown]
    reveal_type(x2)  # revealed: list[Unknown]
    reveal_type(x3)  # revealed: list[Unknown]

def f[T](x: list[T]) -> T:
    return x[0]

def _(a: int, b: str, c: int | str):
    x1: int = f(lst(a))
    reveal_type(x1)  # revealed: int

    x2: int | str = f(lst(a))
    reveal_type(x2)  # revealed: int

    x3: int | None = f(lst(a))
    reveal_type(x3)  # revealed: int

    x4: str = f(lst(b))
    reveal_type(x4)  # revealed: str

    x5: int | str = f(lst(b))
    reveal_type(x5)  # revealed: str

    x6: str | int = f(lst(b))
    reveal_type(x6)  # revealed: str

    x7: str | None = f(lst(b))
    reveal_type(x7)  # revealed: str

    x8: int | str = f(lst(c))
    reveal_type(x8)  # revealed: int | str

    x9: int | str = f(lst(c))
    reveal_type(x9)  # revealed: int | str

    # TODO: Ideally this would reveal `int | str`. This is a known limitation of our
    # call inference solver, and would require an extra inference attempt without type
    # context, or with type context of subsets of the union, both of which are impractical
    # for performance reasons.
    x10: int | str | None = f(lst(c))
    reveal_type(x10)  # revealed: int | str | None
```

This applies to built-in collection constructors as well, mirroring the behavior of collection
literals:

```py
from typing import Mapping, Sequence

x1: list[int | str] | list[int | None] = list((1, 2, 3))
reveal_type(x1)  # revealed: list[int | str]

x2: Sequence[int | str] | Sequence[int | None] = list((1, 2, 3))
reveal_type(x2)  # revealed: list[int]

x3: list[int] | list[int | None] | list[str | None] = list(("1", "2"))
reveal_type(x3)  # revealed: list[str | None]

x4: dict[str, list[int | None]] | dict[str, list[str | None]] = dict([("a", ["b"])])
reveal_type(x4)  # revealed: dict[str, list[str | None]]

x5: Mapping[str, list[int | None]] | Mapping[str, list[str | None]] = dict([("a", ["b"])])
reveal_type(x5)  # revealed: dict[str, list[str | None]]

def _(x6: list[dict[str, list[int] | int] | dict[str, list[int]]]):
    x6.append(reveal_type(dict([("b", 1)])))  # revealed: dict[str, list[int] | int]

type EitherList = list[int | str] | list[int | None]

x7: EitherList = list((None, None))
reveal_type(x7)  # revealed: list[int | None]

x8: EitherList = list(("1", "2", "3"))
reveal_type(x8)  # revealed: list[int | str]
```

## Assignability diagnostics ignore declared type

The type displayed in an invalid assignment diagnostic should account for the type context, e.g., to
avoid literal promotion:

```py
from typing import Literal, TypedDict

def f[T](x: T) -> list[T]:
    return [x]

# error: [invalid-assignment] "Object of type `list[Literal["hello"] | int]` is not assignable to `list[Literal["hello"] | bool]`"
x1: list[Literal["hello"] | bool] = ["hello", 1]

class A(TypedDict):
    bar: int

# error: [invalid-assignment] "Object of type `list[A | int]` is not assignable to `list[A | bool]`"
x2: list[A | bool] = [{"bar": 1}, 1]
```

However, the declared type should be ignored if the specialization is not solvable:

```py
from typing import Any, Callable

def g[T](x: list[T]) -> T:
    return x[0]

def _(a: int | None):
    # error: [invalid-assignment] "Object of type `list[int | None]` is not assignable to `list[str]`"
    x1: list[str] = f(a)

    # error: [invalid-assignment] "Object of type `int | None` is not assignable to `str`"
    x2: str = g(f(a))

def make_callable[T](x: T) -> Callable[[T], bool]:
    raise NotImplementedError

def _(a: int | None):
    # error: [invalid-assignment] "Object of type `(int | None, /) -> bool` is not assignable to `(str, /) -> bool`"
    x1: Callable[[str], bool] = make_callable(a)
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

x1 = f1(reveal_type([1]), 1)  # revealed: list[int | None]
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

x4 = f3(reveal_type({"x": [1]}), "1")  # revealed: TD2
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
x8 = f6(reveal_type(list_or_set2(1, 1)))  # revealed: list[int | None] | set[int]
reveal_type(x8)  # revealed: Unknown

@overload
def f7(y: list[int | str]) -> list[int | str]: ...
@overload
def f7[T](y: list[T]) -> list[T]: ...
def f7(y: object) -> object:
    raise NotImplementedError

x9 = f7(reveal_type(["Sheet1"]))  # revealed: list[int | str]
reveal_type(x9)  # revealed: list[int | str]

def f8(xs: tuple[str, ...]) -> tuple[str, ...]:
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

## Collection literals in boolean and conditional expressions

When a boolean or conditional expression combines a fresh collection literal with another operand,
the other operand can provide type context for the literal:

```py
from collections.abc import Mapping
from dataclasses import dataclass
from typing import Literal, TypedDict, reveal_type

type Key = Literal["foo", "bar"]

class Payload(TypedDict):
    required: int

def from_or(values: list[str] | None) -> None:
    for value in reveal_type(values or []):  # revealed: list[str]
        reveal_type(value)  # revealed: str

def constructor_fallback(values: list[int] | None) -> None:
    reveal_type(values or list())  # revealed: (list[int] & ~AlwaysFalsy) | list[Unknown]

def from_and(values: list[str]) -> None:
    reveal_type(values and [])  # revealed: list[str]

def chained_or(first: list[int], second: list[str]) -> None:
    for value in first or second or []:
        reveal_type(value)  # revealed: int | str

def from_conditional(values: set[str], allowed: set[str] | None) -> None:
    filtered = reveal_type(
        sorted(value for value in values if value not in allowed)  # revealed: list[str]
        if allowed is not None
        else []
    )
    for value in filtered:
        reveal_type(value)  # revealed: str

def collection_literal_first(values: list[str], flag: bool) -> None:
    reveal_type([] if flag else values)  # revealed: list[str]

def non_empty_dict_fallback(values: dict[Key, int] | None) -> None:
    reveal_type(values or {"foo": 0})  # revealed: dict[Literal["foo", "bar"], int]

def non_empty_set_fallback(values: set[Key] | None) -> None:
    reveal_type(values or {"foo"})  # revealed: set[Literal["foo", "bar"]]

class TextContent: ...
class TagContent: ...

def expects_list_content(content: list[TextContent | TagContent]) -> None: ...
def optional_content(content: list[TextContent | TagContent] | None) -> None:
    expects_list_content(content or [TextContent()])

def invalid_fallback(content: list[TextContent | TagContent] | None) -> None:
    expects_list_content(content or [object()])  # error: [invalid-argument-type]

def preserve_generic[T](value: T) -> T:
    return value

def preserve_partially_specialized[T](value: list[T | int]) -> list[T | int]:
    return value

def generic_type_context(values: list[int | str] | None) -> None:
    reveal_type(preserve_generic(values or []))  # revealed: list[int | str]
    reveal_type(preserve_partially_specialized(values or []))  # revealed: list[Unknown | int]

def widened_non_empty_fallback(values: list[int] | None) -> None:
    result = values or ["x"]
    reveal_type(result)  # revealed: (list[int] & ~AlwaysFalsy) | list[int | str]

def incompatible_collection_kind(values: set[str] | None) -> None:
    reveal_type(values or [1])  # revealed: (set[str] & ~AlwaysFalsy) | list[int]

def typed_dict_peer_is_only_a_hint(value: Payload | None, flag: bool) -> None:
    value or {}
    {} if flag else value
    value or {"other": 1}

def stored_literal_is_not_fresh(values: dict[Key, int] | None) -> None:
    fallback = {"foo": 0}
    reveal_type(fallback)  # revealed: dict[str, int]
    result = values or fallback
    reveal_type(result)  # revealed: (dict[Key, int] & ~AlwaysFalsy) | dict[str, int]

@dataclass
class SortParams[F]:
    field: F
    direction: Literal["asc", "desc"] = "desc"

def build_sort_spec[T](
    sort_params: SortParams[T] | None,
) -> dict[T, Literal[1, -1]] | None:
    if not sort_params:
        return None
    return {sort_params.field: 1}

type Path = Literal["name", "age", "created"]

def use_sort(value: Mapping[Path, Literal[1, -1]]) -> None: ...

params: SortParams[Path] | None = None
sort = build_sort_spec(params) or {"name": -1}
use_sort(sort)
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

# The same return-context propagation works for generic calls whose context solves a ParamSpec.
def id_callable[**P, R](x: Callable[P, R]) -> Callable[P, R]:
    return x

f5_paramspec: Callable[[int], int] = id_callable(lambda x: reveal_type(x))  # revealed: int
reveal_type(f5_paramspec)  # revealed: (x: int) -> int

# TODO: This should not error once we support `Unpack`.
# error: [invalid-assignment]
f6: Callable[[*tuple[int, ...]], None] = lambda x, y, z: None
reveal_type(f6)  # revealed: (*tuple[int, ...]) -> None

f7: Callable[[int, str], None] = lambda *args: None
reveal_type(f7)  # revealed: (*args) -> None

# N.B. `Callable` annotations only support positional parameters.
# error: [invalid-assignment]
f8: Callable[[int], None] = lambda *, x=1: None
reveal_type(f8)  # revealed: (int, /) -> None

# `Callable` annotations only describe positional parameters, so the keyword-only `x` is not
# compatible with the positional suffix in the annotation.
# error: [invalid-assignment]
f9: Callable[[*tuple[int, ...], int], None] = lambda *args, x=1: None
reveal_type(f9)  # revealed: (*tuple[int, ...], int) -> None

f10: Callable[[str, int, str], tuple[str, int, str]] = lambda x, y, z: reveal_type((x, y, z))  # revealed: tuple[str, int, str]
reveal_type(f10)  # revealed: (x: str, y: int, z: str) -> tuple[str, int, str]

# TODO: This should reveal `tuple[int, ...]` once we support `Unpack`.
f11: Callable[[*tuple[int, ...]], tuple[int, ...]] = lambda *args: reveal_type(args)  # revealed: tuple[Unknown, ...]
reveal_type(f11)  # revealed: (*args) -> tuple[Unknown, ...]

def _(x: list[int]):
    f12 = list(map(lambda y: reveal_type(y) + 1, x))  # revealed: int
    reveal_type(f12)  # revealed: list[int]

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

## Unified call inference

Generic call arguments are inferred under fixpoint iteration, allowing constraints from call
arguments to contribute type context to sibling arguments within a given generic call, until
convergence.

```py
from typing import Any, Callable, Literal, Sequence, TypedDict, TypeVar, overload

def combine[T](x: T, y: list[T], z: list[T]) -> T:
    return x

def combine_reversed[T](x: T, z: list[T], y: list[T]) -> T:
    return x

def _(x: int, y: int | str, z: int | str | None):
    x1: int | str | None = combine(y, [x], [z])
    reveal_type(x1)  # revealed: int | str | None

    x2 = combine(y, [x], [z])
    reveal_type(x2)  # revealed: int | str | None

    x3 = combine_reversed(y, [z], [x])
    reveal_type(x3)  # revealed: int | str | None

def collection_pair[T](pair: tuple[T, list[T]]) -> T:
    return pair[0]

x = collection_pair((1, [True]))
reveal_type(x)  # revealed: int

def callable_pair[T](pair: tuple[Callable[[T], int], list[T]]) -> None:
    function, values = pair
    function(values[0])

callable_pair((lambda value: reveal_type(value) + 1, [1]))  # revealed: int

def nested_pair[T](pair: tuple[T, list[T]]) -> T:
    return pair[0]

x = nested_pair(("value", [None]))
reveal_type(x)  # revealed: str | None
```

```py
class A(TypedDict):
    a: int
    b: int

def pair_with_list[T](x: T, y: list[T]) -> T:
    return x

def pair_with_sequence[T](x: T, y: Sequence[T]) -> T:
    return x

def list_pair[T](x: list[T], y: list[T]) -> T:
    return x[0]

def pair[T](x: T, y: T) -> T:
    return x

def _(a: A, b: list[A]):
    x1: A = pair_with_list(a, [{"a": 1, "b": 2}])
    reveal_type(x1)  # revealed: A

    # TODO: This should solve to `A`.
    x2 = pair_with_list(a, [{"a": 1, "b": 2}])
    reveal_type(x2)  # revealed: A | dict[str, int]

    x3 = pair_with_sequence(a, [{"a": 1, "b": 2}])
    reveal_type(x3)  # revealed: A

    # TODO: This should solve to `A`.
    x4 = list_pair(b, [{"a": 1, "b": 2}])  # error: [invalid-argument-type]
    reveal_type(x4)  # revealed: A | dict[str, int]

    x5 = pair({"a": 1, "b": 2}, a)
    reveal_type(x5)  # revealed: A

    x6 = pair(a, {"a": 1, "b": 2})
    reveal_type(x6)  # revealed: A
```

```py
from typing import TypedDict, reveal_type

class TD(TypedDict):
    x: int

def f[T](x: T, y: T) -> T:
    return x

def _(td: TD):
    # revealed: TD
    x = reveal_type(f(td, reveal_type({"x": 1})))  # revealed: TD

    # TODO: Generic call narrowing on `reveal_type` happens to choose
    # the `dict` constraint here instead of `TD`, failing to narrow the
    # dictionary literal.
    x = f(td, reveal_type({"x": 1}))  # revealed: dict[str, int]
    reveal_type(x)  # revealed: TD | dict[str, int]
```

```py
class ActiveInitializer[T]:
    def __new__(cls, *args: object) -> "ActiveInitializer[T]":
        return super().__new__(cls)

    def __init__(self, value: T, values: list[T]) -> None:
        pass

x = ActiveInitializer(1, [True])
reveal_type(x)  # revealed: ActiveInitializer[int]

class InactiveInitializer:
    def __new__[T](cls, value: T, values: list[T]) -> T:
        return value

    def __init__(self) -> None:
        pass

x = InactiveInitializer(1, [True])
reveal_type(x)  # revealed: int
```

```py
def consume_and_produce[T, R](
    consumer: Callable[[T], R],
    producer: Callable[[], T],
    value: T,
) -> T:
    produced = producer()
    consumer(produced)
    consumer(value)
    return produced

x = consume_and_produce(
    lambda x: reveal_type(x),  # revealed: str | int
    lambda: "s",
    1,
)

reveal_type(x)  # revealed: Literal["s", 1]
```

```py
def nested_callable[T](
    value: T,
    callbacks: Sequence[Callable[[Callable[[T], None]], None]],
) -> None:
    pass

nested_callable(
    1,
    [lambda callable: print(reveal_type(callable))],  # revealed: (int, /) -> None
)
```

```py
class Base: ...
class Dog(Base): ...
class Cat(Base): ...

BaseType = TypeVar("BaseType", bound=Base)

def register_handlers(handlers: dict[str, type[BaseType]]) -> None: ...

register_handlers({"dog": Dog, "cat": Cat})

class X: ...

def accept_classes[T: X](classes: list[type[T]]) -> None: ...

accept_classes([X])
```

```py
FloatDtype = type[float] | Literal["float"]

@overload
def overloaded_call(data: Sequence[str], dtype: object) -> str: ...
@overload
def overloaded_call(data: list[Any], dtype: FloatDtype) -> float: ...
@overload
def overloaded_call[T](data: Sequence[T], dtype: Literal["generic"]) -> T: ...
def overloaded_call(data: object, dtype: object) -> object:
    return data

def _(dtype: FloatDtype):
    x = overloaded_call([1.0], dtype)
    reveal_type(x)  # revealed: int | float
```

```py
from typing import Protocol, runtime_checkable

@runtime_checkable
class TakesInt(Protocol):
    def __call__(
        self,
        tag: Literal["int"],
        callback: Callable[[int], int],
    ) -> int: ...

@runtime_checkable
class TakesStr(Protocol):
    def __call__(
        self,
        tag: Literal["str"],
        callback: Callable[[str], str],
    ) -> str: ...

def _(callback: TakesInt) -> None:
    if isinstance(callback, TakesStr):
        reveal_type(callback)  # revealed: TakesInt & TakesStr

        # TODO: Perform fixpoint iteration when evaluating callable intersections.
        x1 = callback("int", lambda value: reveal_type(value) + 1)  # revealed: Unknown
        reveal_type(x1)  # revealed: int

        # TODO: Perform fixpoint iteration when evaluating callable intersections.
        x2 = callback("str", lambda value: reveal_type(value) + "!")  # revealed: Unknown
        reveal_type(x2)  # revealed: str
```

Note that long chains of callables with constraint dependencies in reverse source-order may require
multiple fixpoint iterations.

```py
from typing import Callable

def chain[A, B, C, D](
    first: Callable[[C], D],
    second: Callable[[B], C],
    third: Callable[[A], B],
    source: list[A],
) -> D:
    return first(second(third(source[0])))

x = chain(
    lambda c: c + 1,
    lambda b: b + 1,
    lambda a: a + 1,
    [1, 2, 3],
)
reveal_type(x)  # revealed: int
```

The upper bound on iterations is calculated based on the number of independent occurences of
inferable type variables, not the number of arguments.

```py
from typing import Callable

def list_to_callable[T](values: list[T]) -> Callable[[T], T]:
    raise NotImplementedError

def propagate[A, B, C, D](
    first: Callable[[C], D],
    second: Callable[[B], C],
    third: Callable[[A], B],
    source: list[A],
) -> D:
    return first(second(third(source[0])))

def propagate_tuple[A, B, C, D](
    arguments: tuple[
        Callable[[C], D],
        Callable[[B], C],
        Callable[[A], B],
        list[A],
    ],
) -> D:
    raise NotImplementedError

def _(seed: int):
    x = propagate(
        list_to_callable([]),
        list_to_callable([]),
        list_to_callable([]),
        [seed],
    )
    reveal_type(x)  # revealed: int

    x = propagate_tuple((
        list_to_callable([]),
        list_to_callable([]),
        list_to_callable([]),
        [seed],
    ))
    reveal_type(x)  # revealed: int
```

Only diagnostics from the final round of iteration are preserved:

```py
def diagnostic_pair[T](value: T, values: list[T]) -> T:
    return value

# error: [unresolved-reference]
diagnostic_pair(missing_name, [1])
diagnostic_pair(suppressed_missing, [1])  # ty: ignore[unresolved-reference]

def non_generic(value: int) -> int:
    return value

# error: [unresolved-reference]
diagnostic_pair(non_generic(missing_argument), [1])
diagnostic_pair(non_generic(suppressed_argument), [1])  # ty: ignore[unresolved-reference]
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

## Container inference

Empty, unannotated container literals are inferred based on future uses that extend throughout the
entire scope:

```py
x1 = []
x1.append(1)
x1.append("2")
reveal_type(x1)  # revealed: list[int | str]
```

```py
x1_sorted = []
x1_sorted.append("x")
x1_sorted.sort()
reveal_type(x1_sorted)  # revealed: list[str]
```

Bare empty `list()`, `set()`, and `dict()` calls also participate in full-scope inference. Calls
through aliases and shadowed names are deliberately not refined:

```py
list_result = list()
list_result.append(1)
list_result.append("2")
reveal_type(list_result)  # revealed: list[int | str]

set_result = set()
set_result.add(1)
set_result.add("2")
reveal_type(set_result)  # revealed: set[int | str]

dict_result = dict()
dict_result["a"] = 1
dict_result["b"] = "2"
reveal_type(dict_result)  # revealed: dict[str, int | str]

def make_list() -> list[str]:
    result = list()
    result.append(1)
    reveal_type(result)  # revealed: list[int | str]
    return result  # error: [invalid-return-type]

def make_set() -> set[str]:
    result = set()
    result.add(1)
    reveal_type(result)  # revealed: set[int | str]
    return result  # error: [invalid-return-type]

def make_dict() -> dict[str, str]:
    result = dict()
    result["x"] = 1
    reveal_type(result)  # revealed: dict[str, int | str]
    return result  # error: [invalid-return-type]

set_alias = set
aliased_result = set_alias()
aliased_result.add(1)
reveal_type(aliased_result)  # revealed: set[Unknown]

from typing import Never

class Result:
    def abort(self) -> Never:
        raise RuntimeError

def shadowed_constructor() -> int:
    set = Result
    result = set()
    reveal_type(result)  # revealed: Result
    result.abort()
    return "unreachable"
```

```py
class X:
    def __init__(self):
        self.x = []
        self.x.append(1)
        self.x.append("2")
        reveal_type(self.x)  # revealed: list[int | str]

reveal_type(X().x)  # revealed: list[int | str]
```

```py
def _(flag: bool):
    if flag:
        x2 = []
        x2.append(1)
        reveal_type(x2)  # revealed: list[int]
    else:
        x2 = []
        x2.append("2")
        reveal_type(x2)  # revealed: list[str]
```

```py
def takes_list_int(x: list[int]): ...

x3 = []
takes_list_int(x3)
# TODO: This should reveal `list[int]`, but we do not currently record
# argument constraints for arbitrary function calls.
reveal_type(x3)  # revealed: list[Unknown]
```

```py
def append[T](x: list[T], y: T):
    x.append(y)

x4 = []
append(x4, 1)
append(x4, "2")
# TODO: This should reveal `list[int | str]`, but we do not currently record
# argument constraints for arbitrary function calls.
reveal_type(x4)  # revealed: list[Unknown]
```

```py
x5 = []
_: list[int] = reveal_type(x5)  # revealed: list[int]
```

```py
def _() -> list[int | None]:
    x6 = []
    return reveal_type(x6)  # revealed: list[int | None]

def _() -> int:
    invalid_x6 = []
    return invalid_x6  # error: [invalid-return-type]
```

```py
x7 = []
x7[:] = [1, "2", 3.0]
reveal_type(x7)  # revealed: list[int | str | float]
```

```py
from typing import Literal

x8 = []
one: Literal[1] = 1
x8.append(one)
reveal_type(x8)  # revealed: list[Literal[1]]
```

```py
x9 = []
x10 = []
x9.append(1)
x9.append("2")
x10.append(3)

reveal_type(x9)  # revealed: list[int | str]
reveal_type(x10)  # revealed: list[int]
```

```py
x11 = []
x12 = []
x11.append(1)
x12.append(x11)

reveal_type(x11)  # revealed: list[int]
reveal_type(x12)  # revealed: list[list[int]]
```

```py
x13 = []
x13.append(x13)
reveal_type(x13)  # revealed: list[Divergent]
```

```py
x14 = []
x15 = []

x14.append(x15)
x15.append(x14)

reveal_type(x14)  # revealed: list[Divergent]
reveal_type(x15)  # revealed: list[Divergent]
```

Collection-use constraints must converge when multiple collection literals are used in a container
literal. This is a regression test for <https://github.com/astral-sh/ty/issues/3778>:

```py
from typing import Any

def run(cond: bool, d: dict[Any, Any]) -> list[Any]:
    a = {}
    b = {}
    if cond:
        b = d
    return [a.get("x", 0), b.get("x", 0)]

def assigned(cond: bool, d: dict[Any, Any]) -> list[Any]:
    a = {}
    b = {}
    if cond:
        b = d
    result: list[Any] = [a.get("x", 0), b.get("x", 0)]
    return result
```

```py
def _(i):
    x16 = []
    x16.append(x16)
    reveal_type(x16)  # revealed: list[Divergent]
```

```py
x17 = {}
x17.update(a=1)
reveal_type(x17)  # revealed: dict[str, int]
```

```py
x18 = {}
x18.update({"a": 1})
reveal_type(x18)  # revealed: dict[str, int]
```

```py
x19 = {}
x19["a"] = 1
x19["b"] = "2"
reveal_type(x19)  # revealed: dict[str, int | str]
```

```py
x20 = {}
x20["a"] = len(x20)
x20.setdefault("b", str(len(x20)))
reveal_type(x20)  # revealed: dict[str, int | str]
```

```py
x21 = []
_: list[int] = x21  # error: [invalid-assignment]

# TODO: We should error on this `append` instead of the assignment and not union
# later constraints after the element type has been fully constrained above, to
# avoid confusing error messages where the type of the collection may be unexpectedly
# influenced by uses later in the scope.
x21.append("a")

# TODO: This would then reveal `list[int]`.
reveal_type(x21)  # revealed: list[int | str]
```

```py
def _(flag: bool):
    if flag:
        x22 = []
    else:
        x22 = []

    x22.append(1)

    # TODO: This should reveal `list[int]`.
    reveal_type(x22)  # revealed: list[Unknown]
```

```py
x23 = [None, None, None]
x23[0] = 1
x23[1] = "2"
x23[2] = 3.0
reveal_type(x23)  # revealed: list[int | str | float | None]
```

```py
x24 = {"a": 1}
x24[1] = "b"
reveal_type(x24)  # revealed: dict[int | str, str | int]
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
