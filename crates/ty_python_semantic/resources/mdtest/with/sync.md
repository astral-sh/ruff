# With statements

## Basic `with` statement

The type of the target variable in a `with` statement is the return type from the context manager's
`__enter__` method.

```py
class Target: ...

class Manager:
    def __enter__(self) -> Target:
        return Target()

    def __exit__(self, exc_type, exc_value, traceback): ...

with Manager() as f:
    reveal_type(f)  # revealed: Target
```

## Exception-suppressing context managers

A context manager can suppress an exception raised anywhere in its body. Consequently, any binding
visible before or partway through the body must remain visible after the `with` statement.

```py
from contextlib import suppress

def could_raise_returns_str() -> str:
    return "value"

def could_raise_returns_bytes() -> bytes:
    return b"value"

first = 1
second = 2

with suppress(ValueError):
    first = could_raise_returns_str()
    second = could_raise_returns_bytes()

reveal_type(first)  # revealed: Literal[1] | str
reveal_type(second)  # revealed: Literal[2] | bytes
```

## Suppressed exceptions invalidate post-with narrowing

Regression test for <https://github.com/astral-sh/ty/issues/2285>.

```py
class Suppresses:
    def __enter__(self) -> None: ...
    def __exit__(self, exc_type, exc_value, traceback) -> bool:
        return True

def could_raise_returns_str() -> str:
    return "value"

def uses_suppressing_context_manager(value: int) -> str:
    result: int | str = value

    with Suppresses():
        result = could_raise_returns_str()

    return result  # error: [invalid-return-type]
```

## Bindings may be absent after a suppressed exception

```py
from contextlib import suppress

def could_raise_returns_str() -> str:
    return "value"

with suppress(ValueError):
    value = could_raise_returns_str()

value  # error: [possibly-unresolved-reference]
```

## Suppressed exceptions do not terminate control flow

```py
from contextlib import suppress

with suppress(ValueError):
    raise ValueError

reveal_type("reachable")  # revealed: Literal["reachable"]
```

## Suppressing context managers are modeled conservatively

Returning, breaking, and continuing call `__exit__` with three `None` arguments, so a truthy exit
value cannot itself cancel those control-flow transfers. Like mypy and Pyright, however, ty models a
suppressing `with` body as a `try` suite that can exit at any point. It does not try to prove that a
particular `return`, `break`, or `continue` cannot raise. Consequently, the path following the
`with` statement remains reachable even when Python could not take it at runtime.

```py
from contextlib import suppress

def returns_from_body() -> int:  # error: [invalid-return-type]
    with suppress(ValueError):
        return 1

def could_raise_returns_int() -> int:
    return 1

def return_expression_can_be_suppressed() -> int:  # error: [invalid-return-type]
    with suppress(ValueError):
        return could_raise_returns_int()

def breaks_from_body() -> int:
    while True:
        with suppress(ValueError):
            break
        return "unreachable"  # error: [invalid-return-type]
    return 1

def continues_from_body(values: list[int]) -> int:
    for value in values:
        with suppress(ValueError):
            continue
        return "unreachable"  # error: [invalid-return-type]
    return 1
```

## Suppression does not prove that an inner exception escapes

The same conservative model does not determine whether an exception is caught by an inner `except`,
suppressed by an inner manager, overridden by `finally`, or in a statically unreachable branch. The
outer manager can still contribute a possible fall-through path, as it does in mypy and Pyright.

```py
from contextlib import suppress

def caught_by_inner_handler() -> int:  # error: [invalid-return-type]
    with suppress(ValueError):
        try:
            raise ValueError
        except ValueError:
            return 1

def suppressed_by_inner_manager() -> int:  # error: [invalid-return-type]
    with suppress(ValueError):
        with suppress(ValueError):
            raise ValueError
        return 1

def overridden_by_finally() -> int:  # error: [invalid-return-type]
    with suppress(ValueError):
        try:
            raise ValueError
        finally:
            return 1

def statically_unreachable_exception() -> int:  # error: [invalid-return-type]
    with suppress(ValueError):
        if False:
            raise ValueError
        return 1
```

## Assertions and iteration can leave through a suppressing manager

A suppressed exception does not have to originate from a call expression. Assertions and the
iterator protocol can raise implicitly, so they must have the same possible exit as the rest of a
`with` body.

```py
from collections.abc import Iterator
from contextlib import suppress

def suppressed_assertion(condition: bool) -> int:  # error: [invalid-return-type]
    with suppress(AssertionError):
        assert condition
        return 1

class RaisingIterable:
    def __iter__(self) -> Iterator[int]:
        raise ValueError

def suppressed_iteration(values: RaisingIterable) -> int:  # error: [invalid-return-type]
    with suppress(ValueError):
        for value in values:
            pass
        return 1
```

## Literal assignments are conservatively interruptible

Even an assignment whose right-hand side is a literal is modeled as potentially interrupted. This
matches the body-wide approximation used for `try` statements and by mypy and Pyright; the initial
binding remains visible after the manager.

```py
from contextlib import suppress

value = 1

with suppress(ValueError):
    value = "finished"

reveal_type(value)  # revealed: Literal[1, "finished"]
```

## Deletions are not yet recorded as intermediate definitions

The existing `try` snapshot machinery records assignments but does not snapshot `del`. If an
exception is suppressed after deleting a name, ty currently retains its pre-body binding even though
using the name raises `UnboundLocalError` at runtime. This is also a limitation of mypy; Pyright
reports the name as possibly unbound.

```py
from contextlib import suppress

def deleted_after_suppression() -> int:
    value = 1

    with suppress(ValueError):
        del value
        raise ValueError

    # TODO: This should emit [possibly-unresolved-reference].
    return value
```

## Context manager exception suppression follows the typing specification

Only exit methods returning `bool` or `Literal[True]` are considered exception-suppressing. Other
return annotations, including `Any` and `bool | None`, preserve the narrowing that follows an
exception propagating out of the `with` statement.

This is adapted from the Python typing conformance test for context managers.

```py
from typing import Any, Literal
from typing_extensions import assert_type

class Manager:
    def __enter__(self) -> None:
        pass

class SuppressBool(Manager):
    def __exit__(self, exc_type, exc_value, traceback) -> bool:
        return True

class SuppressTrue(Manager):
    def __exit__(self, exc_type, exc_value, traceback) -> Literal[True]:
        return True

class PropagateNone(Manager):
    def __exit__(self, exc_type, exc_value, traceback) -> None:
        return None

class PropagateFalse(Manager):
    def __exit__(self, exc_type, exc_value, traceback) -> Literal[False]:
        return False

class PropagateAny(Manager):
    def __exit__(self, exc_type, exc_value, traceback) -> Any:
        return False

class PropagateOptionalBool(Manager):
    def __exit__(self, exc_type, exc_value, traceback) -> bool | None:
        return None

class PropagateOptionalTrue(Manager):
    def __exit__(self, exc_type, exc_value, traceback) -> Literal[True] | None:
        return None

def suppress_bool(value: int | str) -> None:
    if isinstance(value, int):
        with SuppressBool():
            raise ValueError
    assert_type(value, int | str)

def suppress_true(value: int | str) -> None:
    if isinstance(value, int):
        with SuppressTrue():
            raise ValueError
    assert_type(value, int | str)

def propagate_none(value: int | str) -> None:
    if isinstance(value, int):
        with PropagateNone():
            raise ValueError
    assert_type(value, str)

def propagate_false(value: int | str) -> None:
    if isinstance(value, int):
        with PropagateFalse():
            raise ValueError
    assert_type(value, str)

def propagate_any(value: int | str) -> None:
    if isinstance(value, int):
        with PropagateAny():
            raise ValueError
    assert_type(value, str)

def propagate_optional_bool(value: int | str) -> None:
    if isinstance(value, int):
        with PropagateOptionalBool():
            raise ValueError
    assert_type(value, str)

def propagate_optional_true(value: int | str) -> None:
    if isinstance(value, int):
        with PropagateOptionalTrue():
            raise ValueError
    assert_type(value, str)
```

## Always-falsy exit methods preserve precise bindings

```py
from typing import Literal

class NoneExit:
    def __enter__(self) -> None: ...
    def __exit__(self, exc_type, exc_value, traceback) -> None: ...

class FalseExit:
    def __enter__(self) -> None: ...
    def __exit__(self, exc_type, exc_value, traceback) -> Literal[False]:
        return False

none_value = 1
with NoneExit():
    none_value = "finished"

reveal_type(none_value)  # revealed: Literal["finished"]

false_value = 1
with FalseExit():
    false_value = "finished"

reveal_type(false_value)  # revealed: Literal["finished"]
```

## Truthy exit methods preserve the exceptional path

```py
from typing import Literal

class TrueExit:
    def __enter__(self) -> None: ...
    def __exit__(self, exc_type, exc_value, traceback) -> Literal[True]:
        return True

def could_raise_returns_str() -> str:
    return "finished"

true_value = 1
with TrueExit():
    true_value = could_raise_returns_str()

reveal_type(true_value)  # revealed: Literal[1] | str
```

## Exit overloads are conservatively combined

An overloaded exit method is treated as potentially suppressing if any callable signature returns
`bool` or `Literal[True]`. The implementation does not select an overload based on whether the
manager is exiting normally or handling an exception. This intentionally avoids promising precision
that mypy and Pyright do not consistently provide.

```py
from types import TracebackType
from typing import Literal, overload

class OverloadedExit:
    def __enter__(self) -> None: ...
    @overload
    def __exit__(self, exc_type: None, exc_value: None, traceback: None) -> Literal[False]: ...
    @overload
    def __exit__(
        self,
        exc_type: type[BaseException],
        exc_value: BaseException,
        traceback: TracebackType | None,
    ) -> Literal[True]: ...
    def __exit__(
        self,
        exc_type: type[BaseException] | None,
        exc_value: BaseException | None,
        traceback: TracebackType | None,
    ) -> bool:
        return exc_type is not None

def could_raise_returns_str() -> str:
    return "finished"

value = 1

with OverloadedExit():
    value = could_raise_returns_str()

reveal_type(value)  # revealed: Literal[1] | str
```

## A normal-only truthy exit overload is also treated as suppressing

This is a limitation of considering all exit signatures: the `Literal[True]` overload below applies
only to the normal `(None, None, None)` call, while the exceptional overload always returns
`Literal[False]`. Python propagates an exception, but ty conservatively preserves the initial
binding. Mypy makes the same approximation; Pyright is more precise for this particular overload.

```py
from types import TracebackType
from typing import Literal, overload

class NormalOnlyTruthyExit:
    def __enter__(self) -> None: ...
    @overload
    def __exit__(self, exc_type: None, exc_value: None, traceback: None) -> Literal[True]: ...
    @overload
    def __exit__(
        self,
        exc_type: type[BaseException],
        exc_value: BaseException,
        traceback: TracebackType | None,
    ) -> Literal[False]: ...
    def __exit__(
        self,
        exc_type: type[BaseException] | None,
        exc_value: BaseException | None,
        traceback: TracebackType | None,
    ) -> bool:
        return exc_type is None

def could_raise_returns_str() -> str:
    return "finished"

value = 1

with NormalOnlyTruthyExit():
    value = could_raise_returns_str()

reveal_type(value)  # revealed: Literal[1] | str
```

## Earlier context managers can suppress later entry failures

```py
from contextlib import suppress
from typing import Literal

class Inner:
    def __enter__(self) -> str:
        return "value"

    def __exit__(self, exc_type, exc_value, traceback) -> Literal[False]:
        return False

with suppress(ValueError), Inner() as value:
    pass

value  # error: [possibly-unresolved-reference]
```

## Non-suppressing context managers do not make later targets optional

```py
from contextlib import nullcontext

with nullcontext(), nullcontext("value") as value:
    pass

reveal_type(value)  # revealed: str
```

## Union context manager

```py
class Manager1:
    def __enter__(self) -> str:
        return "foo"

    def __exit__(self, exc_type, exc_value, traceback): ...

class Manager2:
    def __enter__(self) -> int:
        return 42

    def __exit__(self, exc_type, exc_value, traceback): ...

def _(context_expr: Manager1 | Manager2):
    with context_expr as f:
        reveal_type(f)  # revealed: str | int
```

## Type aliases preserve context manager behavior

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Self, TypeAlias
from typing_extensions import TypeAliasType

class A:
    def __enter__(self) -> Self:
        return self

    def __exit__(self, exc_type, exc_value, traceback) -> None: ...

class B:
    def __enter__(self) -> Self:
        return self

    def __exit__(self, exc_type, exc_value, traceback) -> None: ...

UnionAB1: TypeAlias = A | B
type UnionAB2 = A | B
UnionAB3 = TypeAliasType("UnionAB3", A | B)

def f1(x: UnionAB1) -> None:
    with x as y:
        reveal_type(y)  # revealed: A | B

def f2(x: UnionAB2) -> None:
    with x as y:
        reveal_type(y)  # revealed: A | B

def f3(x: UnionAB3) -> None:
    with x as y:
        reveal_type(y)  # revealed: A | B
```

## Context manager without an `__enter__` or `__exit__` method

```py
class Manager: ...

# error: [invalid-context-manager] "Object of type `Manager` cannot be used with `with` because it does not implement `__enter__` and `__exit__`"
with Manager():
    pass
```

## Context manager without an `__enter__` method

```py
class Manager:
    def __exit__(self, exc_tpe, exc_value, traceback): ...

# error: [invalid-context-manager] "Object of type `Manager` cannot be used with `with` because it does not implement `__enter__`"
with Manager():
    pass
```

## Context manager without an `__exit__` method

```py
class Manager:
    def __enter__(self): ...

# error: [invalid-context-manager] "Object of type `Manager` cannot be used with `with` because it does not implement `__exit__`"
with Manager():
    pass
```

## Context manager with non-callable `__enter__` attribute

```py
class Manager:
    __enter__: int = 42

    def __exit__(self, exc_tpe, exc_value, traceback): ...

# error: [invalid-context-manager] "Object of type `Manager` cannot be used with `with` because it does not correctly implement `__enter__`"
with Manager():
    pass
```

## Context manager with non-callable `__exit__` attribute

```py
from typing_extensions import Self

class Manager:
    def __enter__(self) -> Self:
        return self
    __exit__: int = 32

# error: [invalid-context-manager] "Object of type `Manager` cannot be used with `with` because it does not correctly implement `__exit__`"
with Manager():
    pass
```

## Context expression with possibly-unbound union variants

<!-- snapshot-diagnostics -->

```py
class Manager1:
    def __enter__(self) -> str:
        return "foo"

    def __exit__(self, exc_type, exc_value, traceback): ...

class NotAContextManager: ...

def _(context_expr: Manager1 | NotAContextManager):
    # error: [invalid-context-manager] "Object of type `Manager1 | NotAContextManager` cannot be used with `with` because the methods `__enter__` and `__exit__` are possibly missing"
    with context_expr as f:
        reveal_type(f)  # revealed: str
```

## Context expression with overlapping possibly-unbound union variants

<!-- snapshot-diagnostics -->

```py
class GoodManager:
    def __enter__(self) -> str:
        return "foo"

    def __exit__(self, exc_type, exc_value, traceback): ...

class MissingExitManager:
    def __enter__(self) -> str:
        return "bar"

class NotAContextManager: ...

def _(context_expr: GoodManager | MissingExitManager | NotAContextManager):
    # error: [invalid-context-manager] "Object of type `GoodManager | MissingExitManager | NotAContextManager` cannot be used with `with` because the methods `__enter__` and `__exit__` are possibly missing"
    with context_expr as f:
        reveal_type(f)  # revealed: str
```

## Context expression where one union variant has a non-callable dunder

<!-- snapshot-diagnostics -->

If every union element implements the context manager protocol but at least one implements it
incorrectly (e.g. with a non-callable `__exit__` attribute), the diagnostic should reflect that —
*not* report the dunder as "possibly missing".

```py
class GoodManager:
    def __enter__(self) -> str:
        return "foo"

    def __exit__(self, exc_type, exc_value, traceback): ...

class BadManager:
    def __enter__(self) -> str:
        return "bar"

    # `__exit__` is present but not callable
    __exit__: int = 32

def _(context_expr: GoodManager | BadManager):
    # error: [invalid-context-manager] "Object of type `GoodManager | BadManager` cannot be used with `with` because it does not correctly implement `__exit__`"
    with context_expr as f:
        reveal_type(f)  # revealed: str
```

## Context expression with "sometimes" callable `__enter__` method

```py
def _(flag: bool):
    class Manager:
        if flag:
            def __enter__(self) -> str:
                return "abcd"

        def __exit__(self, *args): ...

    # error: [invalid-context-manager] "Object of type `Manager` cannot be used with `with` because the method `__enter__` may be missing"
    with Manager() as f:
        reveal_type(f)  # revealed: str
```

## Invalid `__enter__` signature

```py
class Manager:
    def __enter__() -> str:
        return "foo"

    def __exit__(self, exc_type, exc_value, traceback): ...

context_expr = Manager()

# error: [invalid-context-manager] "Object of type `Manager` cannot be used with `with` because it does not correctly implement `__enter__`"
with context_expr as f:
    reveal_type(f)  # revealed: str
```

## Accidental use of non-async `with`

If a synchronous `with` statement is used on a type with `__aenter__` and `__aexit__`, we show a
diagnostic hint that the user might have intended to use `async with` instead.

```py
class Manager:
    async def __aenter__(self): ...
    async def __aexit__(self, *args): ...

# snapshot: invalid-context-manager
with Manager():
    pass
```

```snapshot
error[invalid-context-manager]: Object of type `Manager` cannot be used with `with` because it does not implement `__enter__` and `__exit__`
 --> src/mdtest_snippet.py:6:6
  |
6 | with Manager():
  |      ^^^^^^^^^
  |
info: Objects of type `Manager` can be used as async context managers
info: Consider using `async with` here
```

## Incorrect signatures

The sub-diagnostic is also provided if the signatures of `__aenter__` and `__aexit__` do not match
the expected signatures for a context manager:

```py
class Manager:
    async def __aenter__(self): ...
    async def __aexit__(self, typ: str, exc, traceback): ...

# error: [invalid-context-manager] "Object of type `Manager` cannot be used with `with` because it does not implement `__enter__` and `__exit__`"
with Manager():
    pass
```

## Incorrect number of arguments

Similarly, we also show the hint if the functions have the wrong number of arguments:

```py
class Manager:
    async def __aenter__(self, wrong_extra_arg): ...
    async def __aexit__(self, typ, exc, traceback, wrong_extra_arg): ...

# error: [invalid-context-manager] "Object of type `Manager` cannot be used with `with` because it does not implement `__enter__` and `__exit__`"
with Manager():
    pass
```
