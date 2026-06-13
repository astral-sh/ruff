# Narrowing for `match` statements

```toml
[environment]
python-version = "3.10"
```

## Single `match` pattern

```py
from typing import Literal

def _(x: Literal[1] | None):
    reveal_type(x)  # revealed: Literal[1] | None

    y = 0

    match x:
        case None:
            y = x

    reveal_type(y)  # revealed: Literal[0] | None
```

## Class patterns

```py
def get_object() -> object:
    return object()

class A: ...
class B: ...

x = get_object()

reveal_type(x)  # revealed: object

match x:
    case A():
        reveal_type(x)  # revealed: A
    case B():
        reveal_type(x)  # revealed: B & ~A

reveal_type(x)  # revealed: object
```

## Class pattern with guard

```py
def get_object() -> object:
    return object()

class A:
    def y() -> int:
        return 1

class B: ...

x = get_object()

reveal_type(x)  # revealed: object

match x:
    case A() if reveal_type(x):  # revealed: A
        pass
    case B() if reveal_type(x):  # revealed: B
        pass

reveal_type(x)  # revealed: object

def mixed_guarded_and_unguarded_patterns(x: A | B, first_flag: bool, second_flag: bool) -> None:
    match x:
        case A():
            pass
        case B() if first_flag:
            pass
        case B() if second_flag:
            pass
        case B():
            # The guarded `B` patterns are not exclusions, but the earlier
            # unguarded `A` pattern is still excluded.
            reveal_type(x)  # revealed: B & ~A

def exhaustive_pattern_with_guard(x: A, flag: bool) -> None:
    match x:
        case A() if flag:
            pass
        case _:
            reveal_type(x)  # revealed: A
```

## Class patterns with generic classes

```toml
[environment]
python-version = "3.12"
```

```py
from typing import assert_never

class Covariant[T]:
    def get(self) -> T:
        raise NotImplementedError

def f(x: Covariant[int]):
    match x:
        case Covariant():
            reveal_type(x)  # revealed: Covariant[int]
        case _:
            reveal_type(x)  # revealed: Never
            assert_never(x)
```

## Class patterns with generic `@final` classes

These work the same as non-`@final` classes.

```toml
[environment]
python-version = "3.12"
```

```py
from typing import assert_never, final

@final
class Covariant[T]:
    def get(self) -> T:
        raise NotImplementedError

def f(x: Covariant[int]):
    match x:
        case Covariant():
            reveal_type(x)  # revealed: Covariant[int]
        case _:
            reveal_type(x)  # revealed: Never
            assert_never(x)
```

## Mapping patterns

```py
from collections.abc import Mapping
from typing import Any

def test_isinstance(x: dict[Any, Any] | int) -> None:
    if isinstance(x, Mapping):
        reveal_type(x)  # revealed: dict[Any, Any] | (int & Top[Mapping[Unknown, object]])
    else:
        reveal_type(x)  # revealed: int & ~Top[Mapping[Unknown, object]]

def test_match(x: dict[Any, Any] | int) -> None:
    match x:
        case {}:
            reveal_type(x)  # revealed: dict[Any, Any] | (int & Top[Mapping[Unknown, object]])
        case _:
            reveal_type(x)  # revealed: int & ~Top[Mapping[Unknown, object]]

def test_match_double_star(x: dict[Any, Any] | int) -> None:
    match x:
        case {**rest}:
            reveal_type(x)  # revealed: dict[Any, Any] | (int & Top[Mapping[Unknown, object]])
        case _:
            reveal_type(x)  # revealed: int & ~Top[Mapping[Unknown, object]]

def test_match_refutable(x: dict[Any, Any] | int) -> None:
    match x:
        case {"k": _}:
            reveal_type(x)  # revealed: dict[Any, Any] | (int & Top[Mapping[Unknown, object]])
        case _:
            reveal_type(x)  # revealed: dict[Any, Any] | int
```

## Sequence patterns

```py
from collections.abc import Sequence

def test_match_star(x: Sequence[int] | int) -> None:
    match x:
        case [*rest]:
            reveal_type(x)  # revealed: (Sequence[int] & ~str & ~bytes & ~bytearray) | (int & Sequence[object])
        case _:
            # `str`, `bytes`, and `bytearray` are subtypes of `Sequence`, but
            # sequence patterns explicitly do not match them. `bytes` and
            # `bytearray` are possible inhabitants of `Sequence[int]`.
            # TODO: After https://github.com/astral-sh/ty/issues/3314 is
            # fixed, the `Sequence[int] & str` intersection should simplify to
            # `Never`.
            reveal_type(x)  # revealed: (int & ~Sequence[object]) | (Sequence[int] & str) | bytes | bytearray

def test_match_star_excludes_text_and_bytes(x: str | bytes | bytearray | list[int]) -> None:
    match x:
        case [*rest]:
            reveal_type(x)  # revealed: list[int]
        case _:
            reveal_type(x)  # revealed: str | bytes | bytearray

def test_match_exact_sequence_excludes_str(x: str | tuple[int, int]) -> None:
    match x:
        case (_, _):
            pass
        case _:
            reveal_type(x)  # revealed: str

def test_match_exact_sequence_excludes_bytes(x: bytes | tuple[int, int]) -> None:
    match x:
        case (_, _):
            pass
        case _:
            reveal_type(x)  # revealed: bytes

def test_match_exact_sequence_excludes_bytearray(x: bytearray | tuple[int, int]) -> None:
    match x:
        case (_, _):
            pass
        case _:
            reveal_type(x)  # revealed: bytearray

def test_match_exact_object_sequence(value: object) -> None:
    match value:
        case int(), str():
            # revealed: Sequence[object] & <Protocol with members '__getitem__', '__len__'> & ~str & ~bytes & ~bytearray
            reveal_type(value)
            reveal_type(len(value))  # revealed: Literal[2]
            reveal_type(value[0])  # revealed: int
            reveal_type(value[1])  # revealed: str

def test_match_empty_object_sequence(value: object) -> None:
    match value:
        case []:
            # revealed: Sequence[object] & <Protocol with members '__len__'> & ~str & ~bytes & ~bytearray
            reveal_type(value)
            reveal_type(len(value))  # revealed: Literal[0]

def test_match_singleton_object_sequence(value: object) -> None:
    match value:
        case [int()]:
            # revealed: Sequence[object] & <Protocol with members '__getitem__', '__len__'> & ~bytearray & ~bytes
            reveal_type(value)
            reveal_type(len(value))  # revealed: Literal[1]
            reveal_type(value[0])  # revealed: int

def test_match_prefix_star_object_sequence(value: object) -> None:
    match value:
        case [int(), *rest]:
            # revealed: Sequence[object] & <Protocol with members '__getitem__'> & ~str & ~bytes & ~bytearray
            reveal_type(value)
            reveal_type(len(value))  # revealed: int
            reveal_type(value[0])  # revealed: int
            reveal_type(value[1])  # revealed: object

def test_match_prefix_and_suffix_star_object_sequence(value: object) -> None:
    match value:
        case [int(), *rest, str()]:
            # revealed: Sequence[object] & <Protocol with members '__getitem__'> & ~str & ~bytes & ~bytearray
            reveal_type(value)
            reveal_type(value[0])  # revealed: int
            reveal_type(value[-1])  # revealed: str
            reveal_type(value[1])  # revealed: object

def test_match_prefix_star_known_sequence(value: Sequence[int | str]) -> None:
    match value:
        case [int(), *rest]:
            reveal_type(value[0])  # revealed: int
            reveal_type(value[1])  # revealed: int | str
            reveal_type(rest)  # revealed: list[int | str]
```

## Sequence capture types

A capture gets its type from the sequence element it binds. A starred capture is always a list. For
a fixed-length tuple, we can determine exactly which elements appear in that list.

```py
from typing import Any
from ty_extensions import Unknown

def test_match_star_capture(value: tuple[int, str, bool]) -> None:
    match value:
        case [first, *rest]:
            reveal_type(first)  # revealed: int
            reveal_type(rest)  # revealed: list[str | bool]

def test_match_star_capture_between_patterns(value: tuple[int, bytes, str]) -> None:
    match value:
        case [int(), *rest, str()]:
            reveal_type(rest)  # revealed: list[bytes]

def test_match_dynamic_sequence_captures(any_value: Any, unknown_value: Unknown) -> None:
    match any_value:
        case [item, *rest]:
            reveal_type(item)  # revealed: Any
            reveal_type(rest)  # revealed: list[Any]

    match unknown_value:
        case [item, *rest]:
            reveal_type(item)  # revealed: Unknown
            reveal_type(rest)  # revealed: list[Unknown]

def test_match_capture_in_guard(value: tuple[int]) -> None:
    match value:
        case [item] if reveal_type(item):  # revealed: int
            pass

def test_impossible_sequence_capture(value: tuple[str]) -> None:
    match value:
        case [int() as item]:
            reveal_type(item)  # revealed: Never

# A pattern only binds names if the complete pattern succeeds. The first element would bind `str`
# on its own, but the second element makes this pattern impossible.
def test_later_failure_rejects_earlier_capture(value: tuple[str, str]) -> None:
    match value:
        case [item, int()]:
            reveal_type(item)  # revealed: Never
```

## Captures from unions of tuples

When a union contains several tuple types, matching one element can determine the types of the other
captures. A wildcard keeps every tuple type that can match. The same rules apply through type
aliases and constrained type variables.

```py
from typing import Literal, TypeAlias, TypeVar

def test_match_star_capture_filters_union_members(
    value: tuple[Literal[1], int, int] | tuple[Literal[2], str, str],
) -> list[int]:
    match value:
        case [1, *rest]:
            reveal_type(rest)  # revealed: list[int]
            return rest
        case _:
            return []

def test_match_star_capture_preserves_compatible_union_members(
    value: tuple[Literal[1], int, int] | tuple[Literal[2], str, str],
) -> None:
    match value:
        case [_, *rest]:
            reveal_type(rest)  # revealed: list[int] | list[str]

def test_match_capture_filters_union_members(
    value: tuple[Literal[1], int] | tuple[Literal[2], str],
) -> int:
    match value:
        case [1, item]:
            reveal_type(item)  # revealed: int
            return item
        case _:
            return 0

def test_match_capture_preserves_compatible_union_members(
    value: tuple[Literal[1], int] | tuple[Literal[2], str],
) -> None:
    match value:
        case [_, item]:
            reveal_type(item)  # revealed: int | str

MatchPair: TypeAlias = tuple[Literal[1], int] | tuple[Literal[2], str]
MatchPairT = TypeVar(
    "MatchPairT",
    tuple[Literal[1], int],
    tuple[Literal[2], str],
)

def test_match_capture_filters_aliased_union_members(value: MatchPair) -> None:
    match value:
        case [1, item]:
            reveal_type(item)  # revealed: int

def test_match_capture_filters_constrained_typevar_members(value: MatchPairT) -> None:
    match value:
        case [1, item]:
            reveal_type(item)  # revealed: int
```

## Pattern aliases

An `as` pattern binds the original matched value. The binding keeps facts already known about the
subject as well as facts established by the nested pattern. A later case also starts with the values
not handled by earlier cases.

```py
from typing import Literal

def test_match_sequence_as_pattern(value: object) -> None:
    match value:
        case [int() as item, _]:
            reveal_type(item)  # revealed: int

def test_match_sequence_as_pattern_preserves_subject_type(
    value: tuple[Literal[1], object],
) -> None:
    match value:
        case [int() as item, _]:
            reveal_type(item)  # revealed: Literal[1]

def test_match_sequence_value_as_pattern_preserves_subject_type(
    value: tuple[Literal[1]],
) -> None:
    match value:
        case [1 as item]:
            reveal_type(item)  # revealed: Literal[1]

def test_match_sequence_wildcard_as_pattern_preserves_subject_type(
    value: tuple[Literal[1]],
) -> None:
    match value:
        case [_ as item]:
            reveal_type(item)  # revealed: Literal[1]

def test_match_sequence_as_pattern_excludes_previous_cases(
    value: tuple[Literal[1], object] | tuple[Literal[2], object],
) -> None:
    match value:
        case [1, _]:
            pass
        case [int() as item, _]:
            reveal_type(item)  # revealed: Literal[2]
```

An earlier OR alternative must only be removed when every value of its type is certain to match. A
protocol class pattern can still fail if a declared member is absent at runtime, so the later
sequence alternative remains possible:

```py
from typing import Protocol, runtime_checkable

@runtime_checkable
class HasX(Protocol):
    x: int

class Values(list[str]):
    x: int

def test_or_binding_keeps_values_that_can_fail_a_class_pattern(value: Values) -> None:
    match value:
        case (HasX() as item) | [item]:
            reveal_type(item)  # revealed: Values | str
```

## Declared pattern captures

A capture still has to satisfy an earlier declaration for the same name. This uses the same
assignment checks as other bindings; the declaration remains the authoritative type when the
captured value is incompatible.

```py
def test_incompatible_declared_capture(subject: int) -> None:
    item: str
    match subject:
        case item:  # error: [invalid-assignment]
            reveal_type(item)  # revealed: str

def test_incompatible_declared_sequence_capture(subject: tuple[int]) -> None:
    item: str
    match subject:
        case [item]:  # error: [invalid-assignment]
            reveal_type(item)  # revealed: str

def test_incompatible_declared_star_capture(subject: tuple[int, int]) -> None:
    rest: list[str]
    match subject:
        case [*rest]:  # error: [invalid-assignment]
            reveal_type(rest)  # revealed: list[str]

def test_incompatible_declared_or_capture(subject: int | str) -> None:
    item: int
    match subject:
        # TODO: Report one error for the logical OR-pattern binding instead of validating each
        # syntactic definition separately.
        # error: [invalid-assignment]
        # error: [invalid-assignment]
        case (int() as item) | (str() as item):
            reveal_type(item)  # revealed: int

def test_compatible_declared_alias(subject: object) -> None:
    item: int
    match subject:
        case int() as item:
            reveal_type(item)  # revealed: int
```

When an alias surrounds the whole pattern, it preserves the subject's type variable instead of
reconstructing a structural type from the pattern. If only some choices of a constrained type
variable can match, the binding keeps the type variable together with the sequence shape established
by the pattern.

```py
from typing import TypeVar

BoundSequenceT = TypeVar("BoundSequenceT", bound=tuple[object])
ConstrainedSequenceT = TypeVar(
    "ConstrainedSequenceT",
    tuple[int],
    tuple[str],
)
PartiallyMatchedSequenceT = TypeVar(
    "PartiallyMatchedSequenceT",
    tuple[int],
    tuple[int, int],
)
SequenceElementT = TypeVar("SequenceElementT")

def test_match_sequence_alias_preserves_bound_typevar(
    value: BoundSequenceT,
) -> BoundSequenceT:
    match value:
        case [_] as whole:
            reveal_type(whole)  # revealed: BoundSequenceT@test_match_sequence_alias_preserves_bound_typevar
            return whole

def test_match_sequence_alias_preserves_constrained_typevar(
    value: ConstrainedSequenceT,
) -> ConstrainedSequenceT:
    match value:
        case [_] as whole:
            # revealed: ConstrainedSequenceT@test_match_sequence_alias_preserves_constrained_typevar
            reveal_type(whole)
            return whole

def test_match_sequence_alias_narrows_constrained_typevar(
    value: PartiallyMatchedSequenceT,
) -> PartiallyMatchedSequenceT:
    match value:
        case [_] as whole:
            # revealed: PartiallyMatchedSequenceT@test_match_sequence_alias_narrows_constrained_typevar & tuple[int]
            reveal_type(whole)
            return whole
        case _:
            raise ValueError

def test_match_sequence_alias_preserves_typevar_union_arm(
    value: BoundSequenceT | str,
) -> BoundSequenceT:
    match value:
        case [_] as whole:
            # revealed: BoundSequenceT@test_match_sequence_alias_preserves_typevar_union_arm
            reveal_type(whole)
            return whole
        case _:
            raise ValueError

def test_match_sequence_alias_preserves_element_narrowing(
    value: list[SequenceElementT],
) -> int:
    match value:
        case [int()] as whole:
            # revealed: SequenceElementT@test_match_sequence_alias_preserves_element_narrowing & int
            reveal_type(whole[0])
            return whole[0]
        case _:
            return 0
```

## Recursive class pattern aliases

The same rule applies outside sequence patterns. Preserving a recursive alias lets later code keep
using the recursive relationship after a class pattern matches.

```toml
[environment]
python-version = "3.12"
```

```py
type RecursiveContainer = int | dict[str, RecursiveContainer] | list[RecursiveContainer]

def test_match_class_alias_preserves_recursive_containers(
    value: RecursiveContainer,
) -> None:
    match value:
        case dict() as mapping:
            reveal_type(mapping)  # revealed: dict[str, RecursiveContainer]
            mapping["bad"] = "bad"  # error: [invalid-assignment]
            for item in mapping.values():
                test_match_class_alias_preserves_recursive_containers(item)
        case list() as sequence:
            reveal_type(sequence)  # revealed: list[RecursiveContainer]
            sequence.append("bad")  # error: [invalid-argument-type]
            for item in sequence:
                test_match_class_alias_preserves_recursive_containers(item)
```

## Class pattern alias intersections

Class patterns retain intersections that can exist through multiple inheritance, but discard classes
that are known to be disjoint.

```py
from typing import final

class OverlapA: ...
class OverlapB: ...
class OverlapC(OverlapA, OverlapB): ...

def test_match_class_alias_preserves_possible_multiple_inheritance(
    value: OverlapA,
) -> None:
    match value:
        case OverlapB() as item:
            reveal_type(item)  # revealed: OverlapA & OverlapB

@final
class FinalA: ...

class FinalB: ...

def test_match_class_alias_rejects_disjoint_final_class(value: FinalA) -> None:
    match value:
        case FinalB() as item:
            reveal_type(item)  # revealed: Never
```

A `TypedDict` is a `dict` at runtime, so it can also satisfy a runtime-checkable protocol. Class
pattern filtering retains that structural overlap instead of treating every non-`dict` class as
disjoint.

```py
from typing import Protocol, TypedDict, runtime_checkable

class ProtocolPayload(TypedDict):
    value: int

@runtime_checkable
class SizedProtocol(Protocol):
    def __len__(self) -> int: ...

def test_match_typed_dict_alias_preserves_runtime_protocol_overlap(
    value: ProtocolPayload,
) -> None:
    match value:
        case SizedProtocol() as item:
            reveal_type(item)  # revealed: ProtocolPayload
```

Class patterns pass the type of each extracted attribute to their nested patterns. The surrounding
`as` pattern keeps the subject's original generic type or type variable.

```py
from dataclasses import dataclass
from typing import Generic, NamedTuple, TypeVar

T = TypeVar("T")

class PatternBox(Generic[T]):
    __match_args__ = ("value",)
    value: T

def test_match_class_keyword_capture(value: PatternBox[T]) -> T:
    match value:
        case PatternBox(value=item) as whole:
            reveal_type(item)  # revealed: T@test_match_class_keyword_capture
            reveal_type(whole)  # revealed: PatternBox[T@test_match_class_keyword_capture]
            return item

@dataclass
class DataclassBox(Generic[T]):
    value: T

def test_match_dataclass_positional_capture(dataclass_box: DataclassBox[T]) -> None:
    match dataclass_box:
        case DataclassBox(item):
            reveal_type(item)  # revealed: T@test_match_dataclass_positional_capture

class NamedPoint(NamedTuple):
    x: int
    label: str

def test_match_named_tuple_positional_captures(point: NamedPoint) -> None:
    match point:
        case NamedPoint(x, label):
            reveal_type(x)  # revealed: int
            reveal_type(label)  # revealed: str

def test_incompatible_declared_class_capture(value: PatternBox[int]) -> None:
    item: str
    match value:
        case PatternBox(value=item):  # error: [invalid-assignment]
            reveal_type(item)  # revealed: str
```

`__match_args__` is read through the pattern class and must identify literal attribute names. This
includes attributes provided by a metaclass. An explicit widened annotation does not tell us which
attribute a positional pattern extracts.

```py
class MatchArgsMeta(type):
    __match_args__ = ("value",)

class MetaclassMatchArgs(metaclass=MatchArgsMeta):
    value: int

def test_metaclass_match_args(value: MetaclassMatchArgs) -> None:
    match value:
        case MetaclassMatchArgs(item):
            reveal_type(item)  # revealed: int

class WidenedMatchArgs:
    __match_args__: tuple[str, ...] = ("value",)
    value: int

def test_widened_match_args_does_not_select_an_attribute(value: WidenedMatchArgs) -> None:
    match value:
        case WidenedMatchArgs(item):
            reveal_type(item)  # revealed: Unknown
```

Each union arm is checked against the complete class pattern before the extracted values are
combined. This keeps a tag, its payload, and the whole-pattern alias correlated. The same rule
applies through an `or` pattern.

```py
from typing import Generic, Literal, TypeVar

TagT = TypeVar("TagT")
PayloadT = TypeVar("PayloadT")

class TaggedPayload(Generic[TagT, PayloadT]):
    __match_args__ = ("tag", "payload")
    tag: TagT
    payload: PayloadT

def test_match_class_capture_filters_union_arms(
    value: TaggedPayload[Literal["int"], int] | TaggedPayload[Literal["str"], str],
) -> None:
    match value:
        case TaggedPayload("int", item) as whole:
            reveal_type(item)  # revealed: int
            reveal_type(whole)  # revealed: TaggedPayload[Literal["int"], int]

def test_match_class_or_pattern_filters_union_arms(
    value: TaggedPayload[Literal["int"], int] | TaggedPayload[Literal["str"], str] | TaggedPayload[Literal["bool"], bool],
) -> None:
    match value:
        case (TaggedPayload("int", item) | TaggedPayload("str", item)) as whole:
            reveal_type(item)  # revealed: int | str
            # revealed: TaggedPayload[Literal["int"], int] | TaggedPayload[Literal["str"], str]
            reveal_type(whole)

def test_match_builtin_match_self(
    value: list[int] | dict[str, int] | int,
) -> None:
    match value:
        case list(contents):
            reveal_type(contents)  # revealed: list[int]
        case dict(contents):
            reveal_type(contents)  # revealed: dict[str, int]
        case int(contents):
            reveal_type(contents)  # revealed: int

class OverlapCaptureA: ...

class OverlapCaptureB:
    member: int

class OverlapCaptureC(OverlapCaptureA, OverlapCaptureB): ...

def test_match_class_capture_preserves_possible_multiple_inheritance(
    value: OverlapCaptureA,
) -> None:
    match value:
        case OverlapCaptureB(member=item) as whole:
            reveal_type(item)  # revealed: int
            reveal_type(whole)  # revealed: OverlapCaptureA & OverlapCaptureB
```

Mapping patterns use the mapping's key and value types. A successful keyed pattern can filter union
arms, while `**rest` is always a new `dict` containing the unmatched items.

```py
from collections.abc import Mapping
from enum import IntEnum
from typing import Literal, TypeVar
from typing_extensions import Never

MappingValueT = TypeVar("MappingValueT")

def test_match_mapping_bindings(value: Mapping[str, MappingValueT]) -> MappingValueT:
    match value:
        case {"item": item, **rest} as whole:
            reveal_type(item)  # revealed: MappingValueT@test_match_mapping_bindings
            reveal_type(rest)  # revealed: dict[str, MappingValueT@test_match_mapping_bindings]
            # revealed: Mapping[str, MappingValueT@test_match_mapping_bindings]
            reveal_type(whole)
            return item
    raise ValueError

def test_match_dict_bindings(value: dict[str, int]) -> None:
    match value:
        case {"item": item, **rest} as whole:
            reveal_type(item)  # revealed: int
            reveal_type(rest)  # revealed: dict[str, int]
            reveal_type(whole)  # revealed: dict[str, int]

def test_incompatible_declared_mapping_captures(value: Mapping[str, int]) -> None:
    item: str
    rest: dict[str, str]
    match value:
        # error: [invalid-assignment]
        # error: [invalid-assignment]
        case {"item": item, **rest}:
            reveal_type(item)  # revealed: str
            reveal_type(rest)  # revealed: dict[str, str]

def test_match_mapping_key_filters_union_arms(
    value: dict[Literal["a"], int] | dict[Literal["b"], str],
) -> None:
    match value:
        case {"a": item} as whole:
            reveal_type(item)  # revealed: int
            reveal_type(whole)  # revealed: dict[Literal["a"], int]

class MappingKey(IntEnum):
    ITEM = 1

def test_match_mapping_intenum_key(
    value: dict[Literal[1], int],
) -> None:
    match value:
        case {MappingKey.ITEM: item}:
            reveal_type(item)  # revealed: int

def test_match_mapping_nested_sequence(
    value: Mapping[str, tuple[int, str]],
) -> None:
    match value:
        case {"pair": [number, text]}:
            reveal_type(number)  # revealed: int
            reveal_type(text)  # revealed: str

def test_match_mapping_rejects_empty_key_domain(
    value: dict[Never, int],
) -> None:
    match value:
        case {"item": item}:
            reveal_type(item)  # revealed: Never
```

For a `TypedDict`, a literal key uses the declared field type. Closed dictionaries can rule out
missing keys, and tagged unions remain correlated through `or` patterns.

```py
from typing import Literal
from typing_extensions import TypedDict

class IntPayload(TypedDict):
    tag: Literal["int"]
    value: int

class StrPayload(TypedDict):
    tag: Literal["str"]
    value: str

def test_match_typed_dict_capture_filters_union_arms(
    value: IntPayload | StrPayload,
) -> None:
    match value:
        case {"tag": "int", "value": item, **rest} as whole:
            reveal_type(item)  # revealed: int
            reveal_type(rest)  # revealed: dict[str, object]
            reveal_type(whole)  # revealed: IntPayload

class ClosedIntPayload(TypedDict, closed=True):
    tag: Literal["int"]
    value: int

class ClosedStrPayload(TypedDict, closed=True):
    tag: Literal["str"]
    value: str

class ClosedBoolPayload(TypedDict, closed=True):
    tag: Literal["bool"]
    value: bool

class ClosedPayload(TypedDict, closed=True):
    x: int

def test_match_closed_typed_dict_rejects_non_string_key(
    value: ClosedPayload,
) -> None:
    match value:
        case {1: item}:
            reveal_type(item)  # revealed: Never

def test_match_closed_typed_dict_rest(value: ClosedIntPayload) -> None:
    match value:
        case {"tag": "int", **rest}:
            reveal_type(rest)  # revealed: dict[str, object]

def test_match_typed_dict_or_pattern_filters_union_arms(
    value: ClosedIntPayload | ClosedStrPayload | ClosedBoolPayload,
) -> None:
    match value:
        case ({"tag": "int", "value": item} | {"tag": "str", "value": item}) as whole:
            reveal_type(item)  # revealed: int | str
            reveal_type(whole)  # revealed: ClosedIntPayload | ClosedStrPayload
```

## Sequence exhaustiveness

Sequence patterns also contribute to negative narrowing and exhaustiveness. Exact tuple shapes can
make a match exhaustive.

```py
from typing_extensions import assert_never

def test_match_exact_tuple_sequence(subj: tuple[int | str, int | str]) -> None:
    match subj:
        case x, str():
            # TODO: This should simplify to `tuple[int | str, str]`.
            # revealed: tuple[int | str, int | str] & <Protocol with members '__getitem__', '__len__'>
            reveal_type(subj)
            reveal_type(subj[0])  # revealed: int | str
            reveal_type(subj[1])  # revealed: str
            first, second = subj
            reveal_type(first)  # revealed: int | str
            # TODO: This should reveal `str`.
            reveal_type(second)  # revealed: int | str
        case y:
            # TODO: This should simplify to `tuple[int | str, int]`.
            # revealed: tuple[int | str, int | str] & ~<Protocol with members '__getitem__', '__len__'>
            reveal_type(subj)
            reveal_type(subj[0])  # revealed: int | str
            # TODO: This should reveal `int` once we simplify the negative
            # intersection above.
            reveal_type(subj[1])  # revealed: int | str

def test_match_exact_tuple_sequence_is_exhaustive(value: int | tuple[int, int]) -> int:
    match value:
        case int(value):
            return value
        case (left, right):
            return left + right
        case _:
            assert_never(value)

def test_match_exact_tuple_element_union_is_exhaustive(x: tuple[int | str]) -> int:  # error: [invalid-return-type]
    match x:
        case [int()]:
            return 42
        case [str()]:
            return 42
        case _:
            # TODO: The previous cases are exhaustive, so this should simplify
            # to `tuple[Never]`, and therefore `Never`.
            # revealed: tuple[int | str] & ~<Protocol with members '__getitem__', '__len__'> & ~<Protocol with members '__getitem__', '__len__'>
            reveal_type(x)

def test_match_exact_mutable_sequence_negative(value: list[int]) -> None:
    match value:
        case [int()]:
            pass
        case _:
            # revealed: list[int] & ~<Protocol with members '__getitem__', '__len__'>
            reveal_type(value)
```

## Nested sequence patterns

Nested patterns narrow the fixed positions they inspect. The narrowed element types remain available
through later indexing and destructuring.

```py
def normalize_nested_record(value: object) -> tuple[None, int, int] | None:
    match value:
        case [None, [int()], {}]:
            ret = value[0], value[1][0], len(value[2])
            reveal_type(ret)  # revealed: tuple[None, int, int]
            return ret
    return None

def unwrap_number_or_label(value: object) -> int | str | None:
    match value:
        case [(int() | str()) as item]:
            reveal_type(value[0])  # revealed: int | str
            return value[0]
    return None
```

## Sequence display subjects

A tuple or list display has no place of its own to narrow. A successful sequence pattern instead
narrows the corresponding narrowable elements. If a multi-element pattern fails, we do not know
which element failed to match.

```py
class TupleSubjectA: ...
class TupleSubjectA1(TupleSubjectA): ...
class TupleSubjectB: ...
class TupleSubjectB1(TupleSubjectB): ...

def match_tuple_expression_subject(a: TupleSubjectA, b: TupleSubjectB) -> None:
    match a, b:
        case [TupleSubjectA1(), TupleSubjectB1()]:
            reveal_type(a)  # revealed: TupleSubjectA1
            reveal_type(b)  # revealed: TupleSubjectB1
        case _:
            reveal_type(a)  # revealed: TupleSubjectA
            reveal_type(b)  # revealed: TupleSubjectB

    reveal_type(a)  # revealed: TupleSubjectA
    reveal_type(b)  # revealed: TupleSubjectB

def match_list_expression_subject(a: TupleSubjectA, b: TupleSubjectB) -> None:
    match [a, b]:
        case [TupleSubjectA1(), TupleSubjectB1()]:
            reveal_type(a)  # revealed: TupleSubjectA1
            reveal_type(b)  # revealed: TupleSubjectB1
```

## Nested sequence display subjects

Element narrowing recurses through nested tuple and list displays. Attributes and subscripts are
narrowed when they occupy a fixed position. Dictionary displays and starred subject elements do not
yet have a fixed element-to-pattern correspondence.

```py
class TupleSubjectA: ...
class TupleSubjectA1(TupleSubjectA): ...
class TupleSubjectB: ...
class TupleSubjectB1(TupleSubjectB): ...

class SequenceSubjectContainer:
    a: TupleSubjectA

def match_nested_sequence_expression_subject(
    container: SequenceSubjectContainer,
    values: list[TupleSubjectB],
) -> None:
    match [[container.a], values[0], object()]:
        case [[TupleSubjectA1()], TupleSubjectB1(), _]:
            reveal_type(container.a)  # revealed: TupleSubjectA1
            reveal_type(values[0])  # revealed: TupleSubjectB1

def match_mapping_expression_subject(value: object) -> None:
    match [{"value": value}]:
        case [{"value": int()}]:
            reveal_type(value)  # revealed: object

def match_starred_list_expression_subject(
    a: TupleSubjectA,
    values: list[object],
) -> None:
    match [a, *values]:
        case [TupleSubjectA1()]:
            reveal_type(a)  # revealed: TupleSubjectA
```

## Sequence pattern forms for display subjects

Element narrowing respects later cases, OR patterns, impossible alternatives, repeated subject
expressions, and starred sequence patterns.

```py
class TupleSubjectA: ...
class TupleSubjectA1(TupleSubjectA): ...
class TupleSubjectA2(TupleSubjectA): ...
class TupleSubjectB: ...
class TupleSubjectB1(TupleSubjectB): ...
class TupleSubjectB2(TupleSubjectB): ...

def match_tuple_expression_later_case(a: TupleSubjectA, b: TupleSubjectB) -> None:
    match a, b:
        case [TupleSubjectA2(), TupleSubjectB2()]:
            pass
        case [TupleSubjectA1(), TupleSubjectB1()]:
            reveal_type(a)  # revealed: TupleSubjectA1
            reveal_type(b)  # revealed: TupleSubjectB1

def match_tuple_expression_or_pattern(a: TupleSubjectA, b: TupleSubjectB) -> None:
    match a, b:
        case [TupleSubjectA1(), TupleSubjectB1()] | [*_]:
            # The second alternative does not constrain either tuple element.
            reveal_type(a)  # revealed: TupleSubjectA
            reveal_type(b)  # revealed: TupleSubjectB

def match_tuple_expression_constrained_or_pattern(
    a: TupleSubjectA,
    b: TupleSubjectB,
) -> None:
    match a, b:
        case [TupleSubjectA1(), TupleSubjectB1()] | [TupleSubjectA2(), TupleSubjectB2()]:
            reveal_type(a)  # revealed: TupleSubjectA1 | TupleSubjectA2
            reveal_type(b)  # revealed: TupleSubjectB1 | TupleSubjectB2

def match_tuple_expression_or_impossible_alternative(
    a: TupleSubjectA,
    b: TupleSubjectB,
) -> None:
    match a, b:
        case [TupleSubjectA1()] | [TupleSubjectA2(), TupleSubjectB1()]:
            reveal_type(a)  # revealed: TupleSubjectA2
            reveal_type(b)  # revealed: TupleSubjectB1

def match_repeated_tuple_expression_subject(a: TupleSubjectA) -> None:
    match a, a:
        case [TupleSubjectA1(), TupleSubjectA()]:
            reveal_type(a)  # revealed: TupleSubjectA1

def match_tuple_expression_starred_pattern(
    a: TupleSubjectA,
    middle: object,
    b: TupleSubjectB,
) -> None:
    match a, middle, b:
        case [TupleSubjectA1(), *_, TupleSubjectB1()]:
            reveal_type(a)  # revealed: TupleSubjectA1
            reveal_type(middle)  # revealed: object
            reveal_type(b)  # revealed: TupleSubjectB1
```

## Bindings used in match subjects

Each element is narrowed using the binding that Python read when it evaluated that part of the
subject. A later assignment in another element, pattern capture, or guard does not change which
binding the earlier element referred to.

```py
from typing import final

class TupleSubjectA: ...
class TupleSubjectA1(TupleSubjectA): ...
class TupleSubjectA2(TupleSubjectA): ...
class TupleSubjectB: ...
class TupleSubjectB1(TupleSubjectB): ...
class ReboundTupleSubject: ...

@final
class ReboundTupleSubject1(ReboundTupleSubject): ...

@final
class ReboundTupleSubject2(ReboundTupleSubject): ...

def match_tuple_expression_rebound_subject(a: ReboundTupleSubject) -> None:
    match a, (a := ReboundTupleSubject2()), a:
        case [ReboundTupleSubject1(), ReboundTupleSubject2(), ReboundTupleSubject2()]:
            reveal_type(a)  # revealed: ReboundTupleSubject2
            1 + "x"  # error: [unsupported-operator]

def match_tuple_expression_multiple_bindings(flag: bool, b: TupleSubjectB) -> None:
    if flag:
        a: TupleSubjectA = TupleSubjectA1()
    else:
        a = TupleSubjectA2()

    match a, b:
        case [TupleSubjectA1(), TupleSubjectB1()]:
            reveal_type(a)  # revealed: TupleSubjectA1
            reveal_type(b)  # revealed: TupleSubjectB1

def match_tuple_expression_subject_capture(
    a: TupleSubjectA | TupleSubjectB,
    b: TupleSubjectB,
) -> None:
    match a, b:
        case [TupleSubjectA1(), a]:
            reveal_type(a)  # revealed: TupleSubjectB

def match_capture_shadows_subject() -> None:
    x = (1,)
    match x:
        case [x]:
            reveal_type(x)  # revealed: Literal[1]

def match_tuple_expression_guard_rebinding(
    a: TupleSubjectA,
    b: TupleSubjectB,
    flag: bool,
) -> None:
    match a, b:
        case [TupleSubjectA1(), TupleSubjectB1()] if (a := TupleSubjectA2()) and flag:
            pass
        case [TupleSubjectA1(), TupleSubjectB1()]:
            reveal_type(a)  # revealed: TupleSubjectA1 | TupleSubjectA2
            reveal_type(b)  # revealed: TupleSubjectB1
```

## Named-expression subjects

A named expression creates a new binding for the subject. The successful pattern narrows that
binding just like it narrows a subject that was already bound.

```py
class NamedSubject: ...

class NamedSubjectChild(NamedSubject):
    child: int

def match_named_expression_subject(value: NamedSubject) -> None:
    match subject := value:
        case NamedSubjectChild():
            reveal_type(subject)  # revealed: NamedSubjectChild
            reveal_type(subject.child)  # revealed: int

def match_named_expression_subject_capture(value: tuple[int]) -> None:
    match subject := value:
        case [subject]:
            # The capture shadows the named-expression binding and receives the element type.
            reveal_type(subject)  # revealed: int
```

## Cycles in pattern binding types

Pattern captures can affect the type of a later match subject, including through a loop or a
function defined before the capture. These cycles should resolve to the same concrete binding types
as equivalent code without a cycle.

```py
def match_loop_carried_capture(flag: bool, x: int) -> None:
    while flag:
        match x:
            case x:
                reveal_type(x)  # revealed: int

def match_loop_carried_sequence_capture(flag: bool) -> None:
    x = (1,)
    while flag:
        match x:
            case [x]:
                reveal_type(x)  # revealed: Literal[1]

def capture_from_later_global() -> int:
    return captured

match capture_from_later_global():
    case captured:
        reveal_type(captured)  # revealed: int
```

## Value patterns

Value patterns are evaluated by equality, which is overridable. Therefore successfully matching on
one can only give us information where we know how the subject type implements equality.

Consider the following example.

```py
from typing import Literal

def _(x: Literal["foo"] | int):
    match x:
        case "foo":
            reveal_type(x)  # revealed: Literal["foo"] | int

    match x:
        case "bar":
            reveal_type(x)  # revealed: int
```

In the first `match`'s `case "foo"` all we know is `x == "foo"`. `x` could be an instance of an
arbitrary `int` subclass with an arbitrary `__eq__`, so we can't actually narrow to
`Literal["foo"]`.

In the second `match`'s `case "bar"` we know `x == "bar"`. As discussed above, this isn't enough to
rule out `int`, but we know that `"foo" == "bar"` is false so we can eliminate `Literal["foo"]`.

A final subclass with inherited builtin equality can compare equal to a literal despite being
disjoint from the literal's type. This applies both to literal patterns and dotted value patterns:

```py
from typing import Final, final

@final
class FinalPatternInt(int): ...

class PatternValues:
    ONE: Final = 1

def _(value: FinalPatternInt):
    match value:
        case 1 as captured:
            reveal_type(value)  # revealed: FinalPatternInt
            reveal_type(captured)  # revealed: FinalPatternInt

    match value:
        case PatternValues.ONE:
            reveal_type(value)  # revealed: FinalPatternInt
```

More examples follow.

```py
from typing import Literal

class C:
    pass

def _(x: Literal["foo", "bar", 42, b"foo"] | bool | complex):
    match x:
        case "foo":
            reveal_type(x)  # revealed: Literal["foo"] | int | float | complex
        case 42:
            reveal_type(x)  # revealed: int | float | complex
        case 6.0:
            reveal_type(x)  # revealed: Literal["bar", b"foo"] | (int & ~Literal[42]) | float | complex
        case 1j:
            reveal_type(x)  # revealed: Literal["bar", b"foo"] | (int & ~Literal[42]) | float | complex
        case b"foo":
            reveal_type(x)  # revealed: (int & ~Literal[42]) | Literal[b"foo"] | float | complex
        case _:
            reveal_type(x)  # revealed: Literal["bar"] | (int & ~Literal[42]) | float | complex
```

The same limitation applies when a value pattern appears inside a sequence: matching a literal
proves equality, but not that the element has the literal's nominal type.

```py
def test_match_value_sequence(value: object) -> None:
    match value:
        case [1]:
            reveal_type(value[0])  # revealed: object
```

## Enum equality semantics

Enum value patterns use the enum class's actual `__eq__` implementation. Members of `StrEnum`
therefore compare equal to string literals with the same value:

```toml
[environment]
python-version = "3.11"
```

```py
from enum import Enum, StrEnum
from typing import Literal, assert_never, reveal_type

class Color(StrEnum):
    RED = "r"
    GREEN = "g"
    BLUE = "b"

def test_literal_as_enum(x: Literal["g"]) -> None:
    match x:
        case Color.RED:
            assert_never(x)
        case Color.GREEN:
            reveal_type(x)  # revealed: Literal["g"]
        case Color.BLUE:
            assert_never(x)
        case _:
            assert_never(x)

def test_enum_as_literal(y: Literal[Color.BLUE]) -> None:
    match y:
        case "r":
            assert_never(y)
        case "g":
            assert_never(y)
        case "b":
            reveal_type(y)  # revealed: Literal[Color.BLUE]
        case _:
            assert_never(y)

class AlwaysEqual(Enum):
    RED = "r"
    GREEN = "g"

    def __eq__(self, other: object) -> bool:
        return True

def custom_eq(value: AlwaysEqual) -> None:
    match value:
        case AlwaysEqual.RED:
            reveal_type(value)  # revealed: AlwaysEqual
        case AlwaysEqual.GREEN:
            reveal_type(value)  # revealed: AlwaysEqual
        case _:
            reveal_type(value)  # revealed: AlwaysEqual
```

Equality also determines the type of captures later in a sequence. An `IntEnum` member can match an
integer, and custom equality can make otherwise distinct enum members compare equal, so the capture
keeps the type of the subject that actually matched.

```py
from enum import Enum, IntEnum
from typing import Literal

class Number(IntEnum):
    ONE = 1

def test_match_capture_preserves_int_enum_equal_member(
    value: tuple[Literal[1], int],
) -> None:
    match value:
        case [Number.ONE, item]:
            reveal_type(item)  # revealed: int

class AlwaysEqualEnum(Enum):
    A = 1
    B = 2

    def __eq__(self, other: object) -> Literal[True]:
        return True

def test_match_capture_preserves_custom_equal_enum_member() -> None:
    value = (AlwaysEqualEnum.B, "actual")
    match value:
        case [AlwaysEqualEnum.A, item]:
            reveal_type(item)  # revealed: Literal["actual"]
```

A fallback alias can still receive a value that failed an earlier value pattern. Match patterns use
`==`, so a non-reflexive value can fail to match itself, while a custom `__ne__` has no effect.

```py
from typing import Literal

class AliasNeverEqualMeta(type):
    def __eq__(cls, other: object) -> Literal[False]:
        return False

class AliasNeverEqualValue(metaclass=AliasNeverEqualMeta):
    pass

class NeverEqualConstants:
    VALUE = AliasNeverEqualValue

def test_match_alias_preserves_nonreflexive_value(flag: bool) -> None:
    value = AliasNeverEqualValue if flag else "fallback"
    match value:
        case NeverEqualConstants.VALUE:
            pass
        case _ as item:
            # revealed: <class 'AliasNeverEqualValue'> | Literal["fallback"]
            reveal_type(item)

class CustomNeMeta(type):
    def __ne__(cls, other: object) -> Literal[True]:
        return True

class CustomNeA(metaclass=CustomNeMeta):
    pass

class CustomNeConstants:
    A = CustomNeA

def test_match_alias_ignores_custom_ne(flag: bool) -> str:
    value = CustomNeA if flag else "fallback"
    match value:
        case CustomNeConstants.A:
            return ""
        case _ as item:
            reveal_type(item)  # revealed: Literal["fallback"]
            return item
```

## Value patterns with guard

```py
from typing import Literal

class C:
    pass

def _(x: Literal["foo", b"bar"] | int):
    match x:
        case "foo" if reveal_type(x):  # revealed: Literal["foo"] | int
            pass
        case b"bar" if reveal_type(x):  # revealed: Literal[b"bar"] | int
            pass
        case 42 if reveal_type(x):  # revealed: int
            pass
```

## Or patterns

```py
from typing import Literal
from enum import Enum

class Color(Enum):
    RED = 1
    GREEN = 2
    BLUE = 3

def _(color: Color):
    match color:
        case Color.RED | Color.GREEN:
            reveal_type(color)  # revealed: Literal[Color.RED, Color.GREEN]
        case Color.BLUE:
            reveal_type(color)  # revealed: Literal[Color.BLUE]

    match color:
        case Color.RED | Color.GREEN | Color.BLUE:
            reveal_type(color)  # revealed: Color

    match color:
        case Color.RED:
            reveal_type(color)  # revealed: Literal[Color.RED]
        case _:
            reveal_type(color)  # revealed: Literal[Color.GREEN, Color.BLUE]

class A: ...
class B: ...
class C: ...

def _(x: A | B | C):
    match x:
        case A() | B():
            reveal_type(x)  # revealed: A | B
        case C():
            reveal_type(x)  # revealed: C & ~A & ~B
        case _:
            reveal_type(x)  # revealed: Never

def _(x: A | B | C):
    match x:
        case A() | B() | C():
            reveal_type(x)  # revealed: A | B | C
        case _:
            reveal_type(x)  # revealed: Never

def _(x: A | B | C):
    match x:
        case A():
            reveal_type(x)  # revealed: A
        case _:
            reveal_type(x)  # revealed: (B & ~A) | (C & ~A)
```

Every `or` alternative binds the same names, but each alternative can give them a different type.
The binding combines the type from each reachable alternative. Because alternatives are tried from
left to right, a later alternative sees only values not matched earlier.

```py
from typing import Literal

def test_match_sequence_or_as_pattern(value: object) -> None:
    match value:
        case [int() as item, _] | [str() as item, _]:
            reveal_type(item)  # revealed: int | str

def test_match_ordered_or_capture(value: tuple[int] | str) -> int | str:
    match value:
        case [item] | item:
            reveal_type(item)  # revealed: int | str
            return item

def test_match_ordered_or_capture_after_star(
    value: tuple[Literal[1], int] | tuple[Literal[2], str],
) -> list[int] | Literal[2]:
    match value:
        case [1, *item] | [item, _]:
            reveal_type(item)  # revealed: list[int] | Literal[2]
            return item
```

## Or patterns with guard

```py
from typing import Literal

def _(x: Literal["foo", b"bar"] | int):
    match x:
        case "foo" | 42 if reveal_type(x):  # revealed: Literal["foo"] | int
            pass
        case b"bar" if reveal_type(x):  # revealed: Literal[b"bar"] | int
            pass
        case _ if reveal_type(x):  # revealed: Literal["foo", b"bar"] | int
            pass
```

## Narrowing due to guard

```py
def _(x: object):
    match x:
        case str() | float() if type(x) is str:
            reveal_type(x)  #  revealed: str
        case "foo" | 42 | None if isinstance(x, int):
            reveal_type(x)  #  revealed: int
        case False if x:
            reveal_type(x)  #  revealed: Never
        case "foo" if x := "bar":
            reveal_type(x)  # revealed: Literal["bar"]
```

## Guard and reveal_type in guard

```py
def get_object() -> object:
    return object()

x = get_object()

reveal_type(x)  # revealed: object

match x:
    case str() | float() if type(x) is str and reveal_type(x):  # revealed: str
        pass
    case "foo" | 42 | None if isinstance(x, int) and reveal_type(x):  #  revealed: int
        pass
    case False if x and reveal_type(x):  #  revealed: Never
        pass
    case "foo" if (x := "bar") and reveal_type(x):  #  revealed: Literal["bar"]
        pass

reveal_type(x)  # revealed: object
```

## Narrowing on `Self` in `match` statements

When performing narrowing on `self` inside methods on enums, we take into account that `Self` might
refer to a subtype of the enum class, like `Literal[Answer.YES]`. This is why we do not simplify
`Self & ~Literal[Answer.YES]` to `Literal[Answer.NO, Answer.MAYBE]`. Otherwise, we wouldn't be able
to return `self` in the `assert_yes` method below:

```py
from enum import Enum
from typing_extensions import Self, assert_never

class Answer(Enum):
    NO = 0
    YES = 1
    MAYBE = 2

    def is_yes_through_class_member(self) -> bool:
        reveal_type(self)  # revealed: Self@is_yes_through_class_member

        match self:
            case Answer.YES:
                reveal_type(self)  # revealed: Self@is_yes_through_class_member
                return True
            case Answer.NO | Answer.MAYBE:
                reveal_type(self)  # revealed: Self@is_yes_through_class_member & ~Literal[Answer.YES]
                return False
            case _:
                assert_never(self)  # no error

    def is_yes_through_self_member(self) -> bool:
        match self:
            case self.YES:
                reveal_type(self)  # revealed: Self@is_yes_through_self_member
                return True
            case self.NO | self.MAYBE:
                reveal_type(self)  # revealed: Self@is_yes_through_self_member & ~Literal[Answer.YES]
                return False
            case _:
                assert_never(self)  # no error

    @classmethod
    def is_yes_through_cls_member(cls, answer: "Answer") -> bool:
        reveal_type(cls.YES)  # revealed: Literal[Answer.YES]

        match answer:
            case cls.YES:
                reveal_type(answer)  # revealed: Literal[Answer.YES]
                return True
            case cls.NO | cls.MAYBE:
                reveal_type(answer)  # revealed: Literal[Answer.NO, Answer.MAYBE]
                return False
            case _:
                assert_never(answer)  # no error

    def assert_yes(self) -> Self:
        reveal_type(self)  # revealed: Self@assert_yes

        match self:
            case Answer.YES:
                reveal_type(self)  # revealed: Self@assert_yes
                return self
            case _:
                reveal_type(self)  # revealed: Self@assert_yes & ~Literal[Answer.YES]
                raise ValueError("Answer is not YES")

Answer.YES.is_yes_through_class_member()

try:
    reveal_type(Answer.MAYBE.assert_yes())  # revealed: Literal[Answer.MAYBE]
except ValueError:
    pass
```

## Narrowing is preserved when a terminal branch prevents a path from flowing through

When one branch of a `match` statement is terminal (e.g. contains `raise`), narrowing from the
non-terminal branches is preserved after the merge point.

```py
class A: ...
class B: ...
class C: ...

def _(x: A | B | C):
    match x:
        case A():
            pass
        case B():
            pass
        case _:
            raise ValueError()

    reveal_type(x)  # revealed: B | A
```

Reassignment in non-terminal branches is also preserved when the default branch is terminal:

```py
def _(number_of_periods: int | None, interval: str):
    match interval:
        case "monthly":
            if number_of_periods is None:
                number_of_periods = 1
        case "daily":
            if number_of_periods is None:
                number_of_periods = 30
        case _:
            raise ValueError("unsupported interval")

    reveal_type(number_of_periods)  # revealed: int
```

## Narrowing tagged unions of tuples

Narrow unions of tuples based on literal tag elements in `match` statements:

```py
from typing import Literal

class A: ...
class B: ...
class C: ...

def _(x: tuple[Literal["tag1"], A] | tuple[Literal["tag2"], B, C]):
    match x[0]:
        case "tag1":
            reveal_type(x)  # revealed: tuple[Literal["tag1"], A]
            reveal_type(x[1])  # revealed: A
        case "tag2":
            reveal_type(x)  # revealed: tuple[Literal["tag2"], B, C]
            reveal_type(x[1])  # revealed: B
            reveal_type(x[2])  # revealed: C
        case _:
            reveal_type(x)  # revealed: Never

# With int literals
def _(x: tuple[Literal[1], A] | tuple[Literal[2], B]):
    match x[0]:
        case 1:
            reveal_type(x)  # revealed: tuple[Literal[1], A]
        case 2:
            reveal_type(x)  # revealed: tuple[Literal[2], B]
        case _:
            reveal_type(x)  # revealed: Never

# With bytes literals
def _(x: tuple[Literal[b"a"], A] | tuple[Literal[b"b"], B]):
    match x[0]:
        case b"a":
            reveal_type(x)  # revealed: tuple[Literal[b"a"], A]
        case b"b":
            reveal_type(x)  # revealed: tuple[Literal[b"b"], B]
        case _:
            reveal_type(x)  # revealed: Never

# Using index 1 instead of 0
def _(x: tuple[A, Literal["tag1"]] | tuple[B, Literal["tag2"]]):
    match x[1]:
        case "tag1":
            reveal_type(x)  # revealed: tuple[A, Literal["tag1"]]
        case "tag2":
            reveal_type(x)  # revealed: tuple[B, Literal["tag2"]]
        case _:
            reveal_type(x)  # revealed: Never
```

Narrowing is restricted to `Literal` tag elements:

```py
def _(x: tuple[Literal["tag1"], A] | tuple[str, B]):
    match x[0]:
        case "tag1":
            # Can't narrow because second tuple has `str` (not literal) at index 0
            reveal_type(x)  # revealed: tuple[Literal["tag1"], A] | tuple[str, B]
        case _:
            # But we *can* narrow with inequality
            reveal_type(x)  # revealed: tuple[str, B]
```

and it is also restricted to `match` patterns that solely consist of value patterns:

```py
class Config:
    MODE: str = "default"

def _(u: tuple[Literal["foo"], int] | tuple[Literal["bar"], str]):
    match u[0]:
        case Config.MODE | "foo":
            # Config.mode has type `str` (not a literal), which could match
            # any string value at runtime. We cannot narrow based on "foo" alone
            # because the actual match might have been against Config.mode.
            reveal_type(u)  # revealed: tuple[Literal["foo"], int] | tuple[Literal["bar"], str]
        case "bar":
            # Since the previous case could match any string, this case can
            # still narrow to `tuple[Literal["bar"], str]` when `u[0]` equals "bar".
            reveal_type(u)  # revealed: tuple[Literal["bar"], str]
```

## Narrowing tagged unions of nominal classes by attribute

```py
from typing import Literal

class A:
    tag: Literal["a"]
    field_a: int

class B:
    tag: Literal["b"]
    field_b: str

def _(x: A | B):
    match x.tag:
        case "a":
            reveal_type(x)  # revealed: A
            reveal_type(x.field_a)  # revealed: int
        case "b":
            reveal_type(x)  # revealed: B
            reveal_type(x.field_b)  # revealed: str
        case _:
            reveal_type(x)  # revealed: Never
```

Non-literal tag arms are preserved during positive narrowing:

```py
from typing import Literal

class A:
    tag: Literal["a"]

class B:
    tag: str

class C:
    tag: Literal["c"]

def _(x: A | B | C):
    match x.tag:
        case "a":
            reveal_type(x)  # revealed: A | B
        case _:
            reveal_type(x)  # revealed: B | C
```
