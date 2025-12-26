# Async with statements

## Basic `async with` statement

The type of the target variable in a `with` statement should be the return type from the context
manager's `__aenter__` method. However, `async with` statements aren't supported yet. This test
asserts that it doesn't emit any context manager-related errors.

```py
class Target: ...

class Manager:
    async def __aenter__(self) -> Target:
        return Target()

    async def __aexit__(self, exc_type, exc_value, traceback): ...

async def test():
    async with Manager() as f:
        reveal_type(f)  # revealed: Target
```

## Multiple targets

```py
class Manager:
    async def __aenter__(self) -> tuple[int, str]:
        return 42, "hello"

    async def __aexit__(self, exc_type, exc_value, traceback): ...

async def test():
    async with Manager() as (x, y):
        reveal_type(x)  # revealed: int
        reveal_type(y)  # revealed: str
```

## Context manager without an `__aenter__` or `__aexit__` method

```py
class Manager: ...

async def main():
    # error: [invalid-context-manager] "Object of type `Manager` cannot be used with `async with` because it does not implement `__aenter__` and `__aexit__`"
    async with Manager():
        ...
```

## Context manager without an `__aenter__` method

```py
class Manager:
    async def __aexit__(self, exc_tpe, exc_value, traceback): ...

async def main():
    # error: [invalid-context-manager] "Object of type `Manager` cannot be used with `async with` because it does not implement `__aenter__`"
    async with Manager():
        ...
```

## Context manager without an `__aexit__` method

```py
class Manager:
    async def __aenter__(self): ...

async def main():
    # error: [invalid-context-manager] "Object of type `Manager` cannot be used with `async with` because it does not implement `__aexit__`"
    async with Manager():
        ...
```

## Context manager with non-callable `__aenter__` attribute

```py
class Manager:
    __aenter__: int = 42

    async def __aexit__(self, exc_tpe, exc_value, traceback): ...

async def main():
    # error: [invalid-context-manager] "Object of type `Manager` cannot be used with `async with` because it does not correctly implement `__aenter__`"
    async with Manager():
        ...
```

## Context manager with non-callable `__aexit__` attribute

```py
from typing_extensions import Self

class Manager:
    def __aenter__(self) -> Self:
        return self
    __aexit__: int = 32

async def main():
    # error: [invalid-context-manager] "Object of type `Manager` cannot be used with `async with` because it does not correctly implement `__aexit__`"
    async with Manager():
        ...
```

## Context expression with possibly-unbound union variants

```py
async def _(flag: bool):
    class Manager1:
        def __aenter__(self) -> str:
            return "foo"

        def __aexit__(self, exc_type, exc_value, traceback): ...

    class NotAContextManager: ...
    context_expr = Manager1() if flag else NotAContextManager()

    # error: [invalid-context-manager] "Object of type `Manager1 | NotAContextManager` cannot be used with `async with` because the methods `__aenter__` and `__aexit__` are possibly missing"
    async with context_expr as f:
        reveal_type(f)  # revealed: str
```

## Context expression with "sometimes" callable `__aenter__` method

```py
async def _(flag: bool):
    class Manager:
        if flag:
            async def __aenter__(self) -> str:
                return "abcd"

        async def __exit__(self, *args): ...

    # error: [invalid-context-manager] "Object of type `Manager` cannot be used with `async with` because the method `__aenter__` may be missing"
    async with Manager() as f:
        reveal_type(f)  # revealed: CoroutineType[Any, Any, str]
```

## Invalid `__aenter__` signature

```py
class Manager:
    async def __aenter__() -> str:
        return "foo"

    async def __aexit__(self, exc_type, exc_value, traceback): ...

async def main():
    context_expr = Manager()

    # error: [invalid-context-manager] "Object of type `Manager` cannot be used with `async with` because it does not correctly implement `__aenter__`"
    async with context_expr as f:
        reveal_type(f)  # revealed: CoroutineType[Any, Any, str]
```

## Accidental use of async `async with`

<!-- snapshot-diagnostics -->

If a asynchronous `async with` statement is used on a type with `__enter__` and `__exit__`, we show
a diagnostic hint that the user might have intended to use `with` instead.

```py
class Manager:
    def __enter__(self): ...
    def __exit__(self, *args): ...

async def main():
    # error: [invalid-context-manager] "Object of type `Manager` cannot be used with `async with` because it does not implement `__aenter__` and `__aexit__`"
    async with Manager():
        ...
```

## Incorrect signatures

The sub-diagnostic is also provided if the signatures of `__enter__` and `__exit__` do not match the
expected signatures for a context manager:

```py
class Manager:
    def __enter__(self): ...
    def __exit__(self, typ: str, exc, traceback): ...

async def main():
    # error: [invalid-context-manager] "Object of type `Manager` cannot be used with `async with` because it does not implement `__aenter__` and `__aexit__`"
    async with Manager():
        ...
```

## Incorrect number of arguments

Similarly, we also show the hint if the functions have the wrong number of arguments:

```py
class Manager:
    def __enter__(self, wrong_extra_arg): ...
    def __exit__(self, typ, exc, traceback, wrong_extra_arg): ...

async def main():
    # error: [invalid-context-manager] "Object of type `Manager` cannot be used with `async with` because it does not implement `__aenter__` and `__aexit__`"
    async with Manager():
        ...
```

## `@asynccontextmanager`

```py
from contextlib import asynccontextmanager
from typing import AsyncGenerator

class Session: ...

@asynccontextmanager
async def connect() -> AsyncGenerator[Session]:
    yield Session()

# revealed: () -> _AsyncGeneratorContextManager[Session, None]
reveal_type(connect)

async def main():
    async with connect() as session:
        reveal_type(session)  # revealed: Session
```

This also works with `AsyncIterator` return types:

```py
from typing import AsyncIterator

@asynccontextmanager
async def connect_iterator() -> AsyncIterator[Session]:
    yield Session()

# revealed: () -> _AsyncGeneratorContextManager[Session, None]
reveal_type(connect_iterator)

async def main_iterator():
    async with connect_iterator() as session:
        reveal_type(session)  # revealed: Session
```

And with `AsyncGeneratorType` return types:

```py
from types import AsyncGeneratorType

@asynccontextmanager
async def connect_async_generator() -> AsyncGeneratorType[Session]:
    yield Session()

# revealed: () -> _AsyncGeneratorContextManager[Session, None]
reveal_type(connect_async_generator)

async def main_async_generator():
    async with connect_async_generator() as session:
        reveal_type(session)  # revealed: Session
```

## `asyncio.timeout`

```toml
[environment]
python-version = "3.11"
```

```py
import asyncio

async def long_running_task():
    await asyncio.sleep(5)

async def main():
    async with asyncio.timeout(1):
        await long_running_task()
```

## `asyncio.TaskGroup`

```toml
[environment]
python-version = "3.11"
```

```py
import asyncio

async def long_running_task():
    await asyncio.sleep(5)

async def main():
    async with asyncio.TaskGroup() as tg:
        reveal_type(tg)  # revealed: TaskGroup

        tg.create_task(long_running_task())
```
