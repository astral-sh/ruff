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

## Error cases

<!-- snapshot-diagnostics -->

### No `__aiter__` method

```py
class NotAsyncIterable: ...

async def foo():
    # error: [not-iterable] "Object of type `NotAsyncIterable` is not async-iterable"
    async for x in NotAsyncIterable():
        reveal_type(x)  # revealed: Unknown
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

    # error: [not-iterable] "Object of type `Iterator` is not async-iterable"
    async for x in Iterator():
        reveal_type(x)  # revealed: Unknown
```

### No `__anext__` method

```py
class NoAnext: ...

class AsyncIterable:
    def __aiter__(self) -> NoAnext:
        return NoAnext()

async def foo():
    # error: [not-iterable] "Object of type `AsyncIterable` is not async-iterable"
    async for x in AsyncIterable():
        reveal_type(x)  # revealed: Unknown
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

    # error: [not-iterable] "Object of type `AsyncIterable` may not be async-iterable"
    async for x in AsyncIterable():
        reveal_type(x)  # revealed: int
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

    # error: "Object of type `PossiblyUnboundAiter` may not be async-iterable"
    async for x in PossiblyUnboundAiter():
        reveal_type(x)  # revealed: int
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
    # error: [not-iterable] "Object of type `AsyncIterable` is not async-iterable"
    async for x in AsyncIterable():
        reveal_type(x)  # revealed: int
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
    # error: [not-iterable] "Object of type `AsyncIterable` is not async-iterable"
    async for x in AsyncIterable():
        reveal_type(x)  # revealed: int
```
