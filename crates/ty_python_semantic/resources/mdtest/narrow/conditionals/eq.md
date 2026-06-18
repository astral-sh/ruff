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
        # `int` is not eliminated here because there could be subclasses of `int` with custom `__eq__`/`__ne__` methods
        reveal_type(x)  # revealed: Single | int

def _(x: Single | int):
    if x == Single.VALUE:
        reveal_type(x)  # revealed: Single | int
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
        reveal_type(x)  # revealed: Literal[Color.GREEN] | int
    else:
        reveal_type(x)  # revealed: Literal[Color.BLUE] | int

def enum_complement_rhs(x: Color, y: Intersection[Color, Not[Literal[Color.RED]]]):
    if x == y:
        reveal_type(x)  # revealed: Literal[Color.GREEN, Color.BLUE]
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
            reveal_type(value)  # revealed: TransformedByMeta | Literal["member"]
        else:
            reveal_type(value)  # revealed: TransformedByMeta
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
from typing import TypeVar, final

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
```

## Union with `Any`

```py
import sys
from enum import Enum, IntEnum
from typing import Any, Literal, TypeVar

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
