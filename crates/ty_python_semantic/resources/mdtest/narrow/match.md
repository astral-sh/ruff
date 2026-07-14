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
from collections.abc import Iterable, Mapping, Sequence
from typing import Any
from typing_extensions import TypedDict

def test_isinstance(x: dict[Any, Any] | int) -> None:
    if isinstance(x, Mapping):
        reveal_type(x)  # revealed: dict[Any, Any]
    else:
        reveal_type(x)  # revealed: int & ~Top[Mapping[Unknown, object]]

def test_match(x: dict[Any, Any] | int) -> None:
    match x:
        case {}:
            reveal_type(x)  # revealed: dict[Any, Any]
        case _:
            reveal_type(x)  # revealed: int & ~Top[Mapping[Unknown, object]]

def test_match_double_star(x: dict[Any, Any] | int) -> None:
    match x:
        case {**rest}:
            reveal_type(x)  # revealed: dict[Any, Any]
        case _:
            reveal_type(x)  # revealed: int & ~Top[Mapping[Unknown, object]]

def test_match_refutable(x: dict[Any, Any] | int) -> None:
    match x:
        case {"k": _}:
            reveal_type(x)  # revealed: dict[Any, Any]
        case _:
            reveal_type(x)  # revealed: dict[Any, Any] | int

def test_known_mapping_overlap(
    x: Mapping[str, int] | Iterable[tuple[str, int]],
) -> None:
    match x:
        case {}:
            reveal_type(x)  # revealed: Mapping[str, int] | (Iterable[tuple[str, int]] & Top[Mapping[Unknown, object]])

def test_synthetic_interface_overlap(
    x: Mapping[str, int] | Sequence[int],
) -> None:
    match x:
        case {}:
            reveal_type(x)  # revealed: Mapping[str, int]

class Payload(TypedDict):
    key: int

def test_typed_dict_mapping_pattern(x: Payload | dict[str, int]) -> None:
    match x:
        case {}:
            reveal_type(x)  # revealed: Payload | dict[str, int]

def test_typed_dict_class_pattern(x: Payload | dict[str, int]) -> None:
    match x:
        case dict():
            reveal_type(x)  # revealed: Payload | dict[str, int]
```

## Mapping patterns with strict subclass narrowing

```toml
[analysis]
strict-subclass-narrowing = true
```

```py
from collections.abc import Mapping
from typing import Any

def test_isinstance(x: dict[Any, Any] | int) -> None:
    if isinstance(x, Mapping):
        reveal_type(x)  # revealed: dict[Any, Any] | (int & Top[Mapping[Unknown, object]])

def test_match(x: dict[Any, Any] | int) -> None:
    match x:
        case {}:
            reveal_type(x)  # revealed: dict[Any, Any] | (int & Top[Mapping[Unknown, object]])
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
            reveal_type(x)  # revealed: (Sequence[int] & str) | bytes | bytearray | (int & ~Sequence[object])

def test_direct_sequence_arm(x: list[int] | int) -> None:
    match x:
        case [*rest]:
            reveal_type(x)  # revealed: list[int]
            reveal_type(rest)  # revealed: list[int]

def test_match_star_excludes_text_and_bytes(x: str | bytes | bytearray | list[int]) -> None:
    match x:
        case [*rest]:
            reveal_type(x)  # revealed: list[int]
        case _:
            reveal_type(x)  # revealed: str | bytes | bytearray

def test_match_exact_sequence_excludes_str(x: str | tuple[int, int]) -> None:
    match x:
        case (a, b):
            reveal_type(a)  # revealed: int
            reveal_type(b)  # revealed: int
        case _:
            reveal_type(x)  # revealed: str

def test_match_exact_sequence_excludes_bytes(x: bytes | tuple[int, int]) -> None:
    match x:
        case (a, b):
            reveal_type(a)  # revealed: int
            reveal_type(b)  # revealed: int
        case _:
            reveal_type(x)  # revealed: bytes

def test_match_exact_sequence_excludes_bytearray(x: bytearray | tuple[int, int]) -> None:
    match x:
        case (a, b):
            reveal_type(a)  # revealed: int
            reveal_type(b)  # revealed: int
        case _:
            reveal_type(x)  # revealed: bytearray

def test_match_exact_object_sequence(value: object) -> None:
    match value:
        case int(), str():
            # revealed: Sequence[object] & ~str & ~bytes & ~bytearray
            reveal_type(value)
            reveal_type(len(value))  # revealed: int
            reveal_type(value[0])  # revealed: object
            reveal_type(value[1])  # revealed: object

def test_match_empty_object_sequence(value: object) -> None:
    match value:
        case []:
            # revealed: Sequence[object] & ~str & ~bytes & ~bytearray
            reveal_type(value)
            reveal_type(len(value))  # revealed: int

def test_match_singleton_object_sequence(value: object) -> None:
    match value:
        case [int()]:
            # revealed: Sequence[object] & ~str & ~bytes & ~bytearray
            reveal_type(value)
            reveal_type(len(value))  # revealed: int
            reveal_type(value[0])  # revealed: object

def test_match_prefix_star_object_sequence(value: object) -> None:
    match value:
        case [int(), *rest]:
            # revealed: Sequence[object] & ~str & ~bytes & ~bytearray
            reveal_type(value)
            reveal_type(len(value))  # revealed: int
            reveal_type(value[0])  # revealed: object
            reveal_type(value[1])  # revealed: object

def test_match_prefix_and_suffix_star_object_sequence(value: object) -> None:
    match value:
        case [int(), *rest, str()]:
            # revealed: Sequence[object] & ~str & ~bytes & ~bytearray
            reveal_type(value)
            reveal_type(value[0])  # revealed: object
            reveal_type(value[-1])  # revealed: object
            reveal_type(value[1])  # revealed: object

def test_match_prefix_star_known_sequence(value: Sequence[int | str]) -> None:
    match value:
        case [int(), *rest]:
            reveal_type(value[0])  # revealed: int | str
            reveal_type(value[1])  # revealed: int | str
            reveal_type(rest)  # revealed: list[int | str]
```

## Sequence patterns with strict subclass narrowing

```toml
[analysis]
strict-subclass-narrowing = true
```

```py
def test_match_star(x: list[int] | int) -> None:
    match x:
        case [*rest]:
            reveal_type(x)  # revealed: list[int] | (int & Sequence[object])
            reveal_type(rest)  # revealed: list[int] | list[object]
```

## Sequence capture types

A capture gets its type from the sequence element it binds. A starred capture is always a list. For
a fixed-length tuple, we can determine exactly which elements appear in that list.

```py
from typing import Any, Literal, TypeVar
from ty_extensions import Unknown

BoundTupleT = TypeVar("BoundTupleT", bound=tuple[int] | tuple[str])

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

# A nested capture receives the element type from a type variable's bound, rather than the type
# variable that represents the complete sequence.
def test_capture_from_typevar_bound(value: BoundTupleT) -> None:
    match value:
        case [item]:
            reveal_type(item)  # revealed: int | str

def match_nested_tuple_captures(
    subject: tuple[Literal[1], str, tuple[Literal[2], int]],
) -> None:
    match subject:
        case [1, item1, [2, item2]]:
            reveal_type(item1)  # revealed: str
            reveal_type(item2)  # revealed: int

def match_nested_list_of_tuples_captures(
    subject: list[tuple[Literal[1], bytes]],
) -> None:
    match subject:
        case [(1, item)]:
            reveal_type(item)  # revealed: bytes
```

## Captures from unions of tuples

When a union contains several tuple types, matching one element can determine the types of the other
captures. A wildcard keeps every tuple type that can match. The same rules apply through type
aliases.

```py
from typing import Literal, TypeAlias

def match_capture_filters_union_members_by_length(
    value: (tuple[Literal[1], int] | tuple[Literal[1], Literal[2], str] | tuple[Literal[1], Literal[2], Literal[3], bytes]),
) -> None:
    match value:
        case [1, item]:
            reveal_type(item)  # revealed: int
        case [1, 2, item]:
            reveal_type(item)  # revealed: str
        case [1, 2, 3, item]:
            reveal_type(item)  # revealed: bytes

def match_capture_rejects_wrong_tuple_length(
    value: tuple[Literal[1], Literal[2], str],
) -> None:
    match value:
        case [1, item]:
            reveal_type(item)  # revealed: Never
        case [1, 2, item]:
            reveal_type(item)  # revealed: str

def test_match_star_capture_filters_union_members(
    value: tuple[Literal[1], int, int] | tuple[Literal[2], str, str],
) -> list[int]:
    match value:
        case [1, *rest]:
            reveal_type(rest)  # revealed: list[int]
            return rest
        case _:
            reveal_type(value)  # revealed: tuple[Literal[2], str, str]
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

MatchPair: TypeAlias = tuple[Literal[1], int] | tuple[Literal[2], str]

def test_match_capture_filters_aliased_union_members(value: MatchPair) -> None:
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

def test_match_alias_excludes_cross_type_equal_values(
    value: Literal[True, 1, 2],
) -> None:
    match value:
        case 1:
            pass
        case _ as item:
            # Both `True` and `1` compare equal to the first pattern.
            reveal_type(item)  # revealed: Literal[2]

def test_ordered_or_alias_excludes_cross_type_equal_values(
    value: tuple[Literal[True], str] | tuple[Literal[2], bytes],
) -> None:
    match value:
        case [1, *item] | [item, _]:
            # The first alternative consumes the `Literal[True]` tuple.
            reveal_type(item)  # revealed: list[str] | Literal[2]
```

## Ordered `or`-pattern bindings

Alternatives are tried from left to right. We assume the declaration of `Values.x` means that every
`Values` instance has an `x` attribute. This makes the protocol pattern exhaustive, so the later
sequence alternative cannot contribute to the binding:

```py
from typing import Protocol, runtime_checkable

@runtime_checkable
class HasX(Protocol):
    x: int

class Values(list[str]):
    x: int

def test_or_binding_omits_values_consumed_by_a_class_pattern(value: Values) -> None:
    match value:
        case (HasX() as item) | [item]:
            reveal_type(item)  # revealed: Values
```

Class and mapping child bindings combine with bindings from other alternatives:

```py
from typing import final
from typing_extensions import TypedDict

@final
class TextValue:
    value: str = ""

class StringMapping(TypedDict):
    value: str

def class_or_sequence_binding(value: TextValue | tuple[int]) -> None:
    match value:
        case TextValue(value=item) | [item]:
            reveal_type(item)  # revealed: str | int

def mapping_or_singleton_binding(value: StringMapping | None) -> None:
    match value:
        case {"value": item} | (None as item):
            reveal_type(item)  # revealed: str | None
```

The first two alternatives bind an `int`. A list that does not contain exactly one element reaches
the final capture instead. If that list is later changed to contain one element, the same sequence
pattern must be able to match it:

```py
@final
class MutableOrBox:
    value: int = 0

def failed_sequence_alternative_does_not_narrow_later_capture(
    value: list[int] | MutableOrBox,
) -> None:
    match value:
        case [item] | MutableOrBox(value=item) | item:
            reveal_type(item)  # revealed: int | list[int]
            if isinstance(item, list):
                item.clear()
                item.append(1)
                match item:
                    case [only]:
                        reveal_type(item)  # revealed: list[int]
```

## Declared pattern captures

A capture still has to satisfy an earlier declaration for the same name. This uses the same
assignment checks as other bindings; the declaration remains the authoritative type when the
captured value is incompatible.

```py
from typing import Literal

def test_incompatible_declared_capture(subject: int) -> None:
    item: str
    match subject:
        case item:  # error: [invalid-assignment]
            reveal_type(item)  # revealed: str

def test_incompatible_declared_star_capture(subject: tuple[int, int]) -> None:
    rest: list[str]
    match subject:
        case [*rest]:  # error: [invalid-assignment]
            reveal_type(rest)  # revealed: list[str]

def test_incompatible_declared_or_capture(
    subject: tuple[Literal[1]] | tuple[Literal["x"]],
) -> None:
    item: int
    match subject:
        # TODO: Report one error for the logical OR-pattern binding instead of validating each
        # syntactic definition separately.
        # error: [invalid-assignment]
        # error: [invalid-assignment]
        case [1 as item] | ["x" as item]:
            reveal_type(item)  # revealed: int

def test_compatible_declared_alias(subject: object) -> None:
    item: int
    match subject:
        case int() as item:
            reveal_type(item)  # revealed: int
```

Pattern captures also respect declarations in global, enclosing function, and class scopes:

```py
global_capture: str

def capture_respects_global_declaration(subject: int) -> None:
    global global_capture
    match subject:
        case global_capture:  # error: [invalid-assignment]
            reveal_type(global_capture)  # revealed: str

def outer() -> None:
    nonlocal_capture: str = ""

    def capture_respects_nonlocal_declaration(subject: int) -> None:
        nonlocal nonlocal_capture
        match subject:
            case nonlocal_capture:  # error: [invalid-assignment]
                reveal_type(nonlocal_capture)  # revealed: str

class CaptureRespectsClassDeclaration:
    class_capture: str

    match 1:
        case class_capture:  # error: [invalid-assignment]
            reveal_type(class_capture)  # revealed: str
```

## Binding the whole pattern

Binding an entire pattern with `as` keeps the subject's original type variable. For a tuple,
successful child patterns can also refine the types at fixed indices.

```py
from typing import Literal, TypeVar

BoundSequenceT = TypeVar("BoundSequenceT", bound=tuple[object])

def test_match_sequence_alias_preserves_bound_typevar(
    value: BoundSequenceT,
) -> BoundSequenceT:
    match value:
        case [_] as whole:
            reveal_type(whole)  # revealed: BoundSequenceT@test_match_sequence_alias_preserves_bound_typevar
            return whole

def test_match_sequence_alias_preserves_typevar_union_member(
    value: BoundSequenceT | str,
) -> BoundSequenceT:
    match value:
        case [_] as whole:
            # revealed: BoundSequenceT@test_match_sequence_alias_preserves_typevar_union_member
            reveal_type(whole)
            return whole
        case _:
            raise ValueError

def test_match_sequence_alias_keeps_matched_element_types(
    value: tuple[Literal[1, 2]],
) -> None:
    match value:
        case [1] as whole:
            reveal_type(len(whole))  # revealed: Literal[1]
            reveal_type(whole[0])  # revealed: Literal[1]

def test_match_starred_sequence_alias_keeps_matched_element_types(
    value: tuple[Literal[1, 2], str, Literal[3, 4]],
) -> None:
    match value:
        case [1, *_, 4] as whole:
            reveal_type(whole[0])  # revealed: Literal[1]
            reveal_type(whole[-1])  # revealed: Literal[4]

def test_mutable_sequence_alias_does_not_keep_index_types(
    value: list[int | str],
) -> None:
    match value:
        case [int(), str()] as whole:
            reveal_type(len(whole))  # revealed: int
            whole.reverse()
            reveal_type(whole[0])  # revealed: int | str
```

Information from a failed sequence pattern must also be discarded before a mutable sequence is
changed and matched again:

```py
def mutable_sequence_alias_does_not_keep_previous_shape_constraints(
    value: list[int],
) -> None:
    match value:
        case []:
            pass
        case whole:
            whole.clear()
            match whole:
                case []:
                    reveal_type(whole)  # revealed: list[int]

def failed_sequence_pattern_does_not_narrow_mutable_subject(
    value: list[int],
) -> None:
    match value:
        case []:
            pass
        case _:
            # Reaching this arm means the preceding `[]` pattern failed, but clearing the list
            # invalidates the resulting non-empty constraint.
            value.clear()
            match value:
                case []:
                    reveal_type(value)  # revealed: list[int]
```

## Indirect class patterns

A class pattern can use a variable whose type is `type[Class]`. Both the subject and an `as` binding
use the instance type described by that annotation.

```py
from typing import Literal

class IndirectPattern: ...

def test_match_indirect_class_pattern(
    value: object,
    PatternClass: type[IndirectPattern],
) -> None:
    match value:
        case PatternClass() as item:
            reveal_type(item)  # revealed: IndirectPattern
            reveal_type(value)  # revealed: IndirectPattern

class IndirectIntPattern:
    tag: Literal["int"]
    payload: int

class IndirectStrPattern:
    tag: Literal["str"]
    payload: str

def test_union_class_pattern_uses_members_from_matching_class(
    value: object,
    PatternClass: type[IndirectIntPattern] | type[IndirectStrPattern],
) -> None:
    match value:
        case PatternClass(tag="int", payload=item):
            reveal_type(item)  # revealed: int
            reveal_type(value)  # revealed: IndirectIntPattern
```

## Class pattern aliases

The same rule applies outside sequence patterns. A class pattern keeps the generic arguments of a
matched alias.

```toml
[environment]
python-version = "3.12"
```

```py
type Container = int | dict[str, int] | list[int]

def class_pattern_preserves_alias(value: Container) -> None:
    match value:
        case dict() as mapping:
            reveal_type(mapping)  # revealed: dict[str, int]
            mapping["bad"] = "bad"  # error: [invalid-assignment]
        case list() as sequence:
            reveal_type(sequence)  # revealed: list[int]
            sequence.append("bad")  # error: [invalid-argument-type]
```

## Union-relative class pattern narrowing

Positive class patterns use the same union-relative subclass policy as `isinstance`.

```py
class Area: ...

def direct_class_pattern(value: list[Area] | Area) -> None:
    match value:
        case list():
            reveal_type(value)  # revealed: list[Area]
        case _:
            reveal_type(value)  # revealed: Area & ~Top[list[Unknown]]
```

## Strict subclass narrowing for class patterns

```toml
[analysis]
strict-subclass-narrowing = true
```

```py
class Area: ...

def strict_class_pattern(value: list[Area] | Area) -> None:
    match value:
        case list():
            reveal_type(value)  # revealed: list[Area] | (Area & Top[list[Unknown]])
        case _:
            reveal_type(value)  # revealed: Area & ~Top[list[Unknown]]
```

## Binding overlapping classes with `as`

Unrelated classes can share a subclass through multiple inheritance. Binding the whole class pattern
therefore preserves their intersection unless the classes are known to be disjoint.

```py
from typing import final

class OverlapA: ...
class OverlapB: ...

def test_match_class_alias_preserves_possible_multiple_inheritance(
    value: OverlapA,
) -> None:
    match value:
        case OverlapB() as item:
            reveal_type(item)  # revealed: OverlapA & OverlapB

def test_match_class_alias_preserves_negative_narrowing(value: object) -> None:
    if isinstance(value, OverlapA):
        return

    match value:
        case OverlapB() as item:
            reveal_type(item)  # revealed: OverlapB & ~OverlapA

@final
class FinalA: ...

class FinalB: ...

def test_match_class_alias_rejects_disjoint_final_class(value: FinalA) -> None:
    match value:
        case FinalB() as item:
            reveal_type(item)  # revealed: Never
```

## Class patterns for runtime dictionary values

A `TypedDict` type does not inherit from `dict`, but its values are dictionaries at runtime. Those
values can therefore match `dict`, `collections.abc.Mapping`, and runtime-checkable protocols
implemented by dictionaries.

```py
from collections.abc import Mapping
from typing import Protocol, TypedDict, runtime_checkable

class ProtocolPayload(TypedDict):
    value: int

@runtime_checkable
class SizedProtocol(Protocol):
    def __len__(self) -> int: ...

@runtime_checkable
class ClearProtocol(Protocol):
    def clear(self) -> None: ...

def test_match_typed_dict_alias_preserves_runtime_protocol_overlap(
    value: ProtocolPayload,
) -> None:
    match value:
        case SizedProtocol() as item:
            reveal_type(item)  # revealed: ProtocolPayload

def test_match_typed_dict_alias_adds_hidden_runtime_protocol(
    value: ProtocolPayload,
) -> None:
    match value:
        case ClearProtocol() as item:
            reveal_type(item)  # revealed: ProtocolPayload & ClearProtocol
            item.clear()

def test_match_typed_dict_alias_preserves_mapping_runtime_type(
    value: ProtocolPayload,
) -> None:
    match value:
        case Mapping() as item:
            reveal_type(item)  # revealed: ProtocolPayload
```

## Class pattern captures

Class patterns pass the type of each extracted attribute to their nested patterns. This also works
when the pattern class is held in a variable typed as `type[Class]`. The surrounding `as` pattern
keeps the subject's original generic type or type variable.

```py
from dataclasses import dataclass
from typing import Generic, NamedTuple, TypeVar

T = TypeVar("T")

class PatternBox(Generic[T]):
    __match_args__ = ("value",)
    value: T

class IndirectCapture:
    value: int

def test_match_class_keyword_capture(value: PatternBox[T]) -> T:
    match value:
        case PatternBox(value=item) as whole:
            reveal_type(item)  # revealed: T@test_match_class_keyword_capture
            reveal_type(whole)  # revealed: PatternBox[T@test_match_class_keyword_capture]
            return item

def test_match_indirect_class_keyword_capture(
    value: object,
    CapturePattern: type[IndirectCapture],
) -> None:
    match value:
        case CapturePattern(value=item):
            reveal_type(item)  # revealed: int

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

## Generic subclass captures

When a generic pattern class inherits from the subject's class through an invariant base, the
subject specialization determines the pattern class's type arguments. This applies to annotated
attributes and properties. Every pattern-class type parameter must have an exact solution; variant
bases and unconstrained parameters retain the existing conservative fallback. When the subject does
not provide type arguments, members declared by the pattern class use `Unknown`; a type parameter
default does not restrict which instances match at runtime.

```py
from typing import final, Generic
from typing_extensions import TypeVar

GenericPatternT = TypeVar("GenericPatternT")
ExtraGenericPatternT = TypeVar("ExtraGenericPatternT")
CovariantGenericPatternT = TypeVar("CovariantGenericPatternT", covariant=True)
DefaultGenericPatternT = TypeVar("DefaultGenericPatternT", default=str)

class GenericPatternBase(Generic[GenericPatternT]): ...

OptionalGenericPatternT = TypeVar(
    "OptionalGenericPatternT",
    bound=GenericPatternBase[int] | None,
)
UnionBoundGenericPatternT = TypeVar(
    "UnionBoundGenericPatternT",
    bound=GenericPatternBase[int] | GenericPatternBase[str],
)

class GenericPatternChild(GenericPatternBase[GenericPatternT]):
    item: GenericPatternT
    items: list[GenericPatternT]

class PartiallySpecializedGenericPatternChild(
    GenericPatternBase[GenericPatternT],
    Generic[GenericPatternT, ExtraGenericPatternT],
):
    item: GenericPatternT

class CovariantGenericPatternBase(Generic[CovariantGenericPatternT]): ...

class CovariantGenericPatternChild(CovariantGenericPatternBase[CovariantGenericPatternT]):
    item: CovariantGenericPatternT

class GenericMemberBase(Generic[GenericPatternT]):
    item: GenericPatternT

class GenericMemberChild(GenericMemberBase[GenericPatternT]): ...
class IntGenericMemberChild(GenericMemberBase[int]): ...

@final
class FinalGenericPatternBox(Generic[GenericPatternT]):
    value: list[GenericPatternT]

class DefaultGenericPatternBox(Generic[DefaultGenericPatternT]):
    value: DefaultGenericPatternT

ResultValueT = TypeVar("ResultValueT")
ResultErrorT = TypeVar("ResultErrorT")

class MatchResult(Generic[ResultValueT, ResultErrorT]): ...

class MatchOk(MatchResult[ResultValueT, ResultErrorT]):
    __match_args__ = ("value",)

    @property
    def value(self) -> ResultValueT:
        raise NotImplementedError

class MatchErr(MatchResult[ResultValueT, ResultErrorT]):
    __match_args__ = ("error",)

    @property
    def error(self) -> ResultErrorT:
        raise NotImplementedError

def test_match_generic_subclass_property_capture(
    result: MatchResult[int, str],
) -> int:
    match result:
        case MatchOk(value):
            reveal_type(value)  # revealed: int
            return value
        case MatchErr(error):
            reveal_type(error)  # revealed: str
            raise ValueError(error)
    raise AssertionError

def test_match_generic_subclass_capture(value: GenericPatternBase[int]) -> None:
    match value:
        case GenericPatternChild(item=item):
            reveal_type(item)  # revealed: int

def test_match_generic_subclass_capture_from_optional_typevar_bound(
    value: OptionalGenericPatternT,
) -> None:
    match value:
        case GenericPatternChild(item=item):
            reveal_type(item)  # revealed: int

def test_match_generic_subclass_capture_from_union_typevar_bound(
    value: UnionBoundGenericPatternT,
) -> None:
    match value:
        case GenericPatternChild(item=item):
            reveal_type(item)  # revealed: int | str

def test_match_nested_generic_subclass_capture(value: GenericPatternBase[int]) -> list[int]:
    match value:
        case GenericPatternChild(items=items):
            reveal_type(items)  # revealed: list[int]
            return items
    return []

def test_match_partially_specialized_generic_subclass(
    value: GenericPatternBase[int],
) -> None:
    match value:
        case PartiallySpecializedGenericPatternChild(item=item):
            # `ExtraGenericPatternT` is not constrained by the subject, so the pattern class does
            # not have one exact specialization.
            reveal_type(item)  # revealed: Unknown

def test_match_covariant_generic_subclass(
    value: CovariantGenericPatternBase[int],
) -> None:
    match value:
        case CovariantGenericPatternChild(item=item):
            # The subject constrains only one end of the possible pattern-class specializations.
            reveal_type(item)  # revealed: Unknown

def test_match_inherited_generic_subclass_capture(
    value: GenericMemberBase[GenericPatternT],
) -> GenericPatternT:
    match value:
        case GenericMemberChild(item=item):
            # revealed: GenericPatternT@test_match_inherited_generic_subclass_capture
            reveal_type(item)
            return item
        case _:
            raise ValueError

def test_match_generic_base_capture_preserves_subject_specialization(
    value: IntGenericMemberChild,
) -> None:
    match value:
        case GenericMemberBase(item=item):
            reveal_type(item)  # revealed: int

def test_match_direct_generic_pattern_preserves_declared_member(value: object) -> None:
    match value:
        case FinalGenericPatternBox(value=int() as item):
            reveal_type(item)  # revealed: Never

def test_match_generic_pattern_ignores_typevar_default(value: object) -> None:
    match value:
        case DefaultGenericPatternBox(value=int() as item):
            reveal_type(item)  # revealed: Unknown & int
```

## Positional class patterns

`__match_args__` is read through the pattern class and must identify literal attribute names. This
includes attributes provided by a metaclass. An annotation such as `tuple[str, ...]` does not
preserve the literal attribute names, so we cannot tell which attribute a positional pattern
extracts.

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

## Class patterns and union members

Each member of a union is checked against the complete class pattern before the extracted values are
combined. This keeps a tag together with its payload and any alias around the whole pattern. The
same rule applies through an `or` pattern.

```py
from typing import Generic, Literal, TypeVar

TagT = TypeVar("TagT")
PayloadT = TypeVar("PayloadT")

class TaggedPayload(Generic[TagT, PayloadT]):
    __match_args__ = ("tag", "payload")
    tag: TagT
    payload: PayloadT

def test_match_class_capture_filters_union_members(
    value: TaggedPayload[Literal["int"], int] | TaggedPayload[Literal["str"], str],
) -> None:
    match value:
        case TaggedPayload("int", item) as whole:
            reveal_type(item)  # revealed: int
            reveal_type(whole)  # revealed: TaggedPayload[Literal["int"], int]
```

A name is bound only when the complete class pattern succeeds. If a later subpattern cannot match,
an earlier capture has type `Never`. A missing attribute rejects a final-class alternative, while a
non-final class remains possible because a subclass can provide the attribute:

```py
from typing import final

class ImpossibleClassPattern:
    __match_args__ = ("first", "second")
    first: str
    second: str

def test_later_class_pattern_failure_rejects_earlier_capture(
    value: ImpossibleClassPattern,
) -> None:
    match value:
        case ImpossibleClassPattern(item, int()):
            reveal_type(item)  # revealed: Never

@final
class MissingClassPatternAttribute: ...

def test_missing_final_class_attribute_rejects_or_alternative(
    value: MissingClassPatternAttribute | int,
) -> None:
    match value:
        case MissingClassPatternAttribute(missing=item) | (int() as item):
            reveal_type(item)  # revealed: int

class NonFinalMissingClassPatternAttribute: ...

def test_missing_non_final_class_attribute_preserves_or_alternative(
    value: NonFinalMissingClassPatternAttribute | int,
) -> None:
    match value:
        case NonFinalMissingClassPatternAttribute(missing=item) | (int() as item):
            reveal_type(item)  # revealed: Unknown | int

def test_match_class_or_pattern_filters_union_members(
    value: TaggedPayload[Literal["int"], int] | TaggedPayload[Literal["str"], str] | TaggedPayload[Literal["bool"], bool],
) -> None:
    match value:
        case (TaggedPayload("int", item) | TaggedPayload("str", item)) as whole:
            reveal_type(item)  # revealed: int | str
            # revealed: TaggedPayload[Literal["int"], int] | TaggedPayload[Literal["str"], str]
            reveal_type(whole)
```

## Ordered class pattern alternatives

`OrderedBase.member` is definitely bound on `OrderedChild`, so the first alternative consumes the
complete subject and the later alternative cannot contribute to the binding:

```py
class OrderedBase:
    member: int = 0

class OrderedChild(OrderedBase): ...

def test_match_ordered_class_alternatives_remove_later_bindings(
    value: OrderedChild,
) -> None:
    match value:
        case OrderedBase(member=item) | (OrderedChild() as item):
            reveal_type(item)  # revealed: int
```

An argumentless class pattern cannot fail after its class check. If it matches the entire subject
type, a later alternative cannot contribute to the binding. When the argumentless pattern comes
second, an earlier class pattern can still contribute if the subject class is not final because a
subclass could match both classes. A final subject class rules out that overlap:

```py
from typing import final

class DefiniteFirst: ...

class UnreachableLater:
    payload: str

@final
class FinalDefiniteFirst: ...

def test_definite_class_alternative_removes_later_bindings(value: DefiniteFirst) -> None:
    match value:
        case (DefiniteFirst() as item) | UnreachableLater(payload=item):
            reveal_type(item)  # revealed: DefiniteFirst

def test_later_non_final_class_alternative_preserves_earlier_bindings(
    value: DefiniteFirst,
) -> None:
    match value:
        case UnreachableLater(payload=item) | (DefiniteFirst() as item):
            reveal_type(item)  # revealed: str | DefiniteFirst

def test_later_final_class_alternative_removes_earlier_bindings(
    value: FinalDefiniteFirst,
) -> None:
    match value:
        case UnreachableLater(payload=item) | (FinalDefiniteFirst() as item):
            reveal_type(item)  # revealed: FinalDefiniteFirst
```

## Positional patterns for built-in classes

For Python's built-in scalar and container classes, the single positional pattern receives the
entire subject instead of reading an attribute:

```py
from typing import Literal, TypeVar

MatchSelfIntT = TypeVar("MatchSelfIntT", bound=int)

def builtin_positional_patterns_capture_subject(
    value: list[int] | dict[str, int] | int,
) -> None:
    match value:
        case list(contents):
            reveal_type(contents)  # revealed: list[int]
        case dict(contents):
            reveal_type(contents)  # revealed: dict[str, int]
        case int(contents):
            reveal_type(contents)  # revealed: int

def builtin_positional_pattern_preserves_typevar(value: MatchSelfIntT) -> MatchSelfIntT:
    match value:
        case int(contents):
            # revealed: MatchSelfIntT@builtin_positional_pattern_preserves_typevar
            reveal_type(contents)
            return contents
        case _:
            raise AssertionError

def builtin_positional_pattern_refines_subject_alias(value: bool) -> Literal[True]:
    match value:
        case bool(True as item) as whole:
            reveal_type(item)  # revealed: Literal[True]
            reveal_type(whole)  # revealed: Literal[True]
            return whole
        case _:
            raise AssertionError
```

## Overlapping class patterns

Two unrelated non-final classes can have a common subclass through multiple inheritance. The
successful pattern therefore preserves both class types. Attributes from both bases remain possible,
even when one annotation is broader than the other. For a generic pattern class whose type arguments
are not known from the subject, its attributes use `Unknown`.

```py
from typing import Generic, TypeVar

OverlapT = TypeVar("OverlapT")

class OverlapCaptureA: ...

class OverlapCaptureB:
    member: int

def test_match_class_capture_preserves_possible_multiple_inheritance(
    value: OverlapCaptureA,
) -> None:
    match value:
        case OverlapCaptureB(member=item) as whole:
            reveal_type(item)  # revealed: int
            reveal_type(whole)  # revealed: OverlapCaptureA & OverlapCaptureB

class OverlapMemberA:
    member: int

class OverlapMemberB:
    member: str

class CompatibleOverlapMemberA:
    member: object = "x"

class CompatibleOverlapMemberB:
    member: int = 1

def test_match_class_capture_combines_overlapping_member_types(
    value: OverlapMemberA,
) -> None:
    match value:
        case OverlapMemberB(member=item):
            reveal_type(item)  # revealed: int | str

def test_match_class_capture_preserves_compatible_overlapping_member_types(
    value: CompatibleOverlapMemberA,
) -> None:
    match value:
        case CompatibleOverlapMemberB(member=str() as item):
            reveal_type(item)  # revealed: str

class GenericOverlapA:
    member: int

class GenericOverlapB(Generic[OverlapT]):
    member: OverlapT

class GenericOverlapC(GenericOverlapB[str], GenericOverlapA):
    member: str

class GenericListOverlapA: ...

class GenericListOverlapB(Generic[OverlapT]):
    values: list[OverlapT]

class GenericListOverlapC(GenericListOverlapA, GenericListOverlapB[int]): ...

def test_match_generic_class_capture_preserves_possible_multiple_inheritance(
    value: GenericOverlapA,
) -> None:
    match value:
        case GenericOverlapB(member=str() as item):
            reveal_type(item)  # revealed: str

def test_match_generic_container_member_keeps_loop_reachable(
    value: GenericListOverlapA,
) -> None:
    match value:
        case GenericListOverlapB(values=items):
            for item in items:
                reveal_type(item)  # revealed: object
```

## Class pattern captures from `Any` and `Unknown`

For an `Any` or `Unknown` subject, a capture keeps that uncertainty together with the attribute type
declared by the pattern class.

```py
from typing import Any
from ty_extensions import Unknown

class GradualPatternBox:
    value: int

def test_match_gradual_class_captures(any_value: Any, unknown_value: Unknown) -> None:
    match any_value:
        case GradualPatternBox(value=item):
            reveal_type(item)  # revealed: Any & int

    match unknown_value:
        case GradualPatternBox(value=item):
            reveal_type(item)  # revealed: Unknown & int
```

## Mapping pattern captures

Python reads an explicit mapping entry by calling `get` with a sentinel. A custom `get` method can
therefore produce a broader type than `__getitem__`; the sentinel's type is treated as `object` when
calling a custom override. The key type of an ordinary `Mapping` does not prove that another key is
absent because a custom `get` method may accept a broader set of keys. When the subject is only
known as `object`, a successful mapping pattern gives its entries the type `object`, not `Unknown`.
`**rest` is always a new `dict` containing the unmatched items.

```py
from collections.abc import Iterator, Mapping
from typing import Literal, overload, Protocol, TypeVar

MappingValueT = TypeVar("MappingValueT")
Default = TypeVar("Default")

def test_match_mapping_bindings(value: Mapping[str, MappingValueT]) -> MappingValueT:
    match value:
        case {"item": item, **rest} as whole:
            reveal_type(item)  # revealed: MappingValueT@test_match_mapping_bindings
            reveal_type(rest)  # revealed: dict[str, MappingValueT@test_match_mapping_bindings]
            # revealed: Mapping[str, MappingValueT@test_match_mapping_bindings]
            reveal_type(whole)
            return item
    raise ValueError

def test_match_dict_alias_preserves_concrete_type(value: dict[str, int]) -> None:
    match value:
        case {"item": item, **rest} as whole:
            reveal_type(whole)  # revealed: dict[str, int]

def test_match_object_mapping_entry_type(value: object) -> None:
    match value:
        case {"item": item}:
            reveal_type(item)  # revealed: object

class CustomGet(Mapping[str, int | str]):
    def __getitem__(self, key: str) -> int:
        return 1

    def __iter__(self) -> Iterator[str]:
        return iter(("item",))

    def __len__(self) -> int:
        return 1

    @overload
    def get(self, key: object) -> int | str | None: ...
    @overload
    def get(self, key: object, default: Default) -> int | str | Default: ...
    def get(self, key: object, default: Default | None = None) -> int | str | Default | None:
        if key == "item":
            return "custom value"
        return default

def test_match_mapping_uses_get(value: CustomGet) -> None:
    match value:
        case {"item": item}:
            reveal_type(item)  # revealed: object

class InstanceGet(Protocol):
    @overload
    def __call__(self, key: object) -> str | None: ...
    @overload
    def __call__(self, key: object, default: Default) -> str | Default: ...

class InstanceGetImpl:
    @overload
    def __call__(self, key: object) -> str | None: ...
    @overload
    def __call__(self, key: object, default: Default) -> str | Default: ...
    def __call__(self, key: object, default: object = None) -> object:
        return "custom" if key == "item" else default

class InstanceGetMapping(Mapping[str, int]):
    def __init__(self) -> None:
        self.get: InstanceGet = InstanceGetImpl()

    def __getitem__(self, key: str) -> int:
        return 1

    def __iter__(self) -> Iterator[str]:
        return iter(())

    def __len__(self) -> int:
        return 0

def test_match_mapping_instance_get(value: InstanceGetMapping) -> None:
    match value:
        case {"item": item}:
            reveal_type(item)  # revealed: object

def test_incompatible_declared_mapping_captures(value: Mapping[str, int]) -> None:
    item: str
    rest: dict[str, str]
    match value:
        # error: [invalid-assignment]
        # error: [invalid-assignment]
        case {"item": item, **rest}:
            reveal_type(item)  # revealed: str
            reveal_type(rest)  # revealed: dict[str, str]

def test_match_mapping_key_keeps_union_members(
    value: dict[Literal["a"], int] | dict[Literal["b"], str],
) -> None:
    match value:
        case {"a": item} as whole:
            reveal_type(item)  # revealed: int | str
            # revealed: dict[Literal["a"], int] | dict[Literal["b"], str]
            reveal_type(whole)
```

Mapping values are passed to nested patterns. If any nested pattern cannot match, the mapping
pattern binds no names:

```py
def test_match_mapping_nested_sequence(
    value: Mapping[str, tuple[int, str]],
) -> None:
    match value:
        case {"pair": [number, text]}:
            reveal_type(number)  # revealed: int
            reveal_type(text)  # revealed: str

def test_later_mapping_pattern_failure_rejects_bindings(
    value: Mapping[str, str],
) -> None:
    match value:
        case {"first": item, "second": int(), **rest}:
            reveal_type(item)  # revealed: Never
            reveal_type(rest)  # revealed: Never
```

Even a dictionary whose declared key type is `Never` may be a subclass with a custom `get` method.
The annotation therefore does not prove that a keyed pattern is impossible:

```py
from typing_extensions import Never

def test_match_mapping_keeps_empty_key_domain(
    value: dict[Never, int],
) -> None:
    match value:
        case {"item": item}:
            reveal_type(item)  # revealed: int
```

## Mapping captures from `Any` and `Unknown`

The rest pattern is a new dictionary. For an `Any` or `Unknown` subject, its key and value types
keep the same uncertainty as the subject.

```py
from typing import Any
from ty_extensions import Unknown

def test_match_gradual_mapping_captures(any_value: Any, unknown_value: Unknown) -> None:
    match any_value:
        case {"item": item, **rest}:
            reveal_type(item)  # revealed: Any
            reveal_type(rest)  # revealed: dict[Any, Any]

    match unknown_value:
        case {"item": item, **rest}:
            reveal_type(item)  # revealed: Unknown
            reveal_type(rest)  # revealed: dict[Unknown, Unknown]
```

## `TypedDict` mapping patterns

For a `TypedDict`, a literal key uses the declared field type. An undeclared key on an implicitly
open `TypedDict` has type `object` because it may be a hidden item. For a closed `TypedDict`, a
pattern using an undeclared key is impossible. Tags keep each `TypedDict` together with its
corresponding value type through an `or` pattern.

```py
from typing import Literal, final
from typing_extensions import NotRequired, TypedDict

class IntPayload(TypedDict):
    tag: Literal["int"]
    value: int

class StrPayload(TypedDict):
    tag: Literal["str"]
    value: str

def test_match_typed_dict_capture_filters_union_members(
    value: IntPayload | StrPayload,
) -> None:
    match value:
        case {"tag": "int", "value": item, **rest} as whole:
            reveal_type(item)  # revealed: int
            reveal_type(rest)  # revealed: dict[str, object]
            reveal_type(whole)  # revealed: IntPayload

class OptionalPayload(TypedDict):
    value: NotRequired[int]

def test_match_optional_typed_dict_field(value: OptionalPayload) -> None:
    match value:
        case {"value": item}:
            reveal_type(item)  # revealed: int

def test_match_implicitly_open_typed_dict_field(value: IntPayload) -> None:
    match value:
        case {"other": item}:
            reveal_type(item)  # revealed: object

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

class ExtraItemsPayload(TypedDict, extra_items=int):
    tag: Literal["extra"]

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

def test_match_typed_dict_extra_items(
    value: ClosedPayload | ExtraItemsPayload,
) -> None:
    match value:
        case {"other": item} as whole:
            reveal_type(item)  # revealed: int
            reveal_type(whole)  # revealed: ExtraItemsPayload

def test_match_typed_dict_or_pattern_filters_union_members(
    value: ClosedIntPayload | ClosedStrPayload | ClosedBoolPayload,
) -> None:
    match value:
        case ({"tag": "int", "value": item} | {"tag": "str", "value": item}) as whole:
            reveal_type(item)  # revealed: int | str
            reveal_type(whole)  # revealed: ClosedIntPayload | ClosedStrPayload

@final
class Token: ...

def test_required_typed_dict_key_excludes_fallback_binding(
    value: IntPayload | Token,
) -> int | Token:
    match value:
        case {"value": item} | item:
            reveal_type(item)  # revealed: int | Token
            return item
```

## Narrowing the match subject

When a class, mapping, or sequence pattern succeeds, it can narrow the original match subject even
if the pattern does not bind a name for the whole value. Nested patterns can remove union members,
and an `or` pattern combines the possibilities from its alternatives. A class or mapping pattern
also keeps the uncertainty of an `Any` or `Unknown` subject.

```py
from typing import Any, Generic, Literal, TypeVar, final
from typing_extensions import TypedDict
from ty_extensions import Unknown

TagT = TypeVar("TagT")
PayloadT = TypeVar("PayloadT")

class TaggedPayload(Generic[TagT, PayloadT]):
    __match_args__ = ("tag", "payload")
    tag: TagT
    payload: PayloadT

class GradualSubjectBox: ...

def match_patterns_preserve_any_and_unknown(
    any_value: Any,
    unknown_value: Unknown,
) -> None:
    match any_value:
        case GradualSubjectBox():
            reveal_type(any_value)  # revealed: Any & GradualSubjectBox

    match unknown_value:
        case {"key": _}:
            reveal_type(unknown_value)  # revealed: Unknown & Top[Mapping[Unknown, object]]

def match_class_narrows_subject(
    value: TaggedPayload[Literal["int"], int] | TaggedPayload[Literal["str"], str],
) -> None:
    match value:
        case TaggedPayload("int", _):
            reveal_type(value)  # revealed: TaggedPayload[Literal["int"], int]

def builtin_class_pattern_narrows_subject(value: bool) -> None:
    match value:
        case bool(True):
            reveal_type(value)  # revealed: Literal[True]

def list_class_pattern_does_not_keep_index_types_after_mutation(
    value: list[int | str],
) -> None:
    match value:
        case list([int(), str()]):
            # Reversing the list invalidates the indexed-element facts established by the pattern.
            value.reverse()
            reveal_type(value[0])  # revealed: int | str

def nested_list_pattern_does_not_keep_index_types_after_mutation(
    value: tuple[list[int | str]],
) -> None:
    match value:
        case [[int(), str()]]:
            # The inner list is mutable, so the indexed-element facts established by the pattern
            # cannot be retained through the outer tuple after this mutation.
            value[0].reverse()
            reveal_type(value[0][0])  # revealed: int | str

def match_class_or_pattern_narrows_subject(
    value: (TaggedPayload[Literal["int"], int] | TaggedPayload[Literal["str"], str] | TaggedPayload[Literal["bool"], bool]),
) -> None:
    match value:
        case TaggedPayload("int", _) | TaggedPayload("str", _):
            # revealed: TaggedPayload[Literal["int"], int] | TaggedPayload[Literal["str"], str]
            reveal_type(value)

def match_sequence_narrows_tuple_element_subject(
    value: tuple[Literal[1, 2]],
) -> None:
    match value:
        case [1]:
            reveal_type(value[0])  # revealed: Literal[1]

@final
class FinalWithoutRequestedAttribute: ...

def missing_final_class_attribute_rejects_subject_alternative(
    value: FinalWithoutRequestedAttribute | TaggedPayload[Literal["int"], int],
) -> None:
    match value:
        case FinalWithoutRequestedAttribute(missing=_) | TaggedPayload("int", _):
            reveal_type(value)  # revealed: TaggedPayload[Literal["int"], int]

class IntPayload(TypedDict):
    tag: Literal["int"]
    value: int

class StrPayload(TypedDict):
    tag: Literal["str"]
    value: str

def match_mapping_narrows_subject(value: IntPayload | StrPayload) -> None:
    match value:
        case {"tag": "int"}:
            reveal_type(value)  # revealed: IntPayload

class PayloadContainer:
    payload: IntPayload | StrPayload

def mapping_pattern_narrows_attribute_subject(container: PayloadContainer) -> None:
    match container.payload:
        case {"tag": "int"}:
            reveal_type(container.payload)  # revealed: IntPayload

def nested_mapping_narrows_sequence_subject(
    value: tuple[IntPayload] | tuple[StrPayload],
) -> None:
    match value:
        case [{"tag": "int"}]:
            reveal_type(value)  # revealed: tuple[IntPayload]
```

## Exhaustive positional patterns for built-in classes

Python defines a fixed set of built-in classes whose single positional subpattern receives the
entire subject. The `float` element also handles `int`, which is assignable to `float` but is not a
`float` instance at runtime:

```py
def builtin_positional_patterns_are_exhaustive(
    value: tuple[
        bool,
        bytearray,
        bytes,
        dict[object, object],
        float,
        frozenset[object],
        int,
        list[object],
        set[object],
        str,
        tuple[object, ...],
    ],
) -> int:
    match value:
        case (
            bool(_),
            bytearray(_),
            bytes(_),
            dict(_),
            (int(_) | float(_)),
            frozenset(_),
            int(_),
            list(_),
            set(_),
            str(_),
            tuple(_),
        ):
            return 1
```

## `TypedDict` class patterns at runtime

A `TypedDict` value is a dictionary at runtime, so argumentless `dict` and `Mapping` patterns always
match it. The positional `dict` pattern does as well. This also applies when the subject is a
truthiness-narrowed intersection or a type variable bounded by or constrained to `TypedDict`s:

```py
from collections.abc import Mapping
from typing import TypeVar, TypedDict

class Movie(TypedDict):
    title: str

class OptionalMovie(TypedDict, total=False):
    title: str

class Series(TypedDict):
    seasons: int

T = TypeVar("T", bound=Movie)
U = TypeVar("U", Movie, Series)

def argumentless_dict_pattern_is_exhaustive(value: Movie) -> int:
    match value:
        case dict():
            return 1

def mapping_pattern_is_exhaustive(value: Movie) -> int:
    match value:
        case Mapping():
            return 1

def positional_dict_pattern_is_exhaustive(value: Movie) -> int:
    match value:
        case dict(_):
            return 1

def narrowed_typed_dict_pattern_is_exhaustive(value: OptionalMovie) -> int:
    if not value:
        return 0
    match value:
        case dict():
            return 1

def bounded_typed_dict_pattern_is_exhaustive(value: T) -> int:
    match value:
        case dict():
            return 1

def constrained_typed_dict_pattern_is_exhaustive(value: U) -> int:
    match value:
        case dict():
            return 1
```

## Required `TypedDict` keys

A mapping pattern is exhaustive for a `TypedDict` when every key in the pattern names a required
field and every value pattern matches all values allowed for that field. The negative cases below
exercise three separate checks: an optional field, an unknown key, and a non-string key.

```py
from typing import Any, Literal, Protocol, TypeVar, TypedDict
from ty_extensions import Intersection, Unknown

class RequiredPayload(TypedDict):
    tag: Literal["int"]
    value: int

class OptionalPayload(TypedDict, total=False):
    value: int

class DynamicPayload(TypedDict):
    any_value: Any
    unknown_value: Unknown

class AlternatePayload(TypedDict):
    tag: Literal["int"]
    value: int

class Marker(Protocol):
    marker: int

P = TypeVar("P", bound=RequiredPayload)
Q = TypeVar("Q", RequiredPayload, AlternatePayload)

def required_typed_dict_keys_are_exhaustive(value: RequiredPayload) -> int:
    match value:
        case {"tag": "int", "value": int()}:
            return 1

def universal_nested_patterns_are_exhaustive(value: DynamicPayload) -> int:
    match value:
        case {"any_value": object(), "unknown_value": object()}:
            return 1

def bounded_typed_dict_mapping_is_exhaustive(value: P) -> int:
    match value:
        case {"tag": "int", "value": int()}:
            return 1

def constrained_typed_dict_mapping_is_exhaustive(value: Q) -> int:
    match value:
        case {"tag": "int", "value": int()}:
            return 1

def intersected_typed_dict_mapping_is_exhaustive(
    value: Intersection[RequiredPayload, Marker],
) -> int:
    match value:
        case {"tag": "int", "value": int()}:
            return 1

def optional_key_is_not_exhaustive(
    value: OptionalPayload,
    # error: [invalid-return-type]
) -> int:
    match value:
        case {"value": _}:
            return 1

def absent_key_is_not_exhaustive(
    value: RequiredPayload,
    # error: [invalid-return-type]
) -> int:
    match value:
        case {"missing": _}:
            return 1

def non_string_key_is_not_exhaustive(
    value: RequiredPayload,
    # error: [invalid-return-type]
) -> int:
    match value:
        case {1: _}:
            return 1
```

## `NamedTuple` positional patterns

A `NamedTuple` provides a generated `__match_args__` tuple containing all of its fields:

```py
from typing import NamedTuple

class NamedPoint(NamedTuple):
    x: int
    label: str

def named_tuple_positional_pattern_is_exhaustive(value: NamedPoint) -> int:
    match value:
        case NamedPoint(_, _):
            return 1
```

## Positional patterns for built-in subclasses

Subclasses inherit this positional behavior. The positional subpattern still needs to match the
entire value, so a literal subpattern is not exhaustive:

```py
class MyInt(int): ...

def builtin_subclass_positional_pattern_is_exhaustive(value: MyInt) -> int:
    match value:
        case MyInt(_):
            return 1

def builtin_positional_literal_is_not_exhaustive(
    value: MyInt,
    # error: [invalid-return-type]
) -> int:
    match value:
        case MyInt(0):
            return 1
```

## Resolving `__match_args__`

For other positional class patterns, Python reads `__match_args__` from the pattern class. A fixed
tuple of attribute names makes the corresponding positional patterns exhaustive when every selected
attribute is present on the subject:

```py
class KnownAttributes:
    __match_args__ = ("x", "y")
    x: int = 0
    y: int = 0

def fixed_match_args_are_exhaustive(value: KnownAttributes) -> int:
    match value:
        case KnownAttributes(_, _):
            return 1

class ValidMatchArgsMeta(type):
    __match_args__ = ("x",)

class WithMetaclassMatchArgs(metaclass=ValidMatchArgsMeta):
    x: int = 0

def metaclass_match_args_is_exhaustive(value: WithMetaclassMatchArgs) -> int:
    match value:
        case WithMetaclassMatchArgs(_):
            return 1
```

The pattern is not exhaustive when a selected attribute is missing, an explicit annotation widens
the tuple type, or a conditional definition can override a built-in class's usual positional
behavior. A metaclass can also provide `__match_args__` that selects a missing attribute:

```py
class IntWithMissingMatchArg(int):
    __match_args__ = ("missing",)

def missing_match_arg_is_not_exhaustive(
    value: IntWithMissingMatchArg,
    # error: [invalid-return-type]
) -> int:
    match value:
        case IntWithMissingMatchArg(_):
            return 1

class MatchArgsMeta(type):
    __match_args__ = ("missing",)

class IntWithMetaclassMatchArgs(int, metaclass=MatchArgsMeta): ...

def metaclass_match_args_is_not_exhaustive(
    value: IntWithMetaclassMatchArgs,
    # error: [invalid-return-type]
) -> int:
    match value:
        case IntWithMetaclassMatchArgs(_):
            return 1

class WidenedMatchArgs:
    __match_args__: tuple[str, ...] = ("x",)
    x: int = 0

def widened_match_args_is_not_exhaustive(
    value: WidenedMatchArgs,
    # error: [invalid-return-type]
) -> int:
    match value:
        case WidenedMatchArgs(_):
            return 1

def condition() -> bool:
    return bool()

flag = condition()

class ConditionalIntMatchArgs(int):
    if flag:
        __match_args__ = ("missing",)

def conditional_match_args_disables_builtin_behavior(
    value: ConditionalIntMatchArgs,
    # error: [invalid-return-type]
) -> int:
    match value:
        case ConditionalIntMatchArgs(_):
            return 1
```

## Properties and declared attributes

Properties and declared attributes count as present when checking exhaustiveness, even though
descriptor access can raise `AttributeError` and an annotated attribute can be absent at runtime:

```py
from typing import Literal

class FallibleProperty:
    @property
    def x(self) -> Literal[1]:
        raise AttributeError

def fallible_property_value_pattern_is_statically_exhaustive(value: FallibleProperty) -> int:
    match value:
        case FallibleProperty(x=1):
            return 1

class DeclaredLiteralAttribute:
    x: Literal[1]

def declared_literal_attribute_is_exhaustive(
    value: DeclaredLiteralAttribute,
) -> int:
    match value:
        case DeclaredLiteralAttribute(x=1):
            return 1
```

## Runtime-checkable protocol patterns

Runtime-checkable protocols use the same rule. The subject below is known to provide `x`, so the
pattern is exhaustive even though the subject class is not final:

```py
from typing import Protocol, runtime_checkable

@runtime_checkable
class RuntimeProtocolWithX(Protocol):
    x: int

class RuntimeProtocolImplementer:
    x: int = 0

def runtime_protocol_pattern_is_exhaustive(value: RuntimeProtocolImplementer) -> int:
    match value:
        case RuntimeProtocolWithX(x=_):
            return 1
```

## Members from the subject type

A keyword pattern reads the attribute from the matched value. The subject type can therefore provide
an attribute that is not declared by the class named in the pattern. This also applies when the
subject class is not final:

```py
class BaseWithoutX: ...

class ChildWithX(BaseWithoutX):
    x: int = 0

def subclass_member_is_exhaustive(value: ChildWithX) -> int:
    match value:
        case BaseWithoutX(x=_):
            return 1
```

## Positional behavior comes from the pattern class

Only the class named in the pattern determines what a positional subpattern receives. Although
`IntPlainChild` also inherits from `int`, `PlainBase(_)` does not receive the whole value:

```py
class PlainBase: ...
class IntPlainChild(int, PlainBase): ...

def builtin_positional_behavior_comes_from_pattern_class(
    value: IntPlainChild,
    # error: [invalid-return-type]
) -> int:
    match value:
        case PlainBase(_):  # error: [invalid-match-pattern]
            return 1
```

## Nested class patterns

The same rule applies recursively: every nested pattern must match every value allowed for the
attribute it receives.

```py
class Inner:
    x: int = 0

class Outer:
    inner: Inner = Inner()

def nested_class_subpattern_is_exhaustive(value: tuple[Outer]) -> int:
    match value:
        case [Outer(inner=Inner(x=_))]:
            return 1
```

## Missing class-pattern attributes

A class pattern can fail after its `isinstance` check if a requested attribute is missing or only
conditionally defined. This applies to both keyword and positional attributes, including inside a
sequence. The failed branch therefore keeps the original subject type:

```py
from typing import final

class MissingAttributes:
    __match_args__ = ("x", "missing")
    x: int = 0

class OtherClass: ...

def missing_attribute_keeps_original_subject(
    value: MissingAttributes | OtherClass,
) -> None:
    match value:
        case MissingAttributes(missing=_):
            pass
        case _:
            reveal_type(value)  # revealed: MissingAttributes | OtherClass

def missing_positional_attribute_keeps_sequence_possible(
    value: tuple[MissingAttributes],
) -> None:
    match value:
        case [MissingAttributes(_, _)]:
            pass
        case _:
            reveal_type(value)  # revealed: tuple[MissingAttributes]

def attribute_condition() -> bool:
    return bool()

@final
class PossiblyMissingAttribute:
    if attribute_condition():
        x: int = 0

def possibly_missing_attribute_is_not_exhaustive(
    value: PossiblyMissingAttribute,
    # error: [invalid-return-type]
) -> int:
    match value:
        case PossiblyMissingAttribute(x=_):
            return 1
```

## Exhaustiveness for types containing `Any`

When `Any` appears within a type, it stands for many possible static types. An exhaustive pattern
must eliminate all of them; otherwise, a contradictory type such as
`Mapping[str, Any] & ~Mapping[str, Any]` can remain in the final case.

```toml
[environment]
python-version = "3.12"
```

The order of the first two cases below reproduces issue #3904:

```py
from collections.abc import Mapping
from typing import Any, assert_never

def mapping_with_any_is_exhaustive(value: Mapping[str, Any] | int) -> None:
    match value:
        case Mapping():
            pass
        case int():
            pass
        case _:
            assert_never(value)
```

Class patterns over generic classes follow the same rule:

```py
class Box[T]:
    value: T

def generic_class_with_any_is_exhaustive(value: Box[Any] | int) -> None:
    match value:
        case Box(value=_):
            pass
        case int():
            pass
        case _:
            assert_never(value)
```

A nested pattern can be exhaustive for only part of a union. Here the first case removes
`Box[Mapping[str, Any]]` but must leave `Box[int]` in the final case:

```py
def nested_pattern_keeps_unmatched_box(
    value: Box[Mapping[str, Any]] | Box[int],
) -> None:
    match value:
        case Box(value=Mapping()):
            pass
        case _:
            reveal_type(value)  # revealed: Box[int]
```

The same check applies to a class pattern nested inside a sequence pattern:

```py
def nested_sequence_pattern_is_exhaustive(
    value: tuple[Mapping[str, Any]] | int,
) -> None:
    match value:
        case [Mapping()]:
            pass
        case int():
            pass
        case _:
            assert_never(value)
```

## Sequence exhaustiveness

Sequence patterns also contribute to negative narrowing and exhaustiveness. Exact tuple shapes can
make a match exhaustive.

```py
from typing import Any, NamedTuple
from typing_extensions import assert_never

class HasX:
    x: int = 0

def test_match_exact_tuple_sequence(subj: tuple[int | str, int | str]) -> None:
    match subj:
        case x, str():
            reveal_type(subj)  # revealed: tuple[int | str, str]
            reveal_type(subj[0])  # revealed: int | str
            reveal_type(subj[1])  # revealed: str
            first, second = subj
            reveal_type(first)  # revealed: int | str
            reveal_type(second)  # revealed: str
        case y:
            reveal_type(subj)  # revealed: tuple[int | str, int]
            reveal_type(subj[0])  # revealed: int | str
            reveal_type(subj[1])  # revealed: int

def match_exact_tuple_sequence_preserves_gradualness(value: tuple[Any]) -> None:
    match value:
        case [str()]:
            reveal_type(value)  # revealed: tuple[Any & str]

def test_match_exact_tuple_sequence_is_exhaustive(value: int | tuple[int, int]) -> int:
    match value:
        case int(value):
            return value
        case (left, right):
            return left + right
        case _:
            assert_never(value)

# Matching the element would succeed, but a one-element pattern cannot match a two-element tuple.
def sequence_length_is_still_checked(
    value: tuple[HasX, HasX],
    # error: [invalid-return-type]
) -> int:
    match value:
        case [HasX(x=_)]:
            return 1

def test_match_exact_tuple_element_union_is_exhaustive(x: tuple[int | str]) -> int:
    match x:
        case [int()]:
            return 42
        case [str()]:
            return 42
        case _:
            assert_never(x)

def test_match_exact_tuple_multiple_negative_constraints(
    value: tuple[int | str, int | str],
) -> tuple[str, int | str] | tuple[int | str, int]:
    match value:
        case [int(), str()]:
            raise ValueError
        case _:
            # revealed: tuple[str, int | str] | tuple[int | str, int]
            reveal_type(value)
            return value

def test_match_exact_mutable_sequence_negative(value: list[int]) -> None:
    match value:
        case [int()]:
            pass
        case _:
            reveal_type(value)  # revealed: list[int]
```

Narrowing with a sequence pattern must not bring back a type removed by an earlier case. After the
first two cases below, only `str` remains:

```py
def sequence_pattern_preserves_earlier_case(
    value: tuple[int] | int | str,
) -> None:
    match value:
        case int():
            pass
        case [int()]:
            pass
        case _:
            reveal_type(value)  # revealed: str
```

Named tuples are statically known tuple subclasses, rather than exact `tuple[...]` instances.
Sequence-pattern fallthrough therefore preserves the named class instead of rebuilding its type from
the element patterns:

```py
class Pair(NamedTuple):
    left: int | str
    right: int | str

def test_match_exact_tuple_sequence_subclass(value: Pair) -> None:
    match value:
        case _, str():
            pass
        case _:
            reveal_type(value)  # revealed: Pair
```

## Nested sequence patterns

Nested patterns narrow values captured from the positions they inspect. For subjects without a known
tuple shape, length and indexed-element facts are not retained on the original subject.

```py
def normalize_nested_record(value: object) -> tuple[None, int, int] | None:
    match value:
        case [None as first, [int() as number], {} as mapping]:
            ret = first, number, len(mapping)
            reveal_type(ret)  # revealed: tuple[None, int, int]
            return ret
    return None

def unwrap_number_or_label(value: object) -> int | str | None:
    match value:
        case [(int() | str()) as item]:
            reveal_type(item)  # revealed: int | str
            return item
    return None

def narrow_nested_exact_tuple_subject(
    value: tuple[tuple[int | str, int | str]],
) -> None:
    match value:
        case [[str(), int()]] as whole:
            reveal_type(value)  # revealed: tuple[tuple[str, int]]
            reveal_type(whole)  # revealed: tuple[tuple[str, int]]
```

Tuple-pattern narrowing limits the total number of alternative tuple types created while matching
nested patterns. Each inner pattern below creates 32 alternatives, and the outer pattern creates two
more. Together, they exceed the limit of 64, so ty uses conservative fallthrough narrowing.

```py
# fmt: off
NestedExpansionInner = tuple[
    bool, bool, bool, bool, bool, bool, bool, bool,
    bool, bool, bool, bool, bool, bool, bool, bool,
    bool, bool, bool, bool, bool, bool, bool, bool,
    bool, bool, bool, bool, bool, bool, bool, bool,
]
NestedExpansionOuter = tuple[NestedExpansionInner, NestedExpansionInner]

def nested_tuple_expansion_limit(value: NestedExpansionOuter) -> None:
    match value:
        case (
            (
                True, True, True, True, True, True, True, True,
                True, True, True, True, True, True, True, True,
                True, True, True, True, True, True, True, True,
                True, True, True, True, True, True, True, True,
            ),
            (
                True, True, True, True, True, True, True, True,
                True, True, True, True, True, True, True, True,
                True, True, True, True, True, True, True, True,
                True, True, True, True, True, True, True, True,
            ),
        ):
            pass
        case _:
            # revealed: tuple[tuple[bool, bool, bool, bool, bool, bool, bool, bool, bool, bool, bool, bool, bool, bool, bool, bool, bool, bool, bool, bool, bool, bool, bool, bool, bool, bool, bool, bool, bool, bool, bool, bool], tuple[bool, bool, bool, bool, bool, bool, bool, bool, bool, bool, bool, bool, bool, bool, bool, bool, bool, bool, bool, bool, bool, bool, bool, bool, bool, bool, bool, bool, bool, bool, bool, bool]] & ~<Protocol with members '__getitem__', '__len__'>
            reveal_type(value)
# fmt: on
```

## Sequence display subjects

A tuple or list display has no place of its own to narrow. A successful sequence pattern instead
narrows the corresponding narrowable elements. If a multi-element pattern fails, we do not know
which element failed to match.

```py
from typing import Generic, Literal, TypeVar

DisplayTagT = TypeVar("DisplayTagT")
DisplayPayloadT = TypeVar("DisplayPayloadT")

class DisplayTaggedPayload(Generic[DisplayTagT, DisplayPayloadT]):
    __match_args__ = ("tag", "payload")
    tag: DisplayTagT
    payload: DisplayPayloadT

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

def match_tuple_expression_class_pattern(
    value: (DisplayTaggedPayload[Literal["int"], int] | DisplayTaggedPayload[Literal["str"], str]),
) -> None:
    match (value,):
        case (DisplayTaggedPayload("int", _),):
            reveal_type(value)  # revealed: DisplayTaggedPayload[Literal["int"], int]
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
from typing import final

class TupleSubjectA: ...
class TupleSubjectA1(TupleSubjectA): ...
class TupleSubjectA2(TupleSubjectA): ...
class TupleSubjectB: ...
class TupleSubjectB1(TupleSubjectB): ...
class TupleSubjectB2(TupleSubjectB): ...
class OrDisplayA: ...

@final
class OrDisplayA1(OrDisplayA): ...

@final
class OrDisplayA2(OrDisplayA): ...

@final
class OrDisplayB1: ...

@final
class OrDisplayB2: ...

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

def match_tuple_expression_or_drops_impossible_class_pattern(
    a: OrDisplayA,
    b: OrDisplayB1,
) -> None:
    match a, b:
        case (OrDisplayA1(), OrDisplayB2()) | (OrDisplayA2(), OrDisplayB1()):
            reveal_type(a)  # revealed: OrDisplayA2

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

def later_case_uses_saved_subject_after_guarded_capture(flag: bool) -> None:
    x = (1,)
    match x:
        case [x] if flag:
            pass
        case [1]:
            reveal_type(x)  # revealed: Literal[1]
            x + "bad"  # error: [unsupported-operator]

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
function defined before the capture. Direct, sequence, class, and built-in positional captures
resolve to a concrete type. Union-relative narrowing also lets the mapping capture below reject the
loop-carried non-mapping arm, so it resolves to the concrete value type of the original dictionary.

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

class CycleBox:
    value: int

def match_loop_carried_class_capture(flag: bool) -> None:
    x = CycleBox()
    while flag:
        match x:
            case CycleBox(value=x):
                reveal_type(x)  # revealed: int

def match_loop_carried_mapping_capture(flag: bool) -> None:
    x = {"value": 1}
    while flag:
        match x:
            case {"value": x}:
                reveal_type(x)  # revealed: int

def match_loop_carried_match_self_capture(flag: bool, x: int) -> None:
    while flag:
        match x:
            case int(x):
                reveal_type(x)  # revealed: int

def capture_from_later_global() -> int:
    return captured

match capture_from_later_global():
    case captured:
        reveal_type(captured)  # revealed: int
```

## Value patterns

Value patterns are evaluated by equality, which is overridable. Apart from the optimistic treatment
of broad builtin types described below, successfully matching one only gives us information where we
know how the subject type implements equality.

Broad builtin types are treated as if they use the builtin equality implementation, so literal
patterns narrow `str`, `int`, and `bytes`:

```py
def string_pattern(value: str):
    match value:
        case "a":
            reveal_type(value)  # revealed: Literal["a"]

def integer_pattern(value: int):
    match value:
        case 1:
            reveal_type(value)  # revealed: Literal[1, True]

def bytes_pattern(value: bytes):
    match value:
        case b"a":
            reveal_type(value)  # revealed: Literal[b"a"]
```

Explicit subclass and custom comparison arms are still preserved:

```py
class StringSubclass(str): ...

class AlwaysEqual:
    def __eq__(self, other: object) -> bool:
        return True

def subclass_pattern(value: StringSubclass):
    match value:
        case "a":
            reveal_type(value)  # revealed: StringSubclass

def custom_comparison_pattern(value: str | AlwaysEqual):
    match value:
        case "a":
            reveal_type(value)  # revealed: Literal["a"] | AlwaysEqual
```

Consider the following example.

```py
from typing import Literal

def _(x: Literal["foo"] | int):
    match x:
        case "foo":
            reveal_type(x)  # revealed: Literal["foo"]

    match x:
        case "bar":
            reveal_type(x)  # revealed: Never
```

In the first `match`, the broad `int` arm is assumed to use builtin equality and cannot compare
equal to `"foo"`. In the second, neither arm can compare equal to `"bar"`. Enabling
`strict-literal-narrowing` disables this optimistic treatment of broad builtin types.

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

Some precisely modeled objects compare equal to themselves, so an equivalent value pattern is
exhaustive:

```py
from types import FunctionType
from typing import NewType, TypeVar

T = TypeVar("T")
UserId = NewType("UserId", int)

class ReflexivePatternValues:
    LIST_INT = list[int]
    TYPE_VAR = T
    NEW_TYPE = UserId

def generic_alias_value_pattern() -> int:
    match list[int]:
        case ReflexivePatternValues.LIST_INT:
            return 1

def type_var_value_pattern() -> int:
    match T:
        case ReflexivePatternValues.TYPE_VAR:
            return 1

def new_type_value_pattern() -> int:
    match UserId:
        case ReflexivePatternValues.NEW_TYPE:
            return 1

def helper() -> None: ...
def wrapper_descriptor_value_pattern() -> int:
    match FunctionType.__get__:
        case FunctionType.__get__:
            return 1

def bound_method_value_pattern() -> int:
    match helper.__get__:
        case helper.__get__:
            return 1
```

Two calls that construct equivalent objects need not produce equal values. For example, separate
`partial` objects do not compare equal, so this match is not exhaustive:

```py
from functools import partial

def target(value: int) -> int:
    return value

class PartialPatternValues:
    VALUE = partial(target, 1)

# error: [invalid-return-type]
def partial_value_pattern() -> int:
    match partial(target, 1):
        case PartialPatternValues.VALUE:
            return 1
```

```py
from typing import Literal

class C:
    pass

def _(x: Literal["foo", "bar", 42, b"foo"] | bool | complex):
    match x:
        case "foo":
            reveal_type(x)  # revealed: Literal["foo"] | float | complex
        case 42:
            reveal_type(x)  # revealed: Literal[42] | float | complex
        case 6.0:
            reveal_type(x)  # revealed: Literal["bar", b"foo"] | (int & ~Literal[42]) | float | complex
        case 1j:
            reveal_type(x)  # revealed: Literal["bar", b"foo"] | (int & ~Literal[42]) | float | complex
        case b"foo":
            reveal_type(x)  # revealed: Literal[b"foo"] | float | complex
        case _:
            reveal_type(x)  # revealed: Literal["bar"] | (int & ~Literal[42]) | float | complex
```

The same limitation applies inside a sequence. Matching a literal proves only that the element
compares equal to that literal, not that the element has the same type.

```py
def test_match_value_sequence(value: object) -> None:
    match value:
        case [1]:
            reveal_type(value[0])  # revealed: object
```

## Enum equality semantics

Enum value patterns use the enum class's actual `__eq__` implementation. Members of an enum whose
`__eq__` resolves to `object.__eq__` compare by identity and cannot equal `None`. `StrEnum` members
compare equal to string literals with the same value. Matching a member against itself is exhaustive
whenever its comparison behavior is known, even if its underlying value is not:

```toml
[environment]
python-version = "3.11"
```

```py
from enum import Enum, IntEnum, StrEnum, auto
from typing import Literal, assert_never

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

class Direction(Enum):
    NORTH = "north"
    SOUTH = "south"

def enum_member_excludes_none(direction: Direction | None) -> None:
    match direction:
        case Direction.NORTH:
            reveal_type(direction)  # revealed: Literal[Direction.NORTH]

class Status(IntEnum):
    READY = 1

def exact_int_enum_member_is_exhaustive(status: Literal[Status.READY]) -> int:
    match status:
        case Status.READY:
            return 1

class First(IntEnum):
    ONE = 1
    TWO = 2

class Second(IntEnum):
    ONE = 1
    TWO = 2

def cross_int_enum_members(value: First | Second) -> None:
    match value:
        case First.ONE:
            reveal_type(value)  # revealed: Literal[First.ONE, Second.ONE]
        case _:
            reveal_type(value)  # revealed: Literal[First.TWO, Second.TWO]

class Warning(Enum):
    W1 = auto()

class Verdict(Enum):
    V0 = auto()
    V1 = auto()
    V2 = auto()
    V3 = auto()
    V4 = auto()
    V5 = auto()
    V6 = auto()
    V7 = auto()
    V8 = auto()
    V9 = auto()
    V10 = auto()
    V11 = auto()

def many_cross_enum_cases(value: Warning | Verdict) -> None:
    match value:
        case Verdict.V0:
            return
        case Verdict.V1:
            return
        case Verdict.V2:
            return
        case Verdict.V3:
            return
        case Verdict.V4:
            return
        case Verdict.V5:
            return
        case Verdict.V6:
            return
        case Verdict.V7:
            return
        case _:
            reveal_type(value)  # revealed: Warning | Literal[Verdict.V8, Verdict.V9, Verdict.V10, Verdict.V11]

class Automatic(StrEnum):
    GENERATED = auto()

def auto_member_value_is_known(value: Literal["generated"]) -> None:
    match value:
        case Automatic.GENERATED:
            return
    assert_never(value)

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
        case "foo" if reveal_type(x):  # revealed: Literal["foo"]
            pass
        case b"bar" if reveal_type(x):  # revealed: Literal[b"bar"]
            pass
        case 42 if reveal_type(x):  # revealed: Literal[42]
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

def test_match_sequence_or_as_pattern(
    value: tuple[None] | tuple[Literal[True]],
) -> None:
    match value:
        case [None as item] | [True as item]:
            reveal_type(item)  # revealed: None | Literal[True]

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
        case "foo" | 42 if reveal_type(x):  # revealed: Literal["foo", 42]
            pass
        case b"bar" if reveal_type(x):  # revealed: Literal[b"bar"]
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

    def alias_through_alternatives(self) -> Self:
        match self:
            case (Answer.NO as item) | (Answer.YES as item) | (Answer.MAYBE as item):
                reveal_type(item)  # revealed: Self@alias_through_alternatives
                return item

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
