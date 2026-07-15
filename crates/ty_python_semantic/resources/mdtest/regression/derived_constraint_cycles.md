# Derived constraint cycles

```toml
[environment]
python-version = "3.13"
```

## Initial repeated-substitution cycle

Before [ty#24660], this example would never complete, because we would repeatedly try to substitute
one of the typevars in a constraint over and over, creating increasingly large types in the lower or
upper bound of the constraint.

```py
from typing import Callable, Protocol

class Foo[In, Out](Protocol):
    def method(self, other: In, /) -> Out:
        raise NotImplementedError

def add[In, Out](a: Foo[In, Out], b: In, /) -> Out:
    raise NotImplementedError

def reduce[T](function: Callable[[T, T], T]) -> T:
    raise NotImplementedError

reduce(add)
```

## Recursive relations between derived constraints

Validating a derived constraint can require another relation check. If that relation recursively
reaches the same pair of types, we assume that it holds until we find a contradiction elsewhere in
the protocol. This lets the check terminate without accepting a protocol that has an incompatible
non-recursive member.

```py
from __future__ import annotations

from typing import Protocol, cast

class Array(Protocol):
    def __abs__(self) -> Array: ...
    def __pos__(self) -> Array: ...
    def marker(self) -> int: ...

class Concrete[T]:
    def __abs__[S](self: S) -> S:
        return self

    def __pos__[S](self: S) -> S:
        return self

    def marker(self) -> str:
        return ""

def convert[T](value: Concrete[T]) -> Array:
    return cast(Array, value)

invalid: Array = Concrete[int]()  # error: [invalid-assignment]
```

## Independent nested substitutions can compose

The repeat guard tracks substitution history separately for each derived constraint. This allows two
independent substitutions to be composed while still preventing either substitution from occurring
twice in the ancestry of one derived constraint.

```py
import operator
from collections.abc import Callable

def consume[T1, T2, S](function: Callable[[T1, T2], S], left: T1, right: T2) -> S:
    return function(left, right)

reveal_type(consume(operator.mul, 1, 1))  # revealed: int
```

## Repeated substitutions across derived constraints

The repeat-guard introduced in [ty#24660] only suppressed a follow-up substitution when the second
attempt was into the same constraint id it had already substituted into. Every substitution produces
a new derived constraint, so chains that alternate between two typevars as each other's bounds kept
generating ever-deeper replacement types — for instance by alternating between substituting
`T1 → Iterable[T2]` and `T2 → T1` into the upper bound of a third constraint, each round adding
another `Iterable[...]` layer.

Keying the repeat-guard by the constrained typevar (which stays stable across the chain) caps each
substitution shape to at most one application per BDD path, preventing unbounded growth.

This pattern shows up in real code via `itertools.accumulate` combined with a builtin like `min`,
whose overloaded signature provides the cross-typevar constraints that drive the chain:

```py
from itertools import accumulate

def running_min(iterable):
    iterator = iter(iterable)
    return accumulate(iterator, func=min)
```

This is a version that more exaggerates the performance degradation. Even in the release build, it
took tens of seconds to complete.

```py
from itertools import accumulate

def nested_running_min(iterable):
    it = iter(iterable)
    return accumulate(
        accumulate(
            accumulate(accumulate(accumulate(it, func=min), func=min), func=min),
            func=min,
        ),
        func=min,
    )
```

## Recursive constraint expansion for an optional tuple element

Inferring this assignment produces both a base lower bound and a recursive lower bound for `T`:

- `Unknown & None ≤ T`
- `Unknown & tuple[T] ≤ T`

Combining them produces the finite consequence `Unknown & tuple[Unknown & None] ≤ T`. Previously, we
repeatedly fed each new consequence back into the recursive bound, adding another tuple layer each
time:

- `Unknown & tuple[Unknown & None] ≤ T`
- `Unknown & tuple[Unknown & tuple[Unknown & None]] ≤ T`
- `Unknown & tuple[Unknown & tuple[Unknown & tuple[...]]] ≤ T`

The constraints therefore grew without bound instead of reaching a fixed point.

```py
def wrap[U](value: U) -> tuple[U]:
    return (value,)

def f[T](sentinel):
    items: list[tuple[T] | None] = [None]
    if items[0] is sentinel:
        items[0] = wrap(sentinel)
```

## Recursive structural growth without nested typevars

A substitution can remove the last nested typevar from a derived constraint while still producing an
increasingly deep family of concrete bounds. Structural growth must continue to consume fuel after
that substitution.

```py
from typing import Iterable, Protocol, TypeAlias, TypeVar

V_co = TypeVar("V_co", covariant=True)

class Compatible(Protocol):
    def convert(self) -> object: ...

OptionSequence: TypeAlias = Iterable[V_co] | Compatible

def convert(obj: OptionSequence[V_co]) -> list[V_co]:
    if isinstance(obj, float):
        return [obj]  # ty: ignore[invalid-return-type]
    return []
```

## Propagating an existing deep concrete bound

Structural fuel is charged only for depth introduced by a derivation. Propagating a deeply nested
concrete bound through a typevar therefore remains cheap.

```py
from typing import Never
from ty_extensions import static_assert
from ty_extensions._internal import ConstraintSet

type Deep = tuple[tuple[tuple[tuple[tuple[tuple[tuple[tuple[tuple[tuple[int]]]]]]]]]]

def check_deep_bound[T, U]():
    constraints = ConstraintSet.range(Never, T, U) & ConstraintSet.range(Never, U, Deep)
    static_assert(constraints.implies_subtype_of(T, Deep))
```

[ty#24660]: https://github.com/astral-sh/ruff/pull/24660
