# Non-inferable constraints and unconstrained solution paths

When inferring the inner `T` for the call to `cast_to_call`, the outer `T` from `wait` is
non-inferable. A satisfiable path constraining only that outer type variable must be recognized as
unconstrained for the inferable domain; otherwise, the inferred specialization degrades to
`Unknown`.

```toml
[environment]
python-version = "3.11"
```

## Existing non-inferable-only path

```py
from collections.abc import Awaitable
from typing import Callable, Generic, TypeVar

T_co = TypeVar("T_co", covariant=True)
T = TypeVar("T")

class Call(Generic[T_co]):
    def __call__(self) -> T_co | Awaitable[T_co]:
        raise NotImplementedError

    def result(self) -> T_co:
        raise NotImplementedError

def cast_to_call(value: Callable[[], T | Awaitable[T]] | Call[T]) -> Call[T]:
    raise NotImplementedError

def wait(value: Callable[[], T] | Call[T]) -> T:
    call = cast_to_call(value)
    reveal_type(call)  # revealed: Call[Awaitable[T@wait] | T@wait]
    return call.result()  # error: [invalid-return-type]
```

## Rigid outer-scope relationships

An inner generic call may infer its own type variable from an outer function's type variable. The
outer variable is rigid for that call and must survive symbolically inside the inferred result.

```py
from typing import TypeVar

Outer = TypeVar("Outer")
Inner = TypeVar("Inner")

def identity(value: Inner) -> Inner:
    return value

def preserve_outer(value: Outer) -> Outer:
    result = identity(value)
    reveal_type(result)  # revealed: Outer@preserve_outer
    return result

def preserve_nested_outer(value: list[Outer]) -> list[Outer]:
    result = identity(value)
    reveal_type(result)  # revealed: list[Outer@preserve_nested_outer]
    return result
```

## Bounded and constrained outer-scope relationships

Declared bounds and finite constraints must remain compatible when a nested call relates its
inferable type variable to an outer, non-inferable type variable.

```py
from typing import TypeVar

BoundedOuter = TypeVar("BoundedOuter", bound=str)
BoundedInner = TypeVar("BoundedInner", bound=str)
IncompatibleBoundedInner = TypeVar("IncompatibleBoundedInner", bound=int)
ConstrainedOuter = TypeVar("ConstrainedOuter", int, str)
ConstrainedInner = TypeVar("ConstrainedInner", int, str)
IncompatibleConstrainedInner = TypeVar("IncompatibleConstrainedInner", bytes, float)

def bounded_identity(value: BoundedInner) -> BoundedInner:
    return value

def constrained_identity(value: ConstrainedInner) -> ConstrainedInner:
    return value

def incompatible_bounded_identity(value: IncompatibleBoundedInner) -> IncompatibleBoundedInner:
    return value

def incompatible_constrained_identity(
    value: IncompatibleConstrainedInner,
) -> IncompatibleConstrainedInner:
    return value

def preserve_bounded(value: BoundedOuter) -> BoundedOuter:
    result = bounded_identity(value)
    reveal_type(result)  # revealed: BoundedOuter@preserve_bounded
    return result

def preserve_constrained(value: ConstrainedOuter) -> ConstrainedOuter:
    result = constrained_identity(value)
    reveal_type(result)  # revealed: ConstrainedOuter@preserve_constrained
    return result

def reject_incompatible_bounded(value: BoundedOuter) -> None:
    incompatible_bounded_identity(value)  # error: [invalid-argument-type]

def reject_incompatible_constrained(value: ConstrainedOuter) -> None:
    incompatible_constrained_identity(value)  # error: [invalid-argument-type]

bounded_identity(1)  # error: [invalid-argument-type]
constrained_identity(b"invalid")  # error: [invalid-argument-type]
```

## Bounded-union method calls with outer return contexts

When a rigid outer variable is bounded by two receiver types, a method shared by those types should
retain both the outer variable and the matching receiver bound. The return diagnostic remains
because `Response` is not necessarily compatible with `T`. The receiver argument diagnostics are
known false positives: member lookup binds each constrained alternative to the original `T` instead
of intersecting it with the matching receiver type.

```py
from __future__ import annotations

from typing import Generic, TypeVar
from typing_extensions import Self

class Response:
    async def __aenter__(self) -> Response:
        return self

class Socket:
    async def __aenter__(self) -> Self:
        return self

T = TypeVar("T", bound=Response | Socket, covariant=True)

class Manager(Generic[T]):
    response: T

    async def __aenter__(self) -> T:
        # error: [invalid-return-type] "expected `T@Manager`, found `Response | (T@Manager & Socket)`"
        # error: [invalid-argument-type]
        # error: [invalid-argument-type]
        return await self.response.__aenter__()
```

## Fixed nested non-inferable relationships

A concrete argument can fix the inner type variable while a second argument preserves a nested
relationship to an outer type variable.

```py
from typing import TypeVar

Outer = TypeVar("Outer")
Inner = TypeVar("Inner")

def choose_nested(value: Inner, nested: list[Inner]) -> list[Inner]:
    return nested

def preserve_nested(value: Outer, nested: list[Outer]) -> list[Outer]:
    result = choose_nested(value, nested)
    reveal_type(result)  # revealed: list[Outer@preserve_nested]
    return result

reveal_type(choose_nested(1, [1]))  # revealed: list[int]
```
