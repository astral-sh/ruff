# Recursive protocol structural relations

When relating recursive protocol specializations, we should avoid expanding the structural interface
if the nominal relation is already viable. When structural comparison is required, an incompatible
non-recursive member should be checked before members that grow the specialization.

This is a regression test for <https://github.com/astral-sh/ty/issues/3954>.

```toml
[environment]
python-version = "3.12"
```

## Viable nominal relation

`lazy.py`:

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

## Incompatible structural member

`structural.py`:

```py
from __future__ import annotations

from collections.abc import Iterable
from typing import Protocol
from ty_extensions import static_assert
from ty_extensions._internal import is_assignable_to, is_subtype_of

class Source[T](Protocol):
    def enumerate(self) -> Source[tuple[int, T]]: ...
    def flatten[U](self: Source[Source[U] | Iterable[U]]) -> Source[U]: ...
    def value(self) -> str: ...

class Target[T](Protocol):
    def enumerate(self) -> Target[tuple[int, T]]: ...
    def flatten[U](self: Target[Target[U] | Iterable[U]]) -> Target[U]: ...
    def value(self) -> int: ...

static_assert(not is_subtype_of(Source[int], Target[int]))
static_assert(not is_assignable_to(Source[int], Target[int]))
```

## Recursive property and attribute

An incompatible overloaded member must still be checked before a recursive property or attribute.

```py
from __future__ import annotations

from typing import Protocol, overload

class PropertySource[T](Protocol):
    @overload
    def a_member(self, value: int) -> int: ...
    @overload
    def a_member(self, value: str) -> str: ...
    def a_member(self, value: int | str) -> int | str: ...
    @property
    def z_child(self) -> PropertySource[list[T]]: ...

class PropertyTarget[T](Protocol):
    @overload
    def a_member(self, value: int) -> str: ...
    @overload
    def a_member(self, value: str) -> int: ...
    def a_member(self, value: int | str) -> int | str: ...
    @property
    def z_child(self) -> PropertyTarget[list[T]]: ...

class AttributeSource[T](Protocol):
    @overload
    def a_member(self, value: int) -> int: ...
    @overload
    def a_member(self, value: str) -> str: ...
    def a_member(self, value: int | str) -> int | str: ...
    z_child: AttributeSource[list[T]]

class AttributeTarget[T](Protocol):
    @overload
    def a_member(self, value: int) -> str: ...
    @overload
    def a_member(self, value: str) -> int: ...
    def a_member(self, value: int | str) -> int | str: ...
    z_child: AttributeTarget[list[T]]

def property_convert(value: PropertySource[int]) -> PropertyTarget[int]:
    return value  # error: [invalid-return-type]

def attribute_convert(value: AttributeSource[int]) -> AttributeTarget[int]:
    return value  # error: [invalid-return-type]
```

## Recursive self-binding

The nominal shortcut must not bind an unconstrained TypeVar to another specialization of the same
protocol or one supplied by a sibling union arm.

```py
from collections.abc import Iterable
from typing import Any, Protocol, TypeAlias, TypeVar, reveal_type

T_co = TypeVar("T_co", covariant=True)

class Recursive(Protocol[T_co]):
    def method(self): ...

def convert(value: Any | Recursive[T_co]) -> list[T_co]:
    result = [value]
    reveal_type(result)  # revealed: list[T_co@convert | Any | Recursive[T_co@convert]]
    return result  # error: [invalid-return-type]

U = TypeVar("U", covariant=True)

class F(Protocol[U]):
    def frame(self): ...

class M(Protocol):
    def marker(self): ...

Options: TypeAlias = Iterable[U] | F[U] | M

def s(value: Options[U]) -> list[U]:
    result = [value]
    reveal_type(result)  # revealed: list[U@s | Iterable[M] | Iterable[Iterable[M]] | Iterable[U@s] | F[U@s] | M]
    return result  # error: [invalid-return-type]
```
