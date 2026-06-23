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
            reveal_type(x)  # revealed: (Sequence[int] & str) | bytes | bytearray | (int & ~Sequence[object])

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

Alternatives are tried from left to right, but a later alternative must keep any value for which an
earlier pattern can fail. Here, `Values.x` is only an annotation, so `HasX()` can fail at runtime
and the sequence alternative can still bind the value:

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
            # Class child bindings are added by a later change, so this branch cannot yet combine
            # the supported whole-pattern alias with the sequence capture.
            reveal_type(item)  # revealed: Unknown
```

Class and mapping child bindings are added by a later change. Until then, an `or` pattern that mixes
one of those patterns with a supported alternative falls back to `Unknown` instead of inferring a
type from only the supported alternative.

```py
from typing import final
from ty_extensions import Unknown

@final
class TextValue:
    value: str = ""

def class_or_sequence_binding(value: TextValue | tuple[int]) -> None:
    match value:
        case TextValue(value=item) | [item]:
            reveal_type(item)  # revealed: Unknown

def mapping_or_sequence_binding(value: dict[str, str] | tuple[int]) -> None:
    match value:
        case {"value": item} | [item]:
            reveal_type(item)  # revealed: Unknown
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
```

## Indirect class patterns

A class pattern can use a variable whose type is `type[Class]`. Both the subject and an `as` binding
use the instance type described by that annotation.

```py
class IndirectPattern: ...

def test_match_indirect_class_pattern(
    value: object,
    PatternClass: type[IndirectPattern],
) -> None:
    match value:
        case PatternClass() as item:
            reveal_type(item)  # revealed: IndirectPattern
            reveal_type(value)  # revealed: IndirectPattern
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

## Exhaustive positional patterns for built-in classes

Python defines a fixed set of built-in classes whose single positional subpattern receives the
entire subject. This example checks every such class, including nested sequence and `or` patterns:

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

## Runtime dictionary class patterns

A `TypedDict` value is a dictionary at runtime, so argumentless `dict` and `Mapping` patterns always
match it. The positional `dict` pattern does as well:

```py
from collections.abc import Mapping
from typing import TypedDict

class Movie(TypedDict):
    title: str

def typed_dict_argumentless_dict_pattern_is_exhaustive(value: Movie) -> int:
    match value:
        case dict():
            return 1

def typed_dict_mapping_pattern_is_exhaustive(value: Movie) -> int:
    match value:
        case Mapping():
            return 1

def typed_dict_positional_dict_pattern_is_exhaustive(value: Movie) -> int:
    match value:
        case dict(_):
            return 1
```

## Required `TypedDict` keys

A mapping pattern is exhaustive for a `TypedDict` when every key in the pattern names a required
field and every nested pattern accepts the field's complete type. Optional, absent, and non-string
keys are not guaranteed to be present.

```py
from typing import Literal, TypedDict

class RequiredPayload(TypedDict):
    tag: Literal["int"]
    value: int

class OptionalPayload(TypedDict, total=False):
    value: int

def required_keys_are_exhaustive(value: RequiredPayload) -> int:
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

def builtin_base_positional_pattern_is_exhaustive_for_subclass(value: MyInt) -> int:
    match value:
        case int(_):
            return 1

def nested_builtin_positional_pattern_is_exhaustive_for_subclass(
    value: tuple[MyInt],
) -> int:
    match value:
        case [int(_)]:
            return 1

def builtin_positional_literal_is_not_exhaustive(
    value: MyInt,
    # error: [invalid-return-type]
) -> int:
    match value:
        case int(0):
            return 1
```

## Resolving `__match_args__`

For other positional class patterns, Python reads `__match_args__` from the pattern class. The
attribute must be definitely present and must be a fixed tuple of literal attribute names. The
selected attributes must also be definitely present on the subject:

```py
class MyIntWithDefinitelyBoundMatchArgs(int):
    __match_args__ = ("real",)

def builtin_subclass_with_definitely_bound_match_args_is_exhaustive(
    value: MyIntWithDefinitelyBoundMatchArgs,
) -> int:
    match value:
        case MyIntWithDefinitelyBoundMatchArgs(_):
            return 1

def nested_builtin_subclass_with_definitely_bound_match_args_is_exhaustive(
    value: tuple[MyIntWithDefinitelyBoundMatchArgs],
) -> int:
    match value:
        case [MyIntWithDefinitelyBoundMatchArgs(_)]:
            return 1

class MyIntWithMissingMatchArg(int):
    __match_args__ = ("missing",)

def builtin_subclass_with_missing_match_arg_is_not_exhaustive(
    value: MyIntWithMissingMatchArg,
    # error: [invalid-return-type]
) -> int:
    match value:
        case MyIntWithMissingMatchArg(_):
            return 1

class MatchArgsMeta(type):
    __match_args__ = ("missing",)

class MyIntWithMetaclassMatchArgs(int, metaclass=MatchArgsMeta): ...

def builtin_subclass_with_metaclass_match_args_is_not_exhaustive(
    value: MyIntWithMetaclassMatchArgs,
    # error: [invalid-return-type]
) -> int:
    match value:
        case MyIntWithMetaclassMatchArgs(_):
            return 1

class KnownAttributes:
    __match_args__ = ("x", "y")
    x: int = 0
    y: int = 0

def fixed_match_args_are_exhaustive(value: KnownAttributes) -> int:
    match value:
        case KnownAttributes(_, _):
            return 1

class WidenedMatchArgs:
    __match_args__: tuple[str, ...] = ("x",)
    x: int = 0

def widened_match_args_does_not_identify_an_attribute(
    value: WidenedMatchArgs,
    # error: [invalid-return-type]
) -> int:
    match value:
        case WidenedMatchArgs(_):
            return 1

class AnnotatedMatchArgs(int):
    __match_args__: tuple[str, ...]

def annotated_match_args_is_not_exhaustive(
    value: AnnotatedMatchArgs,
    # error: [invalid-return-type]
) -> int:
    match value:
        case AnnotatedMatchArgs(_):
            return 1

def condition() -> bool:
    return bool()

flag = condition()

class PossiblyBoundMatchArgs:
    if flag:
        __match_args__ = ("x",)
    x: int = 0

def possibly_bound_match_args_is_not_exhaustive(
    value: PossiblyBoundMatchArgs,
    # error: [invalid-return-type]
) -> int:
    match value:
        case PossiblyBoundMatchArgs(_):
            return 1

class PossiblyBoundIntMatchArgs(int):
    if flag:
        __match_args__ = ("missing",)

def possibly_bound_match_args_does_not_enable_match_self(
    value: PossiblyBoundIntMatchArgs,
    # error: [invalid-return-type]
) -> int:
    match value:
        case PossiblyBoundIntMatchArgs(_):
            return 1
```

## Properties and declared attributes

Properties and declared attributes count as present when checking exhaustiveness, even though
descriptor access can raise `AttributeError` and an annotated attribute can be absent at runtime:

```py
from typing import Literal

class FallibleProperty:
    __match_args__ = ("x",)

    @property
    def x(self) -> Literal[1]:
        raise AttributeError

def direct_fallible_property_is_statically_exhaustive(value: FallibleProperty) -> int:
    match value:
        case FallibleProperty(_):
            return 1

def fallible_property_value_pattern_is_statically_exhaustive(value: FallibleProperty) -> int:
    match value:
        case FallibleProperty(x=1):
            return 1

class DeclaredLiteralAttribute:
    x: Literal[1]

def declared_literal_attribute_subpattern_is_exhaustive(
    value: DeclaredLiteralAttribute,
) -> int:
    match value:
        case DeclaredLiteralAttribute(x=1):
            return 1
```

## Non-final subclasses

Exhaustiveness follows the static member model, so a member inherited by a non-final subclass is
treated as present. The same rule applies inside a sequence pattern:

```py
class BaseWithX:
    x: int = 0

class NonFinalChild(BaseWithX): ...

def non_final_subclass_member_is_exhaustive(value: NonFinalChild) -> int:
    match value:
        case BaseWithX(x=_):
            return 1

def nested_non_final_subclass_member_is_exhaustive(value: tuple[NonFinalChild]) -> int:
    match value:
        case [BaseWithX(x=_)]:
            return 1
```

## Runtime-checkable protocol patterns

Runtime-checkable protocol patterns also follow the static member model. A subject that statically
satisfies the protocol is treated as exhaustive regardless of whether its class is final:

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

class DeclaredRuntimeProtocolImplementer:
    x: int

def argumentless_runtime_protocol_pattern_is_exhaustive(
    value: DeclaredRuntimeProtocolImplementer,
) -> int:
    match value:
        case RuntimeProtocolWithX():
            return 1

def nested_argumentless_runtime_protocol_pattern_is_exhaustive(
    value: tuple[DeclaredRuntimeProtocolImplementer],
) -> int:
    match value:
        case [RuntimeProtocolWithX()]:
            return 1

def nested_argumentless_runtime_protocol_union_preserves_fallback(
    value: tuple[DeclaredRuntimeProtocolImplementer | int],
) -> None:
    match value:
        case [RuntimeProtocolWithX()]:
            pass
        case [DeclaredRuntimeProtocolImplementer()]:
            reveal_type(value[0])  # revealed: DeclaredRuntimeProtocolImplementer

def nested_argumentless_runtime_protocol_list_does_not_narrow_fallthrough(
    value: list[DeclaredRuntimeProtocolImplementer | int],
) -> None:
    match value:
        case [RuntimeProtocolWithX()]:
            pass
        case [DeclaredRuntimeProtocolImplementer()]:
            reveal_type(value[0])  # revealed: DeclaredRuntimeProtocolImplementer | int
```

## Subject-class members

A member known on the static subject type can make a base-class pattern exhaustive. The special
positional behavior of built-in classes still comes from the pattern class, not from another base of
the subject class:

```py
class BaseWithoutX: ...

class ChildWithX(BaseWithoutX):
    x: int = 0

def subclass_member_is_exhaustive(value: ChildWithX) -> int:
    match value:
        case BaseWithoutX(x=_):
            return 1

def nested_subclass_member_is_exhaustive(
    value: tuple[ChildWithX],
) -> int:
    match value:
        case [BaseWithoutX(x=_)]:
            return 1

class PlainBase: ...
class IntPlainChild(int, PlainBase): ...

def builtin_positional_behavior_comes_from_pattern_class(
    value: IntPlainChild,
    # error: [invalid-return-type]
) -> int:
    match value:
        case PlainBase(_):
            return 1
```

## Nested class patterns

Every nested pattern must match all possible values of the attribute it receives:

```py
from typing import Literal

class Inner:
    x: int = 0

class Outer:
    inner: Inner = Inner()

def nested_class_subpattern_is_exhaustive(value: tuple[Outer]) -> int:
    match value:
        case [Outer(inner=Inner(x=_))]:
            return 1

class LiteralAttribute:
    x: Literal[1] = 1

def literal_attribute_subpattern_is_exhaustive(value: LiteralAttribute) -> int:
    match value:
        case LiteralAttribute(x=1):
            return 1
```

## Missing class-pattern attributes

A class pattern can fail after its `isinstance` check if a requested attribute is missing. This
preserves both direct fallthrough and fallthrough from a nested sequence pattern:

```py
from typing import final

class MissingAttributes:
    __match_args__ = ("x", "missing")
    x: int = 0

class OtherClass: ...

def keyword_class_pattern_preserves_direct_fallback(
    value: MissingAttributes | OtherClass,
) -> None:
    match value:
        case MissingAttributes(missing=_):
            pass
        case _:
            reveal_type(value)  # revealed: MissingAttributes | OtherClass

def keyword_class_pattern_in_sequence_preserves_fallback(
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

## Sequence exhaustiveness

Sequence patterns also contribute to negative narrowing and exhaustiveness. Exact tuple shapes can
make a match exhaustive.

```py
from typing import final
from typing_extensions import assert_never

class LengthBase:
    x: int

@final
class LengthChild(LengthBase): ...

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

# Checking an element against its subject type does not replace the sequence pattern's length
# check. This one-element pattern cannot match a two-element tuple.
def subject_aware_element_keeps_length_check(
    value: tuple[LengthChild, LengthChild],
    # error: [invalid-return-type]
) -> int:
    match value:
        case [LengthBase(x=_)]:
            return 1

def test_match_exact_tuple_element_union_is_exhaustive(x: tuple[int | str]) -> int:
    match x:
        case [int()]:
            return 42
        case [str()]:
            return 42
        case _:
            # revealed: Never
            reveal_type(x)

def test_match_exact_mutable_sequence_negative(value: list[int]) -> None:
    match value:
        case [int()]:
            pass
        case _:
            reveal_type(value)  # revealed: list[int]
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
