# Generic type aliases: legacy syntax

## Mappings of growing recursive aliases

Materialization and singleton promotion reach payloads introduced by later recursive
specializations, even when the type arguments keep growing.

TODO: Comparing growing recursive aliases currently rejects some valid list accesses below.

```py
from typing import Any, TypeVar
from ty_extensions import Bottom, Top, static_assert
from ty_extensions._internal import TypeOf, is_equivalent_to, is_subtype_of

A = TypeVar("A")
B = TypeVar("B")
C = TypeVar("C")
Delayed = tuple[A, "Delayed[B, C, tuple[A, Any]] | None"]
Shared = tuple["Shared[B, list[A]] | None", list[A]]

static_assert(not is_equivalent_to(Delayed[int, int, int], Top[Delayed[int, int, int]]))
static_assert(not is_equivalent_to(Delayed[int, int, int], Bottom[Delayed[int, int, int]]))
static_assert(not is_subtype_of(Shared[int, Any], Shared[int, Any]))

Nested = tuple[list[list["Nested[list[A]]"]]]

def materialize_nested(value: Top[Nested[int]]): ...  # no diagnostic

Late = tuple[A, "Late[B, tuple[A, None]]"]

def collect(value: Late[int, int]):
    widened = [value][0]  # error: [invalid-argument-type]
    # TODO: Should reveal tuple[tuple[int, None | Unknown], None | Unknown].
    reveal_type(widened[1][1][1][1][0])  # revealed: Unknown

Materialized = tuple[A, "Top[Materialized[list[A]]] | None"]

def collect_materialized(value: Materialized[TypeOf[1]], flag: bool):
    widened = [value][0]  # error: [invalid-argument-type]
    while flag:
        widened = [widened][0]
    # TODO: Should reveal int.
    reveal_type(widened[0])  # revealed: Unknown
    next = widened[1]
    if next is not None:
        # TODO: Should reveal list[Literal[1]].
        reveal_type(next[0])  # revealed: Unknown
```
