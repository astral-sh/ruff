# Async

Async `for` loops do not work according to the synchronous iteration protocol.

## Basic async for loop

```py
async def foo():
    class IntAsyncIterator:
        async def __anext__(self) -> int:
            return 42

    class IntAsyncIterable:
        def __aiter__(self) -> IntAsyncIterator:
            return IntAsyncIterator()

    async for x in IntAsyncIterable():
        reveal_type(x)  # revealed: int
```

## Async for loop with unpacking

```py
async def foo():
    class AsyncIterator:
        async def __anext__(self) -> tuple[int, str]:
            return 42, "hello"

    class AsyncIterable:
        def __aiter__(self) -> AsyncIterator:
            return AsyncIterator()

    async for x, y in AsyncIterable():
        reveal_type(x)  # revealed: int
        reveal_type(y)  # revealed: str
```

## Async for loop over narrowed TypeVar

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Self

class AsyncStrings:
    def __aiter__(self) -> Self:
        return self

    async def __anext__(self) -> str:
        return "x"

async def foo[T: int | AsyncStrings](value: T) -> None:
    if isinstance(value, int):
        return

    reveal_type(value)  # revealed: T@foo & ~int
    async for item in value:
        reveal_type(item)  # revealed: str
```

## Error cases

### No `__aiter__` method

```py
class NotAsyncIterable: ...

async def foo():
    # snapshot: not-iterable
    async for x in NotAsyncIterable():
        reveal_type(x)  # revealed: Unknown
```

```snapshot
error[not-iterable]: Object of type `NotAsyncIterable` is not async-iterable
 --> src/mdtest_snippet.py:5:20
  |
5 |     async for x in NotAsyncIterable():
  |                    ^^^^^^^^^^^^^^^^^^
  |
info: It has no `__aiter__` method
```

### Synchronously iterable, but not asynchronously iterable

```py
async def foo():
    class Iterator:
        def __next__(self) -> int:
            return 42

    class Iterable:
        def __iter__(self) -> Iterator:
            return Iterator()

    # snapshot: not-iterable
    async for x in Iterator():
        reveal_type(x)  # revealed: Unknown
```

```snapshot
error[not-iterable]: Object of type `Iterator` is not async-iterable
  --> src/mdtest_snippet.py:11:20
   |
11 |     async for x in Iterator():
   |                    ^^^^^^^^^^
   |
info: It has no `__aiter__` method
```

### No `__anext__` method

```py
class NoAnext: ...

class AsyncIterable:
    def __aiter__(self) -> NoAnext:
        return NoAnext()

async def foo():
    # snapshot: not-iterable
    async for x in AsyncIterable():
        reveal_type(x)  # revealed: Unknown
```

```snapshot
error[not-iterable]: Object of type `AsyncIterable` is not async-iterable
 --> src/mdtest_snippet.py:9:20
  |
9 |     async for x in AsyncIterable():
  |                    ^^^^^^^^^^^^^^^
  |
info: Its `__aiter__` method returns an object of type `NoAnext`, which has no `__anext__` method
```

### Possibly missing `__anext__` method

```py
async def foo(flag: bool):
    class PossiblyUnboundAnext:
        if flag:
            async def __anext__(self) -> int:
                return 42

    class AsyncIterable:
        def __aiter__(self) -> PossiblyUnboundAnext:
            return PossiblyUnboundAnext()

    # snapshot: not-iterable
    async for x in AsyncIterable():
        reveal_type(x)  # revealed: int
```

```snapshot
error[not-iterable]: Object of type `AsyncIterable` may not be async-iterable
  --> src/mdtest_snippet.py:12:20
   |
12 |     async for x in AsyncIterable():
   |                    ^^^^^^^^^^^^^^^
   |
info: Its `__aiter__` method returns an object of type `PossiblyUnboundAnext`, which may not have a `__anext__` method
```

### Possibly missing `__aiter__` method

```py
async def foo(flag: bool):
    class AsyncIterable:
        async def __anext__(self) -> int:
            return 42

    class PossiblyUnboundAiter:
        if flag:
            def __aiter__(self) -> AsyncIterable:
                return AsyncIterable()

    # snapshot
    async for x in PossiblyUnboundAiter():
        reveal_type(x)  # revealed: int
```

```snapshot
error[not-iterable]: Object of type `PossiblyUnboundAiter` may not be async-iterable
  --> src/mdtest_snippet.py:12:20
   |
12 |     async for x in PossiblyUnboundAiter():
   |                    ^^^^^^^^^^^^^^^^^^^^^^
   |
info: Its `__aiter__` attribute (with type `bound method PossiblyUnboundAiter.__aiter__() -> AsyncIterable`) may not be callable
```

### Wrong signature for `__aiter__`

```py
class AsyncIterator:
    async def __anext__(self) -> int:
        return 42

class AsyncIterable:
    def __aiter__(self, arg: int) -> AsyncIterator:  # wrong
        return AsyncIterator()

async def foo():
    # snapshot: not-iterable
    async for x in AsyncIterable():
        reveal_type(x)  # revealed: int
```

```snapshot
error[not-iterable]: Object of type `AsyncIterable` is not async-iterable
  --> src/mdtest_snippet.py:11:20
   |
11 |     async for x in AsyncIterable():
   |                    ^^^^^^^^^^^^^^^
   |
info: Its `__aiter__` method has an invalid signature
info: Expected signature `def __aiter__(self): ...`
```

### Wrong signature for `__anext__`

```py
class AsyncIterator:
    async def __anext__(self, arg: int) -> int:  # wrong
        return 42

class AsyncIterable:
    def __aiter__(self) -> AsyncIterator:
        return AsyncIterator()

async def foo():
    # snapshot: not-iterable
    async for x in AsyncIterable():
        reveal_type(x)  # revealed: int
```

```snapshot
error[not-iterable]: Object of type `AsyncIterable` is not async-iterable
  --> src/mdtest_snippet.py:11:20
   |
11 |     async for x in AsyncIterable():
   |                    ^^^^^^^^^^^^^^^
   |
info: Its `__aiter__` method returns an object of type `AsyncIterator`, which has an invalid `__anext__` method
info: Expected signature for `__anext__` is `def __anext__(self): ...`
```
