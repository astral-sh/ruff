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
from typing import Any, AsyncGenerator, AsyncIterable, Generator, Iterable, Literal

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

def expects_list_content(content: list[TextContent | TagContent]) -> None: ...
def optional_content(content: list[TextContent | TagContent] | None) -> None:
    expects_list_content(content or [TextContent()])

def invalid_fallback(content: list[TextContent | TagContent] | None) -> None:
    expects_list_content(content or [object()])  # error: [invalid-argument-type]

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

def f[T](x: T, cond: bool) -> T | list[T]:
    return x if cond else [x]

l5: int | list[int] = f(1, True)

x: list[int] = [1, 2, *(3, 4, 5)]
reveal_type(x)  # revealed: list[int]

x: list[list[int]] = [[1], [2], *([3], [4])]
reveal_type(x)  # revealed: list[list[int]]

type IntDict = dict[str, int]

unique: set[int] = {1, 2, 3}
reveal_type(unique)  # revealed: set[int]

mapping: dict[str, int] = {"a": 1, **{"b": 2}}
reveal_type(mapping)  # revealed: dict[str, int]

def dynamic_mapping() -> Any: ...

dynamic_unpack: dict[str, int] = reveal_type({**dynamic_mapping()})  # revealed: dict[str | Any, int | Any]

alias_mapping: IntDict = {"a": 1}
reveal_type(alias_mapping)  # revealed: dict[str, int]

optional_mapping: dict[str, int] | None = {"a": 1}
reveal_type(optional_mapping)  # revealed: dict[str, int]

either: list[int] | list[str] = [1]
reveal_type(either)  # revealed: list[int]

# A protocol context is not an exact nominal collection context and must use the general path.
iterable: Iterable[int] = [1]
reveal_type(iterable)  # revealed: list[int]

bad_list: list[str] = [1]  # error: [invalid-assignment]
bad_dict: dict[str, int] = {"a": "bad"}  # error: [invalid-assignment]

bad_nested_list: list[list[list[str]]] = [[[1]]]  # error: [invalid-assignment]

# error: [invalid-argument-type] "Argument expression after ** must be a mapping type"
bad_unpack: dict[str, int] = {**42}

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
from typing import Any, Callable, Hashable, Iterable, Mapping, TypedDict
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

def takes_dict(value: dict[str, object]) -> None: ...
def takes_kwargs(**kwargs: object) -> None: ...
def _(data: TD):
    reveal_type(dict(data))  # revealed: dict[str, object]
    takes_dict(dict(data))
    takes_kwargs(**dict(data))

# Note: the second variant (`d5_dict`) is not technically allowed by the `dict.__init__` overloads
# in typeshed, which require the key type to be `str` when using keyword arguments. However, we
# special-case this pattern to match the behavior of `d5_literal`.
d5_literal: dict[Hashable, Callable[..., object]] = {"x": lambda: 1}
d5_dict: dict[Hashable, Callable[..., object]] = dict(x=lambda: 1)

d6_dict: TD = {"x": 1} | {"x": 2}

type IntFloatDict = dict[int, float]
type TypedDictOrDictAlias = TD | IntFloatDict
type TypedDictOrMapping = TD | Mapping[int, float]

# The `dict[int, float]` fallback should still win when it is wrapped in an alias.
d7_alias_fallback: TypedDictOrDictAlias = {1: 5.2}
d8_mapping_fallback: TypedDictOrMapping = {1: 5.2}

# A `Mapping` fallback should only suppress `TypedDict` diagnostics when it accepts the literal.
# error: [missing-typed-dict-key]
# error: [invalid-key]
d9_invalid_mapping_key: TypedDictOrMapping = {"y": 5.2}
# error: [missing-typed-dict-key]
# error: [invalid-key]
d10_invalid_mapping_value: TypedDictOrMapping = {1: "bad"}

def takes_td_or_iterable(value: TD | Iterable[int]) -> None:
    pass

takes_td_or_iterable({42: 42})

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

The return type context should preserve the independent key and value types of a generic `dict`
constructor:

```py
from collections.abc import Iterable, Mapping

def dict_with_numeric_promotion(
    keys: Iterable[float],
    values: Iterable[int],
) -> Mapping[float, int]:
    return dict(zip(keys, values))
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

## Peer type context for collection literals

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

def _(x: list[int]):
    mapped = map(lambda y: reveal_type(y) + 1, x)  # revealed: int
    reveal_type(mapped)  # revealed: map[int]
    f12 = list(mapped)
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

## Generic call fixpoint inference

Generic call arguments are inferred to a fixed point. Constraints from one argument only affect the
type context of other arguments on a later iteration, making inference independent of argument
order.

```py
from typing import Any, Callable, Literal, Sequence, TypedDict, TypeVar, overload

def lst[T](x: T) -> list[T]:
    return [x]

def combine[T](x: T, y: list[T], z: list[T]) -> T:
    return x

def combine_reversed[T](x: T, z: list[T], y: list[T]) -> T:
    return x

def _(x: int, y: int | str, z: int | str | None):
    annotated: int | str | None = combine(y, lst(x), lst(z))
    reveal_type(annotated)  # revealed: int | str | None

    inferred = combine(y, lst(x), lst(z))
    reveal_type(inferred)  # revealed: int | str | None

    reversed = combine_reversed(y, lst(z), lst(x))
    reveal_type(reversed)  # revealed: int | str | None

class A(TypedDict):
    a: int
    b: int

def pair[T](x: T, y: list[T]) -> T:
    return x

def sequence_pair[T](x: T, y: Sequence[T]) -> T:
    return x

def bare_pair[T](x: T, y: T) -> T:
    return x

def _(a: A):
    annotated: A = pair(a, lst({"a": 1, "b": 2}))
    reveal_type(annotated)  # revealed: A
    pair(a, lst({"a": 1, "b": 2}))

    # A covariant context still needs to be re-applied after `T` is specialized. Otherwise, the
    # dictionary remains `dict[str, int]` instead of being inferred as `A`.
    covariant = sequence_pair(a, lst({"a": 1, "b": 2}))
    reveal_type(covariant)  # revealed: A

    # A bare `T` context must also be re-applied. After the other argument specializes `T` to
    # `A`, the dictionary literal can be inferred as that TypedDict in either source order.
    bare_first = bare_pair({"a": 1, "b": 2}, a)
    reveal_type(bare_first)  # revealed: A
    bare_second = bare_pair(a, {"a": 1, "b": 2})
    reveal_type(bare_second)  # revealed: A

# A downstream generic `__init__` participates in the same fixpoint as the upstream `__new__`.
# Its checked specialization must be retained so that it contributes to the constructed class's
# inferred specialization.
class GenericInitializer[T]:
    def __new__(cls, *args: object) -> "GenericInitializer[T]":
        return super().__new__(cls)

    def __init__(self, value: T, values: list[T]) -> None:
        pass

initialized = GenericInitializer(1, [True])
reveal_type(initialized)  # revealed: GenericInitializer[int]

# An inactive downstream must also remain structurally present during speculative rounds because
# the fixed argument-inference candidate table still contains it. It is pruned only after the
# fixpoint has been committed.
class InactiveInitializer:
    def __new__[T](cls, value: T, values: list[T]) -> T:
        return value

    def __init__(self) -> None:
        pass

inactive = InactiveInitializer(1, [True])
reveal_type(inactive)  # revealed: int

# Covariance only guarantees assignability; it does not mean that contextual inference is stable.
# Both callable parameter positions reverse variance, so `T` occurs covariantly in `callbacks`.
def nested_callback_context[T](
    value: T,
    callbacks: Sequence[Callable[[Callable[[T], None]], None]],
) -> None:
    pass

nested_callback_context(
    1,
    [lambda callback: print(reveal_type(callback))],  # revealed: (int, /) -> None
)

# Regression test for https://github.com/astral-sh/ty/issues/3469. `type[T]` must count as a
# generic argument context so that class literals are re-inferred after `T` is specialized.
class Base: ...
class Dog(Base): ...
class Cat(Base): ...

BaseType = TypeVar("BaseType", bound=Base)

def register_handlers(handlers: dict[str, type[BaseType]]) -> None: ...

register_handlers({"dog": Dog, "cat": Cat})

class X: ...

def accept_classes[T: X](classes: list[type[T]]) -> None: ...

accept_classes([X])

# A single call-site argument can contain multiple inference sites that depend on the same
# specialization. The first tuple element establishes `T = int`; the second needs the specialized
# context to infer the invariant list as `list[int]` rather than `list[bool]`.
def collection_pair[T](pair: tuple[T, list[T]]) -> T:
    return pair[0]

collection_result = collection_pair((1, [True]))
reveal_type(collection_result)  # revealed: int

# The list inside the single tuple argument establishes the context for the lambda parameter on the
# next round.
def callable_pair[T](pair: tuple[Callable[[T], int], list[T]]) -> None:
    function, values = pair
    function(values[0])

callable_pair((lambda value: reveal_type(value) + 1, [1]))  # revealed: int

# The specialization inferred from both tuple elements is propagated into the nested generic call
# on the next round, widening its invariant return type.
def nested_pair[T](pair: tuple[T, list[T]]) -> T:
    return pair[0]

nested_result = nested_pair(("value", lst(None)))
reveal_type(nested_result)  # revealed: str | None

# Diagnostics emitted while inferring a context-independent argument are replayed into the final
# fixpoint round exactly once. Suppressed diagnostics also replay their used-suppression state.
def diagnostic_pair[T](value: T, values: list[T]) -> T:
    return value

# error: [unresolved-reference]
diagnostic_pair(missing_name, [1])
diagnostic_pair(suppressed_missing, [1])  # ty: ignore[unresolved-reference]

# Ordinary string literals and non-generic calls are independent of the changing generic context.
string_result = diagnostic_pair("one", ["two"])
reveal_type(string_result)  # revealed: str

def non_generic(value: int) -> int:
    return value

# Diagnostics from an independently cached non-generic call are replayed exactly once.
# error: [unresolved-reference]
diagnostic_pair(non_generic(missing_argument), [1])
diagnostic_pair(non_generic(suppressed_argument), [1])  # ty: ignore[unresolved-reference]

# Every fixpoint round must infer against the complete set of overloads that matched before type
# inference. Narrowing that set between rounds can leave no entry for an earlier overload's
# declared type, causing its check to fall back to an unrelated contextual inference.
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
    result = overloaded_call([1.0], dtype)
    reveal_type(result)  # revealed: int | float
```

Long reverse dependency chains can require more than two speculative iterations:

```py
from typing import Callable

def chain[A, B, C, D](
    first: Callable[[C], D],
    second: Callable[[B], C],
    third: Callable[[A], B],
    source: list[A],
) -> D:
    return first(second(third(source[0])))

result = chain(
    lambda c: c + 1,
    lambda b: b + 1,
    lambda a: a + 1,
    [1, 2, 3],
)
reveal_type(result)  # revealed: int
```

Nested generic calls can also require more than two speculative iterations, without involving lambda
inference:

```py
from typing import Callable

def contextual_identity[T](values: list[T]) -> Callable[[T], T]:
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
    propagated = propagate(
        contextual_identity([]),
        contextual_identity([]),
        contextual_identity([]),
        [seed],
    )
    reveal_type(propagated)  # revealed: int

    tuple_propagated = propagate_tuple((
        contextual_identity([]),
        contextual_identity([]),
        contextual_identity([]),
        [seed],
    ))
    reveal_type(tuple_propagated)  # revealed: int
```

Deferred callable constraints can widen a specialization inferred from another argument:

```py
from typing import Callable

def choose[T](producer: Callable[[], T], value: T) -> T:
    return producer()

reveal_type(choose(lambda: "s", 1))  # revealed: Literal["s", 1]

def consume_and_produce[T](
    consumer: Callable[[T], None],
    producer: Callable[[], T],
    value: T,
) -> T:
    produced = producer()
    consumer(produced)
    consumer(value)
    return produced

reveal_type(consume_and_produce(lambda x: None, lambda: "s", 1))  # revealed: Literal["s", 1]

consume_and_produce(
    lambda x: None if x.bit_length() else None,  # error: [unresolved-attribute]
    lambda: "s",
    1,
)
```

Arguments with concrete contexts remain cacheable while generic arguments iterate:

```py
from typing import Callable

def mixed[A, B](
    prefix: str,
    transform: Callable[[A], B],
    source: list[A],
    one: int,
    two: int,
    three: int,
) -> B:
    return transform(source[0])

mixed_result = mixed(
    "prefix",
    lambda value: reveal_type(value) + 1,  # revealed: int
    [1, 2, 3],
    1,
    2,
    3,
)
reveal_type(mixed_result)  # revealed: int
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
through aliases and shadowed names are deliberately not refined.

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
literal. This is a regression test for <https://github.com/astral-sh/ty/issues/3778>.

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
