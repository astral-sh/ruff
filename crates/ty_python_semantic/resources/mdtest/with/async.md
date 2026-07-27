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

## Exception-suppressing async context managers

An asynchronous context manager can suppress an exception when the awaited result of `__aexit__` may
be truthy.

```py
class Suppresses:
    async def __aenter__(self) -> None: ...
    async def __aexit__(self, exc_type, exc_value, traceback) -> bool:
        return True

async def could_raise_returns_str() -> str:
    return "value"

async def main():
    value = 1

    async with Suppresses():
        value = await could_raise_returns_str()

    reveal_type(value)  # revealed: Literal[1] | str
```

## Bindings may be absent after an asynchronously suppressed exception

```py
class Suppresses:
    async def __aenter__(self) -> None: ...
    async def __aexit__(self, exc_type, exc_value, traceback) -> bool:
        return True

async def could_raise_returns_str() -> str:
    return "value"

async def main():
    async with Suppresses():
        value = await could_raise_returns_str()

    value  # error: [possibly-unresolved-reference]
```

## Asynchronously suppressed exceptions do not terminate control flow

```py
class Suppresses:
    async def __aenter__(self) -> None: ...
    async def __aexit__(self, exc_type, exc_value, traceback) -> bool:
        return True

async def main():
    async with Suppresses():
        raise ValueError

    reveal_type("reachable")  # revealed: Literal["reachable"]
```

## Suppressing async context managers are modeled conservatively

As with synchronous managers, an `async with` body is modeled like a `try` suite that can exit at
any point. A bare `return` cannot actually be suppressed, but ty does not prove that individual
statements cannot raise. This conservative missing-return diagnostic matches mypy and Pyright.

```py
from typing import Literal

class Suppresses:
    async def __aenter__(self) -> None: ...
    async def __aexit__(self, exc_type, exc_value, traceback) -> Literal[True]:
        return True

async def could_raise_returns_int() -> int:
    return 1

async def returns_from_body() -> int:  # error: [invalid-return-type]
    async with Suppresses():
        return 1

async def return_expression_can_be_suppressed() -> int:  # error: [invalid-return-type]
    async with Suppresses():
        return await could_raise_returns_int()
```

## Async suppression does not prove that an inner exception escapes

An asynchronous suppressing manager is conservatively allowed to exit before the end of its body,
even if an inner handler catches the exception or the exception appears in an unreachable branch.

```py
class Suppresses:
    async def __aenter__(self) -> None: ...
    async def __aexit__(self, exc_type, exc_value, traceback) -> bool:
        return True

async def caught_by_inner_handler() -> int:  # error: [invalid-return-type]
    async with Suppresses():
        try:
            raise ValueError
        except ValueError:
            return 1

async def statically_unreachable_exception() -> int:  # error: [invalid-return-type]
    async with Suppresses():
        if False:
            raise ValueError
        return 1
```

## Assertions and async iteration can leave through a suppressing manager

Assertions and asynchronous iteration can raise without a visible call expression. The body-wide
model includes those implicit exception sites.

```py
from typing_extensions import Self

class Suppresses:
    async def __aenter__(self) -> None: ...
    async def __aexit__(self, exc_type, exc_value, traceback) -> bool:
        return True

async def suppressed_assertion(condition: bool) -> int:  # error: [invalid-return-type]
    async with Suppresses():
        assert condition
        return 1

class RaisingAsyncIterable:
    def __aiter__(self) -> Self:
        return self

    async def __anext__(self) -> int:
        raise ValueError

async def suppressed_iteration(values: RaisingAsyncIterable) -> int:  # error: [invalid-return-type]
    async with Suppresses():
        async for value in values:
            pass
        return 1
```

## Async literal assignments are conservatively interruptible

An initial binding remains visible even when the only assignment inside the `async with` body has a
literal right-hand side.

```py
class Suppresses:
    async def __aenter__(self) -> None: ...
    async def __aexit__(self, exc_type, exc_value, traceback) -> bool:
        return True

async def main():
    value = 1

    async with Suppresses():
        value = "finished"

    reveal_type(value)  # revealed: Literal[1, "finished"]
```

## Async deletions are not yet recorded as intermediate definitions

The shared `try` snapshot machinery does not record `del` as an intermediate binding. As with
synchronous context managers, a suppressed exception can therefore leave a deleted name incorrectly
visible.

```py
class Suppresses:
    async def __aenter__(self) -> None: ...
    async def __aexit__(self, exc_type, exc_value, traceback) -> bool:
        return True

async def deleted_after_suppression() -> int:
    value = 1

    async with Suppresses():
        del value
        raise ValueError

    # TODO: This should emit [possibly-unresolved-reference].
    return value
```

## Async context manager exception suppression follows the typing specification

The awaited return type of `__aexit__`, rather than the coroutine object, determines whether an
async context manager can suppress an exception.

```py
from typing import Any, Literal
from typing_extensions import assert_type

class Manager:
    async def __aenter__(self) -> None:
        pass

class SuppressBool(Manager):
    async def __aexit__(self, exc_type, exc_value, traceback) -> bool:
        return True

class SuppressTrue(Manager):
    async def __aexit__(self, exc_type, exc_value, traceback) -> Literal[True]:
        return True

class PropagateNone(Manager):
    async def __aexit__(self, exc_type, exc_value, traceback) -> None:
        return None

class PropagateFalse(Manager):
    async def __aexit__(self, exc_type, exc_value, traceback) -> Literal[False]:
        return False

class PropagateAny(Manager):
    async def __aexit__(self, exc_type, exc_value, traceback) -> Any:
        return False

class PropagateOptionalBool(Manager):
    async def __aexit__(self, exc_type, exc_value, traceback) -> bool | None:
        return None

class PropagateOptionalTrue(Manager):
    async def __aexit__(self, exc_type, exc_value, traceback) -> Literal[True] | None:
        return None

async def suppress_bool(value: int | str) -> None:
    if isinstance(value, int):
        async with SuppressBool():
            raise ValueError
    assert_type(value, int | str)

async def suppress_true(value: int | str) -> None:
    if isinstance(value, int):
        async with SuppressTrue():
            raise ValueError
    assert_type(value, int | str)

async def propagate_none(value: int | str) -> None:
    if isinstance(value, int):
        async with PropagateNone():
            raise ValueError
    assert_type(value, str)

async def propagate_false(value: int | str) -> None:
    if isinstance(value, int):
        async with PropagateFalse():
            raise ValueError
    assert_type(value, str)

async def propagate_any(value: int | str) -> None:
    if isinstance(value, int):
        async with PropagateAny():
            raise ValueError
    assert_type(value, str)

async def propagate_optional_bool(value: int | str) -> None:
    if isinstance(value, int):
        async with PropagateOptionalBool():
            raise ValueError
    assert_type(value, str)

async def propagate_optional_true(value: int | str) -> None:
    if isinstance(value, int):
        async with PropagateOptionalTrue():
            raise ValueError
    assert_type(value, str)
```

## Always-falsy async exit methods preserve precise bindings

```py
from typing import Literal

class NoneExit:
    async def __aenter__(self) -> None: ...
    async def __aexit__(self, exc_type, exc_value, traceback) -> None: ...

class FalseExit:
    async def __aenter__(self) -> None: ...
    async def __aexit__(self, exc_type, exc_value, traceback) -> Literal[False]:
        return False

async def main():
    none_value = 1
    async with NoneExit():
        none_value = "finished"

    reveal_type(none_value)  # revealed: Literal["finished"]

    false_value = 1
    async with FalseExit():
        false_value = "finished"

    reveal_type(false_value)  # revealed: Literal["finished"]
```

## Async exit overloads are conservatively combined

The awaited return types of all `__aexit__` overloads are considered. A truthy normal-exit overload
is enough to make the manager potentially suppressing even when the exceptional overload returns
`Literal[False]`; selecting the applicable exceptional overload is a known precision limitation.

```py
from types import TracebackType
from typing import Literal, overload

class NormalOnlyTruthyExit:
    async def __aenter__(self) -> None: ...
    @overload
    async def __aexit__(
        self,
        exc_type: None,
        exc_value: None,
        traceback: None,
    ) -> Literal[True]: ...
    @overload
    async def __aexit__(
        self,
        exc_type: type[BaseException],
        exc_value: BaseException,
        traceback: TracebackType | None,
    ) -> Literal[False]: ...
    async def __aexit__(
        self,
        exc_type: type[BaseException] | None,
        exc_value: BaseException | None,
        traceback: TracebackType | None,
    ) -> bool:
        return exc_type is None

async def could_raise_returns_str() -> str:
    return "finished"

async def main():
    value = 1

    async with NormalOnlyTruthyExit():
        value = await could_raise_returns_str()

    reveal_type(value)  # revealed: Literal[1] | str
```

## Earlier async context managers can suppress later entry failures

```py
from typing import Literal

class Suppresses:
    async def __aenter__(self) -> None: ...
    async def __aexit__(self, exc_type, exc_value, traceback) -> bool:
        return True

class Inner:
    async def __aenter__(self) -> str:
        return "value"

    async def __aexit__(self, exc_type, exc_value, traceback) -> Literal[False]:
        return False

async def main():
    async with Suppresses(), Inner() as value:
        pass

    value  # error: [possibly-unresolved-reference]
```

## Union async context managers combine awaited exit return types

The awaited `__aexit__` return types of alternative managers must be combined before applying the
typing specification. A conditional union of a `bool`-returning async manager and `nullcontext`
therefore has an effective return type of `bool | None` and is non-suppressing.

```py
from contextlib import nullcontext
from typing_extensions import assert_type

class Suppresses:
    async def __aenter__(self) -> None: ...
    async def __aexit__(self, exc_type, exc_value, traceback) -> bool:
        return True

async def conditional_return(flag: bool) -> int:
    manager = Suppresses() if flag else nullcontext()
    async with manager:
        return 1

async def reversed_conditional_return(flag: bool) -> int:
    manager = nullcontext() if flag else Suppresses()
    async with manager:
        return 1

async def explicit_union_return(manager: Suppresses | nullcontext[None]) -> int:
    async with manager:
        return 1

async def conditional_narrowing(flag: bool, value: int | str) -> None:
    if isinstance(value, int):
        manager = Suppresses() if flag else nullcontext()
        async with manager:
            raise ValueError

    assert_type(value, str)
```

## Falsy async union alternatives do not change a boolean exit type

After awaiting, `bool | Literal[False]` simplifies to `bool`, so a union of these async manager
alternatives remains potentially suppressing.

```py
from typing import Literal
from typing_extensions import assert_type

class Suppresses:
    async def __aenter__(self) -> None: ...
    async def __aexit__(self, exc_type, exc_value, traceback) -> bool:
        return True

class FalseExit:
    async def __aenter__(self) -> None: ...
    async def __aexit__(self, exc_type, exc_value, traceback) -> Literal[False]:
        return False

async def conditional_narrowing(flag: bool, value: int | str) -> None:
    if isinstance(value, int):
        manager = Suppresses() if flag else FalseExit()
        async with manager:
            raise ValueError

    assert_type(value, int | str)
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
