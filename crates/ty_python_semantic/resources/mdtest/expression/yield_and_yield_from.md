# `yield` and `yield from`

## Basic `yield` and `yield from`

The type of a `yield` expression is the "send" type of the generator function. The type of a
`yield from` expression is the return type of the inner generator:

```py
from typing import Generator

def inner_generator() -> Generator[int, bytes, str]:
    yield 1
    yield 2
    x = yield 3

    reveal_type(x)  # revealed: bytes

    return "done"

def outer_generator():
    result = yield from inner_generator()
    reveal_type(result)  # revealed: str
```

## `yield from` with a custom iterable

`yield from` can also be used with custom iterable types. In that case, the type of the `yield from`
expression cannot be determined

```py
from typing import Generator, TypeVar, Generic

T = TypeVar("T")

class OnceIterator(Generic[T]):
    def __init__(self, value: T):
        self.value = value
        self.returned = False

    def __next__(self) -> T:
        if self.returned:
            raise StopIteration(42)

        self.returned = True
        return self.value

class Once(Generic[T]):
    def __init__(self, value: T):
        self.value = value

    def __iter__(self) -> OnceIterator[T]:
        return OnceIterator(self.value)

for x in Once("a"):
    reveal_type(x)  # revealed: str

def generator() -> Generator[str]:
    result = yield from Once("a")

    # At runtime, the value of `result` will be the `.value` attribute of the `StopIteration`
    # error raised by `OnceIterator` to signal to the interpreter that the iterator has been
    # exhausted. Here that will always be 42, but this information cannot be captured in the
    # signature of `OnceIterator.__next__`, since exceptions lie outside the type signature.
    # We therefore just infer `Unknown` here.
    #
    # If the `StopIteration` error in `OnceIterator.__next__` had been simply `raise StopIteration`
    # (the more common case), then the `.value` attribute of the `StopIteration` instance
    # would default to `None`.
    reveal_type(result)  # revealed: Unknown
```

## `yield from` with an iterable annotation

Unlike `Generator[YieldT, SendT, ReturnT]`, `Iterable[YieldT]` and `Iterator[YieldT]` have no type
parameter describing the value carried by `StopIteration` when iteration ends. That value becomes
the result of `yield from`, so ty must infer `Unknown` rather than incorrectly assuming `None`.
Returning that result from a generator annotated as returning `int` must therefore remain valid.

```py
from collections.abc import Generator, Iterable, Iterator

def delegated_iterable(values: Iterable[int]) -> Generator[int, None, int]:
    result = yield from values
    reveal_type(result)  # revealed: Unknown
    return result

def delegated_iterator(values: Iterator[int]) -> Generator[int, None, int]:
    result = yield from values
    reveal_type(result)  # revealed: Unknown
    return result
```

## `yield from` with a generator that return `types.GeneratorType`

`types.GeneratorType` is a nominal type that implements the `typing.Generator` protocol:

```py
from types import GeneratorType

def inner_generator() -> GeneratorType[int, bytes, str]:
    yield 1
    yield 2
    x = yield 3

    reveal_type(x)  # revealed: bytes

    return "done"

def outer_generator():
    result = yield from inner_generator()
    reveal_type(result)  # revealed: str
```

## Inferring with type context

A dict literal that is structurally compatible with a `TypedDict` should be accepted.

```py
from typing import Iterator, Generator, TypedDict

class Person(TypedDict):
    name: str

def persons() -> Iterator[Person]:
    yield {"name": "Alice"}
    yield {"name": "Bob"}

    # error: [invalid-yield]
    # error: [invalid-argument-type]
    yield {"name": 42}
```

Prior to <https://github.com/astral-sh/ruff/pull/27598>, ty incorrectly rejected generator functions
like the one below. The `wrap` function requires its keys and values to share the same `K` type
parameter. Although the keys here use `object` and the values use `str`, the generator promises to
yield `Values[Any]`, so `Any` can accommodate both. The old implementation ignored that promise,
expected `Values[object]`, and incorrectly rejected `Values[str]`.

```py
from collections.abc import Iterable
from typing import Any, Generic, TypeVar

K = TypeVar("K")

class Keys(Generic[K]): ...
class Values(Generic[K]): ...

def wrap(keys: Keys[K], values: Values[K]) -> Values[K]:
    return values

def sources(keys: Keys[object], values: Values[str]) -> Iterable[Values[Any]]:
    # Regression: this used to emit [invalid-argument-type] because ty expected `Values[object]`.
    yield wrap(keys, values)
```

This also works with `yield from`, where the iterable expression is inferred with the outer
generator's yield type as type context:

```py
def persons() -> Iterator[Person]:
    yield from [{"name": "Alice"}, {"name": "Bob"}]

    # error: [invalid-yield]
    # error: [invalid-argument-type]
    yield from [{"name": 42}]
```

This also works for return values:

```py
def persons(f: bool) -> Generator[None, None, Person]:
    yield
    if f:
        return {"name": "Bob"}
    else:
        # error: [invalid-return-type]
        # error: [invalid-argument-type]
        return {"name": 42}
```

## `yield` expression send type inference

```py
from typing import AsyncGenerator, AsyncIterator, AsyncIterable, Iterable, Protocol, Generator, Iterator

def unannotated():
    x = yield 1
    reveal_type(x)  # revealed: Unknown

def default_generator() -> Generator[None]:
    x = yield
    reveal_type(x)  # revealed: None

def generator_one_arg() -> Generator[int]:
    x = yield 1
    reveal_type(x)  # revealed: None

def generator_send_str() -> Generator[int, str]:
    x = yield 1
    reveal_type(x)  # revealed: str

async def async_generator_default() -> AsyncGenerator[int]:
    x = yield 1
    reveal_type(x)  # revealed: None

async def async_generator_send_str() -> AsyncGenerator[int, str]:
    x = yield 1
    reveal_type(x)  # revealed: str
```

Using a union of `Generator` and `AsyncGenerator` in a return type is invalid, as a function can
only ever be a synchronous generator function or an asynchronous generator function.
`Generator | AsyncGenerator` is not assignable to any of `Generator`, `AsyncGenerator`, `Iterable`
or `AsyncIterable`, so a type checker cannot reasonably infer the yield, send and return types of
the generator function. Our behaviour on the following snippet matches pyright and pycroscope,
though it differs from mypy, zuban and pyrefly as of 2026/08/08:

```py
def mixing_generator_async_generator() -> Generator[int, int, None] | AsyncGenerator[int, str]:
    # TODO: we should warn the user somehow that we're falling back to `Unknown` here instead
    # of inferring it silently.
    x = yield 1
    reveal_type(x)  # revealed: Unknown
```

`Iterator`, `Iterable`, and custom equivalent protocols have no send type or return type. Using one
of these is equivalent to using `Generator` with send set to `None` and return type to `Unknown`.

```py
def iterator_send_none() -> Iterator[int]:
    x = yield 1
    reveal_type(x)  # revealed: None

def iterable_send_none() -> Iterable[int]:
    x = yield 1
    reveal_type(x)  # revealed: None

async def async_iterator_send_none() -> AsyncIterator[int]:
    x = yield 1
    reveal_type(x)  # revealed: None

async def async_iterable_send_none() -> AsyncIterable[int]:
    x = yield 1
    reveal_type(x)  # revealed: None

def iterator_yield_from() -> Generator[int, None, int]:
    yield from iterator_send_none()
    return 1

class CustomIteratorProtocol(Protocol):
    def __iter__(self) -> Iterator[int]: ...

def custom_proto_send_none() -> CustomIteratorProtocol:
    x = yield 1
    reveal_type(x)  # revealed: None
```

## Unions of generator send types

Generator send types are contravariant, so a value sent into a union of generators must be accepted
by every member. That means that the inferred type of a `yield` expression inside the generator (the
inferred type of a value sent *into* the generator) will be an intersection of the send types in the
return-type union:

```py
from collections.abc import AsyncGenerator, Generator

class Foo: ...
class Bar: ...

def generator_union() -> Generator[int, Foo, None] | Generator[int, Bar, None]:
    received = yield 1
    reveal_type(received)  # revealed: Foo & Bar
```

The same rule applies to asynchronous generators.

```py
async def async_generator_union() -> AsyncGenerator[int, Foo] | AsyncGenerator[int, Bar]:
    received = yield 1
    reveal_type(received)  # revealed: Foo & Bar
```

If the send types in the union are disjoint, this can therefore result in us inferring `Never` as
the result of a `yield` expression and inferring the remaining code in the function as being
unreachable. This is correct -- since we would reject any inhabited type from being sent into the
generator, the generator can logically never accept any values being sent into it, so any code
following the `yield` expression must be unreachable as a result:

```py
def generator_union_disjoint() -> Generator[int, int, None] | Generator[int, str, None]:
    received = yield 1
    reveal_type(received)  # revealed: Never
    1 + "foo"  # no error (this region is inferred as unreachable)
```

## Generator type aliases

ty "sees through" type aliases used as return annotations when inferring a generator's yield type.

```toml
[environment]
python-version = "3.12"
```

```py
from typing import AsyncGenerator, Generator, Iterator

type GeneratorAlias[T] = Generator[T]

def invalid_yield() -> GeneratorAlias[int]:
    yield "foo"  # error: [invalid-yield]

def invalid_return() -> GeneratorAlias[int]:
    yield 42
    return "foo"  # error: [invalid-return-type]

type NestedGeneratorAlias[T] = GeneratorAlias[T]

def invalid_nested_yield() -> NestedGeneratorAlias[int]:
    yield "foo"  # error: [invalid-yield]

type IteratorAlias[T] = Iterator[T]

def invalid_iterator_return() -> IteratorAlias[int]:
    yield 42
    return "foo"  # error: [invalid-return-type]

type AsyncGeneratorAlias[T] = AsyncGenerator[T]

async def invalid_async_yield() -> AsyncGeneratorAlias[int]:
    yield "foo"  # error: [invalid-yield]
```

The same applies when inferring a generator's return type and send type:

```py
type FullGeneratorAlias[YieldT, SendT, ReturnT] = Generator[YieldT, SendT, ReturnT]

def inner_aliased_generator() -> FullGeneratorAlias[int, bytes, str]:
    sent = yield 42
    reveal_type(sent)  # revealed: bytes
    return "done"

def outer_aliased_generator() -> FullGeneratorAlias[int, bytes, None]:
    result = yield from inner_aliased_generator()
    reveal_type(result)  # revealed: str
```

## Structurally compatible generator protocols

A protocol does not need to explicitly inherit from `Generator` for ty to infer its yield, send, and
return types from its methods.

```toml
[environment]
python-version = "3.13"
```

```py
from types import TracebackType
from typing import Generator, Protocol, overload

class StructuralGenerator(Protocol):
    def __iter__(self) -> Generator[int, bytes, str]: ...
    def __next__(self) -> int: ...
    def send(self, value: bytes, /) -> int: ...
    @overload
    def throw(
        self,
        typ: type[BaseException],
        val: object = None,
        traceback: TracebackType | None = None,
        /,
    ) -> int: ...
    @overload
    def throw(
        self,
        typ: BaseException,
        val: None = None,
        traceback: TracebackType | None = None,
        /,
    ) -> int: ...
    def close(self) -> str | None: ...

def structural_generator() -> StructuralGenerator:
    sent = yield 1
    reveal_type(sent)  # revealed: bytes
    return "done"

def delegated_generator() -> Generator[int, bytes, None]:
    result = yield from structural_generator()
    reveal_type(result)  # revealed: str

def invalid_structural_yield() -> StructuralGenerator:
    yield "wrong"  # error: [invalid-yield]
    return "done"

def invalid_structural_return() -> StructuralGenerator:
    yield 1
    return 42  # error: [invalid-return-type]
```

## Iterable protocols with generator send methods

A protocol can expose a generator's `send` method without declaring every other `Generator` method.
When such a protocol is used as a generator function's return annotation, the type accepted by
`send` determines the type of the `yield` expression, even if `send` advertises a return type that
is broader than the iterator's yielded type.

```py
from collections.abc import AsyncIterator, Awaitable, Iterator
from typing import Protocol, TypeVar

class Sendable(Protocol):
    def __iter__(self) -> Iterator[int]: ...
    def send(self, value: bytes, /) -> object: ...

def generator() -> Sendable:
    received = yield 1
    reveal_type(received)  # revealed: bytes
    received.decode()
```

The send type remains precise when the yielded and sent values share a type variable.

```py
T = TypeVar("T")

class Correlated(Protocol[T]):
    def __iter__(self) -> Iterator[T]: ...
    def send(self, value: T, /) -> T: ...

def correlated(value: T) -> Correlated[T]:
    received = yield value
    reveal_type(received)  # revealed: T@correlated
```

The same rule applies to asynchronous iterable protocols exposing an `asend` method: its awaitable
return type can be broader than the async iterator's yielded type.

```py
class AsyncSendable(Protocol):
    def __aiter__(self) -> AsyncIterator[int]: ...
    def asend(self, value: bytes, /) -> Awaitable[object]: ...

async def async_generator() -> AsyncSendable:
    received = yield 1
    reveal_type(received)  # revealed: bytes
    received.decode()
```

The correlation is also preserved when `asend` returns an awaitable of the yielded type.

```py
class AsyncCorrelated(Protocol[T]):
    def __aiter__(self) -> AsyncIterator[T]: ...
    def asend(self, value: T, /) -> Awaitable[T]: ...

async def async_correlated(value: T) -> AsyncCorrelated[T]:
    received = yield value
    reveal_type(received)  # revealed: T@async_correlated
```

## Intersections of generator types

An intersection of generator types should intersect their yield and return types. Until
<https://github.com/astral-sh/ty/issues/2799> is fixed, the constraint solver incorrectly combines
the specializations using unions, so incompatible yields and returns are accepted.

```py
from collections.abc import AsyncGenerator, Generator
from ty_extensions import Intersection

def incompatible_yield() -> Intersection[Generator[int, None, None], Generator[str, None, None]]:
    # TODO: This should emit [invalid-yield].
    yield 1

def incompatible_return() -> Intersection[Generator[int, None, int], Generator[int, None, str]]:
    yield 1
    # TODO: This should emit [invalid-return-type].
    return 1

async def incompatible_async_yield() -> Intersection[AsyncGenerator[int, None], AsyncGenerator[str, None]]:
    # TODO: This should emit [invalid-yield].
    yield 1
```

## Regression test: generic generator yield and return types

An early version of the generator type-argument solver introduced by
<https://github.com/astral-sh/ruff/pull/27598> confused a generator's generic yield type with its
independently annotated send and return types. Here, the generator yields values of type `T`, but
its send and return types are both `None`: the `yield` expression must therefore have type `None`,
and returning a value of type `T` must be rejected.

```py
from collections.abc import Generator
from typing import TypeVar

T = TypeVar("T")

def generator_return(value: T) -> Generator[T, None, None]:
    sent = yield value
    reveal_type(sent)  # revealed: None

    # error: [invalid-return-type] "Return type does not match returned value: expected `None`, found `T@generator_return`"
    return value
```

## Regression test: delegating to a generic generator expression

An early version of the generator type-argument inference introduced by
<https://github.com/astral-sh/ruff/pull/27598> emitted a spurious `invalid-yield` diagnostic when a
generator delegated to a generator expression over generic values. The generator expression below
yields values of type `T`, matching the enclosing generator's `Iterator[T]` annotation, so
`yield from` must be accepted.

```py
from collections.abc import Iterable, Iterator
from typing import TypeVar

T = TypeVar("T")

def delegated(values: Iterable[T]) -> Iterator[T]:
    yield from (value for value in values)
```

## Error cases

### Non-iterable type

```py
from typing import Generator

def generator() -> Generator[None]:
    yield from 42  # error: [not-iterable] "Object of type `Literal[42]` is not iterable"
```

### Invalid `yield` type

```py
from typing import Generator, Iterable, Iterator, Protocol

def invalid_generator() -> Generator[int, None, None]:
    # snapshot: invalid-yield
    yield ""
```

```snapshot
error[invalid-yield]: Yield expression type does not match annotation
 --> src/mdtest_snippet.py:5:11
  |
3 | def invalid_generator() -> Generator[int, None, None]:
  |                            -------------------------- Function annotated with yield type `int` here
4 |     # snapshot: invalid-yield
5 |     yield ""
  |           ^^ expression of type `Literal[""]`, expected `int`
```

More examples:

```py
def invalid_iterator() -> Iterator[None]:
    yield ""  # error: [invalid-yield]

def invalid_iterable() -> Iterable[None]:
    yield ""  # error: [invalid-yield]

class CustomIteratorProto(Protocol):
    def __iter__(self) -> Iterator[int]: ...

def invalid_custom_proto() -> CustomIteratorProto:
    yield ""  # snapshot: invalid-yield
```

```snapshot
error[invalid-yield]: Yield expression type does not match annotation
  --> src/mdtest_snippet.py:16:11
   |
15 | def invalid_custom_proto() -> CustomIteratorProto:
   |                               ------------------- Function annotated with yield type `int` here
16 |     yield ""  # snapshot: invalid-yield
   |           ^^ expression of type `Literal[""]`, expected `int`
```

### Invalid annotation

```py
from typing import AsyncGenerator, Generator

def returns_str() -> str:  # error: [invalid-return-type]
    x = yield 1
    reveal_type(x)  # revealed: Unknown

def sync_returns_async_generator() -> AsyncGenerator[int, str]:  # error: [invalid-return-type]
    x = yield 1
    reveal_type(x)  # revealed: Unknown

async def async_returns_sync_generator() -> Generator[int, str, None]:  # error: [invalid-return-type]
    x = yield 1
    reveal_type(x)  # revealed: Unknown
```

### Invalid return type

```py
from typing import Generator

# error: [invalid-return-type]
def invalid_generator1() -> Generator[int, None, str]:
    yield 1

def invalid_generator2() -> Generator[int, None, None]:
    yield 1

    # error: [invalid-return-type]
    return "done"
```

### `yield from` with incompatible yield type

```py
from typing import Generator

def inner() -> Generator[str, None, None]:
    yield "hello"

def outer() -> Generator[int, None, None]:
    # error: [invalid-yield] "Yield type `str` does not match annotated yield type `int`"
    yield from inner()
```

### `yield from` with incompatible send type

```py
from typing import Generator

def inner() -> Generator[int, int, None]:
    x = yield 1

def outer() -> Generator[int, str, None]:
    # snapshot: invalid-yield
    yield from inner()
```

```snapshot
error[invalid-yield]: Send type does not match annotation
 --> src/mdtest_snippet.py:8:16
  |
6 | def outer() -> Generator[int, str, None]:
  |                ------------------------- Function annotated with send type `str` here
7 |     # snapshot: invalid-yield
8 |     yield from inner()
  |                ^^^^^^^ generator with send type `int`, expected `str`
```

### Non generator function with `Generator` annotation

```py
from typing import Generator

def non_gen() -> Generator[int, int, None]:
    # snapshot: invalid-return-type
    return 1

reveal_type(non_gen)  # revealed: def non_gen() -> Generator[int, int, None]
```

```snapshot
error[invalid-return-type]: Return type does not match returned value
 --> src/mdtest_snippet.py:5:12
  |
3 | def non_gen() -> Generator[int, int, None]:
  |                  ------------------------- Expected `Generator[int, int, None]` because of return type
4 |     # snapshot: invalid-return-type
5 |     return 1
  |            ^ expected `Generator[int, int, None]`, found `Literal[1]`
info: type `Literal[1]` is not assignable to protocol `Generator[int, int, None]`
info: └── protocol member `__iter__` is not defined on type `Literal[1]`
```

## *Unsound* yield expressions

In addition to `invalid-yield`, we also offer a disabled-by-default stricter rule `unsound-yield`.
This rule forbids `yield` expressions that yield an instance of a type `A` unless `A` is a *subtype*
of the annotated yield type:

```toml
[rules]
unsound-yield = "error"
```

```py
from typing import Any, Generator, Iterator

def returns_any() -> Any:
    return "not an integer"

def generator() -> Generator[int]:
    # snapshot: unsound-yield
    yield returns_any()
```

```snapshot
error[unsound-yield]: Unsound `yield`
 --> src/mdtest_snippet.py:8:11
  |
6 | def generator() -> Generator[int]:
  |                    -------------- Expected a subtype of `int` because of the yield type
7 |     # snapshot: unsound-yield
8 |     yield returns_any()
  |           ^^^^^^^^^^^^^ Inferred as `Any`
info: `Any` is assignable to `int`, but not a subtype of `int`
help: Consider using an `assert` to narrow the type before yielding it
```

The same check applies to generators annotated as iterators. Values that are not even assignable to
the annotated yield type still cause us to emit only `invalid-yield`.

```py
def iterator() -> Iterator[int]:
    yield returns_any()  # error: [unsound-yield]

def invalid_generator() -> Generator[int]:
    yield "not an integer"  # error: [invalid-yield]
```

Narrowing a dynamic value before yielding it makes the yield sound.

```py
def narrowed_generator() -> Generator[int]:
    value = returns_any()
    assert isinstance(value, int)
    yield value

def unannotated_generator():
    yield returns_any()
```

An example with nested error context:

```py
def nested_generator() -> Generator[tuple[tuple[int, int]]]:
    # snapshot: unsound-yield
    yield ((42, returns_any()),)
```

```snapshot
error[unsound-yield]: Unsound `yield`
  --> src/mdtest_snippet.py:23:11
   |
21 | def nested_generator() -> Generator[tuple[tuple[int, int]]]:
   |                           --------------------------------- Expected a subtype of `tuple[tuple[int, int]]` because of the yield type
22 |     # snapshot: unsound-yield
23 |     yield ((42, returns_any()),)
   |           ^^^^^^^^^^^^^^^^^^^^^^ Inferred as `tuple[tuple[Literal[42], Any]]`
info: `tuple[tuple[Literal[42], Any]]` is assignable to `tuple[tuple[int, int]]`, but not a subtype of `tuple[tuple[int, int]]`
info: the first tuple element is not compatible: `tuple[Literal[42], Any]` is not a subtype of `tuple[int, int]`
info: └── the second tuple element is not compatible: `Any` is not a subtype of `int`
help: Consider using an `assert` to narrow the type before yielding it
```

## Unsound yield statements with gradual yield types

The rule applies only when the annotated yield type is fully static. An explicit `Any`, an alias of
`Any`, or an `Any` nested inside the yield type disables the strict check.

```toml
[rules]
unsound-yield = "error"
```

```py
from typing import Any, Generator, Iterator
from typing_extensions import Never, TypeAliasType

AnyAlias = TypeAliasType("AnyAlias", Any)

def returns_any() -> Any:
    return "not an integer"

def dynamic_yield_type() -> Generator[Any]:
    yield returns_any()

def aliased_dynamic_yield_type() -> Generator[AnyAlias]:
    yield returns_any()

def nested_dynamic_yield_type() -> Iterator[tuple[int, Any]]:
    yield returns_any()

# error: [missing-type-argument]
def unknown_yield_type() -> Iterator:
    yield returns_any()
```

Only the yield type determines whether the boundary is fully static; dynamic send and return types
do not disable the check. `Never` is also a fully static yield type.

```py
def dynamic_send_and_return_types() -> Generator[int, Any, Any]:
    yield returns_any()  # error: [unsound-yield]

def never_yields() -> Generator[Never]:
    yield returns_any()  # error: [unsound-yield]
```

## Unsound delegated yield expressions

`yield from` exposes every value produced by the delegated iterator, so its element type must also
be a subtype of the outer generator's fully static yield type.

```toml
[rules]
unsound-yield = "error"
```

```py
from typing import Any, Generator, Iterator

def dynamic_values() -> Generator[Any]:
    yield "not an integer"

def delegated_generator() -> Generator[int]:
    # snapshot: unsound-yield
    yield from dynamic_values()
```

```snapshot
error[unsound-yield]: Unsound `yield from`
 --> src/mdtest_snippet.py:8:16
  |
6 | def delegated_generator() -> Generator[int]:
  |                              -------------- Expected a subtype of `int` because of the yield type
7 |     # snapshot: unsound-yield
8 |     yield from dynamic_values()
  |                ^^^^^^^^^^^^^^^^ Yielded elements inferred as `Any`
info: `Any` is assignable to `int`, but not a subtype of `int`
help: Consider using `assert`s to narrow the types of the elements before yielding them
```

Nested dynamic values are rejected too, while genuinely incompatible iterators cause us to emit
`invalid-yield` instead.

```py
def nested_dynamic_values() -> Iterator[tuple[int, Any]]:
    yield (1, "not an integer")

def nested_delegated_generator() -> Iterator[tuple[int, int]]:
    yield from nested_dynamic_values()  # error: [unsound-yield]

def invalid_delegated_generator() -> Iterator[int]:
    yield from ["not an integer"]  # error: [invalid-yield]

def valid_delegated_generator() -> Iterator[int]:
    yield from [1, 2]
```

## Edge case: `unsound-yield` combined with `yield from` expressions that are not iterable

```toml
[rules]
unsound-yield = "error"
```

In the following situation, we only emit `not-iterable`, even though the inferred `yield` type here
is `Unknown` (not a subtype of `int`). Also emitting `unsound-yield` here would just add confusing
noise to our diagnostics: `Unknown` is just a fallback type here that we "spun out of thin air"
because `42` has no `__iter__` method to tell us any better.

```py
from typing import Iterable, Iterator, Any

def non_iterable_delegated_generator() -> Iterator[int]:
    # Here we only emit `not-iterable`, even though the inferred yield type here
    # is `Unknown`: also emitting `unsound-yield` would just add noise
    yield from 42  # error: [not-iterable]
```

But the following situation is different: here we emit both `not-iterable` *and* `unsound-yield`,
because `Any` was not simply a fallback here that we "invented out of thin air". It's the annotated
iterable type of `BrokenIterable`'s `__iter__` method:

```py
class BrokenIterable:
    def __iter__(self, oh_no) -> Iterator[Any]:
        raise NotImplementedError

def broken_iterable_delegated_generator() -> Iterator[int]:
    # snapshot: not-iterable
    # snapshot: unsound-yield
    yield from BrokenIterable()
```

```snapshot
error[not-iterable]: Object of type `BrokenIterable` is not iterable
  --> src/mdtest_snippet.py:14:16
   |
14 |     yield from BrokenIterable()
   |                ^^^^^^^^^^^^^^^^
info: Its `__iter__` method has an invalid signature
info: type `BrokenIterable` is not assignable to protocol `Iterable[Unknown]`
info: └── protocol member `__iter__` is incompatible
info:     └── unexpected extra parameter `oh_no`
help: Parameter `oh_no` must have a default value
info: Expected signature `def __iter__(self): ...`


error[unsound-yield]: Unsound `yield from`
  --> src/mdtest_snippet.py:14:16
   |
11 | def broken_iterable_delegated_generator() -> Iterator[int]:
   |                                              ------------- Expected a subtype of `int` because of the yield type
12 |     # snapshot: not-iterable
13 |     # snapshot: unsound-yield
14 |     yield from BrokenIterable()
   |                ^^^^^^^^^^^^^^^^ Yielded elements inferred as `Any`
info: `Any` is assignable to `int`, but not a subtype of `int`
help: Consider using `assert`s to narrow the types of the elements before yielding them
```

## Unsound asynchronous yield statements

The strict yield check also applies to asynchronous generators and asynchronous iterators.

```toml
[rules]
unsound-yield = "error"
```

```py
from typing import Any, AsyncGenerator, AsyncIterator

def returns_any() -> Any:
    return "not an integer"

async def asynchronous_generator() -> AsyncGenerator[int]:
    yield returns_any()  # error: [unsound-yield]

async def asynchronous_iterator() -> AsyncIterator[int]:
    yield returns_any()  # error: [unsound-yield]

async def dynamic_asynchronous_generator() -> AsyncGenerator[Any]:
    yield returns_any()
```
