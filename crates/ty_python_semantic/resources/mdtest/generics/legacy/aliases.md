# Generic type aliases: legacy syntax

## Mappings of growing recursive aliases

Materialization and singleton promotion reach payloads introduced by later recursive
specializations, even when the type arguments keep growing.

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
    widened = [value][0]
    reveal_type(widened[1][1][1][1][0])  # revealed: tuple[tuple[int, None | Unknown], None | Unknown]

def transform(value: Any, /) -> Any:
    return value

Materialized = tuple[A, "Top[Materialized[list[A]]] | None", Any, TypeOf[transform], list[Any], TypeOf[list]]

def collect_materialized(value: Materialized[TypeOf[1]], flag: bool):
    widened = [value][0]
    while flag:
        widened = [widened][0]
    reveal_type(widened[0])  # revealed: int
    reveal_type(widened[2])  # revealed: Any
    next = widened[1]
    if next is not None:
        reveal_type(next[0])  # revealed: list[Literal[1]]
        reveal_type(next[2])  # revealed: object
        reveal_type(next[3])  # revealed: (value: Never, /) -> object
        reveal_type(next[4])  # revealed: Top[list[Any]]
        reveal_type(next[5])  # revealed: <class 'list'>
        tail = next[1]
        if tail is not None:
            reveal_type(tail[0])  # revealed: list[list[int]]
            reveal_type(tail[2])  # revealed: object

U = TypeVar("U", bound=int)
V = TypeVar("V", int, str)

def collect_parameters(value: Materialized[tuple[U, V]]):
    widened = [value][0]
    reveal_type(widened[0])  # revealed: tuple[U@collect_parameters, V@collect_parameters]
```
