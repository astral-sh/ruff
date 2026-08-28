# Async with statements

## Basic `async with` statement

An `async with` statement awaits the return value of `__aenter__` and binds the result.

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

## Exception-suppressing async context managers and union aliases

An asynchronous context manager can suppress exceptions if its `__aexit__` method returns `bool`:

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Literal

class Suppresses:
    async def __aenter__(self) -> None: ...
    async def __aexit__(self, exc_type, exc_value, traceback) -> bool:
        return True

async def may_raise() -> str:
    raise ValueError

async def preserved_binding() -> None:
    result = None
    async with Suppresses():
        result = await may_raise()
    reveal_type(result)  # revealed: None | str
```

If an exception interrupts an assignment to a new name, that name may remain undefined:

```py
async def missing_binding() -> None:
    async with Suppresses():
        value = await may_raise()
    # error: [possibly-unresolved-reference]
    reveal_type(value)  # revealed: str
```

An `__aexit__` return type of `None` does not suppress exceptions:

```py
class Propagates:
    async def __aenter__(self) -> None: ...
    async def __aexit__(self, exc_type, exc_value, traceback) -> None: ...

async def propagating_exit() -> None:
    result = None
    async with Propagates():
        result = await may_raise()
    reveal_type(result)  # revealed: str
```

[The typing specification](https://typing.python.org/en/latest/spec/exceptions.html#context-managers)
treats an awaited `Literal[True] | None` return type as non-suppressing, even though a truthy return
value would suppress an exception at runtime:

```py
class OptionalTrueExit:
    async def __aenter__(self) -> None: ...
    async def __aexit__(self, exc_type, exc_value, traceback) -> Literal[True] | None:
        return True

async def optional_true_exit() -> None:
    result = None
    async with OptionalTrueExit():
        result = await may_raise()
    reveal_type(result)  # revealed: str
```

A PEP 695 alias does not prevent a suppressing union member from preserving an earlier binding:

```py
type Managers = Suppresses | Propagates

async def preserved_union_binding(manager: Managers) -> None:
    result = None
    async with manager:
        result = await may_raise()
    reveal_type(result)  # revealed: None | str
```

A suppressed exception can also leave a new binding undefined:

```py
async def missing_union_binding(manager: Managers) -> None:
    async with manager:
        result = await may_raise()
    # error: [possibly-unresolved-reference]
    reveal_type(result)  # revealed: str
```

## Earlier async context managers can suppress later entry failures

If an earlier async context manager suppresses an exception while a later manager enters, the later
manager's target may never be assigned:

```py
class Suppresses:
    async def __aenter__(self) -> None: ...
    async def __aexit__(self, exc_type, exc_value, traceback) -> bool:
        return True

class EnterFails:
    async def __aenter__(self) -> str:
        raise ValueError

    async def __aexit__(self, exc_type, exc_value, traceback) -> None: ...

async def later_entry_fails() -> None:
    async with Suppresses(), EnterFails() as target:
        pass
    # error: [possibly-unresolved-reference]
    reveal_type(target)  # revealed: str
```

## Returning from an exception-suppressing async context manager

A context manager cannot suppress a return statement:

```py
class Suppresses:
    async def __aenter__(self) -> None: ...
    async def __aexit__(self, exc_type, exc_value, traceback) -> bool:
        return True

async def bare_return() -> str:
    async with Suppresses():
        return "finished"
```

An exception raised while evaluating an awaited return expression can be suppressed instead:

```py
async def may_raise() -> str:
    raise ValueError

async def interrupted_return() -> str:  # error: [invalid-return-type]
    async with Suppresses():
        return await may_raise()
```

## Overloaded async context manager exit methods

An overloaded async exit method can distinguish normal exits from exceptions:

```py
from typing import Awaitable, Literal, overload
from typing_extensions import Never

async def may_raise() -> str:
    raise ValueError
```

An overload returning `True` only on a normal exit cannot suppress an exception:

```py
class NormalExitOnly:
    async def __aenter__(self) -> None: ...
    @overload
    async def __aexit__(self, exc_type: None, exc_value, traceback) -> Literal[True]: ...
    @overload
    async def __aexit__(self, exc_type: type[BaseException], exc_value, traceback) -> Literal[False]: ...
    async def __aexit__(self, exc_type, exc_value, traceback) -> bool:
        return exc_type is None

async def normal_exit_only() -> None:
    result = None
    async with NormalExitOnly():
        result = await may_raise()
    reveal_type(result)  # revealed: str
```

Of the following three overloads, the second applies when an exception is raised, and the third
applies when the suite exits without an exception. The first overload never applies because its
exception argument is `Never`:

```py
class NeverExit:
    async def __aenter__(self) -> None: ...
    @overload
    async def __aexit__(self, exc_type: Never, exc_value, traceback) -> Literal[True]: ...
    @overload
    async def __aexit__(self, exc_type: type[BaseException], exc_value, traceback) -> Literal[False]: ...
    @overload
    async def __aexit__(self, exc_type: None, exc_value, traceback) -> Literal[False]: ...
    async def __aexit__(self, exc_type, exc_value, traceback) -> bool:
        return False

async def impossible_exit() -> None:
    result = None
    async with NeverExit():
        result = await may_raise()
    reveal_type(result)  # revealed: str
```

An exceptional overload can suppress its exception even if another exceptional overload cannot:

```py
class SuppressesValueError:
    async def __aenter__(self) -> None: ...
    @overload
    async def __aexit__(self, exc_type: type[ValueError], exc_value: ValueError, traceback: object) -> Literal[True]: ...
    @overload
    async def __aexit__(self, exc_type: type[TypeError], exc_value: TypeError, traceback: object) -> None: ...
    @overload
    async def __aexit__(self, exc_type: None, exc_value: None, traceback: None) -> None: ...
    async def __aexit__(self, exc_type, exc_value, traceback) -> Literal[True] | None:
        return True if exc_type is ValueError else None

async def mixed_exceptional_exits() -> None:
    result = None
    async with SuppressesValueError():
        result = await may_raise()
    reveal_type(result)  # revealed: None | str
```

An exceptional overload that returns a non-awaitable does not prevent a later awaitable overload
from suppressing a different exception:

```py
class SuppressesAfterNonAwaitable:
    async def __aenter__(self) -> None: ...
    @overload
    def __aexit__(self, exc_type: type[TypeError], exc_value: TypeError, traceback: object) -> bool: ...
    @overload
    def __aexit__(self, exc_type: type[ValueError], exc_value: ValueError, traceback: object) -> Awaitable[Literal[True]]: ...
    @overload
    def __aexit__(self, exc_type: None, exc_value: None, traceback: None) -> Awaitable[None]: ...
    def __aexit__(self, exc_type, exc_value, traceback) -> bool | Awaitable[Literal[True]] | Awaitable[None]:
        raise NotImplementedError

async def suppresses_after_non_awaitable() -> None:
    result = None
    async with SuppressesAfterNonAwaitable():
        result = await may_raise()
    reveal_type(result)  # revealed: None | str
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

A union can contain a valid context manager and an object with no context-manager methods. The valid
manager still determines the type of the value bound by `async with`.

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

## Missing and non-awaitable methods in a union

If one member of a union does not define the context-manager methods, still check the return values
of the methods defined on the other member.

```py
class Manager:
    def __aenter__(self) -> int:
        return 0

    def __aexit__(self, exc_type, exc, tb) -> bool:
        return False

class NotAManager: ...

async def main(manager: Manager | NotAManager):
    # snapshot: invalid-context-manager
    async with manager as value:
        reveal_type(value)  # revealed: Unknown
```

```snapshot
error[invalid-context-manager]: Object of type `Manager | NotAManager` cannot be used with `async with` because `__aenter__` and `__aexit__` may be missing or return non-awaitables
  --> src/mdtest_snippet.py:12:16
   |
12 |     async with manager as value:
   |                ^^^^^^^
info: `NotAManager` does not implement `__aenter__` or `__aexit__`
info: `__aenter__` returns `int`, which is not awaitable
info: `__aexit__` returns `bool`, which is not awaitable
info: Consider declaring the methods with `async def`
```

## Conditionally defined `__aenter__` method

A conditionally defined `__aenter__` method may be missing. When it exists, its awaited return type
still determines the type of the bound value.

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

## Synchronous context manager in `async with`

An object that only defines `__enter__` and `__exit__` cannot be used with `async with`. Suggest
using `with` instead.

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

`async with` awaits the value returned by `__aenter__`. Returning an `int` therefore raises a
`TypeError`.

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

`async with` also awaits the value returned by `__aexit__`. The value from `__aenter__` is still
bound before the invalid exit method runs.

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

## Non-awaitable `__aenter__` and `__aexit__`

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

## Awaitable return from a regular method

A context-manager method does not need to be declared with `async def`. A regular method can return
an `Awaitable` instead.

```py
from typing import Awaitable

class Manager:
    def __aenter__(self) -> Awaitable[int]:
        raise NotImplementedError

    def __aexit__(self, exc_type, exc, tb) -> Awaitable[None]:
        raise NotImplementedError

async def main():
    async with Manager() as value:
        reveal_type(value)  # revealed: int
```

## Awaitable return from a custom `__await__` method

An object is awaitable when its `__await__` method returns an iterator.

```py
from typing import Generator

class AwaitableValue:
    def __await__(self) -> Generator[None, None, int]:
        raise NotImplementedError

class Manager:
    def __aenter__(self) -> AwaitableValue:
        raise NotImplementedError

    def __aexit__(self, exc_type, exc, tb) -> AwaitableValue:
        raise NotImplementedError

async def main():
    async with Manager() as value:
        reveal_type(value)  # revealed: int
```

## Union of awaitable return types

When every possible return value is awaitable, the bound value includes the awaited result from each
union member.

```py
from typing import Awaitable

class Manager:
    def __aenter__(self) -> Awaitable[int] | Awaitable[str]:
        raise NotImplementedError

    def __aexit__(self, exc_type, exc, tb) -> Awaitable[None]:
        raise NotImplementedError

async def main():
    async with Manager() as value:
        reveal_type(value)  # revealed: int | str
```

## Union containing a non-awaitable return type

Every possible return value must be awaitable. A union containing `int` does not satisfy that
requirement.

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

## `Any` return type

A return type of `Any` might be awaitable, so it must not produce an error.

```py
from typing import Any

class Manager:
    def __aenter__(self) -> Any: ...
    def __aexit__(self, exc_type, exc, tb) -> Any: ...

async def main():
    async with Manager():
        pass
```

## Unknown return type

An unannotated return type might also be awaitable, so it must not produce an error.

```py
class Manager:
    def __aenter__(self): ...
    def __aexit__(self, exc_type, exc, tb): ...

async def main():
    async with Manager():
        pass
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
