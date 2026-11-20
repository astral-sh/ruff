## What it does

Checks for objects that are not iterable but are used in a context that requires them to be.

## Why is this bad?

Iterating over an object that is not iterable will raise a `TypeError` at runtime.

## Examples

```python
# TypeError: 'int' object is not iterable
for i in 34:  # error
    pass
```

## Common issues

### Async generator stubs

Calling an `async def` function whose body contains `yield` produces an async iterator, which can be
consumed with `async for`. Without `yield`, calling the function produces a coroutine, and its
return annotation describes the result of awaiting that coroutine.

This distinction matters in stub files, where replacing the implementation with `...` removes the
`yield`. For example, this stub describes a coroutine function, even though its return annotation is
`AsyncIterator[int]`:

`stubs.pyi`:

```pyi
from collections.abc import AsyncIterator

async def values() -> AsyncIterator[int]: ...
```

Iterating over the coroutine is an error. An `async for` loop awaits each item; it does not
automatically await a coroutine to obtain the iterator:

`main.py`:

```python
from stubs import values


async def consume() -> None:
    # error: "Object of type `CoroutineType[Any, Any, AsyncIterator[int]]` is not async-iterable"
    async for value in values():
        print(value)
```

To declare a function that directly produces an async iterator, use `def` rather than `async def` in
the stub:

`with_def.pyi`:

```pyi
from collections.abc import AsyncIterator

def values() -> AsyncIterator[int]: ...
```

Alternatively, keep `async def` and include a `yield` expression in the stub body:

`with_yield.pyi`:

```pyi
from collections.abc import AsyncIterator

async def values() -> AsyncIterator[int]:
    yield 1
```

If the function intentionally returns a coroutine that produces an async iterator, await it before
iterating: `async for value in await values(): ...`.
