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

When a context manager suppresses an exception during an assignment, the previous binding remains
visible after the `with` statement:

```py
from contextlib import suppress

def may_raise() -> str:
    raise ValueError

result = None
with suppress(ValueError):
    result = may_raise()

reveal_type(result)  # revealed: None | str
```

A new name may remain undefined when an exception interrupts its assignment:

```py
with suppress(ValueError):
    value = may_raise()

# error: [possibly-unresolved-reference]
reveal_type(value)  # revealed: str
```

A deleted binding is not restored if a later exception is suppressed:

```py
deleted = 1
with suppress(ValueError):
    del deleted
    may_raise()

deleted  # error: [unresolved-reference]
```

An assignment that cannot raise is not affected by exception suppression:

```py
with suppress(ValueError):
    safe_value = 1

reveal_type(safe_value)  # revealed: Literal[1]
```

## Assigning a context manager target can raise

Unpacking the result of `__enter__` can raise after the context manager has entered. Suppressing
that exception preserves an earlier binding, while a new target may remain undefined:

```py
class EmptyIterableManager:
    def __enter__(self) -> list[int]:
        return []

    def __exit__(self, exc_type, exc_value, traceback) -> bool:
        return True

value = "before"
with EmptyIterableManager() as (value, missing):
    pass

reveal_type(value)  # revealed: Literal["before"] | int
# error: [possibly-unresolved-reference]
reveal_type(missing)  # revealed: int
```

## Earlier context managers can suppress later entry failures

If an earlier context manager suppresses an exception while a later manager enters, the later
manager's target may never be assigned:

```py
from contextlib import suppress

class EnterFails:
    def __enter__(self) -> str:
        raise ValueError

    def __exit__(self, exc_type, exc_value, traceback) -> None: ...

with suppress(ValueError), EnterFails() as target:
    pass

# error: [possibly-unresolved-reference]
reveal_type(target)  # revealed: str
```

## Loop exits inside multiple context managers

A context manager cannot suppress a `break`, but it can suppress an exception while the next manager
enters. An assignment after the managers is therefore only possibly reached:

```py
from contextlib import nullcontext, suppress

for _ in [1]:
    with suppress(ValueError), nullcontext():
        break
    after_break = 1

after_break  # error: [possibly-unresolved-reference]
```

It cannot suppress a `continue` either:

```py
for _ in [1]:
    with suppress(ValueError), nullcontext():
        continue
    after_continue = 1

after_continue  # error: [possibly-unresolved-reference]
```

An exception inside one manager can likewise be suppressed before a `break`:

```py
for _ in [1]:
    with suppress(ValueError):
        int("invalid")
        break
    after_exception = 1

after_exception  # error: [possibly-unresolved-reference]
```

## Loop exits inside nested context managers

Nested context managers cannot suppress a `break`, but the outer manager can suppress an exception
while the inner manager enters:

```py
from contextlib import nullcontext, suppress

for _ in [1]:
    with suppress(ValueError):
        with nullcontext():
            break
    after_break = 1

after_break  # error: [possibly-unresolved-reference]
```

They cannot suppress a `continue` either:

```py
for _ in [1]:
    with suppress(ValueError):
        with nullcontext():
            continue
    after_continue = 1

after_continue  # error: [possibly-unresolved-reference]
```

## Returning from an exception-suppressing context manager

A context manager cannot suppress a return statement:

```py
from contextlib import suppress

def bare_return() -> int:
    with suppress(ValueError):
        return 1
```

It can suppress an exception raised while evaluating the return expression, allowing the function to
continue without returning a value:

```py
def may_raise() -> int:
    raise ValueError

# error: [invalid-return-type] "Function can implicitly return `None`, which is not assignable to return type `int`"
def interrupted_return() -> int:
    with suppress(ValueError):
        return may_raise()
```

## Exception handlers inside a suppressing context manager

A bare `except:` catches an exception before it can reach the surrounding context manager:

```py
from contextlib import suppress

def caught_before_suppression() -> int:
    with suppress(ValueError):
        try:
            raise ValueError
        except:
            return 1
```

## A terminal `finally` prevents exception suppression

A `return` in a `finally` block replaces the exception before it can reach an enclosing context
manager:

```py
from contextlib import suppress

def always_returns() -> int:
    with suppress(ValueError):
        try:
            raise ValueError
        finally:
            return 1
```

## Cleanup runs before an enclosing context manager suppresses an exception

Assignments in a `finally` block are visible after an enclosing context manager suppresses the
exception:

```py
from contextlib import suppress

def cleanup_before_suppression() -> None:
    result = None
    with suppress(ValueError):
        try:
            raise ValueError
        finally:
            result = "cleaned"
    reveal_type(result)  # revealed: Literal["cleaned"]
```

## Eager expressions inside a suppressing context manager

A list comprehension evaluates its body eagerly, so a context manager can suppress an exception
raised inside it:

```py
from contextlib import suppress

def may_raise() -> int:
    raise ValueError

# error: [invalid-return-type] "Function can implicitly return `None`, which is not assignable to return type `int`"
def eager_comprehension() -> int:
    with suppress(ValueError):
        [may_raise() for _ in [0]]
        return 1
```

Generator expressions are also assumed to run eagerly, so their exceptions can be suppressed:

```py
# error: [invalid-return-type] "Function can implicitly return `None`, which is not assignable to return type `int`"
def eager_generator() -> int:
    with suppress(ValueError):
        (may_raise() for _ in [0])
        return 1
```

## Context manager exit return types

The typing specification treats an `__exit__` return type of `bool` as potentially suppressing:

```py
from typing import Any, Literal

class Manager:
    def __enter__(self) -> None: ...

class ReturnsBool(Manager):
    def __exit__(self, exc_type, exc_value, traceback) -> bool:
        return True

def may_raise() -> str:
    raise ValueError

bool_result = None
with ReturnsBool():
    bool_result = may_raise()
reveal_type(bool_result)  # revealed: None | str
```

An `__exit__` return type of `Literal[True]` can also suppress exceptions:

```py
class ReturnsTrue(Manager):
    def __exit__(self, exc_type, exc_value, traceback) -> Literal[True]:
        return True

true_result = None
with ReturnsTrue():
    true_result = may_raise()
reveal_type(true_result)  # revealed: None | str
```

An `__exit__` return type of `Literal[False]` cannot suppress exceptions:

```py
class ReturnsFalse(Manager):
    def __exit__(self, exc_type, exc_value, traceback) -> Literal[False]:
        return False

false_result = None
with ReturnsFalse():
    false_result = may_raise()
reveal_type(false_result)  # revealed: str
```

An `__exit__` return type of `None` cannot suppress exceptions:

```py
class ReturnsNone(Manager):
    def __exit__(self, exc_type, exc_value, traceback) -> None: ...

none_result = None
with ReturnsNone():
    none_result = may_raise()
reveal_type(none_result)  # revealed: str
```

[The typing specification](https://typing.python.org/en/latest/spec/exceptions.html#context-managers)
classifies `bool | None` as non-suppressing for compatibility with common non-suppressing context
managers, even though a truthy return value can suppress an exception at runtime:

```py
class ReturnsOptionalBool(Manager):
    def __exit__(self, exc_type, exc_value, traceback) -> bool | None:
        return None

optional_result = None
with ReturnsOptionalBool():
    optional_result = may_raise()
reveal_type(optional_result)  # revealed: str
```

This convention also treats `Literal[True] | None` as non-suppressing:

```py
class ReturnsOptionalTrue(Manager):
    def __exit__(self, exc_type, exc_value, traceback) -> Literal[True] | None:
        return True

optional_true_result = None
with ReturnsOptionalTrue():
    optional_true_result = may_raise()
reveal_type(optional_true_result)  # revealed: str
```

An `__exit__` return type of `Literal[False] | None` cannot suppress exceptions either:

```py
class ReturnsOptionalFalse(Manager):
    def __exit__(self, exc_type, exc_value, traceback) -> Literal[False] | None:
        return False

optional_false_result = None
with ReturnsOptionalFalse():
    optional_false_result = may_raise()
reveal_type(optional_false_result)  # revealed: str
```

An `__exit__` return type of `Any` does not indicate exception suppression either:

```py
class ReturnsAny(Manager):
    def __exit__(self, exc_type, exc_value, traceback) -> Any:
        return False

any_result = None
with ReturnsAny():
    any_result = may_raise()
reveal_type(any_result)  # revealed: str
```

## Context managers with union and aliased union types

A context manager with a union type may suppress an exception if any member can suppress it, even
when another member cannot:

```toml
[environment]
python-version = "3.12"
```

```py
class Manager:
    def __enter__(self) -> None: ...

class Suppresses(Manager):
    def __exit__(self, exc_type, exc_value, traceback) -> bool:
        return True

class Propagates(Manager):
    def __exit__(self, exc_type, exc_value, traceback) -> bool | None:
        return None

def may_raise() -> str:
    raise ValueError

def possibly_suppressing(manager: Suppresses | Propagates) -> None:
    result = None
    with manager:
        result = may_raise()
    reveal_type(result)  # revealed: None | str
```

A PEP 695 alias does not prevent a suppressing union member from preserving an earlier binding:

```py
type Managers = Suppresses | Propagates

def preserved_binding(manager: Managers) -> None:
    result = None
    with manager:
        result = may_raise()
    reveal_type(result)  # revealed: None | str
```

A suppressed exception can also leave a new binding undefined:

```py
def missing_binding(manager: Managers) -> None:
    with manager:
        result = may_raise()
    # error: [possibly-unresolved-reference]
    reveal_type(result)  # revealed: str
```

## Non-suppressing context managers preserve narrowing

A non-suppressing manager does not change narrowing after an exception propagates:

```py
class Manager:
    def __enter__(self) -> None: ...
    def __exit__(self, exc_type, exc_value, traceback) -> None: ...

def propagating_exception(value: int | str) -> None:
    if isinstance(value, int):
        with Manager():
            raise ValueError
    reveal_type(value)  # revealed: str
```

Narrowing established after an earlier operation that may raise is preserved too:

```py
def narrowing_after_possible_exception(value: int | str) -> None:
    with Manager():
        int("invalid")
        if isinstance(value, int):
            raise ValueError
    reveal_type(value)  # revealed: str
```

Type guard narrowing on one exception path is preserved when another path introduces a new binding
inside a non-suppressing context manager:

```py
from typing import TypeGuard

class Base: ...

def is_string(value: Base) -> TypeGuard[str]:
    return isinstance(value, str)

def make_integer() -> int:
    return 1

def type_guard_across_exception_handler() -> None:
    value = Base()
    try:
        if not is_string(value):
            return
    except Exception:
        with Manager():
            value = make_integer()

    reveal_type(value)  # revealed: str | int
```

Ordinary `isinstance` narrowing follows the same rule: an excluded `None` does not reappear when the
exception-handler assignment is merged:

```py
def isinstance_across_exception_handler(source: str | None) -> None:
    value = source
    try:
        if not isinstance(value, str):
            return
    except Exception:
        with Manager():
            value = make_integer()

    reveal_type(value)  # revealed: str | int
```

## Overloaded context manager exit methods

Whether an overloaded exit method can suppress an exception depends on the overload used when an
exception occurs, not the overload used when its suite exits without an exception. In the latter
case, Python calls `__exit__(None, None, None)`:

```py
from typing import Literal, overload
from typing_extensions import Never

class Manager:
    def __enter__(self) -> None: ...

def may_raise() -> str:
    raise ValueError
```

A manager that returns `True` only during normal exit cannot suppress exceptions:

```py
class NormalOnly(Manager):
    @overload
    def __exit__(self, exc_type: None, exc_value: None, traceback: None) -> Literal[True]: ...
    @overload
    def __exit__(self, exc_type: type[BaseException], exc_value: BaseException, traceback: object) -> Literal[False]: ...
    def __exit__(self, exc_type, exc_value, traceback) -> bool:
        return exc_type is None

normal_value = None
with NormalOnly():
    normal_value = may_raise()
reveal_type(normal_value)  # revealed: str
```

An exceptional overload cannot suppress an exception if either exception argument is uninhabited:

```py
class ImpossibleExceptionalExit(Manager):
    @overload
    def __exit__(self, exc_type: Never, exc_value: BaseException, traceback: object) -> Literal[True]: ...
    @overload
    def __exit__(self, exc_type: type[BaseException], exc_value: Never, traceback: object) -> Literal[True]: ...
    @overload
    def __exit__(
        self, exc_type: type[BaseException] | None, exc_value: BaseException | None, traceback: object | None
    ) -> Literal[False]: ...
    def __exit__(self, exc_type, exc_value, traceback) -> bool:
        return False

impossible_exception_value = None
with ImpossibleExceptionalExit():
    impossible_exception_value = may_raise()
reveal_type(impossible_exception_value)  # revealed: str
```

An exceptional overload can suppress its exception even if another exceptional overload cannot:

```py
class SuppressesValueError(Manager):
    @overload
    def __exit__(self, exc_type: type[ValueError], exc_value: ValueError, traceback: object) -> Literal[True]: ...
    @overload
    def __exit__(self, exc_type: type[TypeError], exc_value: TypeError, traceback: object) -> None: ...
    @overload
    def __exit__(self, exc_type: None, exc_value: None, traceback: None) -> None: ...
    def __exit__(self, exc_type, exc_value, traceback) -> Literal[True] | None:
        return True if exc_type is ValueError else None

mixed_exceptional_value = None
with SuppressesValueError():
    mixed_exceptional_value = may_raise()
reveal_type(mixed_exceptional_value)  # revealed: None | str
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
