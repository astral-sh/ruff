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

## Leading async literal assignments are not interrupted

An assignment of a literal to a name cannot raise a suppressible exception before the new binding is
established, so the previous binding is not visible after the async context manager.

```py
class Suppresses:
    async def __aenter__(self) -> None: ...
    async def __aexit__(self, exc_type, exc_value, traceback) -> bool:
        return True

async def main():
    value = 1

    async with Suppresses():
        value = "finished"

    reveal_type(value)  # revealed: Literal["finished"]
```

## Async literal initializers before a suppressible operation remain defined

Leading literal assignments are complete before a later awaited operation can raise an exception
that the async context manager suppresses.

```py
from typing import Literal
from typing_extensions import assert_type

class Suppresses:
    async def __aenter__(self) -> None: ...
    async def __aexit__(self, exc_type, exc_value, traceback) -> bool:
        return True

async def could_raise() -> None:
    raise ValueError

async def literal_initializers() -> None:
    async with Suppresses():
        x_values = [1, 2, 4]
        y_values = [0.5, 0.8]
        reservoir: list[int] = []
        first, second = (1, 2)
        await could_raise()

    assert_type(x_values, list[int])
    assert_type(y_values, list[float])
    assert_type(reservoir, list[int])
    assert_type(first, Literal[1])
    assert_type(second, Literal[2])
```

## Async exception-assertion managers retain their setup assignments

The exception-assertion name heuristic also applies when the manager implements the async
context-manager protocol.

```py
from typing_extensions import assert_type

class Raises:
    async def __aenter__(self) -> None: ...
    async def __aexit__(self, exc_type, exc_value, traceback) -> bool:
        return True

def assert_raises(exception: type[ValueError]) -> Raises:
    return Raises()

def custom_raises(exception: type[ValueError]) -> Raises:
    return Raises()

async def make_values() -> list[int]:
    return [1, 2, 4]

async def operation_under_test() -> None:
    raise ValueError

async def setup_before_exception() -> None:
    async with assert_raises(ValueError):
        x_values = await make_values()
        y_values = await make_values()
        first_values, second_values = await make_values(), await make_values()
        await operation_under_test()

    assert_type(x_values, list[int])
    assert_type(y_values, list[int])
    assert_type(first_values, list[int])
    assert_type(second_values, list[int])

async def unrecognized_exception_assertion_name() -> list[int]:
    async with custom_raises(ValueError):
        values = await make_values()
        await operation_under_test()

    return values  # error: [possibly-unresolved-reference]
```

## An awaited operation before literal initialization can still be suppressed

```py
class Suppresses:
    async def __aenter__(self) -> None: ...
    async def __aexit__(self, exc_type, exc_value, traceback) -> bool:
        return True

async def could_raise() -> None:
    raise ValueError

async def call_before_initializer() -> int:
    async with Suppresses():
        await could_raise()
        value = 1

    return value  # error: [possibly-unresolved-reference]

async def could_raise_int() -> int:
    raise ValueError

async def await_as_initializer() -> int:
    async with Suppresses():
        value = await could_raise_int()

    return value  # error: [possibly-unresolved-reference]

async def await_inside_literal_initializer() -> list[int]:
    async with Suppresses():
        values = [await could_raise_int()]

    return values  # error: [possibly-unresolved-reference]
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

## Async exit methods are looked up on the context manager's class

An `async with` statement invokes `type(manager).__aexit__`, not an attribute stored on the manager
instance. An instance-only `__aexit__` is therefore neither a valid async context-manager method nor
a source of exception suppression.

```py
from collections.abc import Awaitable, Callable
from typing_extensions import assert_type

async def suppress_exit(*args: object) -> bool:
    return True

class InstanceOnlyAsyncExit:
    def __init__(self) -> None:
        self.__aexit__: Callable[..., Awaitable[bool]] = suppress_exit

    async def __aenter__(self) -> None:
        return None

async def instance_attributes_do_not_suppress(value: int | str) -> None:
    if isinstance(value, int):
        # error: [invalid-context-manager]
        async with InstanceOnlyAsyncExit():
            raise ValueError

    assert_type(value, str)
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
    def __aenter__(self) -> str:
        return "foo"

    def __aexit__(self, exc_type, exc_value, traceback): ...

class NotAContextManager: ...

async def _(context_expr: Manager1 | NotAContextManager):
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
  |
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
