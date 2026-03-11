# Unused awaitable

## Basic coroutine not awaited

Calling an `async def` function produces a coroutine that must be awaited.

```py
async def fetch() -> int:
    return 42

async def main():
    fetch()  # error: [unused-awaitable]
```

## Awaited coroutine is fine

```py
async def fetch() -> int:
    return 42

async def main():
    await fetch()
```

## Assigned coroutine is fine

```py
async def fetch() -> int:
    return 42

async def main():
    # TODO: ty should eventually warn about unused coroutines assigned to variables
    coro = fetch()
```

## Coroutine passed to a function

When a coroutine is passed as an argument rather than used as an expression statement, no diagnostic
should be emitted.

```py
async def fetch() -> int:
    return 42

async def main():
    print(fetch())
```

## Top-level coroutine call

The lint fires even outside of `async def`, since the coroutine is still discarded.

```py
async def fetch() -> int:
    return 42

fetch()  # error: [unused-awaitable]
```

## Union of awaitables

When every element of a union is awaitable, the lint should fire.

```py
from types import CoroutineType
from typing import Any

def get_coroutine() -> CoroutineType[Any, Any, int] | CoroutineType[Any, Any, str]:
    raise NotImplementedError

async def main():
    get_coroutine()  # error: [unused-awaitable]
```

## Union with non-awaitable

When a union contains a non-awaitable element, the lint should not fire.

```py
from types import CoroutineType
from typing import Any

def get_maybe_coroutine() -> CoroutineType[Any, Any, int] | int:
    raise NotImplementedError

async def main():
    get_maybe_coroutine()
```

## Intersection with awaitable

When an intersection type contains an awaitable element, the lint should fire.

```py
from collections.abc import Coroutine
from types import CoroutineType
from ty_extensions import Intersection

class Foo: ...
class Bar: ...

def get_coroutine() -> Intersection[Coroutine[Foo, Foo, Foo], CoroutineType[Bar, Bar, Bar]]:
    raise NotImplementedError

async def main():
    get_coroutine()  # error: [unused-awaitable]
```

## `reveal_type` and `assert_type` are not flagged

Calls to `reveal_type` and `assert_type` should not trigger this lint, even when their argument is
an awaitable.

```py
from typing_extensions import assert_type
from types import CoroutineType
from typing import Any

async def fetch() -> int:
    return 42

async def main():
    reveal_type(fetch())  # revealed: CoroutineType[Any, Any, int]
    assert_type(fetch(), CoroutineType[Any, Any, int])
```

## Non-awaitable expression statement

Regular non-awaitable expression statements should not trigger this lint.

```py
def compute() -> int:
    return 42

def main():
    compute()
```

## Dynamic type

`Any` and `Unknown` types should not trigger the lint.

```py
from typing import Any

def get_any() -> Any:
    return None

async def main():
    get_any()
```
