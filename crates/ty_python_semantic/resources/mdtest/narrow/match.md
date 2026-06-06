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
from enum import Enum, IntEnum
from typing import Any, Literal, TypeAlias, TypeVar
from typing_extensions import assert_never

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

def test_match_star_capture(value: tuple[int, str, bool]) -> None:
    match value:
        case [first, *rest]:
            reveal_type(first)  # revealed: int
            reveal_type(rest)  # revealed: list[str | bool]

def test_match_star_capture_between_patterns(value: tuple[int, bytes, str]) -> None:
    match value:
        case [int(), *rest, str()]:
            reveal_type(rest)  # revealed: list[bytes]

def test_match_star_capture_filters_union_arms(
    value: tuple[Literal[1], int, int] | tuple[Literal[2], str, str],
) -> list[int]:
    match value:
        case [1, *rest]:
            reveal_type(rest)  # revealed: list[int]
            return rest
        case _:
            return []

def test_match_star_capture_preserves_compatible_union_arms(
    value: tuple[Literal[1], int, int] | tuple[Literal[2], str, str],
) -> None:
    match value:
        case [_, *rest]:
            reveal_type(rest)  # revealed: list[int] | list[str]

def test_match_capture_filters_union_arms(
    value: tuple[Literal[1], int] | tuple[Literal[2], str],
) -> int:
    match value:
        case [1, item]:
            reveal_type(item)  # revealed: int
            return item
        case _:
            return 0

def test_match_capture_preserves_compatible_union_arms(
    value: tuple[Literal[1], int] | tuple[Literal[2], str],
) -> None:
    match value:
        case [_, item]:
            reveal_type(item)  # revealed: int | str

MatchPair: TypeAlias = tuple[Literal[1], int] | tuple[Literal[2], str]
MatchStarPair: TypeAlias = tuple[Literal[1], int, int] | tuple[Literal[2], str, str]
MatchPairT = TypeVar(
    "MatchPairT",
    tuple[Literal[1], int],
    tuple[Literal[2], str],
)

def test_match_capture_filters_aliased_union_arms(value: MatchPair) -> None:
    match value:
        case [1, item]:
            reveal_type(item)  # revealed: int

def test_match_star_capture_filters_aliased_union_arms(value: MatchStarPair) -> None:
    match value:
        case [1, *rest]:
            reveal_type(rest)  # revealed: list[int]

def test_match_capture_filters_constrained_typevar_arms(value: MatchPairT) -> None:
    match value:
        case [1, item]:
            reveal_type(item)  # revealed: int

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
) -> tuple[int]:
    match value:
        case [_] as whole:
            reveal_type(whole)  # revealed: tuple[int]
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

class Number(IntEnum):
    ONE = 1

class RecursiveNumber(IntEnum):
    _value_: "RecursiveNumber"
    ONE = 1

def test_match_capture_preserves_int_enum_equal_arm(
    value: tuple[Literal[1], int],
) -> str:
    match value:
        case [Number.ONE, item]:
            reveal_type(item)  # revealed: int
            return item  # error: [invalid-return-type]
        case _:
            return ""

def test_match_capture_preserves_recursive_int_enum_arm(
    value: tuple[Literal[1], int],
) -> str:
    match value:
        case [RecursiveNumber.ONE, item]:
            reveal_type(item)  # revealed: int
            return item  # error: [invalid-return-type]
        case _:
            return ""

def test_match_capture_int_enum_correlation_todo(
    value: tuple[Literal[1], int] | tuple[Literal[2], str],
) -> None:
    match value:
        case [Number.ONE, item]:
            # TODO: Narrow this to `int` by comparing known `IntEnum` member values.
            reveal_type(item)  # revealed: int | str

class CustomNeEnum(Enum):
    A = 1
    B = 2

    def __ne__(self, other: object) -> Literal[True]:
        return True

def test_match_capture_enum_custom_ne_todo() -> None:
    value = (CustomNeEnum.B, "actual")
    match value:
        case [CustomNeEnum.A, item]:
            # TODO: Preserve enum-member identity when equality is overridden.
            reveal_type(item)  # revealed: Literal["actual"]

class AlwaysEqualEnum(Enum):
    A = 1
    B = 2

    def __eq__(self, other: object) -> Literal[True]:
        return True

def test_match_capture_preserves_custom_equal_enum_arm() -> int:
    value = (AlwaysEqualEnum.B, "actual")
    match value:
        case [AlwaysEqualEnum.A, item]:
            reveal_type(item)  # revealed: Literal["actual"]
            return item  # error: [invalid-return-type]
        case _:
            return 0

class AlwaysEqualTuple(tuple[int, ...]):
    def __eq__(self, other: object) -> Literal[True]:
        return True

class InheritedAlwaysEqualTuple(AlwaysEqualTuple):
    pass

def test_match_alias_preserves_custom_equal_tuple_subclass(
    value: InheritedAlwaysEqualTuple | bytes,
) -> bytes:
    match value:
        case 1 as item:
            return item  # error: [invalid-return-type]
        case _:
            return b""

class AlwaysEqualMeta(type):
    def __eq__(cls, other: object) -> Literal[True]:
        return True

class EqualA(metaclass=AlwaysEqualMeta):
    pass

class EqualB(metaclass=AlwaysEqualMeta):
    pass

class EqualConstants:
    A = EqualA

def test_match_capture_preserves_custom_equal_class_arm() -> int:
    value = (EqualB, "actual")
    match value:
        case [EqualConstants.A, item]:
            return item  # error: [invalid-return-type]
        case _:
            return 0

class NeverEqualMeta(type):
    def __eq__(cls, other: object) -> Literal[False]:
        return False

class NeverEqualValue(metaclass=NeverEqualMeta):
    pass

class NeverEqualConstants:
    VALUE = NeverEqualValue

def test_match_alias_preserves_nonreflexive_value(flag: bool) -> str:
    value = NeverEqualValue if flag else "fallback"
    match value:
        case NeverEqualConstants.VALUE:
            return ""
        case _ as item:
            return item  # error: [invalid-return-type]

class CustomNeMeta(type):
    def __ne__(cls, other: object) -> Literal[True]:
        return True

class CustomNeA(metaclass=CustomNeMeta):
    pass

class CustomNeB(metaclass=CustomNeMeta):
    pass

class CustomNeConstants:
    A = CustomNeA

def test_match_capture_ignores_custom_ne() -> None:
    value = (CustomNeB, "actual")
    match value:
        case [CustomNeConstants.A, item]:
            reveal_type(item)  # revealed: Never

def test_match_alias_ignores_custom_ne(flag: bool) -> str:
    value = CustomNeA if flag else "fallback"
    match value:
        case CustomNeConstants.A:
            return ""
        case _ as item:
            reveal_type(item)  # revealed: Literal["fallback"]
            return item

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

def test_match_builtin_self_patterns_are_exhaustive(
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

class MyInt(int): ...

def test_match_builtin_self_subclass_is_exhaustive(value: MyInt) -> int:
    match value:
        case MyInt(item):
            return item

class MyIntWithMatchArgs(int):
    __match_args__ = ("missing",)

def test_match_builtin_subclass_with_match_args_is_not_exhaustive(
    value: MyIntWithMatchArgs,
    # error: [invalid-return-type]
) -> int:
    match value:
        case MyIntWithMatchArgs(_):
            return 1

class MatchArgsMeta(type):
    __match_args__ = ("missing",)

class MyIntWithMetaclassMatchArgs(int, metaclass=MatchArgsMeta): ...

def test_match_builtin_subclass_with_metaclass_match_args_is_not_exhaustive(
    value: MyIntWithMetaclassMatchArgs,
    # error: [invalid-return-type]
) -> int:
    match value:
        case MyIntWithMetaclassMatchArgs(_):
            return 1

def test_match_variable_class_sequence_is_not_exhaustive(
    value: tuple[int],
    C: type[int] | type[str],
    # error: [invalid-return-type]
) -> int:
    match value:
        case [C()]:
            return 1

def test_match_variable_class_sequence_fallback(
    value: tuple[int],
    C: type[int] | type[str],
) -> None:
    match value:
        case [C()]:
            pass
        case _:
            reveal_type(value)  # revealed: tuple[int]

def test_match_dynamic_class_sequence_is_not_exhaustive(
    value: tuple[int],
    C: Any,
    # error: [invalid-return-type]
) -> int:
    match value:
        case [C()]:
            return 1

def test_match_subclass_class_sequence_is_not_exhaustive(
    value: tuple[int],
    C: type[int],
    # error: [invalid-return-type]
) -> int:
    match value:
        case [C()]:
            return 1

def test_match_fixed_class_sequence_is_exhaustive(value: tuple[int]) -> int:
    match value:
        case [int()]:
            return 1

class ClassWithoutX: ...

class ClassWithMatchArgs:
    __match_args__ = ("x",)

def test_match_class_positional_sequence_is_not_exhaustive(
    value: tuple[ClassWithMatchArgs],
    # error: [invalid-return-type]
) -> int:
    match value:
        case [ClassWithMatchArgs(_)]:
            return 1

def test_match_class_attribute_sequence_is_not_exhaustive(
    value: tuple[ClassWithoutX],
    # error: [invalid-return-type]
) -> int:
    match value:
        case [ClassWithoutX(x=_)]:
            return 1

def test_match_class_attribute_sequence_fallback(
    value: tuple[ClassWithoutX],
) -> None:
    match value:
        case [ClassWithoutX(x=_)]:
            pass
        case _:
            reveal_type(value)  # revealed: tuple[ClassWithoutX]

def test_match_exact_mutable_sequence_negative(value: list[int]) -> None:
    match value:
        case [int()]:
            pass
        case _:
            # revealed: list[int] & ~<Protocol with members '__getitem__', '__len__'>
            reveal_type(value)

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

def test_match_sequence_as_pattern_excludes_previous_cases(
    value: tuple[Literal[1], object] | tuple[Literal[2], object],
) -> None:
    match value:
        case [1, _]:
            pass
        case [int() as item, _]:
            reveal_type(item)  # revealed: Literal[2]

def test_match_value_sequence(value: object) -> None:
    match value:
        case [1]:
            # Value patterns use equality, so matching `1` does not prove that
            # the element is an `int`.
            reveal_type(value[0])  # revealed: object
```

## Sequence patterns with `StrEnum`

```toml
[environment]
python-version = "3.11"
```

```py
from enum import StrEnum
from typing import Literal

class Word(StrEnum):
    ONE = "one"

def test_match_capture_preserves_str_enum_equal_arm(
    value: tuple[Literal["one"], str],
) -> int:
    match value:
        case [Word.ONE, item]:
            reveal_type(item)  # revealed: str
            return item  # error: [invalid-return-type]
        case _:
            return 0
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

    reveal_type(x)  # revealed: B | (A & ~B)
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
