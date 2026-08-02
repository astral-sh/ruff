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
        pass
```

## Context manager without an `__aenter__` method

```py
class Manager:
    async def __aexit__(self, exc_tpe, exc_value, traceback): ...

async def main():
    # error: [invalid-context-manager] "Object of type `Manager` cannot be used with `async with` because it does not implement `__aenter__`"
    async with Manager():
        pass
```

## Context manager without an `__aexit__` method

```py
class Manager:
    async def __aenter__(self): ...

async def main():
    # error: [invalid-context-manager] "Object of type `Manager` cannot be used with `async with` because it does not implement `__aexit__`"
    async with Manager():
        pass
```

## Context manager with non-callable `__aenter__` attribute

```py
class Manager:
    __aenter__: int = 42

    async def __aexit__(self, exc_tpe, exc_value, traceback): ...

async def main():
    # error: [invalid-context-manager] "Object of type `Manager` cannot be used with `async with` because it does not correctly implement `__aenter__`"
    async with Manager():
        pass
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
        pass
```

## Context expression with possibly-unbound union variants

<!-- snapshot-diagnostics -->

```py
class Manager1:
    async def __aenter__(self) -> str:
        return "foo"

    async def __aexit__(self, exc_type, exc_value, traceback): ...

class NotAContextManager: ...

async def _(context_expr: Manager1 | NotAContextManager):
    # error: [invalid-context-manager] "Object of type `Manager1 | NotAContextManager` cannot be used with `async with` because the methods `__aenter__` and `__aexit__` are possibly missing"
    async with context_expr as f:
        reveal_type(f)  # revealed: str
```

## Possibly unbound non-awaitable context-manager methods

When a union member lacks the async context-manager methods, the methods on the remaining member
must still return awaitables:

```py
class Invalid:
    def __aenter__(self) -> int:
        return 0

    def __aexit__(self, exc_type, exc, tb) -> bool:
        return False

class Missing: ...

async def main(manager: Invalid | Missing):
    # snapshot: invalid-context-manager
    async with manager as value:
        reveal_type(value)  # revealed: Unknown
```

```snapshot
error[invalid-context-manager]: Object of type `Invalid | Missing` cannot be used with `async with` because `__aenter__` and `__aexit__` may be missing or return non-awaitables
  --> src/mdtest_snippet.py:12:16
   |
12 |     async with manager as value:
   |                ^^^^^^^
info: `Missing` does not implement `__aenter__` or `__aexit__`
info: `__aenter__` returns `int`, which is not awaitable
info: `__aexit__` returns `bool`, which is not awaitable
info: Consider declaring the methods with `async def`
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
        reveal_type(f)  # revealed: str
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

If a asynchronous `async with` statement is used on a type with `__enter__` and `__exit__`, we show
a diagnostic hint that the user might have intended to use `with` instead.

```py
class Manager:
    def __enter__(self): ...
    def __exit__(self, *args): ...

async def main():
    # snapshot: invalid-context-manager
    async with Manager():
        pass
```

```snapshot
error[invalid-context-manager]: Object of type `Manager` cannot be used with `async with` because it does not implement `__aenter__` and `__aexit__`
 --> src/mdtest_snippet.py:7:16
  |
7 |     async with Manager():
  |                ^^^^^^^^^
info: Objects of type `Manager` can be used as sync context managers
info: Consider using `with` here
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
        pass
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
        pass
```

## Non-awaitable `__aenter__`

`async with` awaits whatever `__aenter__` returns, so a method that is callable but returns a
non-awaitable fails at runtime with
`TypeError: 'async with' received an object from __aenter__ that does not implement __await__: int`:

```py
class Manager:
    def __aenter__(self) -> int:
        return 0

    async def __aexit__(self, exc_type, exc, tb) -> None: ...

async def main():
    # snapshot: invalid-context-manager
    async with Manager():
        pass
```

```snapshot
error[invalid-context-manager]: Object of type `Manager` cannot be used with `async with` because `__aenter__` does not return an awaitable
 --> src/mdtest_snippet.py:9:16
  |
9 |     async with Manager():
  |                ^^^^^^^^^
info: `__aenter__` returns `int`, which is not awaitable
info: Consider declaring the method with `async def`
```

## Non-awaitable `__aexit__`

The same applies on the way out. Here `__aenter__` is correct, so the target still binds, but
leaving the block would await a `bool`:

```py
class Manager:
    async def __aenter__(self) -> int:
        return 0

    def __aexit__(self, exc_type, exc, tb) -> bool:
        return False

async def main():
    # error: [invalid-context-manager] "Object of type `Manager` cannot be used with `async with` because `__aexit__` does not return an awaitable"
    async with Manager() as value:
        reveal_type(value)  # revealed: int
```

## Non-awaitable `__aenter__` with missing `__aexit__`

A missing exit method does not excuse an entry method that returns a non-awaitable:

```py
class Manager:
    def __aenter__(self) -> int:
        return 0

async def main():
    # snapshot: invalid-context-manager
    async with Manager():
        pass
```

```snapshot
error[invalid-context-manager]: Object of type `Manager` cannot be used with `async with` because it does not implement `__aexit__`, and `__aenter__` does not return an awaitable
 --> src/mdtest_snippet.py:7:16
  |
7 |     async with Manager():
  |                ^^^^^^^^^
info: `__aenter__` returns `int`, which is not awaitable
info: Consider declaring the method with `async def`
```

## Missing `__aenter__` with non-awaitable `__aexit__`

An exit method must return an awaitable even when the entry method is missing:

```py
class Manager:
    def __aexit__(self, exc_type, exc, tb) -> bool:
        return False

async def main():
    # snapshot: invalid-context-manager
    async with Manager():
        pass
```

```snapshot
error[invalid-context-manager]: Object of type `Manager` cannot be used with `async with` because it does not implement `__aenter__`, and `__aexit__` does not return an awaitable
 --> src/mdtest_snippet.py:7:16
  |
7 |     async with Manager():
  |                ^^^^^^^^^
info: `__aexit__` returns `bool`, which is not awaitable
info: Consider declaring the method with `async def`
```

## Both methods non-awaitable

When neither method returns an awaitable, both are named in a single diagnostic:

```py
class Manager:
    def __aenter__(self) -> int:
        return 0

    def __aexit__(self, exc_type, exc, tb) -> bool:
        return False

async def main():
    # error: [invalid-context-manager] "Object of type `Manager` cannot be used with `async with` because `__aenter__` and `__aexit__` do not return awaitables"
    async with Manager():
        pass
```

## Awaitable returns that are not `async def`

A method does not have to be `async` to satisfy `async with`; it only has to return something
awaitable. None of these are errors:

```py
from typing import Any, Awaitable, Coroutine, Generator

class ReturnsAwaitable:
    def __aenter__(self) -> Awaitable[int]:
        raise NotImplementedError

    def __aexit__(self, exc_type, exc, tb) -> Awaitable[None]:
        raise NotImplementedError

class Custom:
    def __await__(self) -> Generator[Any, None, int]:
        raise NotImplementedError

class ReturnsCustomAwaitable:
    def __aenter__(self) -> Custom:
        raise NotImplementedError

    def __aexit__(self, exc_type, exc, tb) -> Custom:
        raise NotImplementedError

class ReturnsCoroutine:
    def __aenter__(self) -> Coroutine[Any, Any, int]:
        raise NotImplementedError

    def __aexit__(self, exc_type, exc, tb) -> Coroutine[Any, Any, None]:
        raise NotImplementedError

async def main():
    async with ReturnsAwaitable() as a:
        reveal_type(a)  # revealed: int
    async with ReturnsCustomAwaitable() as b:
        reveal_type(b)  # revealed: int
    async with ReturnsCoroutine() as c:
        reveal_type(c)  # revealed: int
```

A union return type is awaitable when every member is, and `Never` is vacuously awaitable:

```py
from typing import Awaitable
from typing_extensions import Never

class UnionOfAwaitables:
    def __aenter__(self) -> Awaitable[int] | Awaitable[str]:
        raise NotImplementedError

    def __aexit__(self, exc_type, exc, tb) -> Awaitable[None]:
        raise NotImplementedError

class NeverReturns:
    def __aenter__(self) -> Never:
        raise NotImplementedError

    async def __aexit__(self, exc_type, exc, tb) -> None: ...

async def main():
    async with UnionOfAwaitables() as a:
        reveal_type(a)  # revealed: int | str
    async with NeverReturns() as b:
        reveal_type(b)  # revealed: Never
```

A union is only awaitable if every member is, so mixing an awaitable with a plain value is still an
error:

```py
from typing import Awaitable

class Manager:
    def __aenter__(self) -> int | Awaitable[int]:
        raise NotImplementedError

    async def __aexit__(self, exc_type, exc, tb) -> None: ...

async def main():
    # error: [invalid-context-manager] "Object of type `Manager` cannot be used with `async with` because `__aenter__` does not return an awaitable"
    async with Manager():
        pass
```

A method whose return type is not known does not produce a diagnostic either:

```py
from typing import Any

class ReturnsAny:
    def __aenter__(self) -> Any: ...
    def __aexit__(self, exc_type, exc, tb) -> Any: ...

class Unannotated:
    def __aenter__(self): ...
    def __aexit__(self, exc_type, exc, tb): ...

async def main():
    async with ReturnsAny() as a:
        reveal_type(a)  # revealed: Any
    async with Unannotated() as b:
        reveal_type(b)  # revealed: Unknown
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

Generic type parameters are preserved through the decorator:

Regression test for <https://github.com/astral-sh/ty/issues/3692>.

```toml
[environment]
python-version = "3.12"
```

```py
from collections.abc import AsyncGenerator, AsyncIterator
from contextlib import asynccontextmanager

@asynccontextmanager
async def nullcontext[T](value: T) -> AsyncGenerator[T, None]:
    yield value

# revealed: [T](value: T) -> _AsyncGeneratorContextManager[T, None]
reveal_type(nullcontext)

async def gen() -> AsyncIterator[str]:
    yield "hello"

async def main_generic():
    async with nullcontext(gen()) as lines:
        reveal_type(lines)  # revealed: AsyncIterator[str]
        async for line in lines:
            reveal_type(line)  # revealed: str
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
