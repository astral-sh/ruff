# Narrowing for `!=` and `==` conditionals

## `x != None`

```py
from typing import Literal

def _(x: None | Literal[1]):
    if x != None:
        reveal_type(x)  # revealed: Literal[1]
    else:
        reveal_type(x)  # revealed: None
```

## `None != x` (reversed operands)

```py
from typing import Literal

def _(x: None | Literal[1]):
    if None != x:
        reveal_type(x)  # revealed: Literal[1]
    else:
        reveal_type(x)  # revealed: None
```

This also works for `==` with reversed operands:

```py
from typing import Literal

def _(x: None | Literal[1]):
    if None == x:
        reveal_type(x)  # revealed: None
    else:
        reveal_type(x)  # revealed: Literal[1]
```

## `!=` for other singleton types

### Bool

```py
def _(x: bool):
    if x != False:
        reveal_type(x)  # revealed: Literal[True]
    else:
        reveal_type(x)  # revealed: Literal[False]

def _(x: bool):
    if x == False:
        reveal_type(x)  # revealed: Literal[False]
    else:
        reveal_type(x)  # revealed: Literal[True]
```

### Enums

```py
from enum import Enum
from typing import Literal

from ty_extensions import Intersection, Not

class Answer(Enum):
    NO = 0
    YES = 1

def _(answer: Answer):
    if answer != Answer.NO:
        reveal_type(answer)  # revealed: Literal[Answer.YES]
    else:
        reveal_type(answer)  # revealed: Literal[Answer.NO]

def _(answer: Answer):
    if answer == Answer.NO:
        reveal_type(answer)  # revealed: Literal[Answer.NO]
    else:
        reveal_type(answer)  # revealed: Literal[Answer.YES]

class Single(Enum):
    VALUE = 1

def _(x: Single | int):
    if x != Single.VALUE:
        reveal_type(x)  # revealed: int
    else:
        reveal_type(x)  # revealed: Single

def _(x: Single | int):
    if x == Single.VALUE:
        reveal_type(x)  # revealed: Single
    else:
        reveal_type(x)  # revealed: int

def _(x: list[int] | Literal[Answer.NO]):
    if x != Answer.NO:
        reveal_type(x)  # revealed: list[int]

def _(x: list[int] | Literal[Answer.NO]):
    if x == Answer.NO:
        return
    reveal_type(x)  # revealed: list[int]

class Color(Enum):
    RED = "red"
    GREEN = "green"
    BLUE = "blue"

def after_excluding_red(x: Color | int):
    if x is Color.RED:
        return

    if x == Color.GREEN:
        reveal_type(x)  # revealed: Literal[Color.GREEN]
    else:
        reveal_type(x)  # revealed: Literal[Color.BLUE] | int

def enum_complement_rhs(x: Color, y: Intersection[Color, Not[Literal[Color.RED]]]):
    if x == y:
        reveal_type(x)  # revealed: Literal[Color.GREEN, Color.BLUE]
```

When both operands are restricted to members of the same enum, equality narrows each operand to the
members allowed by both. If the restrictions do not overlap, the comparison is always false:

```py
from enum import Enum, IntEnum, StrEnum
from typing import Literal

class Choice(StrEnum):
    FIRST = "first"
    SECOND = "second"
    THIRD = "third"
    FOURTH = "fourth"

def compare_after_truthiness_check(left: Choice, right: Choice):
    if right and left != right:
        reveal_type(right)  # revealed: Choice & ~AlwaysFalsy
        return

    reveal_type(right)  # revealed: Choice

def compare_with_narrowed_right(left: Choice, right: Choice):
    if right == Choice.FIRST:
        return
    if left == right:
        reveal_type(left)  # revealed: Literal[Choice.SECOND, Choice.THIRD, Choice.FOURTH]

def compare_non_overlapping_narrowed_values(left: Choice, right: Choice):
    if left == Choice.FIRST or left == Choice.SECOND:
        return
    if right == Choice.THIRD or right == Choice.FOURTH:
        return

    reveal_type(left == right)  # revealed: Literal[False]

def compare_literal_unions(
    left: Literal[Choice.FIRST, Choice.SECOND],
    right: Literal[Choice.SECOND, Choice.THIRD],
):
    if left == right:
        reveal_type(left)  # revealed: Literal[Choice.SECOND]
        reveal_type(right)  # revealed: Literal[Choice.SECOND]

def compare_non_overlapping_literal_unions(
    left: Literal[Choice.FIRST, Choice.SECOND],
    right: Literal[Choice.THIRD, Choice.FOURTH],
):
    reveal_type(left == right)  # revealed: Literal[False]
```

Adding `None` to either side must not change which enum values can match:

```py
def compare_optional_left(left: Choice | None, right: Choice):
    if left == right:
        reveal_type(left)  # revealed: Choice
    else:
        reveal_type(left)  # revealed: Choice | None

def compare_optional_right(left: Choice, right: Choice | None):
    if left == right:
        reveal_type(right)  # revealed: Choice
```

With ty's default builtin-equality assumptions, neither an integer nor `None` matches a
string-valued enum member:

```py
def compare_enum_with_integer(left: Choice | int | None, right: Choice):
    if left == right:
        reveal_type(left)  # revealed: Choice
    else:
        reveal_type(left)  # revealed: Choice | int | None
```

Members with the same known value are aliases, even when one value comes from a function call.
Comparisons between their canonical members are always true:

```py
def make_value() -> Literal["value"]:
    return "value"

class RuntimeAlias(StrEnum):
    FIRST = make_value()
    SECOND = "value"

reveal_type(RuntimeAlias.FIRST == RuntimeAlias.SECOND)  # revealed: Literal[True]

def make_int_value() -> Literal[1]:
    return 1

class RuntimeIntAlias(IntEnum):
    FIRST = make_int_value()
    SECOND = 1

reveal_type(RuntimeIntAlias.FIRST == RuntimeIntAlias.SECOND)  # revealed: Literal[True]
```

An enum with a `str` data type constructs its values before checking for aliases. Here, `str`
converts `1` to `"1"`, so the two members are aliases:

```py
class CoercingAlias(str, Enum):
    FIRST = 1
    SECOND = "1"

reveal_type(CoercingAlias.FIRST == CoercingAlias.SECOND)  # revealed: Literal[True]
reveal_type(CoercingAlias.SECOND == "1")  # revealed: Literal[True]
```

When alias detection is inconclusive, equality between different declarations is also unknown. The
two declarations below are aliases at runtime:

```py
class Behavior:
    pass

class OpaqueAliases(Behavior, Enum):
    FIRST = 1
    SECOND = 1

reveal_type(OpaqueAliases.FIRST == OpaqueAliases.SECOND)  # revealed: bool
```

Equality can transfer restrictions on enum members, but other intersection elements must stay on the
operand where they originated:

```py
from enum import StrEnum
from typing import Any, Literal, NewType
from ty_extensions import Intersection

class Response(StrEnum):
    ACCEPT = "accept"
    REJECT = "reject"

Tag = NewType("Tag", str)

def compare_any(
    left: Response,
    right: Intersection[Literal[Response.REJECT], Any],
):
    if left != right:
        return
    reveal_type(left)  # revealed: Literal[Response.REJECT]
    reveal_type(right)  # revealed: Literal[Response.REJECT] & Any

def compare_newtype(left: Response, right: Intersection[Response, Tag]):
    if left != right:
        return
    reveal_type(left)  # revealed: Response
```

`Flag` and `IntFlag` values can include zero and unnamed combinations, so their named members do not
cover every possible value:

```py
from enum import Flag, IntFlag
from typing import Literal

class Permission(Flag):
    READ = 1

class Mode(IntFlag):
    READ = 1

FunctionalPermission = Flag("FunctionalPermission", {"READ": 1})

def compare_flags(left: Permission, right: Permission):
    reveal_type(left == right)  # revealed: bool

    if left != right:
        reveal_type(left)  # revealed: Permission

def exclude_declared_flag(value: Permission):
    if value is Permission.READ:
        return
    reveal_type(value)  # revealed: Permission & ~Literal[Permission.READ]

def compare_flag_literals(
    left: Literal[Permission.READ],
    right: Literal[Permission.READ],
):
    reveal_type(left == right)  # revealed: Literal[True]

def compare_int_flags(left: Mode, right: Mode):
    reveal_type(left == right)  # revealed: bool

def compare_functional_flags(left: FunctionalPermission, right: FunctionalPermission):
    reveal_type(left == right)  # revealed: bool
```

A custom `_missing_` method does not change the enum's static member set, so an enum with one
declared member remains a singleton:

```py
from enum import Enum

class MissingValueEnum(Enum):
    ONLY = 1

    @classmethod
    def _missing_(cls, value: object) -> "MissingValueEnum":
        return cls.ONLY

def compare_custom_missing_enums(left: MissingValueEnum, right: MissingValueEnum):
    reveal_type(left == right)  # revealed: Literal[True]

    if left != right:
        reveal_type(left)  # revealed: Never
```

A custom enum metaclass can add members that do not appear in the class body. Two values of a
one-member class therefore need not be equal:

```py
from enum import Enum, EnumMeta

class InjectingEnumMeta(EnumMeta):
    def __new__(metacls, name, bases, namespace, **kwargs):
        namespace["INJECTED"] = 2
        return super().__new__(metacls, name, bases, namespace, **kwargs)

class TransformedEnum(Enum, metaclass=InjectingEnumMeta):
    ONLY = 1

def compare_transformed_enums(left: TransformedEnum, right: TransformedEnum):
    reveal_type(left == right)  # revealed: bool
```

A custom comparison method determines the result even when both operands have the same enum type:

```py
from enum import Enum
from typing import Literal

class NeverEqual(Enum):
    FIRST = 1
    SECOND = 2
    THIRD = 3

    def __eq__(self, other: object) -> Literal[False]:
        return False

def compare_custom(left: NeverEqual, right: NeverEqual):
    reveal_type(left == right)  # revealed: Literal[False]

    if left is NeverEqual.FIRST:
        return
    reveal_type(left == right)  # revealed: Literal[False]
```

When member values are not known statically, two different members may still compare equal:

```py
from enum import StrEnum
from typing import Literal

def runtime_value(value: str) -> str:
    return value

class UnknownValues(StrEnum):
    FIRST = runtime_value("first")
    SECOND = runtime_value("second")

def compare_unknown_values(
    left: Literal[UnknownValues.FIRST],
    right: Literal[UnknownValues.SECOND],
):
    reveal_type(left == right)  # revealed: bool
```

Unlike plain `Enum` members, `IntEnum` members inherit integer equality. Members of different
`IntEnum` classes therefore compare equal when they have the same integer value, so both equality
and inequality narrowing must account for matching members from every class in the union:

```py
from enum import IntEnum

class Foo(IntEnum):
    X = 1
    Y = 2

class Bar(IntEnum):
    A = 1
    B = 2

reveal_type(Foo.X.value)  # revealed: Literal[1]

def _(value: Foo | Bar):
    if value == Foo.X:
        reveal_type(value)  # revealed: Literal[Foo.X, Bar.A]
    else:
        reveal_type(value)  # revealed: Literal[Foo.Y, Bar.B]

    if value != Foo.X:
        reveal_type(value)  # revealed: Literal[Foo.Y, Bar.B]
    else:
        reveal_type(value)  # revealed: Literal[Foo.X, Bar.A]
```

`StrEnum` domains from different classes are compared by their string values. Equality retains the
members whose values occur in both domains; inequality against a singleton excludes the matching
member. Exact member comparisons are true or false when both values are known:

```py
from enum import StrEnum
from typing import Any, Literal
from typing_extensions import assert_type

class Left(StrEnum):
    A = "a"
    SHARED = "shared"
    C = "c"

class Right(StrEnum):
    SHARED = "shared"
    B = "b"
    D = "d"

reveal_type(Left.SHARED == Right.SHARED)  # revealed: Literal[True]
reveal_type(Left.A == Right.B)  # revealed: Literal[False]
reveal_type(Left.SHARED != Right.SHARED)  # revealed: Literal[False]

def compare_domains(left: Left, right: Right):
    if left == right:
        reveal_type(left)  # revealed: Literal[Left.SHARED]
        reveal_type(right)  # revealed: Literal[Right.SHARED]
    else:
        reveal_type(left)  # revealed: Left
        reveal_type(right)  # revealed: Right

    if left != right:
        reveal_type(left)  # revealed: Left
        reveal_type(right)  # revealed: Right
    else:
        reveal_type(left)  # revealed: Literal[Left.SHARED]
        reveal_type(right)  # revealed: Literal[Right.SHARED]

def compare_singleton(left: Left, right: Literal[Right.SHARED]):
    if left != right:
        reveal_type(left)  # revealed: Literal[Left.A, Left.C]
    else:
        reveal_type(left)  # revealed: Literal[Left.SHARED]

def compare_subsets(
    left: Literal[Left.A, Left.SHARED],
    right: Literal[Right.SHARED, Right.B],
):
    if left == right:
        reveal_type(left)  # revealed: Literal[Left.SHARED]
        reveal_type(right)  # revealed: Literal[Right.SHARED]
```

When only one side can be `None`, equality still narrows both enums to their shared value:

```py
def compare_optional_cross_enum_left(left: Left | None, right: Right):
    if left == right:
        reveal_type(left)  # revealed: Literal[Left.SHARED]
        reveal_type(right)  # revealed: Literal[Right.SHARED]

def compare_optional_cross_enum_right(left: Left, right: Right | None):
    if left == right:
        reveal_type(left)  # revealed: Literal[Left.SHARED]
        reveal_type(right)  # revealed: Literal[Right.SHARED]
```

When both sides can be `None`, equality can match `None` or the shared string:

```py
def compare_both_optional_cross_enums(left: Left | None, right: Right | None):
    if left == right:
        reveal_type(left)  # revealed: Literal[Left.SHARED] | None
        reveal_type(right)  # revealed: Literal[Right.SHARED] | None
```

Under the same assumptions, an unrelated integer does not change which enum members match, whether
the condition uses `==` or `!=`:

```py
def compare_cross_enums_with_integer(left: Left | None, right: Right | int):
    if left == right:
        reveal_type(left)  # revealed: Literal[Left.SHARED]
        reveal_type(right)  # revealed: Literal[Right.SHARED]

    if left != right:
        reveal_type(left)  # revealed: Left | None
        reveal_type(right)  # revealed: Right | int
    else:
        reveal_type(left)  # revealed: Literal[Left.SHARED]
        reveal_type(right)  # revealed: Literal[Right.SHARED]
```

A plain string can also match a member of the other enum. The string and every matching enum member
must remain possible:

```py
def compare_left_string_against_enum_members(left: Left | Literal["b"], right: Right):
    if left == right:
        reveal_type(left)  # revealed: Literal[Left.SHARED, "b"]
        reveal_type(right)  # revealed: Literal[Right.SHARED, Right.B]

def compare_right_string_against_enum_members(left: Left, right: Right | Literal["a"]):
    if left == right:
        reveal_type(left)  # revealed: Literal[Left.SHARED, Left.A]
        assert_type(right, Literal[Right.SHARED, "a"])
```

A `dict[str, Any]` is treated as having dictionary equality, so it cannot match a string-valued enum
member:

```py
def compare_cross_enum_with_dictionary(left: Left | dict[str, Any], right: Right | None):
    if left == right:
        reveal_type(left)  # revealed: Literal[Left.SHARED]
        reveal_type(right)  # revealed: Literal[Right.SHARED]
```

By contrast, `Any` can match any enum member. It must not exclude `None` from the other side:

```py
def compare_optional_enum_against_any(left: Left | None, right: Right | Any):
    if left == right:
        reveal_type(left)  # revealed: Left | None
        reveal_type(right)  # revealed: Literal[Right.SHARED] | Any

def compare_any_against_optional_enum(left: Left | Any, right: Right | None):
    if left == right:
        reveal_type(left)  # revealed: Literal[Left.SHARED] | Any
        reveal_type(right)  # revealed: Right | None
```

If the two sides have no matching values, `==` is always false and `!=` is always true. A shared
`None` makes `==` uncertain:

```py
def compare_disjoint_cross_enum_alternatives(
    left: Literal[Left.A] | None,
    disjoint: Literal[Right.B] | Literal[1],
    overlapping: Literal[Right.B] | None,
):
    reveal_type(left == disjoint)  # revealed: Literal[False]
    reveal_type(left != disjoint)  # revealed: Literal[True]
    reveal_type(left == overlapping)  # revealed: bool
```

When all possible values match, `==` is always true:

```py
def compare_matching_cross_enum_alternatives(
    left: Literal[Left.SHARED] | Literal["shared"],
    right: Literal[Right.SHARED],
):
    reveal_type(left == right)  # revealed: Literal[True]
    reveal_type(left != right)  # revealed: Literal[False]
```

The same comparison-key projection applies when each operand spans several enum classes. This
example represents 18 possible values on each side, which would otherwise require 324 pairwise
comparisons:

```py
from enum import IntEnum
from typing import Literal

class MixedLeft0(IntEnum):
    A = 0
    B = 1
    C = 2
    D = 3
    E = 4
    F = 5
    G = 6
    H = 7
    I = 8

class MixedLeft1(IntEnum):
    A = 9
    B = 10
    C = 11
    D = 12
    E = 13
    F = 14
    G = 15
    H = 16
    I = 17

class MixedRight0(IntEnum):
    A = 0
    B = 1
    C = 2
    D = 3
    E = 4
    F = 5
    G = 6
    H = 7
    I = 8

class MixedRight1(IntEnum):
    A = 18
    B = 19
    C = 20
    D = 21
    E = 22
    F = 23
    G = 24
    H = 25
    I = 26

def compare_mixed_domains(
    left: MixedLeft0 | MixedLeft1,
    right: MixedRight0 | MixedRight1,
):
    if left == right:
        reveal_type(left)  # revealed: MixedLeft0
        reveal_type(right)  # revealed: MixedRight0
```

Treating `str` as having builtin equality, adding `None` or `str` does not prevent matches between
integer-valued enum classes:

```py
def compare_multiple_integer_enums_with_other_values(
    left: MixedLeft0 | MixedLeft1 | None,
    right: MixedRight0 | MixedRight1 | str,
):
    if left == right:
        reveal_type(left)  # revealed: MixedLeft0
        reveal_type(right)  # revealed: MixedRight0
```

Python considers `False` equal to `0`, so a `False` alternative can match an integer-valued enum
member even when the other enum has no matching members:

```py
def compare_false_to_integer_enum(left: MixedLeft1 | Literal[False], right: MixedRight0):
    if left == right:
        reveal_type(left)  # revealed: Literal[False]
        reveal_type(right)  # revealed: Literal[MixedRight0.A]
```

An identity-comparing enum with a custom `_missing_` method remains equivalent to the union of its
declared members:

```py
from enum import Enum
from typing import Literal

class CustomMissingIdentity(Enum):
    A = "a"
    B = "b"

    @classmethod
    def _missing_(cls, value: object) -> "CustomMissingIdentity":
        raise ValueError

class OtherIdentity(Enum):
    C = "c"

def compare_custom_missing_identity(
    left: CustomMissingIdentity | OtherIdentity,
    right: Literal[CustomMissingIdentity.A, CustomMissingIdentity.B],
):
    if left == right:
        reveal_type(left)  # revealed: CustomMissingIdentity
```

A metaclass can inject undeclared members, leaving an identity-comparing enum genuinely open.
Comparing against its declared members can still exclude those undeclared members.

```py
class OpenIdentity(Enum, metaclass=InjectingEnumMeta):
    A = "a"
    B = "b"

def compare_open_identity(
    left: OpenIdentity | OtherIdentity,
    right: Literal[OpenIdentity.A, OpenIdentity.B],
):
    if left == right:
        reveal_type(left)  # revealed: Literal[OpenIdentity.A, OpenIdentity.B]
```

Integer comparison keys normalize booleans in the same way as Python equality:

```py
from enum import Enum, IntEnum

class BooleanKey(int, Enum):
    FALSE = False

class IntegerKey(IntEnum):
    ZERO = 0

reveal_type(BooleanKey.FALSE == IntegerKey.ZERO)  # revealed: Literal[True]

class IntegerAliases(IntEnum):
    ZERO = 0
    FALSE = False

reveal_type(IntegerAliases.ZERO == IntegerAliases.FALSE)  # revealed: Literal[True]
```

Plain enum members from different classes use identity comparison, even when their declared values
are equal. Custom comparison methods remain ambiguous, while scalar enums can compare across enum
classes:

```py
from enum import Enum, StrEnum

class PlainLeft(Enum):
    MEMBER = "shared"

class PlainRight(Enum):
    MEMBER = "shared"

reveal_type(PlainLeft.MEMBER == PlainRight.MEMBER)  # revealed: Literal[False]

def compare_plain(left: PlainLeft, right: PlainRight):
    if left == right:
        reveal_type(left)  # revealed: Never

class CustomLeft(StrEnum):
    MEMBER = "shared"

    def __eq__(self, other: object) -> bool:
        return False

class CustomRight(StrEnum):
    MEMBER = "shared"

reveal_type(CustomLeft.MEMBER == CustomRight.MEMBER)  # revealed: bool

class CustomNeLeft(StrEnum):
    MEMBER = "shared"

    def __ne__(self, other: object) -> bool:
        return False

reveal_type(CustomNeLeft.MEMBER == CustomRight.MEMBER)  # revealed: Literal[True]
reveal_type(CustomNeLeft.MEMBER != CustomRight.MEMBER)  # revealed: bool

class CustomMissingLeft(StrEnum):
    MEMBER = "shared"

    @classmethod
    def _missing_(cls, value: object) -> "CustomMissingLeft":
        raise ValueError

def compare_custom_missing(left: CustomMissingLeft, right: CustomRight):
    if left == right:
        reveal_type(left)  # revealed: CustomMissingLeft
```

A metaclass can add undeclared scalar members, so cross-enum comparison must retain the full open
enum:

```py
class OpenLeft(StrEnum, metaclass=InjectingEnumMeta):
    MEMBER = "shared"

def compare_open(left: OpenLeft, right: CustomRight):
    if left == right:
        reveal_type(left)  # revealed: OpenLeft
```

A custom equality method must still determine the result when the enum is combined with `None`:

```py
def compare_optional_custom(left: CustomLeft | None, right: CustomRight):
    if left == right:
        reveal_type(left)  # revealed: CustomLeft
```

A custom `_missing_` method does not affect comparison narrowing, including when the enum is
combined with `None`:

```py
def compare_optional_custom_missing(left: CustomMissingLeft | None, right: CustomRight):
    if left == right:
        reveal_type(left)  # revealed: CustomMissingLeft
```

Undeclared members of a genuinely open scalar enum must survive cross-enum comparison even when the
enum is combined with `None`:

```py
def compare_optional_open(left: OpenLeft | None, right: CustomRight):
    if left == right:
        reveal_type(left)  # revealed: OpenLeft
```

The same narrowing applies when comparing enum members directly with their inherited integer or
string values. The negative constraint excludes both the builtin literal and every enum member known
to compare equal to it:

```py
from enum import Enum, IntEnum, StrEnum

class IntMember(int, Enum):
    X = 1
    Y = 2

class Integer(IntEnum):
    X = 1
    Y = 2

class String(StrEnum):
    X = "X"
    Y = "Y"

class StrMember(str, Enum):
    X = "X"
    Y = "Y"

def _(value: IntMember | Integer | String | StrMember):
    if value == 1:
        pass
    else:
        reveal_type(value)  # revealed: Literal[IntMember.Y, Integer.Y] | String | StrMember

    if value != 1:
        reveal_type(value)  # revealed: Literal[IntMember.Y, Integer.Y] | String | StrMember

    if value == "X":
        pass
    else:
        reveal_type(value)  # revealed: IntMember | Integer | Literal[String.Y, StrMember.Y]

    if value != "X":
        reveal_type(value)  # revealed: IntMember | Integer | Literal[String.Y, StrMember.Y]

def random() -> bool:
    return False

def loop_back():
    value = IntMember.X if random() else IntMember.Y
    if value != 1:
        while random():
            reveal_type(value)  # revealed: Literal[IntMember.Y, Integer.Y]
            value = Integer.Y
```

A custom `__new__` can replace the value declared in an `IntEnum` class body. We can still narrow
the members of `Foo`, whose runtime values are known, but must preserve all of `Shifted` because its
members' runtime values cannot be determined statically:

```py
from enum import IntEnum

class Foo(IntEnum):
    X = 1
    Y = 2

class Shifted(IntEnum):
    def __new__(cls, value: int) -> "Shifted":
        member = int.__new__(cls, value + 1)
        member._value_ = value + 1
        return member

    A = 1
    B = 2

def _(value: Foo | Shifted):
    if value == Foo.X:
        reveal_type(value)  # revealed: Literal[Foo.X] | Shifted
    else:
        reveal_type(value)  # revealed: Literal[Foo.Y] | Shifted
```

An explicit `_value_` annotation controls the public `.value` type without erasing a concrete
comparison payload:

```py
from enum import IntEnum

class AnnotatedInteger(IntEnum):
    _value_: int
    ONE = 1

reveal_type(AnnotatedInteger.ONE.value)  # revealed: int
reveal_type(AnnotatedInteger.ONE == 1)  # revealed: Literal[True]
```

When a custom constructor transforms the member, however, the annotation does not describe the
scalar payload used by inherited comparison methods:

```py
from enum import IntEnum
from typing import Literal

class AnnotatedShifted(IntEnum):
    _value_: Literal[1]

    def __new__(cls, value: int) -> "AnnotatedShifted":
        member = int.__new__(cls, value + 1)
        member._value_ = 1
        return member

    MEMBER = 1

class Other(IntEnum):
    MEMBER = 1

reveal_type(AnnotatedShifted.MEMBER.value)  # revealed: Literal[1]
reveal_type(AnnotatedShifted.MEMBER == Other.MEMBER)  # revealed: bool

if AnnotatedShifted.MEMBER != Other.MEMBER:
    reveal_type(AnnotatedShifted.MEMBER)  # revealed: AnnotatedShifted

class AnnotatedInitialized(IntEnum):
    _value_: Literal[2]

    def __init__(self, value: int) -> None:
        self._value_ = 2

    MEMBER = 1

reveal_type(AnnotatedInitialized.MEMBER.value)  # revealed: Literal[2]
reveal_type(AnnotatedInitialized.MEMBER == Other.MEMBER)  # revealed: bool
```

A scalar data-type mixin can also transform a declared value before it becomes the enum member's
comparison payload. Such a value is not a safe comparison key:

```py
from enum import Enum, IntEnum

class ShiftedInt(int):
    def __new__(cls, value: int) -> "ShiftedInt":
        return int.__new__(cls, value + 1)

class MixinShifted(ShiftedInt, Enum):
    MEMBER = 1

class Normal(IntEnum):
    MEMBER = 2

reveal_type(MixinShifted.MEMBER == Normal.MEMBER)  # revealed: bool

if MixinShifted.MEMBER == Normal.MEMBER:
    reveal_type(MixinShifted.MEMBER)  # revealed: MixinShifted
```

The return value of `_generate_next_value_` is not necessarily the final value of an `IntEnum`
member. Here, the inherited `int.__new__` converts the generated string `"1"` to the integer `1`.
Because the generated value's exact conversion is not modeled, we cannot use it to decide whether
members of `Generated` and `Other` compare equal:

```py
from enum import IntEnum, auto
from typing import Literal

class Generated(IntEnum):
    # error: [invalid-method-override]
    def _generate_next_value_(name, start, count, last_values) -> Literal["1"]:
        return "1"

    ONE = auto()

class Other(IntEnum):
    ONE = 1

reveal_type(Generated.ONE.value)  # revealed: int
reveal_type(Generated.ONE == Other.ONE)  # revealed: bool

def _(value: Generated | Other):
    if value == Generated.ONE:
        reveal_type(value)  # revealed: Generated | Other
```

An assignment to `__new__`, `__init__`, or other methods can replace the value declared in the class
body. In that case, we cannot compare an enum member with its declared value statically:

```toml
[environment]
python-version = "3.11"
```

```py
from enum import EnumMeta, StrEnum
from typing import Any, Literal

def _(new: Any, init: Any, prepare: Any):
    class OpaqueNew(StrEnum):
        __new__ = new

        MEMBER = "member"

    class OpaqueInit(StrEnum):
        __init__ = init

        MEMBER = "member"

    class OpaqueMeta(EnumMeta):
        __prepare__ = prepare

    class TransformedByMeta(StrEnum, metaclass=OpaqueMeta):
        MEMBER = "member"

    def opaque_new(value: Literal[OpaqueNew.MEMBER] | Literal["member"]):
        if value == "member":
            reveal_type(value)  # revealed: OpaqueNew | Literal["member"]
        else:
            reveal_type(value)  # revealed: OpaqueNew

    def opaque_init(value: Literal[OpaqueInit.MEMBER] | Literal["member"]):
        if value == "member":
            reveal_type(value)  # revealed: OpaqueInit | Literal["member"]
        else:
            reveal_type(value)  # revealed: OpaqueInit

    def transformed_by_metaclass(value: Literal[TransformedByMeta.MEMBER] | Literal["member"]):
        if value == "member":
            reveal_type(value)  # revealed: Literal[TransformedByMeta.MEMBER, "member"]
        else:
            reveal_type(value)  # revealed: Literal[TransformedByMeta.MEMBER]
```

An opaque `_generate_next_value_` affects `auto()` members, but explicit members still have their
declared values:

```py
from enum import StrEnum, auto
from typing import Any, Literal

def _(generate_next_value: Any):
    class OpaqueGenerator(StrEnum):
        _generate_next_value_ = generate_next_value

        AUTOMATIC = auto()
        EXPLICIT = "explicit"

    def opaque_generated_value(
        value: Literal[OpaqueGenerator.AUTOMATIC] | Literal["automatic"],
    ):
        if value == "automatic":
            reveal_type(value)  # revealed: Literal[OpaqueGenerator.AUTOMATIC, "automatic"]
        else:
            reveal_type(value)  # revealed: Literal[OpaqueGenerator.AUTOMATIC]

    def explicit_value(
        value: Literal[OpaqueGenerator.EXPLICIT] | Literal["other"],
    ):
        if value == "explicit":
            reveal_type(value)  # revealed: Literal[OpaqueGenerator.EXPLICIT]
        else:
            reveal_type(value)  # revealed: Literal["other"]
```

This narrowing behavior is only safe if the enum has no custom `__eq__`/`__ne__` method:

```py
from enum import Enum

class AmbiguousEnum(Enum):
    NO = 0
    YES = 1

    def __ne__(self, other) -> bool:
        return True

def _(answer: AmbiguousEnum):
    if answer != AmbiguousEnum.NO:
        reveal_type(answer)  # revealed: AmbiguousEnum
    else:
        reveal_type(answer)  # revealed: AmbiguousEnum
```

Similar if that method is inherited from a base class:

```py
from enum import Enum

class Mixin:
    def __eq__(self, other) -> bool:
        return True

class AmbiguousEnum(Mixin, Enum):
    NO = 0
    YES = 1

def _(answer: AmbiguousEnum):
    if answer == AmbiguousEnum.NO:
        reveal_type(answer)  # revealed: AmbiguousEnum
    else:
        reveal_type(answer)  # revealed: AmbiguousEnum
```

`==` and `!=` must use the semantics of their respective dunder methods. In particular, a custom
`__ne__` method does not affect narrowing based on `__eq__`. Conversely, a custom `__eq__` method
affects narrowing based on both operators because the default `__ne__` delegates to `__eq__`:

```py
from enum import Enum

class IndependentEquality(Enum):
    NO = 0
    YES = 1

    def __ne__(self, other: object) -> bool:
        return True

def _(answer: IndependentEquality):
    if answer == IndependentEquality.NO:
        reveal_type(answer)  # revealed: Literal[IndependentEquality.NO]
    else:
        reveal_type(answer)  # revealed: Literal[IndependentEquality.YES]

    if answer != IndependentEquality.NO:
        reveal_type(answer)  # revealed: IndependentEquality
    else:
        reveal_type(answer)  # revealed: IndependentEquality

class CoupledInequality(Enum):
    NO = 0
    YES = 1

    def __eq__(self, other: object) -> bool:
        return True

def _(answer: CoupledInequality):
    if answer == CoupledInequality.NO:
        reveal_type(answer)  # revealed: CoupledInequality
    else:
        reveal_type(answer)  # revealed: CoupledInequality

    if answer != CoupledInequality.NO:
        reveal_type(answer)  # revealed: CoupledInequality
    else:
        reveal_type(answer)  # revealed: CoupledInequality
```

## Recursive aliases containing enum domains

Comparisons involving recursive enum aliases remain valid. Comparing against a specific enum member
narrows both branches to their remaining members while preserving any `NewType` tag.

```toml
[environment]
python-version = "3.12"
```

```py
from enum import Enum
from typing import NewType

class EnumValue(Enum):
    VALUE = 1
    OTHER = 2

type Recursive = EnumValue | Recursive

def _(left: Recursive, right: EnumValue):
    reveal_type(left == right)  # revealed: bool

BrandedEnumValue = NewType("BrandedEnumValue", EnumValue)
type RecursiveBrand = BrandedEnumValue | RecursiveBrand

def compare_recursive_brand_to_member(left: RecursiveBrand) -> None:
    if left == EnumValue.VALUE:
        reveal_type(left)  # revealed: BrandedEnumValue & Literal[EnumValue.VALUE]
    else:
        reveal_type(left)  # revealed: BrandedEnumValue & Literal[EnumValue.OTHER]

    if left != EnumValue.VALUE:
        reveal_type(left)  # revealed: BrandedEnumValue & Literal[EnumValue.OTHER]
    else:
        reveal_type(left)  # revealed: BrandedEnumValue & Literal[EnumValue.VALUE]
```

A recursive alias with changing type arguments may introduce values outside its original enum
domain. Here, `True` compares equal to the integer-valued enum member, so the `bool` alternative
must remain reachable.

```py
from enum import IntEnum

class Number(IntEnum):
    ONE = 1
    TWO = 2

BrandedNumber = NewType("BrandedNumber", Number)
type Changing[T] = T | Changing[bool]

def compare_changing_specialization(value: Changing[BrandedNumber]) -> None:
    if value == Number.ONE:
        reveal_type(value)  # revealed: (BrandedNumber & Literal[Number.ONE]) | bool
    else:
        reveal_type(value)  # revealed: (BrandedNumber & Literal[Number.TWO]) | bool
```

Mutually recursive aliases can likewise admit values outside their enum domain. Intersecting the
aliases does not remove their shared `bool` alternative.

```py
from ty_extensions import Intersection

type RecursiveWithBool = RecursiveWithBrand | bool
type RecursiveWithBrand = RecursiveWithBool | BrandedNumber

def compare_mutually_recursive_intersection(
    value: Intersection[RecursiveWithBool, RecursiveWithBrand],
) -> None:
    if value == Number.ONE:
        reveal_type(value)  # revealed: bool | BrandedNumber
    else:
        reveal_type(value)  # revealed: bool | BrandedNumber
```

## Recursive aliases containing gradual generic branches

Equality narrowing must terminate when a recursive sequence alias contains a mapping with a gradual
key.

```toml
[environment]
python-version = "3.12"
```

```py
from collections.abc import Mapping, Sequence
from typing import Any

type RecursiveMappingKey = Sequence[RecursiveMappingKey] | Mapping[Any, int]

def narrow_recursive_mapping_key(value: RecursiveMappingKey) -> None:
    assert value == 0
    _ = value
```

A gradual mapping value also must not cause recursive materialization to unfold indefinitely.

```py
type RecursiveMappingValue = Sequence[RecursiveMappingValue] | Mapping[int, Any]

def narrow_recursive_mapping_value(value: RecursiveMappingValue) -> None:
    assert value == 0
    _ = value
```

## Known built-in equality behavior

`bool`, `LiteralString`, `TypedDict`, and final classes that inherit `object.__eq__` have known
built-in equality behavior. Comparing two values with the same known behavior can therefore
eliminate disjoint union elements:

```py
from typing import TypedDict, final
from typing_extensions import LiteralString

class Payload(TypedDict):
    value: int

@final
class A: ...

@final
class B: ...

def narrow_bool(value: bool | None, other: bool):
    if value == other:
        reveal_type(value)  # revealed: bool
    else:
        reveal_type(value)  # revealed: bool | None

    if value != other:
        reveal_type(value)  # revealed: bool | None
    else:
        reveal_type(value)  # revealed: bool

def narrow_literal_string(value: LiteralString | None, other: LiteralString):
    if value == other:
        reveal_type(value)  # revealed: LiteralString
    else:
        reveal_type(value)  # revealed: LiteralString | None

def narrow_typed_dict(value: Payload | None, other: Payload):
    if value == other:
        reveal_type(value)  # revealed: Payload
    else:
        reveal_type(value)  # revealed: Payload | None

def narrow_final_object_equality(value: A | B, other: A):
    if value == other:
        reveal_type(value)  # revealed: A

    if value != other:
        reveal_type(value)  # revealed: A | B
    else:
        reveal_type(value)  # revealed: A
```

Different inherited built-in implementations cannot compare equal:

```py
from typing import final

@final
class FinalObject: ...

@final
class FinalInt(int): ...

def narrow_different_equality_implementations(value: FinalObject | FinalInt, other: FinalObject):
    if value == other:
        reveal_type(value)  # revealed: FinalObject
```

## Sentinels

Sentinels always compare equal to themselves, since they are singletons:

```py
from typing_extensions import Sentinel

MISSING = Sentinel("MISSING")

reveal_type(MISSING == MISSING)  # revealed: Literal[True]
```

## Known typing-object equality behavior

Certain typing APIs are heavily special-cased by ty, which makes it tempting to special case
equality inference for these symbols. This, however, is error-prone: for example, ty currently
infers the same type for `typing_extensions.Literal` as it does for `typing.Literal`, even though
these may not be the same runtime object and may not compare equal. There's also no known use case
for precisely inferring equality comparisons between these objects.

For most special-cased typing APIs, therefore, we simply fallback to the nominal instance that the
typing symbol is known to be an instance of:

```toml
[environment]
python-version = "3.12"
```

```py
from functools import partial
from typing import Literal, NamedTuple
from typing_extensions import NamedTuple as ExtensionsNamedTuple
from ty_extensions._internal import generic_context

type Alias = int

class GenericClass[T]: ...

reveal_type(Alias == Alias)  # revealed: bool
reveal_type(generic_context(GenericClass) == generic_context(GenericClass))  # revealed: bool
reveal_type((int | str) == (int | str))  # revealed: bool
reveal_type(Literal[1] == Literal[1])  # revealed: bool

def target(value: int) -> int:
    return value

# The bound `__call__` methods belong to distinct `partial` objects.
reveal_type(partial(target, 1).__call__ == partial(target, 1).__call__)  # revealed: bool

reveal_type(NamedTuple == ExtensionsNamedTuple)  # revealed: bool
reveal_type(NamedTuple != ExtensionsNamedTuple)  # revealed: bool
```

Repeated construction of `dataclasses.Field` and `typing_extensions.deprecated` produces distinct
objects that will compare unequal, even when their inferred payloads are identical:

```py
from dataclasses import dataclass, field
from typing_extensions import deprecated

@dataclass
class FieldComparisons:
    # False at runtime!
    equals: bool = reveal_type(field(default=1) == field(default=1))  # revealed: bool
    # True at runtime!
    not_equals: bool = reveal_type(field(default=1) != field(default=1))  # revealed: bool

# False at runtime!
reveal_type(deprecated("gone") == deprecated("gone"))  # revealed: bool
# True at runtime!
reveal_type(deprecated("gone") != deprecated("gone"))  # revealed: bool
```

Runtime-significant metadata, spelling, and origin can be erased from the types that ty records for
many of these APIs. Just because ty infers two of these objects as being of the same type does not
therefore mean that they are equal:

```py
import builtins
from collections.abc import Callable as AbcCallable
from typing import Annotated, Callable, List, Type, TypeAlias

A: TypeAlias = "int"
B: TypeAlias = "builtins.int"

# The `Annotated[]` metadata is discarded and ignored by ty, so these are inferred
# as having the same type, but they will compare unequal at runtime
reveal_type(Annotated[int, "a"] == Annotated[int, "b"])  # revealed: bool

reveal_type(A == B)  # revealed: bool
reveal_type(Callable[[int], str] == AbcCallable[[int], str])  # revealed: bool
reveal_type(List[int] == list[int])  # revealed: bool
reveal_type(Type[int] == type[int])  # revealed: bool
```

## Constrained type variables

Equality analysis expands the constraints of a constrained type variable in either operand position.
The resulting constraint is intersected with the type variable, preserving its identity:

```py
from enum import Enum
from typing import Any, Generic, Literal, TypeVar, final
from ty_extensions import Intersection, Top

@final
class ConstraintA: ...

@final
class ConstraintB: ...

T = TypeVar("T", ConstraintA, ConstraintB)

def constrained_left(value: T | None, other: ConstraintA):
    if value != other:
        pass
    else:
        reveal_type(value)  # revealed: T@constrained_left & ConstraintA

def constrained_right(value: ConstraintA | None, other: T):
    if value != other:
        pass
    else:
        reveal_type(value)  # revealed: ConstraintA

class E(Enum):
    A = 1
    B = 2

EnumT = TypeVar("EnumT", Literal[E.A], Literal[E.B])

def correlated_typevar_eq(value: E, other: EnumT) -> EnumT:
    if value == other:
        reveal_type(value)  # revealed: EnumT@correlated_typevar_eq
        return value
    return other

def correlated_typevar_ne(value: E, other: EnumT) -> EnumT:
    if value != other:
        return other
    reveal_type(value)  # revealed: EnumT@correlated_typevar_ne
    return value

LiteralT = TypeVar("LiteralT", Literal[1], Literal[2])

def correlated_literal_typevar_eq(value: Literal[1, 2], other: LiteralT) -> LiteralT:
    if value == other:
        return value
    return other

def correlated_literal_typevar_ne(value: Literal[1, 2], other: LiteralT) -> LiteralT:
    if value != other:
        return other
    return value

MaterializedT = TypeVar("MaterializedT", Literal[1], Intersection[Literal[2], Any])

HolderT = TypeVar("HolderT")

class Holder(Generic[HolderT]):
    def __init__(self, value: HolderT) -> None:
        self.value = value

def correlated_materialized_pattern(left: Top[MaterializedT], right: MaterializedT) -> int:
    holder = Holder(right)
    match left:
        case holder.value:
            return 1
```

## `LiteralString` and string-valued enums

`LiteralString` can be narrowed by comparison with a string-valued enum member that inherits `str`'s
equality implementation:

```toml
[environment]
python-version = "3.11"
```

```py
from enum import StrEnum
from typing_extensions import LiteralString

class Color(StrEnum):
    RED = "red"

def narrow_literal_string_with_enum(value: LiteralString | None):
    if value == Color.RED:
        reveal_type(value)  # revealed: Literal["red"]
    else:
        reveal_type(value)  # revealed: (LiteralString & ~Literal["red"]) | None

    if Color.RED != value:
        reveal_type(value)  # revealed: (LiteralString & ~Literal["red"]) | None
    else:
        reveal_type(value)  # revealed: Literal["red"]
```

## Module literals

Modules compare equal only to the same module object:

```py
import sys
import typing

def narrow_module_literal(flag: bool):
    value = sys if flag else typing

    if value == sys:
        reveal_type(value)  # revealed: <module 'sys'>
    else:
        reveal_type(value)  # revealed: <module 'typing'>

    if value != sys:
        reveal_type(value)  # revealed: <module 'typing'>
    else:
        reveal_type(value)  # revealed: <module 'sys'>
```

## Comparisons with user-defined methods

Arbitrary user-defined comparison methods are not used to narrow their operands. In particular, we
don't inspect the bodies of user-defined `__eq__` or `__ne__` methods to predict their results:

```py
class Left:
    def __eq__(self, other: object) -> bool:
        return True

class Right:
    def __eq__(self, other: object) -> bool:
        return False

def _(value: Right | None):
    if Left() == value:
        reveal_type(value)  # revealed: Right | None
    else:
        reveal_type(value)  # revealed: Right | None
```

Custom comparison methods also remain visible when an `isinstance` check intersects a builtin type
with a mixin. Ignoring the mixin would incorrectly treat the builtin comparison as authoritative:

```py
from typing import Literal

class NeMixin:
    def __ne__(self, other: object) -> bool:
        return False

class EqMixin:
    def __eq__(self, other: object) -> bool:
        return True

def custom_intersection_inequality(value: Literal["x", 1], other: str):
    if isinstance(other, NeMixin):
        if value != other:
            reveal_type(value)  # revealed: Literal["x", 1]
        else:
            reveal_type(value)  # revealed: Literal["x", 1]

def custom_intersection_equality(value: Literal["x", 1], other: str):
    if isinstance(other, EqMixin):
        if value == other:
            reveal_type(value)  # revealed: Literal["x", 1]
        else:
            reveal_type(value)  # revealed: Literal["x", 1]
```

## Narrowing unions and inferring comparisons against broad types

When comparing against a broad type, we assume that its subclasses do not override equality. This
allows union members with incompatible builtin comparison semantics to be removed:

```py
class Foo: ...

class AlwaysEqual:
    def __eq__(self, other: object) -> bool:
        return True

def strings(value: str | None, other: str):
    reveal_type(None == other)  # revealed: Literal[False]
    reveal_type(None != other)  # revealed: Literal[True]

    if value == other:
        reveal_type(value)  # revealed: str
    else:
        reveal_type(value)  # revealed: str | None

    if value != other:
        reveal_type(value)  # revealed: str | None
    else:
        reveal_type(value)  # revealed: str

def classes(value: Foo | None, other: Foo):
    reveal_type(None == other)  # revealed: Literal[False]
    reveal_type(None != other)  # revealed: Literal[True]

    if value == other:
        reveal_type(value)  # revealed: Foo

class Base: ...
class Child(Base): ...

def inherited_classes(value: Base | None, other: Child):
    reveal_type(value == other)  # revealed: bool
    reveal_type(value != other)  # revealed: bool

    if value == other:
        reveal_type(value)  # revealed: Base

    if value != other:
        reveal_type(value)  # revealed: Base | None
    else:
        reveal_type(value)  # revealed: Base

class Left: ...
class Right: ...
class Shared(Left, Right): ...

def overlapping_classes(value: Left | None, other: Right):
    reveal_type(value == other)  # revealed: bool

    if value == other:
        reveal_type(value)  # revealed: Left

def custom_equality(value: AlwaysEqual | None, other: AlwaysEqual):
    if value == other:
        reveal_type(value)  # revealed: AlwaysEqual | None
```

## Narrowing builtin types to literals

Equality with a literal narrows broad `str`, `int`, and `bytes` types to that literal. By default,
integer literals do not introduce the boolean values that compare equal to `0` or `1`:

```py
def narrow_string(value: str):
    if value == "a":
        reveal_type(value)  # revealed: Literal["a"]
    else:
        reveal_type(value)  # revealed: str & ~Literal["a"]

def narrow_reversed_string(value: str):
    if "a" == value:
        reveal_type(value)  # revealed: Literal["a"]

def narrow_integer(value: int):
    if value == 1:
        reveal_type(value)  # revealed: Literal[1]

def narrow_zero(value: int):
    if value == 0:
        reveal_type(value)  # revealed: Literal[0]

def narrow_reversed_integer(value: int):
    if 1 == value:
        reveal_type(value)  # revealed: Literal[1]

def narrow_bytes(value: bytes):
    if value == b"a":
        reveal_type(value)  # revealed: Literal[b"a"]

def narrow_mixed_builtins(value: str | int | bytes):
    if value == "a":
        reveal_type(value)  # revealed: Literal["a"]

def narrow_inequality_else(value: str):
    if value != "a":
        reveal_type(value)  # revealed: str & ~Literal["a"]
    else:
        reveal_type(value)  # revealed: Literal["a"]
```

The narrowing only treats the broad builtin types optimistically. Explicit subclass and custom
comparison arms are preserved:

```py
class StringSubclass(str): ...

class AlwaysEqual:
    def __eq__(self, other: object) -> bool:
        return True

def preserve_subclass(value: StringSubclass):
    if value == "a":
        reveal_type(value)  # revealed: StringSubclass

def preserve_custom_comparison(value: str | AlwaysEqual):
    if value == "a":
        reveal_type(value)  # revealed: Literal["a"] | AlwaysEqual
```

## String-literal origin and exclusions

A string without literal origin can equal a string literal without acquiring the literal's origin.
The successful branch remains reachable and preserves the original exclusion.

```py
from typing import Literal
from typing_extensions import LiteralString
from ty_extensions import Intersection, Not

def without_literal_origin(value: Intersection[str, Not[LiteralString]]) -> None:
    if value == "hello":
        reveal_type(value)  # revealed: str & ~LiteralString
        value.definitely_missing_attribute  # error: [unresolved-attribute]

    if "hello" == value:
        reveal_type(value)  # revealed: str & ~LiteralString

    if value != "hello":
        reveal_type(value)  # revealed: str & ~LiteralString
    else:
        reveal_type(value)  # revealed: str & ~LiteralString
```

Excluding a particular string literal also leaves its runtime value possible when literal origin is
not known. A different literal can still narrow the string normally.

```py
def without_literal_value(value: Intersection[str, Not[Literal["hello"]]]) -> None:
    if value == "hello":
        reveal_type(value)  # revealed: str & ~Literal["hello"]

    if value == "goodbye":
        reveal_type(value)  # revealed: Literal["goodbye"]
```

Optional alternatives that cannot compare equal are still removed without discarding the possible
string value.

```py
def optional_without_literal_origin(value: Intersection[str, Not[LiteralString]] | None) -> None:
    if value == "hello":
        reveal_type(value)  # revealed: str & ~LiteralString
```

Once literal origin is known, excluding a string literal really does exclude its runtime value.

```py
def trusted_value_is_excluded(value: Intersection[LiteralString, Not[Literal["hello"]]]) -> None:
    if value == "hello":
        reveal_type(value)  # revealed: Never
```

## `x != y` where `y` is of literal type

```py
from typing import Literal

def _(x: Literal[1, 2]):
    if x != 1:
        reveal_type(x)  # revealed: Literal[2]
```

## `x != y` where `y` is a class literal

```py
def _(flag: bool):
    class A: ...
    class B: ...
    C = A if flag else B

    if C != A:
        reveal_type(C)  # revealed: <class 'B'>
    else:
        reveal_type(C)  # revealed: <class 'A'>
```

## `x != y` where `y` has multiple literal options

```py
from typing import Literal

def _(x: Literal[1, 2], y: Literal[2, 3]):
    if x != y:
        reveal_type(x)  # revealed: Literal[1, 2]
    else:
        reveal_type(x)  # revealed: Literal[2]
```

## `==` with PEP 695 alias to a union of literals

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Literal

type Y = Literal[2, 3]

def _(x: Literal[1, 2], y: Y):
    if x == y:
        reveal_type(x)  # revealed: Literal[2]
    else:
        reveal_type(x)  # revealed: Literal[1, 2]
```

## `!=` for broad types

A broad right-hand type cannot narrow `x`:

```py
def _(x: int | None, y: int):
    if x != y:
        reveal_type(x)  # revealed: int | None
```

## Mix of literal and broad types

```py
from typing import Literal

def _(x: Literal[1, 2], y: int):
    if x != y:
        reveal_type(x)  # revealed: Literal[1, 2]
    else:
        reveal_type(x)  # revealed: Literal[1, 2]
```

## `==` / `!=` with two narrowable operands

Both operands should be narrowed when both are narrowable expressions.

```py
from typing import Literal

def _(x: Literal[1], y: Literal[1, 2]):
    if x == y:
        reveal_type(y)  # revealed: Literal[1]
    if y == x:
        reveal_type(y)  # revealed: Literal[1]
    if x != y:
        reveal_type(y)  # revealed: Literal[2]
    if y != x:
        reveal_type(y)  # revealed: Literal[2]
```

## Assignment expressions

```py
from typing import Literal

def f() -> Literal[1, 2, 3]:
    return 1

if (x := f()) != 1:
    reveal_type(x)  # revealed: Literal[2, 3]
else:
    reveal_type(x)  # revealed: Literal[1]

value = f()
if result := (value == 1):
    reveal_type(value)  # revealed: Literal[1]
    reveal_type(result)  # revealed: Literal[True]
else:
    reveal_type(value)  # revealed: Literal[2, 3]
    reveal_type(result)  # revealed: Literal[False]

class A:
    tag: Literal["a"]

class B:
    tag: Literal["b"]

def overwritten_tagged_union(value: A | B | bool):
    if isinstance(value, (A, B)):
        if value := (value.tag == "a"):
            reveal_type(value)  # revealed: Literal[True]
        else:
            reveal_type(value)  # revealed: Literal[False]

def overwritten_tagged_union_attribute(value: A | B | str):
    if isinstance(value, (A, B)):
        if (value := value.tag) == "a":
            reveal_type(value)  # revealed: Literal["a"]
        else:
            reveal_type(value)  # revealed: Literal["b"]

def tagged_union_rebound_by_comparator(value: A | B | str):
    if isinstance(value, (A, B)):
        if value.tag == (value := "a"):
            reveal_type(value)  # revealed: Literal["a"]
        else:
            reveal_type(value)  # revealed: Literal["a"]

def tagged_union_with_unrelated_assignment(value: A | B):
    if value.tag == (tag := "a"):
        reveal_type(value)  # revealed: A
        reveal_type(tag)  # revealed: Literal["a"]
    else:
        reveal_type(value)  # revealed: B
        reveal_type(tag)  # revealed: Literal["a"]
```

## Union with `Any`

```py
import sys
from enum import Enum, IntEnum
from typing import Any, Literal, TypeAlias, TypeVar

from ty_extensions._internal import Unknown
from typing_extensions import assert_never, assert_type

T = TypeVar("T", bound=object)
U = TypeVar("U")
EQUAL_VALUES = TypeVar("EQUAL_VALUES", Literal[0], Literal[False])
RUNTIME_TYPE_VAR = TypeVar("RUNTIME_TYPE_VAR")

class Color(Enum):
    RED = 1
    BLUE = 2

class OtherColor(Enum):
    RED = 1

class NonReflexive(Enum):
    VALUE = 1

    def __eq__(self, other: object) -> Literal[False]:
        return False

    def __ne__(self, other: object) -> Literal[True]:
        return True

class Marker: ...

class SingleIntEnum(IntEnum):
    VALUE = 1

def _(x: Any | None, y: Any | None):
    if x != 1:
        reveal_type(x)  # revealed: (Any & ~Literal[1] & ~Literal[True]) | None
    if y == 1:
        reveal_type(y)  # revealed: Any & ~None

def _(x: Any):
    if x == True:
        reveal_type(x)  # revealed: Any
    else:
        reveal_type(x)  # revealed: Any & ~Literal[True] & ~Literal[1]

    if x != True:
        reveal_type(x)  # revealed: Any & ~Literal[True] & ~Literal[1]
    else:
        reveal_type(x)  # revealed: Any

def _(x: Literal["foo", "bar"] | Any):
    if x != "bar":
        reveal_type(x)  # revealed: Literal["foo"] | (Any & ~Literal["bar"])
    else:
        reveal_type(x)  # revealed: Literal["bar"] | (Any & ~Literal["foo"])

def _(x: Any):
    if x != Color.RED:
        reveal_type(x)  # revealed: Any & ~Literal[Color.RED]

    if x != NonReflexive.VALUE:
        reveal_type(x)  # revealed: Any

    if x != Marker:
        reveal_type(x)  # revealed: Any & ~<class 'Marker'>

def _(x: T):
    if x != Color.RED:
        reveal_type(x)  # revealed: T@_ & ~Literal[Color.RED]

def _(x: U | Literal[Color.RED]):
    if x == Color.RED:
        return
    reveal_type(x)  # revealed: U@_ & ~Literal[Color.RED]

def _(x: Any, y: EQUAL_VALUES):
    if x != y:
        reveal_type(x)  # revealed: Any & ~EQUAL_VALUES@_

def _(x: Any, y: T | str):
    if x != y:
        reveal_type(x)  # revealed: Any

def _(x: Any, y: Any | str):
    if x != y:
        reveal_type(x)  # revealed: Any

def _(x: Any):
    if x != list[Any]:
        reveal_type(x)  # revealed: Any

def _(x: Any, y: SingleIntEnum):
    if x == y:
        pass
    else:
        reveal_type(x)  # revealed: Any & ~Literal[SingleIntEnum.VALUE]

def _(x: Any):
    if x == sys.version_info:
        pass
    else:
        reveal_type(x)  # revealed: Any & ~_version_info

    if x == RUNTIME_TYPE_VAR:
        pass
    else:
        reveal_type(x)  # revealed: Any
```

`Any` must stay `Any` when compared with an enum, on either side of the comparison:

```py
def enum_against_any(value: Color, other: Any):
    if value != other:
        reveal_type(other)  # revealed: Any

def any_against_enum(value: Any, other: Color):
    if value != other:
        reveal_type(value)  # revealed: Any
```

`Any` must also stay `Any` when the enum can be `None`:

```py
def optional_enum_against_any(value: Color | None, other: Any):
    if value != other:
        reveal_type(other)  # revealed: Any

def any_against_optional_enum(value: Any, other: Color | None):
    if value != other:
        reveal_type(value)  # revealed: Any
```

`Any` must also stay `Any` when compared with `bool | None`:

```py
def optional_bool_against_any(value: bool | None, other: Any):
    if value != other:
        reveal_type(other)  # revealed: Any
```

Comparing `Color | Any` with `Color | None` must keep both `Color` and `Any`:

```py
def gradual_enum_union(value: Color | Any, other: Color | None):
    if value != other:
        reveal_type(value)  # revealed: Color | Any
```

`Color | Any` must stay unchanged when the other value can be an enum member or `None`. This applies
to `!=` and the false branch of `==`:

```py
def any_union_against_optional_enum_member(value: Color | Any, other: Literal[Color.RED] | None):
    if value != other:
        reveal_type(value)  # revealed: Color | Any
        assert_type(value, Color | Any)

def any_union_against_optional_enum_member_equality_else(value: Color | Any, other: Literal[Color.RED] | None):
    if value == other:
        return
    reveal_type(value)  # revealed: Color | Any
```

An alias for `Any` must preserve the same result:

```py
AnyAlias: TypeAlias = Any

def any_alias_union_against_optional_enum_member(value: Color | AnyAlias, other: Literal[Color.RED] | None):
    if value != other:
        reveal_type(value)  # revealed: Color | Any
```

The same comparisons also preserve `Unknown`:

```py
def unknown_union_against_optional_enum_member(value: Color | Unknown, other: Literal[Color.RED] | None):
    if value != other:
        reveal_type(value)  # revealed: Color | Unknown
        assert_type(value, Color | Unknown)

def unknown_union_against_optional_enum_member_equality_else(value: Color | Unknown, other: Literal[Color.RED] | None):
    if value == other:
        return
    reveal_type(value)  # revealed: Color | Unknown
```

When an enum check and a comparison are combined with `and`, either condition can be false. The
original union must therefore be preserved:

```py
def any_union_after_enum_check(value: Color | Any, other: Color | Any):
    if isinstance(value, Color) and value == other:
        return
    reveal_type(value)  # revealed: Color | Any
    assert_type(value, Color | Any)

def unknown_union_after_enum_check(value: Color | Unknown, other: Color | Unknown):
    if isinstance(value, Color) and value == other:
        return
    reveal_type(value)  # revealed: Color | Unknown
    assert_type(value, Color | Unknown)
```

The second comparison can fail even when the first one matches, so both possible types must remain:

```py
def any_union_after_failed_comparisons(value: Color | Any, other: OtherColor | None):
    if value == Color.RED and value == other:
        return
    reveal_type(value)  # revealed: Color | Any
    assert_type(value, Color | Any)

def unknown_union_after_failed_comparisons(value: Color | Unknown, other: OtherColor | None):
    if value == Color.RED and value == other:
        return
    reveal_type(value)  # revealed: Color | Unknown
    assert_type(value, Color | Unknown)
```

These `Enum` classes compare by identity, so their members are not equal even when their underlying
values match. Comparing with `OtherColor.RED` must therefore exclude every `Color` member:

```py
def any_comparison_with_other_enum(value: Color | OtherColor | Any):
    if value == OtherColor.RED:
        reveal_type(value)  # revealed: OtherColor | (Any & ~Color)
        if isinstance(value, Color):
            assert_never(value)

def unknown_comparison_with_other_enum(value: Color | OtherColor | Unknown):
    if value == OtherColor.RED:
        reveal_type(value)  # revealed: OtherColor | (Unknown & ~Color)
        if isinstance(value, Color):
            assert_never(value)
```

`Color | Any` must also stay unchanged after either `==` or `!=`:

```py
def gradual_enum_union_against_enum(value: Color | Any, other: Color):
    if value == other:
        reveal_type(value)  # revealed: Color | Any

def gradual_enum_union_inequality(value: Color | Any, other: Color):
    if value != other:
        reveal_type(value)  # revealed: Color | Any
```

## Unions of gradual string literals

Comparing a union of string literals intersected with `Any` keeps the matching alternative for
equality and removes it for inequality:

```py
from typing import Any, Literal
from ty_extensions import Intersection

def equality(value: Intersection[Any, Literal["a"]] | Intersection[Any, Literal["b"]]):
    if value == "a":
        reveal_type(value)  # revealed: Any & Literal["a"]
    else:
        reveal_type(value)  # revealed: Any & Literal["b"]

    if value != "a":
        reveal_type(value)  # revealed: Any & Literal["b"]
    else:
        reveal_type(value)  # revealed: Any & Literal["a"]
```

Larger unions must narrow without expanding the complement of every rejected alternative, which
would make memory use grow exponentially:

```py
def larger_union(
    value: (
        Intersection[Any, Literal["a"]]
        | Intersection[Any, Literal["b"]]
        | Intersection[Any, Literal["c"]]
        | Intersection[Any, Literal["d"]]
        | Intersection[Any, Literal["e"]]
        | Intersection[Any, Literal["f"]]
        | Intersection[Any, Literal["g"]]
        | Intersection[Any, Literal["h"]]
        | Intersection[Any, Literal["i"]]
        | Intersection[Any, Literal["j"]]
        | Intersection[Any, Literal["k"]]
        | Intersection[Any, Literal["l"]]
        | Intersection[Any, Literal["m"]]
        | Intersection[Any, Literal["n"]]
        | Intersection[Any, Literal["o"]]
        | Intersection[Any, Literal["p"]]
        | Intersection[Any, Literal["q"]]
        | Intersection[Any, Literal["r"]]
        | Intersection[Any, Literal["s"]]
        | Intersection[Any, Literal["t"]]
    ),
):
    if value == "a":
        reveal_type(value)  # revealed: Any & Literal["a"]
```

## Booleans and integers

```py
from typing import Literal

def _(b: bool, i: Literal[1, 2]):
    if b == 1:
        reveal_type(b)  # revealed: Literal[True]
    else:
        reveal_type(b)  # revealed: Literal[False]

    if b == 6:
        reveal_type(b)  # revealed: Never
    else:
        reveal_type(b)  # revealed: bool

    if b == 0:
        reveal_type(b)  # revealed: Literal[False]
    else:
        reveal_type(b)  # revealed: Literal[True]

    if i == True:
        reveal_type(i)  # revealed: Literal[1]
    else:
        reveal_type(i)  # revealed: Literal[2]
```

## Integers and booleans with non-strict equality semantics

With non-strict equality semantics, broad integers narrow to integer literals, while boolean
literals that compare equal remain in explicitly annotated literal unions.

```toml
[analysis]
strict-equality-semantics = false
```

```py
from typing import Literal

reveal_type(1 == True)  # revealed: Literal[True]

def f(x: int, y: Literal[1, True, 2]):
    if x == 1:
        reveal_type(x)  # revealed: Literal[1]

    if y == 1:
        reveal_type(y)  # revealed: Literal[1, True]

    if x in [1, 2]:
        reveal_type(x)  # revealed: Literal[1, 2]

    if y in [1, True]:
        reveal_type(y)  # revealed: Literal[1, True]
```

## Integers and booleans with strict equality semantics

With strict equality semantics, broad integers are preserved, while explicitly annotated literal
unions still narrow to the integer and boolean literals that compare equal.

```toml
[analysis]
strict-equality-semantics = true
```

```py
from typing import Literal

reveal_type(1 == True)  # revealed: Literal[True]

def f(x: int, y: Literal[1, True, 2]):
    if x == 1:
        reveal_type(x)  # revealed: int

    if y == 1:
        reveal_type(y)  # revealed: Literal[1, True]

    if x in [1, 2]:
        reveal_type(x)  # revealed: int

    if y in [1, True]:
        reveal_type(y)  # revealed: Literal[1, True]
```

## Final subclasses of scalar builtins

Final subclasses can inherit the equality behavior of `int`, `str`, or `bytes`. Instances of these
subclasses can compare equal to builtin literals even though the subclass and literal types are
disjoint, so equality does not narrow the subclass to the literal type.

```py
from typing import final

@final
class FinalInt(int): ...

@final
class FinalStr(str): ...

@final
class FinalBytes(bytes): ...

def _(value: FinalInt):
    if value == 1:
        reveal_type(value)  # revealed: FinalInt
    else:
        reveal_type(value)  # revealed: FinalInt

    if 1 == value:
        reveal_type(value)  # revealed: FinalInt

    if value != 1:
        reveal_type(value)  # revealed: FinalInt
    else:
        reveal_type(value)  # revealed: FinalInt

def _(value: FinalStr):
    if value == "value":
        reveal_type(value)  # revealed: FinalStr
    else:
        reveal_type(value)  # revealed: FinalStr

def _(value: FinalBytes):
    if value == b"value":
        reveal_type(value)  # revealed: FinalBytes
    else:
        reveal_type(value)  # revealed: FinalBytes
```

## Narrowing `LiteralString` in union

```py
from typing_extensions import Literal, LiteralString, Any

def _(s: LiteralString | None, t: LiteralString | Any):
    if s == "foo":
        reveal_type(s)  # revealed: Literal["foo"]
    elif s == "bar":
        reveal_type(s)  # revealed: Literal["bar"]
    else:
        reveal_type(s)  # revealed: (LiteralString & ~Literal["foo"] & ~Literal["bar"]) | None

    if s == 1:
        reveal_type(s)  # revealed: Never

    if t == "foo":
        reveal_type(t)  # revealed: Literal["foo"] | Any
```

## Narrowing with tuple types

We assume that tuple subclasses don't override `tuple.__eq__`, which only returns True for other
tuples. So they are excluded from the narrowed type when comparing to non-tuple values.

```py
from typing import Literal, cast

def _(x: Literal["a", "b"] | tuple[int, int]):
    if x == "a":
        # tuple type is excluded because it's disjoint from the string literal
        reveal_type(x)  # revealed: Literal["a"]
    else:
        # tuple type remains in the else branch
        reveal_type(x)  # revealed: Literal["b"] | tuple[int, int]

class OpenTupleSubclass(tuple[int, int]): ...

def _(x: Literal["a", "b"] | OpenTupleSubclass):
    if x == "a":
        reveal_type(x)  # revealed: Literal["a"]
    else:
        reveal_type(x)  # revealed: Literal["b"] | OpenTupleSubclass

def inequality_else(value: str | tuple[str | None, str | None, str] | None) -> None:
    if value == "files":
        pass
    elif value != "response":
        return

    reveal_type(value)  # revealed: Literal["files", "response"]
    cast(Literal["files", "response"], value)  # error: [redundant-cast]
```

Fixed-length tuples compare corresponding elements using identity before equality, so distinct
inferred element types can still make the result definite. Different lengths cannot compare equal:

```py
from enum import Enum
from typing import Final, Literal, NewType

class TupleValues:
    TRUE: Final = (True,)
    LONGER: Final = (True, 0)

def equivalent_tuple_pattern(value: tuple[Literal[1]]) -> int:
    match value:
        case TupleValues.TRUE:
            return 1

def different_length_tuple_pattern(value: tuple[Literal[1]]) -> None:
    match value:
        case TupleValues.LONGER:
            reveal_type(value)  # revealed: Never

class NeverEqualTupleElement(Enum):
    A = 1
    B = 2

    def __eq__(self, other: object) -> Literal[False]:
        return False

reveal_type((NeverEqualTupleElement.A,) == (NeverEqualTupleElement.A,))  # revealed: Literal[True]
reveal_type((NeverEqualTupleElement.A,) != (NeverEqualTupleElement.A,))  # revealed: Literal[False]

def tuple_with_non_reflexive_elements(left: NeverEqualTupleElement, right: NeverEqualTupleElement) -> None:
    reveal_type((left,) == (right,))  # revealed: bool
    reveal_type((left,) != (right,))  # revealed: bool

LeftElement = NewType("LeftElement", NeverEqualTupleElement)
RightElement = NewType("RightElement", NeverEqualTupleElement)

def tuple_with_erased_element_identity(value: NeverEqualTupleElement) -> None:
    reveal_type((LeftElement(value),) == (RightElement(value),))  # revealed: bool
    reveal_type((LeftElement(value),) != (RightElement(value),))  # revealed: bool
```

## Narrowing with NewTypes

A `NewType` constructor returns its argument unchanged at runtime. A `WrappedIdentityEnum` value can
therefore be either `IdentityEnum.A` or `IdentityEnum.B`, so comparing it with `IdentityEnum.A` has
an unknown result:

```py
from enum import Enum
from typing import NewType

class IdentityEnum(Enum):
    A = 1
    B = 2

WrappedIdentityEnum = NewType("WrappedIdentityEnum", IdentityEnum)

def literal_with_erased_identity(value: WrappedIdentityEnum) -> None:
    reveal_type(IdentityEnum.A == value)  # revealed: bool
    reveal_type(IdentityEnum.A != value)  # revealed: bool
```

When a `WrappedIdentityEnum` value is `IdentityEnum.B`, equality narrows another `IdentityEnum`
value to the same member. The first value keeps its `WrappedIdentityEnum` type, and both operands
can be passed to a function accepting `Literal[IdentityEnum.B]`.

```py
from typing import Literal, TypeAlias
from ty_extensions import Intersection

def accepts_b(value: Literal[IdentityEnum.B]) -> None: ...
def compare_branded_member(
    branded: Intersection[WrappedIdentityEnum, Literal[IdentityEnum.B]],
    other: IdentityEnum,
) -> None:
    if branded == other:
        reveal_type(branded)  # revealed: WrappedIdentityEnum & Literal[IdentityEnum.B]
        reveal_type(other)  # revealed: Literal[IdentityEnum.B]
        accepts_b(branded)
        accepts_b(other)
    else:
        reveal_type(other)  # revealed: Literal[IdentityEnum.A]

NestedIdentityEnum = NewType("NestedIdentityEnum", WrappedIdentityEnum)
NestedAlias: TypeAlias = NestedIdentityEnum

def compare_nested_brand(value: NestedAlias, other: Literal[IdentityEnum.A]) -> None:
    if value == other:
        reveal_type(value)  # revealed: NestedIdentityEnum & Literal[IdentityEnum.A]
    else:
        reveal_type(value)  # revealed: NestedIdentityEnum & Literal[IdentityEnum.B]
```

`NewType` does not change how an `IntEnum` compares: values from different `IntEnum` classes still
compare by their integer values. A custom enum `__eq__` method likewise still determines the result
after its value is passed through a `NewType` constructor.

```py
from enum import IntEnum

class FirstNumber(IntEnum):
    ONE = 1
    TWO = 2

class SecondNumber(IntEnum):
    ONE = 1
    THREE = 3

BrandedFirstNumber = NewType("BrandedFirstNumber", FirstNumber)
BrandedSecondNumber = NewType("BrandedSecondNumber", SecondNumber)

def compare_branded_int_enums(left: BrandedFirstNumber, right: BrandedSecondNumber) -> None:
    if left == right:
        reveal_type(left)  # revealed: BrandedFirstNumber & Literal[FirstNumber.ONE]
        reveal_type(right)  # revealed: BrandedSecondNumber & Literal[SecondNumber.ONE]

class NeverEqualEnum(Enum):
    A = 1
    B = 2

    def __eq__(self, other: object) -> Literal[False]:
        return False

BrandedNeverEqual = NewType("BrandedNeverEqual", NeverEqualEnum)

def branded_custom_equality(value: BrandedNeverEqual, other: NeverEqualEnum) -> None:
    reveal_type(value == other)  # revealed: Literal[False]
    reveal_type(value != other)  # revealed: bool
```

## Narrowing with enums that have custom `__eq__` methods

Custom enum comparison methods with definite return types determine equality and inequality
independently:

```py
from enum import Enum
from typing import Any, Literal

class AlwaysEqualEnum(Enum):
    A = 1
    B = 2

    def __eq__(self, other: object) -> Literal[True]:
        return True

class NeverUnequalEnum(Enum):
    A = 1
    B = 2

    def __ne__(self, other: object) -> Literal[False]:
        return False

reveal_type(AlwaysEqualEnum.A == AlwaysEqualEnum.B)  # revealed: Literal[True]
reveal_type(NeverUnequalEnum.A != NeverUnequalEnum.B)  # revealed: Literal[False]

def tuple_with_custom_equality(left: AlwaysEqualEnum, right: AlwaysEqualEnum) -> None:
    reveal_type((left,) == (right,))  # revealed: Literal[True]
    reveal_type((left,) != (right,))  # revealed: Literal[False]

def never_unequal_narrowing(x: Any, value: Literal[NeverUnequalEnum.A]) -> None:
    if x != value:
        reveal_type(x)  # revealed: Any & ~Literal[NeverUnequalEnum.A]
```

## Narrowing tagged unions by attribute

```py
from typing import Literal, Protocol

from ty_extensions import Intersection

class BaseA:
    tag: Literal["a"]

class A(BaseA):
    field_a: int

class B:
    tag: Literal["b"]
    field_b: str

class Marker(Protocol):
    marked: bool

class TaggedA(Protocol):
    field_a: int

    @property
    def tag(self) -> Literal["a"]: ...

class TaggedB(Protocol):
    field_b: str

    @property
    def tag(self) -> Literal["b"]: ...

class Container:
    value: A | B | None

def _(x: A | B):
    if x.tag == "a":
        reveal_type(x)  # revealed: A
        reveal_type(x.field_a)  # revealed: int
    else:
        reveal_type(x)  # revealed: B
        reveal_type(x.field_b)  # revealed: str

    if "b" == x.tag:
        reveal_type(x)  # revealed: B
    else:
        reveal_type(x)  # revealed: A

    if x.tag != "a":
        reveal_type(x)  # revealed: B
    else:
        reveal_type(x)  # revealed: A

def truthiness_guard(value: A | B | None):
    if not value:
        return

    reveal_type(value)  # revealed: (A & ~AlwaysFalsy) | (B & ~AlwaysFalsy)

    if value.tag == "a":
        reveal_type(value)  # revealed: A & ~AlwaysFalsy
        reveal_type(value.field_a)  # revealed: int
    else:
        reveal_type(value)  # revealed: B & ~AlwaysFalsy
        reveal_type(value.field_b)  # revealed: str

def nested_attribute_after_truthiness_guard(container: Container):
    if not container.value:
        return

    if container.value.tag == "a":
        reveal_type(container.value)  # revealed: A & ~AlwaysFalsy
        reveal_type(container.value.field_a)  # revealed: int
    else:
        reveal_type(container.value)  # revealed: B & ~AlwaysFalsy
        reveal_type(container.value.field_b)  # revealed: str

def positive_intersection(value: Intersection[A, Marker] | Intersection[B, Marker]):
    if value.tag == "a":
        reveal_type(value)  # revealed: A & Marker
    else:
        reveal_type(value)  # revealed: B & Marker

def protocol_union(value: TaggedA | TaggedB):
    if value.tag == "a":
        reveal_type(value)  # revealed: TaggedA
        reveal_type(value.field_a)  # revealed: int
    else:
        reveal_type(value)  # revealed: TaggedB
        reveal_type(value.field_b)  # revealed: str
```

Enum literals are also supported as attribute tags:

```py
from enum import Enum
from typing import Literal

class Tag(Enum):
    A = 1
    B = 2

class A:
    tag: Literal[Tag.A]

class B:
    tag: Literal[Tag.B]

def _(x: A | B):
    if x.tag == Tag.A:
        reveal_type(x)  # revealed: A
    else:
        reveal_type(x)  # revealed: B
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
    if x.tag == "a":
        reveal_type(x)  # revealed: A | B
    else:
        reveal_type(x)  # revealed: B | C
```

This also works for `NamedTuple` classes:

```py
from typing import Literal, NamedTuple

class A(NamedTuple):
    tag: Literal["a"]
    field_a: int

class B(NamedTuple):
    tag: Literal["b"]
    field_b: str

def _(x: A | B):
    if x[0] == "a":
        reveal_type(x)  # revealed: A
    else:
        reveal_type(x)  # revealed: B

    if x.tag == "a":
        reveal_type(x)  # revealed: A
    else:
        reveal_type(x)  # revealed: B
```

## Enabling strict equality narrowing

Enabling `strict-equality-semantics` accounts for builtin subclasses that override `__eq__` or
compare equal to a literal without belonging to its `Literal` type. It preserves broad builtin types
and union alternatives that could compare equal, including tuples. Literal unions and enum members
are still narrowed when it is safe.

```toml
[environment]
python-version = "3.11"

[analysis]
strict-equality-semantics = true
```

```py
from enum import IntEnum, StrEnum
from typing import Any, Literal, LiteralString
from ty_extensions import Intersection, Not

def broad(value: str):
    if value == "a":
        reveal_type(value)  # revealed: str
    else:
        reveal_type(value)  # revealed: str & ~Literal["a"]

def broad_integer(value: int):
    if value == 1:
        reveal_type(value)  # revealed: int

def inequality(value: str):
    if value != "a":
        reveal_type(value)  # revealed: str & ~Literal["a"]
    else:
        reveal_type(value)  # revealed: str

def without_literal_origin(value: Intersection[str, Not[LiteralString]]):
    if value == "a":
        reveal_type(value)  # revealed: str & ~LiteralString

def trusted_value_is_excluded(value: Intersection[LiteralString, Not[Literal["a"]]]):
    reveal_type(value == "a")  # revealed: Literal[False]

def literal(value: Literal["a", "b"]):
    if value == "a":
        reveal_type(value)  # revealed: Literal["a"]

class Left(StrEnum):
    A = "a"
    SHARED = "shared"

class Right(StrEnum):
    SHARED = "shared"
    B = "b"

def compare_enum_with_integer(left: Left | int | None, right: Left):
    if left == right:
        reveal_type(left)  # revealed: Left | int

def compare_cross_enums_with_integer(left: Left | None, right: Right | int):
    if left == right:
        reveal_type(left)  # revealed: Left | None
        reveal_type(right)  # revealed: Literal[Right.SHARED] | int

    if left != right:
        reveal_type(left)  # revealed: Left | None
        reveal_type(right)  # revealed: Right | int
    else:
        reveal_type(left)  # revealed: Left | None
        reveal_type(right)  # revealed: Literal[Right.SHARED] | int

def compare_cross_enum_with_dictionary(left: Left | dict[str, Any], right: Right | None):
    if left == right:
        reveal_type(left)  # revealed: Literal[Left.SHARED] | dict[str, Any]
        reveal_type(right)  # revealed: Right | None

def compare_both_optional_cross_enums(left: Left | None, right: Right | None):
    if left == right:
        reveal_type(left)  # revealed: Literal[Left.SHARED] | None
        reveal_type(right)  # revealed: Literal[Right.SHARED] | None

class MixedLeft0(IntEnum):
    ZERO = 0
    ONE = 1

class MixedLeft1(IntEnum):
    TWO = 2
    THREE = 3

class MixedRight0(IntEnum):
    ZERO = 0
    ONE = 1

class MixedRight1(IntEnum):
    FOUR = 4
    FIVE = 5

def compare_multiple_integer_enums_with_other_values(
    left: MixedLeft0 | MixedLeft1 | None,
    right: MixedRight0 | MixedRight1 | str,
):
    if left == right:
        reveal_type(left)  # revealed: MixedLeft0 | MixedLeft1 | None
        reveal_type(right)  # revealed: MixedRight0 | str

class Foo: ...

def union(value: Foo | None, other: Foo):
    reveal_type(None == other)  # revealed: bool
    reveal_type(None != other)  # revealed: bool

    if value == other:
        reveal_type(value)  # revealed: Foo | None

class EqualTuple(tuple[int, ...]):
    def __eq__(self, other: object) -> bool:
        return True

def tuple_union(value: Foo | None, other: tuple[int, ...]):
    reveal_type(None == other)  # revealed: bool
    reveal_type(None != other)  # revealed: bool

    if value == other:
        reveal_type(value)  # revealed: Foo | None

    if value != other:
        reveal_type(value)  # revealed: Foo | None
    else:
        reveal_type(value)  # revealed: Foo | None
```

## The strict literal narrowing alias

The `strict-literal-narrowing` option remains an alias for `strict-equality-semantics`.

```toml
[analysis]
strict-literal-narrowing = true
```

```py
class Foo: ...

def union(value: Foo | None, other: Foo):
    if value == other:
        reveal_type(value)  # revealed: Foo | None

def literal(value: str):
    if value == "a":
        reveal_type(value)  # revealed: str
```
