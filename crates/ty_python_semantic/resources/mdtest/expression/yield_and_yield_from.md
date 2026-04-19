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

def generator() -> Generator:
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

## Infering with type context

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

def default_generator() -> Generator:
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

`Iterator` has no send type or return type, It is equivalent to using `Generator` with send set to
`None` and return type to `Unknown`.

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

## Error cases

### Non-iterable type

```py
from typing import Generator

def generator() -> Generator:
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
 --> src/mdtest_snippet.py:3:28
  |
3 | def invalid_generator() -> Generator[int, None, None]:
  |                            -------------------------- Function annotated with yield type `int` here
4 |     # snapshot: invalid-yield
5 |     yield ""
  |           ^^ expression of type `Literal[""]`, expected `int`
  |
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
 --> src/mdtest_snippet.py:6:16
  |
6 | def outer() -> Generator[int, str, None]:
  |                ------------------------- Function annotated with send type `str` here
7 |     # snapshot: invalid-yield
8 |     yield from inner()
  |                ^^^^^^^ generator with send type `int`, expected `str`
  |
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
 --> src/mdtest_snippet.py:3:18
  |
3 | def non_gen() -> Generator[int, int, None]:
  |                  ------------------------- Expected `Generator[int, int, None]` because of return type
4 |     # snapshot: invalid-return-type
5 |     return 1
  |            ^ expected `Generator[int, int, None]`, found `Literal[1]`
  |
```
