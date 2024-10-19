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

    # TODO
    reveal_type(x)  # revealed: Unbound | @Todo
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

    reveal_type(x)  # revealed: Unbound | @Todo
```
