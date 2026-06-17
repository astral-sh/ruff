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
            reveal_type(a)  # revealed: @Todo(`match` pattern definition types)
            reveal_type(b)  # revealed: @Todo(`match` pattern definition types)
        case _:
            reveal_type(x)  # revealed: str

def test_match_exact_sequence_excludes_bytes(x: bytes | tuple[int, int]) -> None:
    match x:
        case (a, b):
            reveal_type(a)  # revealed: @Todo(`match` pattern definition types)
            reveal_type(b)  # revealed: @Todo(`match` pattern definition types)
        case _:
            reveal_type(x)  # revealed: bytes

def test_match_exact_sequence_excludes_bytearray(x: bytearray | tuple[int, int]) -> None:
    match x:
        case (a, b):
            reveal_type(a)  # revealed: @Todo(`match` pattern definition types)
            reveal_type(b)  # revealed: @Todo(`match` pattern definition types)
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

def test_match_value_sequence(value: object) -> None:
    match value:
        case [1]:
            # Value patterns use equality, so matching `1` does not prove that
            # the element is an `int`.
            reveal_type(value[0])  # revealed: object
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

## Subject-time bindings in display subjects

Each element constraint applies to the binding read while that subject element was evaluated. It
does not constrain a binding introduced by a later subject element, pattern capture, or guard.

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

def match_tuple_expression_subject_capture(a: TupleSubjectA, b: TupleSubjectB) -> None:
    match a, b:
        case [TupleSubjectA1(), a]:
            reveal_type(a)  # revealed: @Todo(`match` pattern definition types)

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
            reveal_type(captured)  # revealed: @Todo(`match` pattern definition types)

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
