# Async

Async `for` loops do not work according to the synchronous iteration protocol.

## Invalid async for loop

```py
async def foo():
    class Iterator:
        def __next__(self) -> int:
            return 42

    class Iterable:
        def __iter__(self) -> Iterator:
            return Iterator()

    async for x in Iterator():
        # TODO: should emit an error, `__aiter__` is not defined
        reveal_type(x)  # revealed: Unknown
```

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
