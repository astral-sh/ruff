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
        pass

    # TODO: should reveal `Unknown` because `__aiter__` is not defined
    # revealed: @Todo(async iterables/iterators)
    # error: [possibly-unresolved-reference]
    reveal_type(x)
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

    # TODO(Alex): async iterables/iterators!
    async for x in IntAsyncIterable():
        pass

    # error: [possibly-unresolved-reference]
    # revealed: @Todo(async iterables/iterators)
    reveal_type(x)
```
