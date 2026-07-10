# Recursive overload relations

When a recursive protocol relation revisits an overloaded method, an already-active signature pair
should be checked before the other source overloads. Otherwise, each recursive return comparison
explores the earlier overloads again before reaching the pair that closes the cycle.

This is a regression test for <https://github.com/astral-sh/ty/issues/3954>.

```toml
[environment]
python-version = "3.12"
```

`perf.py`:

```py
from __future__ import annotations

from types import UnionType
from typing import Any, Protocol, overload

type RecursiveLike[T] = RecursiveProtocol[T]

class RecursiveProtocol[T](Protocol):
    def collect[K](self, value: T) -> dict[K, T]: ...
    @overload
    def combine[U1](self, other1: RecursiveLike[U1], /) -> RecursiveProtocol[T | U1]: ...
    @overload
    def combine[U1, U2](self, other1: RecursiveLike[U1], other2: RecursiveLike[U2], /) -> RecursiveProtocol[T | U1 | U2]: ...
    @overload
    def combine[U1, U2, U3](
        self,
        other1: RecursiveLike[U1],
        other2: RecursiveLike[U2],
        other3: RecursiveLike[U3],
        /,
    ) -> RecursiveProtocol[T | U1 | U2 | U3]: ...
    @overload
    def combine[U1, U2, U3, U4](
        self,
        other1: RecursiveLike[U1],
        other2: RecursiveLike[U2],
        other3: RecursiveLike[U3],
        other4: RecursiveLike[U4],
        /,
    ) -> RecursiveProtocol[T | U1 | U2 | U3 | U4]: ...
    @overload
    def combine[U1, U2, U3, U4, U5](
        self,
        other1: RecursiveLike[U1],
        other2: RecursiveLike[U2],
        other3: RecursiveLike[U3],
        other4: RecursiveLike[U4],
        other5: RecursiveLike[U5],
        /,
    ) -> RecursiveProtocol[T | U1 | U2 | U3 | U4 | U5]: ...
    @overload
    def combine[U1, U2, U3, U4, U5, U6](
        self,
        other1: RecursiveLike[U1],
        other2: RecursiveLike[U2],
        other3: RecursiveLike[U3],
        other4: RecursiveLike[U4],
        other5: RecursiveLike[U5],
        other6: RecursiveLike[U6],
        /,
    ) -> RecursiveProtocol[T | U1 | U2 | U3 | U4 | U5 | U6]: ...
    def combine(self, *others: RecursiveLike[Any]) -> RecursiveProtocol[Any]: ...

class Constraint(Protocol): ...
class FirstConstraint(Constraint): ...
class SecondConstraint(Constraint): ...
class SpecializedProtocol[T](RecursiveProtocol[T], Protocol): ...

class OuterProtocol(Protocol):
    @overload
    def select(self, *, constraint: type[FirstConstraint]) -> SpecializedProtocol[FirstConstraint]: ...
    @overload
    def select(self, *, constraint: type[SecondConstraint]) -> SpecializedProtocol[SecondConstraint]: ...
    @overload
    def select(self, *, constraint: UnionType) -> SpecializedProtocol[Constraint]: ...
    @overload
    def select[T: Constraint](self, *, constraint: type[T]) -> SpecializedProtocol[T]: ...
    def select[T: Constraint](
        self, *, constraint: type[T] | UnionType | None = None
    ) -> SpecializedProtocol[T] | SpecializedProtocol[Constraint]: ...

class Implementation(OuterProtocol):
    @property
    def wrapped(self) -> OuterProtocol:
        raise NotImplementedError
```

Closing an active overload pair must not skip an incompatible sibling that only appears after a
recursive specialization.

`specialization.py`:

```py
from typing import Protocol, overload
from ty_extensions import static_assert
from ty_extensions._internal import is_assignable_to, is_subtype_of

class Source[T](Protocol):
    @overload
    def recurse(self, value: int) -> "Source[str]": ...
    @overload
    def recurse(self, value: str) -> T: ...

class Target[T](Protocol):
    @overload
    def recurse(self, value: int) -> "Target[str]": ...
    @overload
    def recurse(self, value: str) -> int: ...

static_assert(not is_subtype_of(Source[int], Target[int]))
static_assert(not is_assignable_to(Source[int], Target[int]))
```
