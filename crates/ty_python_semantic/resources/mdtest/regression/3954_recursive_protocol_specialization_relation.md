# Recursive protocol specialization relations

When two instances originate from the same generic protocol, their structural relation can be
derived from the variance of the protocol interface. This avoids repeatedly expanding recursive
members during inference while preserving constraints for type parameters that the interface
actually exposes.

This is a regression test for <https://github.com/astral-sh/ty/issues/3954>.

```toml
[environment]
python-version = "3.12"
```

## Recursive overload returns

Generic overload returns introduce fresh method-local type variables on every unfolding. Relating
the protocol specializations directly avoids that unbounded growth.

`overloads.py`:

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
    @overload
    def concat[U1, U2, U3, U4](self, other1: U1, other2: U2, other3: U3, other4: U4, /) -> Streamable[T | U1 | U2 | U3 | U4]: ...
    @overload
    def concat[U1, U2, U3, U4, U5](
        self, other1: U1, other2: U2, other3: U3, other4: U4, other5: U5, /
    ) -> Streamable[T | U1 | U2 | U3 | U4 | U5]: ...
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

def element[TurnT: TurnView](turns: TurnViewConnection[TurnT]) -> TurnT:
    raise NotImplementedError

async def check(path: Path) -> None:
    reveal_type(element(path.turns()))  # revealed: TurnView
```

## Recursive receiver constraints

A receiver constraint can relate a specialization to a recursively transformed specialization.
Computing the structural variance on the identity interface prevents `tuple[int, ...]` from growing
on every relation step.

`receiver.py`:

```py
from __future__ import annotations

from collections.abc import AsyncIterator, Iterable
from typing import Protocol

class Streamable[T](Protocol):
    def __aiter__(self) -> AsyncIterator[T]: ...
    def enumerate(self) -> Streamable[tuple[int, T]]: ...
    def flatten[U](self: Streamable[Streamable[U] | Iterable[U]]) -> Streamable[U]: ...

def consume[T](items: Streamable[T]) -> None: ...
def check(items: Streamable[int]) -> None:
    consume(items)
```

## Recursive `ParamSpec` protocols

`ParamSpec` specializations are callable-shaped, so their structural variance must still be flipped
when relating the specialization arguments.

`paramspec.py`:

```py
from __future__ import annotations

from typing import Protocol

class Factory[**P](Protocol):
    def __call__(self, *args: P.args, **kwargs: P.kwargs) -> int: ...
    def child(self) -> Factory[P]: ...

def consume[**P](factory: Factory[P], *args: P.args, **kwargs: P.kwargs) -> int:
    return factory(*args, **kwargs)

def check(factory: Factory[[str]]) -> None:
    reveal_type(consume(factory, "ok"))  # revealed: int
    consume(factory, 1)  # error: [invalid-argument-type]
```

## Phantom parameters do not constrain structural relations

The declared covariance of a legacy type variable is not enough to constrain a structural relation:
if the interface never references that type variable, its structural variance is bivariant. In
particular, a union containing the protocol must not infer nested instances of the same protocol for
the surrounding type variable.

`phantom.py`:

```py
from typing import Any, Protocol, TypeVar

T_co = TypeVar("T_co", covariant=True)

class Recursive(Protocol[T_co]):
    def method(self): ...

def convert(value: Any | Recursive[T_co]) -> list[T_co]:
    try:
        raise Exception
    except:
        result = [value]
        reveal_type(result)  # revealed: list[T_co@convert | Any | Recursive[T_co@convert]]
        return result  # error: [invalid-return-type]
```

## Recursive properties and attributes retain finite mismatches

A recursive property or mutable attribute cannot hide an incompatible overload member.

`property.py`:

```py
from __future__ import annotations

from typing import Protocol, overload

class Source[T](Protocol):
    @overload
    def a_member(self, value: int) -> int: ...
    @overload
    def a_member(self, value: str) -> str: ...
    def a_member(self, value: int | str) -> int | str: ...
    @property
    def z_child(self) -> Source[list[T]]: ...

class Target[T](Protocol):
    @overload
    def a_member(self, value: int) -> str: ...
    @overload
    def a_member(self, value: str) -> int: ...
    def a_member(self, value: int | str) -> int | str: ...
    @property
    def z_child(self) -> Target[list[T]]: ...

def convert(value: Source[int]) -> Target[int]:
    return value  # error: [invalid-return-type]
```

`attribute.py`:

```py
from __future__ import annotations

from typing import Protocol, overload

class Source[T](Protocol):
    @overload
    def a_member(self, value: int) -> int: ...
    @overload
    def a_member(self, value: str) -> str: ...
    def a_member(self, value: int | str) -> int | str: ...
    z_child: Source[list[T]]

class Target[T](Protocol):
    @overload
    def a_member(self, value: int) -> str: ...
    @overload
    def a_member(self, value: str) -> int: ...
    def a_member(self, value: int | str) -> int | str: ...
    z_child: Target[list[T]]

def convert(value: Source[int]) -> Target[int]:
    return value  # error: [invalid-return-type]
```
