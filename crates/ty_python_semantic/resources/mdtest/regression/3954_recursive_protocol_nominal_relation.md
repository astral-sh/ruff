# Recursive protocol nominal relations

When relating two specializations of the same recursive protocol during call inference, a viable
nominal relation should avoid the redundant structural fallback. Otherwise, every overloaded method
on the protocol is recursively compared while constructing the constraint set.

This is a regression test for <https://github.com/astral-sh/ty/issues/3954>.

```toml
[environment]
python-version = "3.12"
```

`protocol.py`:

```py
from __future__ import annotations

from collections.abc import AsyncIterator
from typing import Protocol, overload

class TurnView(Protocol):
    @property
    def id(self) -> str: ...

class Streamable[T](Protocol):
    def __aiter__(self) -> AsyncIterator[T]: ...
    @overload
    def concat(self) -> Streamable[T]: ...
    @overload
    def concat[U1](self, other1: U1, /) -> Streamable[T | U1]: ...
    @overload
    def concat[U1, U2](self, other1: U1, other2: U2, /) -> Streamable[T | U1 | U2]: ...
    @overload
    def concat[U1, U2, U3](self, other1: U1, other2: U2, other3: U3, /) -> Streamable[T | U1 | U2 | U3]: ...
    def concat(self, *others: object) -> Streamable[object]: ...

class Connection[T](Streamable[T], Protocol): ...

class TurnViewConnection[TurnT: TurnView](Connection[TurnT], Protocol):
    def __aiter__(self) -> AsyncIterator[TurnT]: ...

class Path:
    @overload
    def turns[TurnT: TurnView](self, *, type: type[TurnT]) -> TurnViewConnection[TurnT]: ...
    @overload
    def turns(self, *, type: None = None) -> TurnViewConnection[TurnView]: ...
    def turns[TurnT: TurnView](
        self, *, type: type[TurnT] | None = None
    ) -> TurnViewConnection[TurnT] | TurnViewConnection[TurnView]:
        raise NotImplementedError

async def snapshot[TurnT: TurnView](turns: TurnViewConnection[TurnT]) -> list[str]:
    return [turn.id async for turn in turns]

def element[TurnT: TurnView](turns: TurnViewConnection[TurnT]) -> TurnT:
    raise NotImplementedError

async def check(path: Path) -> None:
    reveal_type(element(path.turns()))  # revealed: TurnView
    await snapshot(path.turns())
```

The nominal shortcut must not recursively embed a protocol specialization inside itself while
solving a covariant legacy type variable.

`expanding.py`:

```py
from typing import Any, Protocol, TypeVar

T_co = TypeVar("T_co", covariant=True)

class Recursive(Protocol[T_co]):
    def method(self): ...

def convert(value: Any | Recursive[T_co]) -> list[T_co]:
    try:
        raise Exception
    except:
        return [value]  # error: [invalid-return-type]
```

The cycle can also alternate between different protocol declarations. The nominal shortcut must not
bind an unconstrained type variable from one union arm before the sibling arm is related.

`indirect.py`:

```py
from collections.abc import Iterable
from typing import Protocol, TypeAlias, TypeVar

T_co = TypeVar("T_co", covariant=True)

class Frame(Protocol[T_co]):
    def method(self): ...

class FirstMarker(Protocol):
    def first(self): ...

class SecondMarker(Protocol):
    def second(self): ...

Options: TypeAlias = Iterable[T_co] | Frame[T_co] | FirstMarker | SecondMarker

def convert(value: Options[T_co]) -> list[T_co]:
    try:
        raise Exception
    except:
        return [value]  # error: [invalid-return-type]
```

Nested, non-recursive protocol members must still expose an incompatible leaf signature; an active
signature relation is not sufficient reason to assume that an unrelated nested relation succeeds.

`nested.py`:

```py
from typing import Protocol
from ty_extensions import static_assert
from ty_extensions._internal import is_assignable_to, is_subtype_of

class Source3(Protocol):
    def leaf(self) -> str: ...

class Target3(Protocol):
    def leaf(self) -> int: ...

class Source2(Protocol):
    def middle(self) -> Source3: ...

class Target2(Protocol):
    def middle(self) -> Target3: ...

class Source1(Protocol):
    def root(self) -> Source2: ...

class Target1(Protocol):
    def root(self) -> Target2: ...

static_assert(not is_subtype_of(Source1, Target1))
static_assert(not is_assignable_to(Source1, Target1))
```

A subclass can override a protocol member with a narrower type. Its structural relation can infer a
different specialization than the nominal inheritance edge, so this alternative must remain
available for both concrete and protocol subclasses.

`subclasses.py`:

```py
from typing import Protocol

class Box[T](Protocol):
    def get(self) -> T: ...

class ConcreteBox(Box[int]):
    def get(self) -> bool:
        return True

class ProtocolBox(Box[int], Protocol):
    def get(self) -> bool: ...

def consume[T](box: Box[T], values: list[T]) -> None: ...

consume(ConcreteBox(), [True])

def consume_protocol(box: ProtocolBox) -> None:
    consume(box, [True])
```
