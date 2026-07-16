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
