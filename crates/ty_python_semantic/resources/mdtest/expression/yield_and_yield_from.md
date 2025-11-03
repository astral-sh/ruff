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

    # TODO: this should be `bytes`
    reveal_type(x)  # revealed: @Todo(yield expressions)

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

    # TODO: this should be `bytes`
    reveal_type(x)  # revealed: @Todo(yield expressions)

    return "done"

def outer_generator():
    result = yield from inner_generator()
    reveal_type(result)  # revealed: str
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

# TODO: This should be an error. Claims to yield `int`, but yields `str`.
def invalid_generator() -> Generator[int, None, None]:
    yield "not an int"  # This should be an `int`
```

### Invalid return type

```py
from typing import Generator

# TODO: should emit an error (does not return `str`)
def invalid_generator1() -> Generator[int, None, str]:
    yield 1

# TODO: should emit an error (does not return `int`)
def invalid_generator2() -> Generator[int, None, None]:
    yield 1

    return "done"
```
