# Generic type aliases: legacy syntax

## Mappings of growing recursive aliases

A type argument passed only to recursive references does not affect the alias's values.
Materializing such an argument leaves the alias equivalent to the original type.

```py
from typing import Any, TypeVar
from ty_extensions import Bottom, Top, static_assert
from ty_extensions._internal import is_equivalent_to, is_subtype_of

T = TypeVar("T")
Hidden = int | tuple["Hidden[list[T]]"]

static_assert(is_equivalent_to(Hidden[Any], Top[Hidden[Any]]))
static_assert(is_equivalent_to(Hidden[Any], Bottom[Hidden[Any]]))
```

An argument can reach a value position only after several recursive steps. Materialization must
still distinguish the resulting dynamic leaves, including when the same argument also occurs inside
another recursive reference.

```py
A = TypeVar("A")
B = TypeVar("B")
C = TypeVar("C")
Delayed = tuple[A, "Delayed[B, C, tuple[A, Any]] | None"]
Shared = tuple["Shared[B, list[A]] | None", list[A]]

static_assert(not is_equivalent_to(Delayed[int, int, int], Top[Delayed[int, int, int]]))
static_assert(not is_equivalent_to(Delayed[int, int, int], Bottom[Delayed[int, int, int]]))
static_assert(not is_equivalent_to(Shared[int, Any], Top[Shared[int, Any]]))
static_assert(not is_subtype_of(Shared[int, Any], Shared[int, Any]))
```

Singleton promotion also reaches `None` introduced by later recursive specializations.

```py
Late = tuple[A, "Late[B, tuple[A, None]]"]

def collect(value: Late[int, int]):
    widened = [value][0]
    reveal_type(widened[1][1][0])  # revealed: tuple[int, None | Unknown]
    # revealed: tuple[tuple[int, None | Unknown], None | Unknown]
    reveal_type(widened[1][1][1][1][0])
```
