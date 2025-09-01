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

## Union context managers with specific member issues

### Union where one member lacks `__aenter__`

```py
async def _(flag: bool):
    class Bound:
        async def __aenter__(self) -> str:
            return "foo"

        async def __aexit__(self, exc_type, exc_value, traceback): ...

    class EnterUnbound:
        async def __aexit__(self): ...

    context_expr = Bound() if flag else EnterUnbound()

    # error: [invalid-context-manager] "Object of type `Bound | EnterUnbound` cannot be used with `async with` because the method `__aenter__` of `EnterUnbound` is possibly unbound"
    async with context_expr as f:
        reveal_type(f)  # revealed: CoroutineType[Any, Any, str]
```

### Union where one member lacks `__aexit__`

```py
async def _(flag: bool):
    class Bound:
        async def __aenter__(self) -> str:
            return "foo"

        async def __aexit__(self, exc_type, exc_value, traceback): ...

    class ExitUnbound:
        async def __aenter__(self): ...

    context_expr = Bound() if flag else ExitUnbound()

    # error: [invalid-context-manager] "Object of type `Bound | ExitUnbound` cannot be used with `async with` because the method `__aexit__` of `ExitUnbound` is possibly unbound"
    async with context_expr as f:
        reveal_type(f)  # revealed: str | Unknown
```

### Union where one member lacks both methods

```py
async def _(flag: bool):
    class Bound:
        async def __aenter__(self) -> str:
            return "foo"

        async def __aexit__(self, exc_type, exc_value, traceback): ...

    class Unbound: ...
    context_expr = Bound() if flag else Unbound()

    # error: [invalid-context-manager] "Object of type `Bound | Unbound` cannot be used with `async with` because the methods `__aenter__` and `__aexit__` of `Unbound` are possibly unbound"
    async with context_expr as f:
        reveal_type(f)  # revealed: CoroutineType[Any, Any, str]
```

### Complex union with multiple issues

```py
async def _(flag: int):
    class Bound:
        async def __aenter__(self) -> str:
            return "foo"

        async def __aexit__(self, exc_type, exc_value, traceback): ...

    class EnterUnbound:
        async def __aexit__(self): ...

    class ExitUnbound:
        async def __aenter__(self): ...

    if flag == 0:
        context_expr = Bound()
    elif flag == 1:
        context_expr = EnterUnbound()
    else:
        context_expr = ExitUnbound()

    # error: [invalid-context-manager] "Object of type `Bound | EnterUnbound | ExitUnbound` cannot be used with `async with` because the method `__aenter__` of `EnterUnbound` is possibly unbound, and the method `__aexit__` of `ExitUnbound` is possibly unbound"
    async with context_expr as f:
        reveal_type(f)  # revealed: CoroutineType[Any, Any, str] | Unknown
```

### Union with multiple members missing the same methods

```py
async def _(flag: int):
    class EnterUnbound:
        async def __aexit__(self): ...

    class ExitUnbound:
        async def __aenter__(self): ...

    class Unbound: ...
    if flag == 0:
        context_expr = EnterUnbound()
    elif flag == 1:
        context_expr = ExitUnbound()
    else:
        context_expr = Unbound()

    # error: [invalid-context-manager] "Object of type `EnterUnbound | ExitUnbound | Unbound` cannot be used with `async with` because the method `__aenter__` of `EnterUnbound` and `Unbound` are possibly unbound, and the method `__aexit__` of `ExitUnbound` and `Unbound` are possibly unbound"
    async with context_expr:
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

    # error: [invalid-context-manager] "Object of type `Manager1 | NotAContextManager` cannot be used with `async with` because the methods `__aenter__` and `__aexit__` of `NotAContextManager` are possibly unbound"
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

    # error: [invalid-context-manager] "Object of type `Manager` cannot be used with `async with` because the method `__aenter__` is possibly unbound"
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

# TODO: this should be `() -> _AsyncGeneratorContextManager[Session, None]`
reveal_type(connect)  # revealed: (...) -> _AsyncGeneratorContextManager[Unknown, None]

async def main():
    async with connect() as session:
        # TODO: should be `Session`
        reveal_type(session)  # revealed: Unknown
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
        # TODO: should be `TaskGroup`
        reveal_type(tg)  # revealed: Unknown

        tg.create_task(long_running_task())
```
