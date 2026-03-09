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

def iterator_send_none() -> Iterator[int]:
    x = yield 1
    reveal_type(x)  # revealed: None

async def async_generator_default() -> AsyncGenerator[int]:
    x = yield 1
    reveal_type(x)  # revealed: None

async def async_generator_send_str() -> AsyncGenerator[int, str]:
    x = yield 1
    reveal_type(x)  # revealed: str

async def async_iterator_send_none() -> AsyncIterator[int]:
    x = yield 1
    reveal_type(x)  # revealed: None

def mixing_generator_async_generator() -> Generator[int, int, None] | AsyncGenerator[int, str]:
    x = yield 1
    reveal_type(x)  # revealed: int | str
    return None
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
    # error: [invalid-return-type]
    yield "not an int"
```

### Invalid return type

```py
from typing import Generator

def invalid_return_type() -> Generator[None, None, None]:
    yield
    # TODO: error: [invalid-return-type]
    return ""
```

### Invalid annotation

```py
from typing import AsyncGenerator, Generator

def returns_str() -> str:  # error: [invalid-return-type]
    x = yield 1
    reveal_type(x)  # revealed: Unknown

# error: [invalid-return-type]
def sync_returns_async_generator() -> AsyncGenerator[int, str]:
    x = yield 1
    reveal_type(x)  # revealed: str
```

### Using a generator with incompatible annotation in `yield from`

```py
from typing import Generator

def f() -> Generator[None, float, None]:
    x = yield

def g() -> Generator[None, int, None]:
    yield from f()
```
