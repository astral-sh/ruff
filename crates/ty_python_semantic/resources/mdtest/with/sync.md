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

A context manager can suppress an exception raised in its body. Consequently, any binding visible at
an operation that may raise remains a possible binding after the `with` statement.

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

## Interrupted assignments cannot narrow a value

If an assignment inside a suppressing context manager raises before it completes, the original value
remains in use after the `with` statement.

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

A name assigned only inside the `with` body may remain undefined if its value raises before the
assignment completes.

```py
from contextlib import suppress

def could_raise_returns_str() -> str:
    return "value"

with suppress(ValueError):
    value = could_raise_returns_str()

value  # error: [possibly-unresolved-reference]
```

## Suppressed name lookups can leave later bindings undefined

A potentially unbound name can raise before a later assignment executes.

```py
from contextlib import suppress

with suppress(NameError):
    missing  # ty: ignore[unresolved-reference]
    value = 1

value  # error: [possibly-unresolved-reference]
```

## Returns, breaks, and continues are not suppressed

Returning, breaking, and continuing exit the context manager normally. The return value of
`__exit__` cannot cancel those operations.

```py
from contextlib import suppress

def returns_from_body() -> int:
    with suppress(ValueError):
        return 1
```

A function call in a return expression can raise before the function returns. The context manager
may suppress that exception, allowing execution to continue after the `with` statement.

```py
def could_raise_returns_int() -> int:
    return 1

def return_expression_can_be_suppressed() -> int:  # error: [invalid-return-type]
    with suppress(ValueError):
        return could_raise_returns_int()
```

The same normal-exit rule applies to `break` and `continue`:

```py
def breaks_from_body() -> int:
    while True:
        with suppress(ValueError):
            break
        return "unreachable"
    return 1

def continues_from_body(values: list[int]) -> int:
    for value in values:
        with suppress(ValueError):
            continue
        return "unreachable"
    return 1
```

## Exception handlers inside a suppressing context manager

The type checker does not yet determine which exceptions match a typed handler. An exception may
therefore also be treated as reaching the enclosing context manager.

```py
from contextlib import suppress

def caught_by_inner_handler() -> int:  # error: [invalid-return-type]
    with suppress(ValueError):
        try:
            raise ValueError
        except ValueError:
            return 1
```

A bare handler, in contrast, catches every exception before it can reach the outer context manager:

```py
def caught_by_bare_inner_handler() -> int:
    with suppress(ValueError):
        try:
            raise ValueError
        except:
            return 1
```

## Entering a nested context manager can raise

Evaluating or entering a nested context manager can raise before its body begins. The outer manager
may suppress that exception, skipping the later return:

```py
from contextlib import suppress

def suppressed_by_inner_manager() -> int:  # error: [invalid-return-type]
    with suppress(ValueError):
        with suppress(ValueError):
            raise ValueError
        return 1
```

## Class bodies, comprehensions, and generator expressions

A class body executes immediately, so its exceptions can be suppressed by the enclosing context
manager:

```py
from contextlib import suppress

def could_raise() -> None:
    raise ValueError

def eager_class_body() -> int:  # error: [invalid-return-type]
    with suppress(ValueError):
        class RunsImmediately:
            could_raise()

        return 1
```

A list comprehension also executes immediately:

```py
def eager_comprehension() -> int:  # error: [invalid-return-type]
    with suppress(ValueError):
        [could_raise() for _ in [0]]
        return 1
```

A generator expression does not execute its body when the generator is created:

```py
def lazy_generator_body() -> int:
    with suppress(ValueError):
        (could_raise() for _ in [0])
        return 1
```

## Assertions inside a suppressing context manager

A failed assertion raises an exception that the context manager can suppress:

```py
from contextlib import suppress

def suppressed_assertion(condition: bool) -> int:  # error: [invalid-return-type]
    with suppress(AssertionError):
        assert condition
        return 1
```

## Iteration inside a suppressing context manager

Requesting an item from an iterator can raise before the loop body runs:

```py
from collections.abc import Iterator
from contextlib import suppress

class RaisingIterable:
    def __iter__(self) -> Iterator[int]:
        raise ValueError

def suppressed_iteration(values: RaisingIterable) -> int:  # error: [invalid-return-type]
    with suppress(ValueError):
        for value in values:
            pass
        return 1
```

## Assignments that cannot raise

Assigning a literal cannot raise. Adding support for exception suppression must not introduce a path
where that assignment fails to complete.

```py
from contextlib import suppress

value = 1

with suppress(ValueError):
    value = "finished"

reveal_type(value)  # revealed: Literal["finished"]
```

An assignment that cannot raise also remains defined when a later call raises:

```py
from typing_extensions import assert_type

def could_raise() -> None:
    raise ValueError

def initialized_before_exception() -> None:
    with suppress(ValueError):
        values = [1, 2, 4]
        could_raise()

    assert_type(values, list[int])
```

## Deleted names remain undefined

Deleting a name removes its binding. Suppressing a later exception does not restore the deleted
value.

```py
from contextlib import suppress

def deleted_after_suppression() -> int:
    value = 1

    with suppress(ValueError):
        del value
        raise ValueError

    return value  # error: [unresolved-reference]
```

## Context manager exception suppression follows the typing specification

Exit methods annotated with `bool` or `Literal[True]` can suppress an exception:

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
```

Exit methods returning `None` or `Literal[False]` allow the exception to propagate:

```py
class PropagateNone(Manager):
    def __exit__(self, exc_type, exc_value, traceback) -> None:
        return None

class PropagateFalse(Manager):
    def __exit__(self, exc_type, exc_value, traceback) -> Literal[False]:
        return False

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
```

The typing specification also treats `Any`, `bool | None`, and `Literal[True] | None` as
non-suppressing return annotations:

```py
class PropagateAny(Manager):
    def __exit__(self, exc_type, exc_value, traceback) -> Any:
        return False

class PropagateOptionalBool(Manager):
    def __exit__(self, exc_type, exc_value, traceback) -> bool | None:
        return None

class PropagateOptionalTrue(Manager):
    def __exit__(self, exc_type, exc_value, traceback) -> Literal[True] | None:
        return None

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

## Overloaded exit methods

All overloads of `__exit__` are considered together. Here, only the normal-exit overload returns
`True`, so Python would propagate an exception. The type checker does not yet distinguish the two
calls and therefore treats the context manager as potentially suppressing.

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

If entering a later context manager raises, an earlier context manager can suppress that exception.
The later context manager's target may therefore remain undefined.

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

## Union context managers combine their exit return types

A conditional choice between `suppress` and `nullcontext` gives `__exit__` the return type
`bool | None`. The typing specification does not treat that type as exception-suppressing.

```py
from contextlib import nullcontext, suppress
from typing_extensions import assert_type

def conditional_narrowing(flag: bool, value: int | str) -> None:
    if isinstance(value, int):
        manager = suppress(ValueError) if flag else nullcontext()
        with manager:
            raise ValueError

    assert_type(value, str)
```

An explicitly annotated union follows the same rule:

```py
def explicitly_annotated_union(manager: suppress | nullcontext[None], value: int | str) -> None:
    if isinstance(value, int):
        with manager:
            raise ValueError

    assert_type(value, str)
```

## A `False` alternative does not change a boolean exit type

Unlike `bool | None`, the union `bool | Literal[False]` is still `bool`. This context manager may
therefore suppress an exception.

```py
from contextlib import suppress
from typing import Literal
from typing_extensions import assert_type

class FalseExit:
    def __enter__(self) -> None: ...
    def __exit__(self, exc_type, exc_value, traceback) -> Literal[False]:
        return False

def conditional_narrowing(flag: bool, value: int | str) -> None:
    if isinstance(value, int):
        manager = suppress(ValueError) if flag else FalseExit()
        with manager:
            raise ValueError

    assert_type(value, int | str)
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

## Exit methods are looked up on the context manager's class

A `with` statement invokes `type(manager).__exit__`, not an attribute stored on the manager
instance. An instance-only `__exit__` is therefore neither a valid context-manager method nor a
source of exception suppression.

```py
from collections.abc import Callable
from typing_extensions import assert_type

def suppress_exit(*args: object) -> bool:
    return True

class InstanceOnlyExit:
    def __init__(self) -> None:
        self.__exit__: Callable[..., bool] = suppress_exit

    def __enter__(self) -> None:
        return None

def instance_attributes_do_not_suppress(value: int | str) -> None:
    if isinstance(value, int):
        # error: [invalid-context-manager]
        with InstanceOnlyExit():
            raise ValueError

    assert_type(value, str)
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
