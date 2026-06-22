# Hashability

```toml
[environment]
python-version = "3.12"
```

ty currently understands the standard-library `Hashable` protocol as equivalent to `object` because
`object` defines a `__hash__` method. Hashability-specific union simplification still needs to
preserve types that can contain unhashable values.

```py
from typing import Hashable, Protocol
from ty_extensions import (
    Intersection,
    Not,
    is_subtype_of,
    static_assert,
)

class HasX(Protocol):
    x: int

class UniversalSet(Protocol): ...

class SupportsHash(Protocol):
    def __hash__(self) -> int: ...

class InvalidHashProtocol(Protocol):
    def __hash__(self) -> object: ...  # error: [invalid-method-override]

def check_object_or_hashable(x: object | Hashable):
    reveal_type(x)  # revealed: object

def check_hashable_or_object(x: Hashable | object):
    reveal_type(x)  # revealed: object

def check_hashable_or_not_hashable(x: Hashable | Not[Hashable]):
    reveal_type(x)  # revealed: object

def check_not_hashable_or_hashable(x: Not[Hashable] | Hashable):
    reveal_type(x)  # revealed: object

def check_hashable_or_not_int(x: Hashable | Not[int]):
    reveal_type(x)  # revealed: object

def check_not_int_or_hashable(x: Not[int] | Hashable):
    reveal_type(x)  # revealed: object

def check_hashable_or_non_final_intersection(x: Hashable | Intersection[int, Not[bool]]):
    reveal_type(x)  # revealed: Hashable | (int & ~bool)

def check_hashable_or_supports_hash(x: Hashable | SupportsHash):
    reveal_type(x)  # revealed: Hashable

def check_supports_hash_or_hashable(x: SupportsHash | Hashable):
    reveal_type(x)  # revealed: SupportsHash

def check_hashable_or_universal(x: Hashable | UniversalSet):
    reveal_type(x)  # revealed: Hashable

def check_universal_or_hashable(x: UniversalSet | Hashable):
    reveal_type(x)  # revealed: UniversalSet
```

This means that any type considered assignable to `object` (which is all types) is considered by ty
to be assignable to `Hashable`. However, ty preserves a non-final nominal type in a union with
`Hashable` instead of discarding it as redundant. A non-final class can have unhashable subclasses,
so keeping the corresponding union element retains the annotation's more precise description of
those subclasses. For example, `list[str]` is unhashable but is a subtype of `Sequence[Hashable]`:

```py
from collections.abc import Hashable as AbcHashable
from typing import Sequence
from ty_extensions import is_disjoint_from

def takes_hashable_or_sequence(x: Hashable | list[Hashable]): ...
def check_hashable_or_sequence(x: Hashable | Sequence[Hashable]):
    reveal_type(x)  # revealed: Hashable | Sequence[Hashable]

def check_sequence_or_hashable(x: Sequence[Hashable] | Hashable):
    reveal_type(x)  # revealed: Sequence[Hashable] | Hashable

def check_supports_hash_or_sequence(x: SupportsHash | Sequence[int]):
    reveal_type(x)  # revealed: SupportsHash | Sequence[int]

def check_sequence_or_supports_hash(x: Sequence[int] | SupportsHash):
    reveal_type(x)  # revealed: Sequence[int] | SupportsHash

def check_invalid_hash_protocol_or_sequence(x: InvalidHashProtocol | Sequence[int]):
    reveal_type(x)  # revealed: InvalidHashProtocol

def check_abc_hashable_or_sequence(x: AbcHashable | Sequence[AbcHashable]):
    reveal_type(x)  # revealed: Hashable | Sequence[Hashable]

def check_sequence_or_abc_hashable(x: Sequence[AbcHashable] | AbcHashable):
    reveal_type(x)  # revealed: Sequence[Hashable] | Hashable

takes_hashable_or_sequence(["foo"])  # fine
takes_hashable_or_sequence(None)  # fine

static_assert(not is_disjoint_from(list[str], Hashable | list[Hashable]))
static_assert(not is_disjoint_from(list[str], Sequence[Hashable]))

static_assert(is_subtype_of(list[Hashable], Sequence[Hashable]))
static_assert(is_subtype_of(list[str], Sequence[Hashable]))
```

The additional union element is only simplified if every value represented by the type is known to
be hashable. For classes, this requires both finality and a valid effective `__hash__` slot:

```py
from dataclasses import dataclass
from enum import Enum
from random import random
from typing import Literal, NewType, final

@final
class C: ...

@final
class Box[T]: ...

class NonFinal: ...

@final
class ExplicitHash:
    def __hash__(self) -> int:
        return 0

@final
class InvalidHash:
    def __hash__(self) -> str:  # error: [invalid-method-override]
        return ""

@final
class ConditionalHash:
    if random():
        def __hash__(self) -> int:
            return 0

    def __eq__(self, other: object, /) -> bool:
        return False

@final
class Unhashable:
    __hash__: None = None

@final
class EqOnly:
    def __eq__(self, other: object, /) -> bool:
        return False

class EqOnlyBase:
    def __eq__(self, other: object, /) -> bool:
        return False

@final
class EqOnlyChild(EqOnlyBase): ...

@final
@dataclass
class UnhashableDataclass: ...

@final
@dataclass(frozen=True)
class HashableDataclass: ...

class UnhashableEnum(Enum):
    FIRST = 1
    SECOND = 2
    __hash__: None = None

FinalC = NewType("FinalC", C)

def check_hashable_or_final(x: Hashable | C):
    reveal_type(x)  # revealed: Hashable

def check_final_or_hashable(x: C | Hashable):
    reveal_type(x)  # revealed: Hashable

def check_hashable_or_final_generic(x: Hashable | Box[int]):
    reveal_type(x)  # revealed: Hashable

def check_final_generic_or_hashable(x: Box[int] | Hashable):
    reveal_type(x)  # revealed: Hashable

def check_hashable_or_explicit_hash(x: Hashable | ExplicitHash):
    reveal_type(x)  # revealed: Hashable

def check_explicit_hash_or_hashable(x: ExplicitHash | Hashable):
    reveal_type(x)  # revealed: Hashable

def check_hashable_or_invalid_hash(x: Hashable | InvalidHash):
    reveal_type(x)  # revealed: Hashable | InvalidHash

def check_hashable_or_final_newtype(x: Hashable | FinalC):
    reveal_type(x)  # revealed: Hashable

def check_hashable_or_hashable_dataclass(x: Hashable | HashableDataclass):
    reveal_type(x)  # revealed: Hashable

def check_hashable_dataclass_or_hashable(x: HashableDataclass | Hashable):
    reveal_type(x)  # revealed: Hashable

def check_hashable_or_non_final(x: Hashable | NonFinal):
    reveal_type(x)  # revealed: Hashable | NonFinal

def check_non_final_or_hashable(x: NonFinal | Hashable):
    reveal_type(x)  # revealed: NonFinal | Hashable

def check_hashable_or_conditional_hash(x: Hashable | ConditionalHash):
    reveal_type(x)  # revealed: Hashable | ConditionalHash

def check_conditional_hash_or_hashable(x: ConditionalHash | Hashable):
    reveal_type(x)  # revealed: ConditionalHash | Hashable

def check_hashable_or_unhashable_final(x: Hashable | Unhashable):
    reveal_type(x)  # revealed: Hashable | Unhashable

def check_unhashable_final_or_hashable(x: Unhashable | Hashable):
    reveal_type(x)  # revealed: Unhashable | Hashable

def check_hashable_or_eq_only(x: Hashable | EqOnly):
    reveal_type(x)  # revealed: Hashable | EqOnly

def check_eq_only_or_hashable(x: EqOnly | Hashable):
    reveal_type(x)  # revealed: EqOnly | Hashable

def check_hashable_or_eq_only_child(x: Hashable | EqOnlyChild):
    reveal_type(x)  # revealed: Hashable | EqOnlyChild

def check_eq_only_child_or_hashable(x: EqOnlyChild | Hashable):
    reveal_type(x)  # revealed: EqOnlyChild | Hashable

def check_hashable_or_unhashable_dataclass(x: Hashable | UnhashableDataclass):
    reveal_type(x)  # revealed: Hashable | UnhashableDataclass

def check_unhashable_dataclass_or_hashable(x: UnhashableDataclass | Hashable):
    reveal_type(x)  # revealed: UnhashableDataclass | Hashable

def check_hashable_or_unhashable_enum_literal(x: Hashable | Literal[UnhashableEnum.FIRST]):
    reveal_type(x)  # revealed: Hashable | Literal[UnhashableEnum.FIRST]

def check_unhashable_enum_literal_or_hashable(x: Literal[UnhashableEnum.FIRST] | Hashable):
    reveal_type(x)  # revealed: Literal[UnhashableEnum.FIRST] | Hashable
```

The same classification applies to type variables, TypedDicts, and structural protocols:

```py
from typing import TypeVar, TypedDict

T = TypeVar("T")
THashable = TypeVar("THashable", bound=Hashable)
TUnhashable = TypeVar("TUnhashable", bound=Unhashable)
TMixed = TypeVar("TMixed", C, Unhashable)

class Payload(TypedDict):
    value: int

def check_hashable_or_typevar(x: Hashable | T):
    reveal_type(x)  # revealed: Hashable | T@check_hashable_or_typevar

def check_typevar_or_hashable(x: T | Hashable):
    reveal_type(x)  # revealed: T@check_typevar_or_hashable | Hashable

def check_hashable_or_hashable_typevar(x: Hashable | THashable):
    reveal_type(x)  # revealed: Hashable

def check_hashable_typevar_or_hashable(x: THashable | Hashable):
    reveal_type(x)  # revealed: Hashable

def check_hashable_or_intersection(
    x: Hashable | Intersection[THashable, Not[bool]],
):
    reveal_type(x)  # revealed: Hashable

def check_intersection_or_hashable(
    x: Intersection[THashable, Not[bool]] | Hashable,
):
    reveal_type(x)  # revealed: Hashable

def check_hashable_or_unhashable_typevar(x: Hashable | TUnhashable):
    reveal_type(x)  # revealed: Hashable | TUnhashable@check_hashable_or_unhashable_typevar

def check_unhashable_typevar_or_hashable(x: TUnhashable | Hashable):
    reveal_type(x)  # revealed: TUnhashable@check_unhashable_typevar_or_hashable | Hashable

def check_hashable_or_mixed_typevar(x: Hashable | TMixed):
    reveal_type(x)  # revealed: Hashable | TMixed@check_hashable_or_mixed_typevar

def check_mixed_typevar_or_hashable(x: TMixed | Hashable):
    reveal_type(x)  # revealed: TMixed@check_mixed_typevar_or_hashable | Hashable

# TODO: Infer hashability through recursive aliases once they are fully supported.
type RecursiveA = RecursiveB | int
type RecursiveB = RecursiveA | str

def check_hashable_or_recursive_alias(x: Hashable | RecursiveA):
    reveal_type(x)  # revealed: Hashable | str | int

def check_hashable_or_typed_dict(x: Hashable | Payload):
    reveal_type(x)  # revealed: Hashable | Payload

def check_typed_dict_or_hashable(x: Payload | Hashable):
    reveal_type(x)  # revealed: Payload | Hashable

def check_hashable_or_protocol(x: Hashable | HasX):
    reveal_type(x)  # revealed: Hashable | HasX

def check_protocol_or_hashable(x: HasX | Hashable):
    reveal_type(x)  # revealed: HasX | Hashable

def function() -> None: ...
def check_function_literal(flag: bool, value: Hashable):
    function_first = function if flag else value
    reveal_type(function_first)  # revealed: Hashable

    function_second = value if flag else function
    reveal_type(function_second)  # revealed: Hashable
```

We do not detect errors in cases like the following, which are flagged by other type checkers:

```py
def needs_something_hashable(x: Hashable):
    hash(x)

needs_something_hashable([])
```
