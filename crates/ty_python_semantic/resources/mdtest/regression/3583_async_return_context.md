# Async return context for nested generics

Regression test for [#3583](https://github.com/astral-sh/ty/issues/3583). When inferring the generic
async call in a return statement, the `Awaitable[int]` context should not infer `None` through
`Generator.close()` on Python 3.13 or newer.

```toml
[environment]
python-version = "3.13"
```

```py
from typing import Any, Generic, TypeVar

T = TypeVar("T", bound=tuple[Any, ...])

class Select(Generic[T]):
    pass

def first[T](v: T) -> Select[tuple[T]]:
    raise NotImplementedError

async def second[T](query: Select[tuple[T]]) -> T:
    raise NotImplementedError

async def variant_one() -> int:
    result = await second(first(123))
    return result

async def variant_two() -> int:
    return await second(first(123))
```
