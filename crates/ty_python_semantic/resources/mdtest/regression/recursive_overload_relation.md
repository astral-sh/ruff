# Recursive overload relations

When a recursive protocol relation revisits an overloaded method, an already-active signature pair
should be checked before the other source overloads. Otherwise, each recursive return comparison
explores the earlier overloads again before reaching the pair that closes the cycle.

```toml
[environment]
python-version = "3.12"
```

`perf.py`:

```py
from __future__ import annotations

from types import UnionType
from typing import Any, Protocol, overload

type StreamLike[T] = Streamable[T]

class Streamable[T](Protocol):
    def to_map[K](self, value: T) -> dict[K, T]: ...
    @overload
    def merge[U1](self, other1: StreamLike[U1], /) -> Streamable[T | U1]: ...
    @overload
    def merge[U1, U2](self, other1: StreamLike[U1], other2: StreamLike[U2], /) -> Streamable[T | U1 | U2]: ...
    @overload
    def merge[U1, U2, U3](
        self,
        other1: StreamLike[U1],
        other2: StreamLike[U2],
        other3: StreamLike[U3],
        /,
    ) -> Streamable[T | U1 | U2 | U3]: ...
    @overload
    def merge[U1, U2, U3, U4](
        self,
        other1: StreamLike[U1],
        other2: StreamLike[U2],
        other3: StreamLike[U3],
        other4: StreamLike[U4],
        /,
    ) -> Streamable[T | U1 | U2 | U3 | U4]: ...
    @overload
    def merge[U1, U2, U3, U4, U5](
        self,
        other1: StreamLike[U1],
        other2: StreamLike[U2],
        other3: StreamLike[U3],
        other4: StreamLike[U4],
        other5: StreamLike[U5],
        /,
    ) -> Streamable[T | U1 | U2 | U3 | U4 | U5]: ...
    @overload
    def merge[U1, U2, U3, U4, U5, U6](
        self,
        other1: StreamLike[U1],
        other2: StreamLike[U2],
        other3: StreamLike[U3],
        other4: StreamLike[U4],
        other5: StreamLike[U5],
        other6: StreamLike[U6],
        /,
    ) -> Streamable[T | U1 | U2 | U3 | U4 | U5 | U6]: ...
    def merge(self, *others: StreamLike[Any]) -> Streamable[Any]: ...

class Turn(Protocol): ...
class AgentTurn(Turn): ...
class UserTurn(Turn): ...
class Connection[T](Streamable[T], Protocol): ...

class View(Protocol):
    @overload
    def turns(self, *, type: type[AgentTurn]) -> Connection[AgentTurn]: ...
    @overload
    def turns(self, *, type: type[UserTurn]) -> Connection[UserTurn]: ...
    @overload
    def turns(self, *, type: UnionType) -> Connection[Turn]: ...
    @overload
    def turns[T: Turn](self, *, type: type[T]) -> Connection[T]: ...
    def turns[T: Turn](self, *, type: type[T] | UnionType | None = None) -> Connection[T] | Connection[Turn]: ...

class Delegating(View):
    @property
    def delegate(self) -> View:
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
