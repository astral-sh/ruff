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

def compare_optional_left(left: Choice | None, right: Choice):
    if left == right:
        reveal_type(left)  # revealed: Choice
        reveal_type(right)  # revealed: Choice
    else:
        reveal_type(left)  # revealed: Choice | None
        reveal_type(right)  # revealed: Choice

def compare_optional_right(left: Choice, right: Choice | None):
    if left == right:
        reveal_type(left)  # revealed: Choice
        reveal_type(right)  # revealed: Choice
    else:
        reveal_type(left)  # revealed: Choice
        reveal_type(right)  # revealed: Choice | None

def compare_optional_singleton(left: Choice | None, right: Literal[Choice.FIRST]):
    if left == right:
        reveal_type(left)  # revealed: Literal[Choice.FIRST]
    else:
        reveal_type(left)  # revealed: Literal[Choice.SECOND, Choice.THIRD, Choice.FOURTH] | None

class Number(IntEnum):
    ONE = 1
    TWO = 2

def compare_optional_integer_enum(left: Number | None, right: Literal[1]):
    if left == right:
        reveal_type(left)  # revealed: Literal[Number.ONE]
    else:
        reveal_type(left)  # revealed: Literal[Number.TWO] | None
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

An enum with a custom `_missing_` method can create unnamed members, so two values need not be equal
even when only one member is declared:

```py
from enum import Enum

class MissingValueEnum(Enum):
    ONLY = 1

    @classmethod
    def _missing_(cls, value: object) -> "MissingValueEnum":
        return object.__new__(cls)

def compare_open_enums(left: MissingValueEnum, right: MissingValueEnum):
    reveal_type(left == right)  # revealed: bool

    if left != right:
        reveal_type(left)  # revealed: MissingValueEnum
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
from typing import Literal

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

The same comparison-key projection applies when each operand spans several enum classes. This
example represents 18 possible values on each side, which would otherwise require 324 pairwise
comparisons:

```py
from enum import IntEnum

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

An open identity-comparing enum can still be narrowed to all of its declared members. Undeclared
runtime members are not retained merely because every declared member matches:

```py
from enum import Enum
from typing import Literal

class OpenIdentity(Enum):
    A = "a"
    B = "b"

    @classmethod
    def _missing_(cls, value: object) -> "OpenIdentity":
        raise ValueError

class OtherIdentity(Enum):
    C = "c"

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
are equal. Custom comparison methods and open scalar enums remain ambiguous:

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

class OpenLeft(StrEnum):
    MEMBER = "shared"

    @classmethod
    def _missing_(cls, value: object) -> "OpenLeft":
        raise ValueError

def compare_open(left: OpenLeft, right: CustomRight):
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

Enum domains nested in a recursive alias fall back to general comparison inference:

```toml
[environment]
python-version = "3.12"
```

```py
from enum import Enum

class EnumValue(Enum):
    VALUE = 1
    OTHER = 2

type Recursive = EnumValue | Recursive

def _(left: Recursive, right: EnumValue):
    reveal_type(left == right)  # revealed: bool
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

## Constrained type variables

Equality analysis expands the constraints of a constrained type variable in either operand position.
The resulting constraint is intersected with the type variable, preserving its identity:

```py
from enum import Enum
from typing import Literal, TypeVar, final

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

Equality with a literal narrows broad `str`, `int`, and `bytes` types to the values that compare
equal to that literal:

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
        # `True == 1` at runtime.
        reveal_type(value)  # revealed: Literal[1, True]

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

## `x != y` where `y` is of literal type

```py
from typing import Literal

def _(x: Literal[1, 2]):
    if x != 1:
        reveal_type(x)  # revealed: Literal[2]
```

## `x != y` where `y` is a single-valued type

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

## `x != y` where `y` has multiple single-valued options

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

## `!=` for non-single-valued types

Only single-valued types should narrow the type:

```py
def _(x: int | None, y: int):
    if x != y:
        reveal_type(x)  # revealed: int | None
```

## Mix of single-valued and non-single-valued types

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
```

## Union with `Any`

```py
import sys
from enum import Enum, IntEnum
from typing import Any, Literal, TypeVar
from typing_extensions import assert_type

T = TypeVar("T", bound=object)
U = TypeVar("U")
EQUAL_VALUES = TypeVar("EQUAL_VALUES", Literal[0], Literal[False])
RUNTIME_TYPE_VAR = TypeVar("RUNTIME_TYPE_VAR")

class Color(Enum):
    RED = 1
    BLUE = 2

class NonReflexive(Enum):
    VALUE = 1

    def __eq__(self, other: object) -> Literal[False]:
        return False

    def __ne__(self, other: object) -> Literal[True]:
        return True

class Marker: ...

class SingleIntEnum(IntEnum):
    VALUE = 1

def optional_enum_against_any(value: Color | None, other: Any):
    if value != other:
        reveal_type(other)  # revealed: Any
        assert_type(other, Any)

def any_against_optional_enum(value: Any, other: Color | None):
    if value != other:
        reveal_type(value)  # revealed: Any
        assert_type(value, Any)

def optional_bool_against_any(value: bool | None, other: Any):
    if value != other:
        reveal_type(other)  # revealed: Any
        assert_type(other, Any)

def gradual_enum_union(value: Color | Any, other: Color | None):
    if value != other:
        reveal_type(value)  # revealed: Color | Any
        assert_type(value, Color | Any)

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
        reveal_type(x)  # revealed: Any & ~<class 'list[Any]'>

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
        reveal_type(x)  # revealed: Any & ~TypeVar
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
from typing import Literal

def _(x: Literal["a", "b"] | tuple[int, int]):
    if x == "a":
        # tuple type is excluded because it's disjoint from the string literal
        reveal_type(x)  # revealed: Literal["a"]
    else:
        # tuple type remains in the else branch
        reveal_type(x)  # revealed: Literal["b"] | tuple[int, int]
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

The `strict-equality-semantics` option can be enabled to preserve broad builtin types and union
members that a subclass could compare equal to. Narrowing types that are already literal unions
remains safe and is unaffected. This also applies to tuples, whose subclasses can override equality.

```toml
[analysis]
strict-equality-semantics = true
```

```py
from typing import Literal

def broad(value: str):
    if value == "a":
        reveal_type(value)  # revealed: str
    else:
        reveal_type(value)  # revealed: str & ~Literal["a"]

def inequality(value: str):
    if value != "a":
        reveal_type(value)  # revealed: str & ~Literal["a"]
    else:
        reveal_type(value)  # revealed: str

def literal(value: Literal["a", "b"]):
    if value == "a":
        reveal_type(value)  # revealed: Literal["a"]

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
