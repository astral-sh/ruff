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

`yield from` can also be used with custom iterable types. If the iterator returned by `__iter__` is
not a generator, the type of the `yield from` expression cannot be determined:

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

## `yield from` with a custom iterable whose `__iter__` returns a generator

`yield from x` delegates to `iter(x)`. The send and return types of the `yield from` expression are
therefore determined by the iterator returned by `x.__iter__()`, even if `x` itself is not a
generator:

```py
from typing import Generator

class Box:
    def __iter__(self) -> Generator[str, None, int]:
        yield "hello"
        return 42

def main() -> Generator[str, None, int]:
    x = yield from Box()
    reveal_type(x)  # revealed: int

    y: str = yield from Box()  # error: [invalid-assignment]
    return x
```

The send type of the inner generator is also validated against the outer generator's send type:

```py
class SendBox:
    def __iter__(self) -> Generator[int, int, None]:
        x = yield 1

def outer() -> Generator[int, str, None]:
    # error: [invalid-yield] "Send type `int` does not match annotated send type `str`"
    yield from SendBox()

def outer_ok() -> Generator[int, int, None]:
    yield from SendBox()
```

## `yield from` with a plain `Iterator`

An `Iterator` annotation specifies the yielded type, but not the value of `StopIteration.value`. The
result of delegating to such an iterator is therefore `Unknown`, even when it is returned by a
custom iterable's `__iter__` method:

```py
from typing import Generator, Iterator

class Finished:
    def __iter__(self) -> "Finished":
        return self

    def __next__(self) -> str:
        raise StopIteration(42)

class Plain:
    def __iter__(self) -> Iterator[str]:
        return Finished()

def plain() -> Generator[str, None, int]:
    result = yield from Plain()
    reveal_type(result)  # revealed: Unknown
    return result
```

The same applies to an iterator used directly and to a built-in iterable whose `__iter__` method
returns a plain `Iterator`:

```py
def direct(iterator: Iterator[str]) -> Generator[str, None, int]:
    result = yield from iterator
    reveal_type(result)  # revealed: Unknown
    return result

def builtin() -> Generator[str, None, None]:
    result = yield from ["a", "b"]
    reveal_type(result)  # revealed: Unknown
```

## `yield from` with alternative iteration protocols

An iterable union can mix a generator-returning `__iter__` with the sequence protocol. The
`__getitem__` alternative contributes to the result of `yield from`, so the return type of the
generator alone does not describe every possible result:

```py
from typing import Generator

class Wrapped:
    def __iter__(self) -> Generator[int, int | None, str]:
        yield 1
        return "done"

class Sequence:
    def __getitem__(self, index: int) -> int:
        raise IndexError

def mixed(value: Wrapped | Sequence) -> Generator[int, None, object]:
    result = yield from value
    reveal_type(result)  # revealed: Unknown
    return result
```

The sequence iterator does not support sending a non-`None` value, even though the generator does:

```py
def mixed_send(value: Wrapped | Sequence) -> Generator[int, int, None]:
    # error: [invalid-yield] "Send type `None` does not match annotated send type `int`"
    yield from value
```

## `yield from` with union send types

The outer generator's send type must be accepted by every possible delegated generator. An `int`
cannot be forwarded to an iterator that might require `str`:

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Generator

class IntBox:
    def __iter__(self) -> Generator[int, int, None]:
        yield 1

class StrBox:
    def __iter__(self) -> Generator[int, str, None]:
        yield 1

def incompatible_boxes(box: IntBox | StrBox) -> Generator[int, int, None]:
    yield from box  # error: [invalid-yield]
```

The same check applies when `__iter__` itself returns a union, including through a type alias, or
when the operand is already a union of generators:

```py
type EitherGenerator = Generator[int, int, None] | Generator[int, str, None]

class UnionBox:
    def __iter__(self) -> EitherGenerator:
        yield 1

def incompatible_iterators() -> Generator[int, int, None]:
    yield from UnionBox()  # error: [invalid-yield]

def incompatible_generators(inner: EitherGenerator) -> Generator[int, int, None]:
    yield from inner  # error: [invalid-yield]
```

Delegation is valid when every alternative accepts the outer send type, even if the alternatives
also accept different additional types:

```py
class OverlappingBox:
    def __iter__(self) -> Generator[int, int | str, None] | Generator[int, int | bytes, None]:
        yield 1

def compatible_iterators() -> Generator[int, int, None]:
    yield from OverlappingBox()
```

Gradual send types are checked against each alternative separately. `list[Any]` is assignable to
both `list[int]` and `list[str]`, so this delegation is accepted:

```py
from typing import Any

def gradual_send(
    inner: Generator[int, list[int], None] | Generator[int, list[str], None],
) -> Generator[int, list[Any], None]:
    yield from inner
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
from typing import AsyncGenerator, AsyncIterator, Generator, Iterator

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

def mixing_generator_async_generator() -> Generator[int, int, None] | AsyncGenerator[int, str]:
    x = yield 1
    reveal_type(x)  # revealed: int | str
    return None
```

Within generator functions annotated as `Iterator` or `AsyncIterator`, we infer `None` for `yield`
expressions. These annotations expose iteration with `next()` or `anext()`, not a `send` or `asend`
method.

```py
def iterator_send_none() -> Iterator[int]:
    x = yield 1
    reveal_type(x)  # revealed: None

async def async_iterator_send_none() -> AsyncIterator[int]:
    x = yield 1
    reveal_type(x)  # revealed: None

def iterator_yield_from() -> Generator[int, None, int]:
    yield from iterator_send_none()
    return 1
```

## `yield from` with an `Iterator` return annotation

An outer `Iterator` annotation does not expose a `send` method. Advancing the outer iterator with
`next()` also advances the delegated generator with `next()`, so its send type does not restrict
this delegation:

```py
from typing import Generator, Iterator

class Wrapped:
    def __iter__(self) -> Generator[int, str, None]:
        yield 1

def iterator() -> Iterator[int]:
    yield from Wrapped()
```

The yielded values are still checked against the outer annotation:

```py
def invalid_yield() -> Iterator[str]:
    # error: [invalid-yield] "Yield type `int` does not match annotated yield type `str`"
    yield from Wrapped()
```

An explicit `Generator` annotation still constrains the values sent to the delegated generator:

```py
def invalid_send() -> Generator[int, int, None]:
    # error: [invalid-yield] "Send type `str` does not match annotated send type `int`"
    yield from Wrapped()
```

An `Iterator` member in a return-type union does not remove the other members' explicit send
requirements. Here the yielded `int` values require the `Generator` alternative, whose send type
must be compatible with the delegated generator:

```py
def mixed_annotation() -> Iterator[str] | Generator[int, int, None]:
    # error: [invalid-yield] "Send type `str` does not match annotated send type `int`"
    yield from Wrapped()

def compatible_mixed_annotation() -> Iterator[str] | Generator[int, str, None]:
    yield from Wrapped()
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

def aliased_iterator(inner: Generator[int, str, None]) -> IteratorAlias[int]:
    yield from inner

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

## Error cases

### Non-iterable type

```py
from typing import Generator

def generator() -> Generator[None]:
    yield from 42  # error: [not-iterable] "Object of type `Literal[42]` is not iterable"
```

### Invalid `yield` type

```py
from typing import Generator

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

### Invalid annotation

```py
from typing import AsyncGenerator, Generator

def returns_str() -> str:  # error: [invalid-return-type]
    x = yield 1
    reveal_type(x)  # revealed: Unknown

def sync_returns_async_generator() -> AsyncGenerator[int, str]:  # error: [invalid-return-type]
    x = yield 1
    reveal_type(x)  # revealed: str
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
