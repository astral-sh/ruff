# Recursive protocol constraint solving

When binding an inherited method on a specialization of a recursive protocol, the explicit protocol
inheritance edge between the receiver and the method's generic self type should not fall back to
comparing every protocol member structurally.

This is a regression test for <https://github.com/astral-sh/ty/issues/3954>.

```toml
[environment]
python-version = "3.12"
```

`binding.py`:

```py
from __future__ import annotations

from collections.abc import AsyncIterable, AsyncIterator, Iterable
from typing import Any, Protocol, overload

type RecursiveLike[T] = RecursiveProtocol[T] | AsyncIterable[T] | Iterable[T]

class RecursiveProtocol[T](AsyncIterable[T], Protocol):
    @overload
    def combine[U1](self, other1: RecursiveLike[U1], /) -> RecursiveProtocol[T | U1]: ...
    @overload
    def combine[U1, U2](
        self,
        other1: RecursiveLike[U1],
        other2: RecursiveLike[U2],
        /,
    ) -> RecursiveProtocol[T | U1 | U2]: ...
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

class Reversible[T](RecursiveProtocol[T], Protocol):
    def __aiter__(self) -> AsyncIterator[T]: ...

class Connection[T](Reversible[T], Protocol): ...

async def consume(connection: Connection[int]) -> None:
    async for item in connection:
        reveal_type(item)  # revealed: int
```

A conditional nominal match must preserve a structural alternative that infers a different
specialization from an overridden member. This applies to concrete and protocol subclasses.

`alternatives.py`:

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

Only a genuine implicit positional receiver can be consumed before call inference. Other parameter
shapes must continue through the ordinary bound-method call path.

`receiver_shapes.py`:

```py
from typing import Protocol

class Variadic(Protocol):
    def method(*args: int) -> int: ...

class KeywordOnly(Protocol):
    def method(*, value: int) -> int: ...

def check_variadic(value: Variadic) -> None:
    value.method()  # error: [invalid-argument-type]

def check_keyword_only(value: KeywordOnly) -> None:
    value.method(value=1)  # error: [too-many-positional-arguments]
```
