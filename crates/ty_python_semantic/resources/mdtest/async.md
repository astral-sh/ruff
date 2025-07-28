# `async` / `await`

## Basic

```py
async def retrieve() -> int:
    return 42

reveal_type(retrieve)  # revealed: def retrieve() -> CoroutineType[Any, Any, int]

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

reveal_type(persist)  # revealed: def persist(x: T) -> CoroutineType[Any, Any, T]

async def f(x: int):
    result = await persist(x)

    reveal_type(result)  # revealed: int
```
