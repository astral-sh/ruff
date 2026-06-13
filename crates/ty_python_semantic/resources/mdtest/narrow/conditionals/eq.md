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

`IntEnum` members use integer equality. The standard `IntEnum` constructor preserves integer literal
values, so these comparisons can narrow both operands:

```py
from enum import IntEnum, auto
from typing import Literal
from typing_extensions import assert_never

class Number(IntEnum):
    ONE = 1
    TWO = 2

def narrow_int_enum(number: Number):
    if number == 1:
        reveal_type(number)  # revealed: Literal[Number.ONE]
    else:
        reveal_type(number)  # revealed: Literal[Number.TWO]

def int_enum_on_left(number: Literal[Number.ONE]):
    if number == 1:
        reveal_type(number)  # revealed: Literal[Number.ONE]
    else:
        assert_never(number)

def int_enum_on_right(number: Literal[Number.ONE]):
    if 1 == number:
        reveal_type(number)  # revealed: Literal[Number.ONE]
    else:
        assert_never(number)

class AutoNumber(IntEnum):
    ONE = auto()
    TWO = auto()

def narrow_auto_int_enum(number: AutoNumber):
    if number == 1:
        reveal_type(number)  # revealed: Literal[AutoNumber.ONE]
    else:
        reveal_type(number)  # revealed: Literal[AutoNumber.TWO]
```

A custom constructor can replace an `IntEnum` member's runtime value, so these comparisons remain
ambiguous:

```py
from enum import IntEnum

class ShiftedNumber(IntEnum):
    def __new__(cls, value: int) -> "ShiftedNumber":
        member = int.__new__(cls, value + 1)
        member._value_ = value + 1
        return member

    ONE = 1
    TWO = 2

def _(number: ShiftedNumber):
    if number == 1:
        reveal_type(number)  # revealed: ShiftedNumber
    else:
        reveal_type(number)  # revealed: ShiftedNumber
```

This also applies when the custom constructor is assigned through `staticmethod`:

```py
from enum import IntEnum, auto
from typing import Literal

def shifted_new(cls: type["AssignedShiftedNumber"], value: int) -> "AssignedShiftedNumber":
    member = int.__new__(cls, value + 10)
    member._value_ = value + 10
    return member

class AssignedShiftedNumber(IntEnum):
    __new__ = staticmethod(shifted_new)
    ONE = 1

def shifted_auto_new(cls: type["AssignedAutoShiftedNumber"], value: int) -> "AssignedAutoShiftedNumber":
    member = int.__new__(cls, value + 10)
    member._value_ = value + 10
    return member

class AssignedAutoShiftedNumber(IntEnum):
    __new__ = staticmethod(shifted_auto_new)
    ONE = 1
    ANSWER = auto()

def _(number: Literal[AssignedShiftedNumber.ONE]):
    if number == 1:
        reveal_type(number)  # revealed: AssignedShiftedNumber
    else:
        reveal_type(number)  # revealed: AssignedShiftedNumber

    if number == 11:
        reveal_type(number)  # revealed: AssignedShiftedNumber
    else:
        reveal_type(number)  # revealed: AssignedShiftedNumber

def _(number: Literal[AssignedAutoShiftedNumber.ANSWER]):
    reveal_type(number)  # revealed: Literal[AssignedAutoShiftedNumber.ANSWER]
```

A custom `_generate_next_value_` means the placeholder for an `auto()` member is not its runtime
value. Comparisons against either value therefore remain ambiguous:

```py
from enum import IntEnum, auto
from typing import Literal

class GeneratedNumber(IntEnum):
    @staticmethod
    def _generate_next_value_(name, start, count, last_values):
        return 42

    ANSWER = auto()

def _(number: Literal[GeneratedNumber.ANSWER]):
    if number == 1:
        reveal_type(number)  # revealed: GeneratedNumber
    else:
        reveal_type(number)  # revealed: GeneratedNumber

    if number == 42:
        reveal_type(number)  # revealed: GeneratedNumber
    else:
        reveal_type(number)  # revealed: GeneratedNumber
```

Assigned custom generators are likewise treated conservatively:

```py
from enum import IntEnum, auto
from typing import Literal

def generated(name: str, start: int, count: int, last_values: list[int]) -> int:
    return 42

class AssignedGeneratedNumber(IntEnum):
    _generate_next_value_ = staticmethod(generated)
    ONE = 1
    ANSWER = auto()

def _(number: Literal[AssignedGeneratedNumber.ANSWER]):
    if number == 1:
        reveal_type(number)  # revealed: Literal[AssignedGeneratedNumber.ANSWER]
    else:
        reveal_type(number)  # revealed: Literal[AssignedGeneratedNumber.ANSWER]

    if number == 42:
        reveal_type(number)  # revealed: Literal[AssignedGeneratedNumber.ANSWER]
    else:
        reveal_type(number)  # revealed: Literal[AssignedGeneratedNumber.ANSWER]
```

A custom metaclass can also rewrite values through the namespace returned by `__prepare__`:

```toml
[environment]
python-version = "3.13"
```

```py
from enum import EnumDict, EnumType, IntEnum
from typing import Any, Literal

class ShiftedEnumDict(EnumDict):
    def __setitem__(self, key: str, value: object) -> None:
        if key.isupper() and isinstance(value, int):
            value += 10
        super().__setitem__(key, value)

class ShiftedEnumType(EnumType):
    @classmethod
    def __prepare__(metacls, cls: str, bases: tuple[type, ...], **kwds: Any) -> ShiftedEnumDict:
        return ShiftedEnumDict(cls)

class PreparedNumber(IntEnum, metaclass=ShiftedEnumType):
    ONE = 1

def _(number: Literal[PreparedNumber.ONE]):
    if number == 1:
        reveal_type(number)  # revealed: PreparedNumber
    else:
        reveal_type(number)  # revealed: PreparedNumber

    if number == 11:
        reveal_type(number)  # revealed: PreparedNumber
    else:
        reveal_type(number)  # revealed: PreparedNumber
```

The right operand's reflected equality method takes precedence when it is a strict subclass of the
left operand's type, so reversing the operands is still ambiguous for an `IntEnum` with a custom
`__eq__`:

```py
from enum import IntEnum
from typing import Literal

class NeverEqualNumber(IntEnum):
    ONE = 1

    def __eq__(self, other: object) -> bool:
        return False

def _(number: Literal[NeverEqualNumber.ONE]):
    if 1 == number:
        reveal_type(number)  # revealed: NeverEqualNumber
    else:
        reveal_type(number)  # revealed: NeverEqualNumber
```

Boolean and integer values with the same runtime value are aliases under the standard `IntEnum`
constructor:

```py
from enum import IntEnum
from typing import Literal
from typing_extensions import assert_never

class BooleanNumber(IntEnum):
    FALSE = False
    ZERO = 0
    TRUE = True
    ONE = 1

def _(number: Literal[BooleanNumber.ZERO]):
    if number == BooleanNumber.FALSE:
        reveal_type(number)  # revealed: Literal[BooleanNumber.FALSE]
    else:
        assert_never(number)

    if number == 0:
        reveal_type(number)  # revealed: Literal[BooleanNumber.FALSE]
    else:
        assert_never(number)
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
`__ne__` method does not affect narrowing based on `__eq__`:

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
```

## Equality between concrete runtime classes

Types such as `bool`, `LiteralString`, and `TypedDict` correspond to specific runtime classes.
Equality with another instance of the same runtime class can therefore eliminate `None`:

```py
from typing import TypedDict
from typing_extensions import LiteralString

class Payload(TypedDict):
    value: int

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
```

## Comparisons with user-defined methods

Arbitrary user-defined comparison methods are not used for narrowing, i.e., we don't inspect the
bodies of user-defined `__eq__` or `__ne__` methods to predict their results, and instead require a
`Literal` return type annotation:

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
from typing import Any, Literal

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
