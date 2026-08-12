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

A binding introduced by an assignment that may fail is not guaranteed to exist:

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

An assignment that cannot raise remains definitely bound:

```py
with suppress(ValueError):
    safe_value = 1

reveal_type(safe_value)  # revealed: Literal[1]
```

## Assigning a context manager target can raise

Unpacking the result of `__enter__` can raise after the context manager has entered. If its exit
method suppresses that exception, earlier bindings remain visible and new targets may be undefined:

```py
from collections.abc import Iterable

class EmptyIterableManager:
    def __enter__(self) -> Iterable[int]:
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

## Returning from an exception-suppressing context manager

A context manager cannot suppress a return statement that does not raise:

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

def interrupted_return() -> int:  # error: [invalid-return-type]
    with suppress(ValueError):
        return may_raise()
```

## Exception handlers inside a suppressing context manager

A bare handler catches an exception before it can reach the surrounding context manager:

```py
from contextlib import suppress

def caught_before_suppression() -> int:
    with suppress(ValueError):
        try:
            raise ValueError
        except:
            return 1
```

## Eager and lazy expressions inside a suppressing context manager

A list comprehension can raise immediately, but a generator expression does not evaluate its body
until it is iterated:

```py
from contextlib import suppress

def may_raise() -> int:
    raise ValueError

def eager_comprehension() -> int:  # error: [invalid-return-type]
    with suppress(ValueError):
        [may_raise() for _ in [0]]
        return 1

def lazy_generator() -> int:
    with suppress(ValueError):
        (may_raise() for _ in [0])
        return 1
```

## Context manager exit return types

The typing specification treats `bool` and `Literal[True]` exit return types as potentially
exception-suppressing:

```py
from typing import Literal

class Manager:
    def __enter__(self) -> None: ...

class Suppresses(Manager):
    def __exit__(self, exc_type, exc_value, traceback) -> bool:
        return True

class AlwaysSuppresses(Manager):
    def __exit__(self, exc_type, exc_value, traceback) -> Literal[True]:
        return True

def may_raise() -> str:
    raise ValueError

for manager in [Suppresses(), AlwaysSuppresses()]:
    result = 0
    with manager:
        result = may_raise()
    reveal_type(result)  # revealed: Literal[0] | str
```

Other exit return types, including `Literal[False]`, `None`, and `bool | None`, do not suppress
exceptions:

```py
class PropagatesFalse(Manager):
    def __exit__(self, exc_type, exc_value, traceback) -> Literal[False]:
        return False

class PropagatesNone(Manager):
    def __exit__(self, exc_type, exc_value, traceback) -> None: ...

class PropagatesOptionalBool(Manager):
    def __exit__(self, exc_type, exc_value, traceback) -> bool | None:
        return None

for manager in [PropagatesFalse(), PropagatesNone(), PropagatesOptionalBool()]:
    result = 0
    with manager:
        result = may_raise()
    reveal_type(result)  # revealed: str
```

A union can still suppress an exception when one of its context managers suppresses exceptions:

```py
def possibly_suppressing(manager: Suppresses | PropagatesOptionalBool) -> None:
    result = 0
    with manager:
        result = may_raise()
    reveal_type(result)  # revealed: Literal[0] | str
```

A non-suppressing manager does not restore a path that ends in an exception:

```py
def propagating_exception(value: int | str) -> None:
    if isinstance(value, int):
        with PropagatesNone():
            raise ValueError
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
