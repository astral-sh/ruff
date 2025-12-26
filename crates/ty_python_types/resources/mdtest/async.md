# `async` / `await`

## Basic

```py
async def retrieve() -> int:
    return 42

async def main():
    result = await retrieve()

    reveal_type(result)  # revealed: int
```

## Generic `async` functions

```py
from typing import TypeVar

T = TypeVar("T")

async def persist(x: T) -> T:
    return x

async def f(x: int):
    result = await persist(x)

    reveal_type(result)  # revealed: int
```

## Use cases

### `Future`

```py
import asyncio
import concurrent.futures

def blocking_function() -> int:
    return 42

async def main():
    loop = asyncio.get_event_loop()
    with concurrent.futures.ThreadPoolExecutor() as pool:
        result = await loop.run_in_executor(pool, blocking_function)
        reveal_type(result)  # revealed: int
```

### `asyncio.Task`

```py
import asyncio

async def f() -> int:
    return 1

async def main():
    task = asyncio.create_task(f())

    result = await task

    reveal_type(result)  # revealed: int
```

### `asyncio.gather`

```py
import asyncio

async def task(name: str) -> int:
    return len(name)

async def main():
    (a, b) = await asyncio.gather(
        task("A"),
        task("B"),
    )

    reveal_type(a)  # revealed: int
    reveal_type(b)  # revealed: int
```

## Under the hood

```toml
[environment]
python-version = "3.12"  # Use 3.12 to be able to use PEP 695 generics
```

Let's look at the example from the beginning again:

```py
async def retrieve() -> int:
    return 42
```

When we look at the signature of this function, we see that it actually returns a `CoroutineType`:

```py
reveal_type(retrieve)  # revealed: def retrieve() -> CoroutineType[Any, Any, int]
```

The expression `await retrieve()` desugars into a call to the `__await__` dunder method on the
`CoroutineType` object, followed by a `yield from`. Let's first see the return type of `__await__`:

```py
reveal_type(retrieve().__await__())  # revealed: Generator[Any, None, int]
```

We can see that this returns a `Generator` that yields `Any`, and eventually returns `int`. For the
final type of the `await` expression, we retrieve that third argument of the `Generator` type:

```py
from typing import Generator

def _():
    result = yield from retrieve().__await__()
    reveal_type(result)  # revealed: int
```
