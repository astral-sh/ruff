# Enums

## Basic

```py
from enum import Enum
from typing import Literal

class Color(Enum):
    RED = 1
    GREEN = 2
    BLUE = 3

reveal_type(Color.RED)  # revealed: Literal[Color.RED]
reveal_type(Color.RED.name)  # revealed: Literal["RED"]
reveal_type(Color.RED.value)  # revealed: Literal[1]

# TODO: Could be `Literal[Color.RED]` to be more precise
reveal_type(Color["RED"])  # revealed: Color
reveal_type(Color(1))  # revealed: Color

reveal_type(Color.RED in Color)  # revealed: bool
```

## Constructor calls

```py
from enum import Enum, IntEnum

class Number(Enum):
    ONE = 1
    TWO = 2

reveal_type(Number(1))  # revealed: Number
reveal_type(Number(value=1))  # revealed: Number

class MixedInt(IntEnum):
    ONE = 1
    TWO = 2

reveal_type(MixedInt(1))  # revealed: MixedInt

class MixedStr(str, Enum):
    RED = "red"
    BLUE = "blue"

reveal_type(MixedStr("red"))  # revealed: MixedStr

class Maybe(Enum):
    NONE = None
    SOME = "some"

reveal_type(Maybe(None))  # revealed: Maybe

class Planet(Enum):
    _value_: int

    def __init__(self, value: int, mass: float, radius: float):
        self._value_ = value

    MERCURY = (1, 3.303e23, 2.4397e6)
    VENUS = (2, 4.869e24, 6.0518e6)

# TODO: `Planet(1)` raises `ValueError` at runtime. `EnumType.__call__` accepts positional
# arguments only, then forwards them to the enum's `__new__` / `__init__`, so multi-argument
# enum members still require the full positional member payload (for example `Planet(1, ...)`).
reveal_type(Planet(1))  # revealed: Planet
reveal_type(Planet(1, 3.303e23, 2.4397e6))  # revealed: Planet

class EmptyEnum(Enum): ...

# TODO: these raise `TypeError` at runtime, but we do not yet emit diagnostics for them.
reveal_type(EmptyEnum(foo=1))  # revealed: EmptyEnum
reveal_type(EmptyEnum(1, 2))  # revealed: EmptyEnum

Dynamic = Enum("Dynamic", {"RED": "red", "GREEN": "green"})

reveal_type(Dynamic("red"))  # revealed: Dynamic
```

## Constructor calls on Python 3.12

```toml
[environment]
python-version = "3.12"
```

```py
from enum import Enum

class Triple(Enum):
    XYZ = 1, 2, 3
    OTHER = 4, 5, 6

reveal_type(Triple(1, 2, 3))  # revealed: Triple
```

## Enum members

### Basic

Simple enums with integer or string values:

```py
from enum import Enum
from ty_extensions import enum_members

class ColorInt(Enum):
    RED = 1
    GREEN = 2
    BLUE = 3

# revealed: tuple[Literal["RED"], Literal["GREEN"], Literal["BLUE"]]
reveal_type(enum_members(ColorInt))

class ColorStr(Enum):
    RED = "red"
    GREEN = "green"
    BLUE = "blue"

# revealed: tuple[Literal["RED"], Literal["GREEN"], Literal["BLUE"]]
reveal_type(enum_members(ColorStr))
```

### When deriving from `IntEnum`

```py
from enum import IntEnum
from ty_extensions import enum_members

class ColorInt(IntEnum):
    RED = 1
    GREEN = 2
    BLUE = 3

# revealed: tuple[Literal["RED"], Literal["GREEN"], Literal["BLUE"]]
reveal_type(enum_members(ColorInt))
```

### Annotated assignments with values are still members

If an enum attribute has both an annotation and a value, it is still an enum member at runtime, even
though the annotation is invalid:

```py
from enum import Enum
from ty_extensions import enum_members

class Answer(Enum):
    YES = 1
    NO = 2

    annotated_member: str = "some value"  # error: [invalid-enum-member-annotation]

# revealed: tuple[Literal["YES"], Literal["NO"], Literal["annotated_member"]]
reveal_type(enum_members(Answer))
reveal_type(Answer.annotated_member)  # revealed: Literal[Answer.annotated_member]
reveal_type(Answer.YES.annotated_member)  # revealed: Literal[Answer.annotated_member]
```

Enum members are allowed to be marked `Final` (without a type), even if unnecessary:

```py
from enum import Enum
from typing import Final
from ty_extensions import enum_members

class Answer(Enum):
    YES: Final = 1
    NO: Final = 2

# revealed: tuple[Literal["YES"], Literal["NO"]]
reveal_type(enum_members(Answer))
```

### Annotated enum members

The [typing spec] states that enum members should not have explicit type annotations. Type checkers
should report an error for annotated enum members because the annotation is misleading — the actual
type of an enum member is the enum class itself, not the annotated type.

```toml
[environment]
python-version = "3.11"
```

```py
from enum import Enum, IntEnum, StrEnum, member
from typing import Callable, Final

class Pet(Enum):
    CAT = 1
    DOG: int = 2  # error: [invalid-enum-member-annotation] "Type annotation on enum member `DOG` is not allowed"
    BIRD: str = "bird"  # error: [invalid-enum-member-annotation]
```

Bare `Final` annotations are allowed (they don't specify a type):

```py
class Pet2(Enum):
    CAT: Final = 1  # OK
    DOG: Final = 2  # OK
```

But `Final` with a type argument is not allowed:

```py
class Pet3(Enum):
    CAT: Final[int] = 1  # error: [invalid-enum-member-annotation]
    DOG: Final[str] = "woof"  # error: [invalid-enum-member-annotation]
```

`enum.member` used as value wrapper is the standard way to declare members explicitly:

```py
class Pet4(Enum):
    CAT = member(1)  # OK
```

Dunder and private names are not enum members, so they don't trigger the diagnostic:

```py
class Pet5(Enum):
    CAT = 1
    __private: int = 2  # OK: dunder/private names are never members
    __module__: str = "my_module"  # OK
```

Pure declarations (annotations without values) are non-members and are fine:

```py
class Pet6(Enum):
    CAT = 1
    species: str  # OK: no value, so this is a non-member declaration

reveal_type(Pet6.species)  # revealed: str
reveal_type(Pet6.CAT.species)  # revealed: str
```

### Pure declarations in stubs

In stubs, these should still be treated as non-member attributes rather than enum members:

```pyi
from enum import Enum

class Pet6Stub(Enum):
    species: str

    CAT = ...
    DOG = ...

reveal_type(Pet6Stub.species)  # revealed: str
```

### Callable values and subclasses

Callable values are never enum members at runtime, so annotating them is fine:

```toml
[environment]
python-version = "3.11"
```

```py
from enum import Enum, IntEnum, StrEnum
from typing import Callable

def identity(x: int) -> int:
    return x

class Pet7(Enum):
    CAT = 1
    declared_callable: Callable[[int], int] = identity  # OK: callables are never members
```

The check also works for subclasses of `Enum`:

```py
class Status(IntEnum):
    OK: int = 200  # error: [invalid-enum-member-annotation]
    NOT_FOUND = 404  # OK

class Color(StrEnum):
    RED: str = "red"  # error: [invalid-enum-member-annotation]
    GREEN = "green"  # OK
```

Special sunder names like `_value_` and `_ignore_` are not flagged:

```py
class Pet8(Enum):
    _value_: int = 0  # OK: `_value_` is a special enum name
    _ignore_: str = "TEMP"  # OK: `_ignore_` is a special enum name
    CAT = 1
```

Names listed in `_ignore_` are not members, so annotating them is fine:

```py
class Pet9(Enum):
    _ignore_ = "A B"
    A: int = 42  # OK: `A` is listed in `_ignore_`
    B: str = "hello"  # OK: `B` is listed in `_ignore_`
    C: int = 3  # error: [invalid-enum-member-annotation]
```

### Unreachable declarations do not change membership

Statically unreachable declarations should be ignored when deciding whether a name is an enum
member:

```py
from enum import Enum
from ty_extensions import enum_members

class Pet10(Enum):
    if False:
        CAT: int

    CAT = 1
    DOG = 2

# revealed: tuple[Literal["CAT"], Literal["DOG"]]
reveal_type(enum_members(Pet10))
reveal_type(Pet10.CAT)  # revealed: Literal[Pet10.CAT]
reveal_type(Pet10.DOG)  # revealed: Literal[Pet10.DOG]
```

### Declared `_value_` annotation

If a `_value_` annotation is defined on an `Enum` class, all enum member values must be compatible
with the declared type:

```pyi
from enum import Enum

class Color(Enum):
    _value_: int
    RED = 1
    GREEN = "green"  # error: [invalid-assignment]
    BLUE = ...
    YELLOW = None  # error: [invalid-assignment]
    PURPLE = []  # error: [invalid-assignment]
```

When `_value_` is annotated, `.value` and `._value_` are inferred as the declared type:

```py
from enum import Enum
from typing import Final

class Color2(Enum):
    _value_: int
    RED = 1
    GREEN = 2

reveal_type(Color2.RED.value)  # revealed: int
reveal_type(Color2.RED._value_)  # revealed: int

class WantsInt(Enum):
    _value_: int
    OK: Final = 1
    BAD: Final = "oops"  # error: [invalid-assignment]
```

### `_value_` annotation with `__init__`

When `__init__` is defined, member values are validated by synthesizing a call to `__init__`. The
`_value_` annotation still constrains assignments to `self._value_` inside `__init__`:

```py
from enum import Enum

class Planet(Enum):
    _value_: int

    def __init__(self, value: int, mass: float, radius: float):
        self._value_ = value

    MERCURY = (1, 3.303e23, 2.4397e6)
    SATURN = "saturn"  # error: [invalid-assignment]

reveal_type(Planet.MERCURY.value)  # revealed: int
reveal_type(Planet.MERCURY._value_)  # revealed: int
```

`Final`-annotated members are also validated against `__init__`:

```py
from enum import Enum
from typing import Final

class Planet(Enum):
    def __init__(self, mass: float, radius: float):
        self.mass = mass
        self.radius = radius

    MERCURY: Final = (3.303e23, 2.4397e6)
    BAD: Final = "not a planet"  # error: [invalid-assignment]
```

### `_value_` annotation incompatible with `__init__`

When `_value_` and `__init__` disagree, the assignment inside `__init__` is flagged:

```py
from enum import Enum

class Planet(Enum):
    _value_: str

    def __init__(self, value: int, mass: float, radius: float):
        self._value_ = value  # error: [invalid-assignment]

    MERCURY = (1, 3.303e23, 2.4397e6)
    SATURN = "saturn"  # error: [invalid-assignment]

reveal_type(Planet.MERCURY.value)  # revealed: str
reveal_type(Planet.MERCURY._value_)  # revealed: str
```

### `__init__` without `_value_` annotation

When `__init__` is defined but no explicit `_value_` annotation exists, member values are validated
against the `__init__` signature. Values that are incompatible with `__init__` are flagged:

```py
from enum import Enum

class Planet2(Enum):
    def __init__(self, mass: float, radius: float):
        self.mass = mass
        self.radius = radius

    MERCURY = (3.303e23, 2.4397e6)
    VENUS = (4.869e24, 6.0518e6)
    INVALID = "not a planet"  # error: [invalid-assignment]

reveal_type(Planet2.MERCURY.value)  # revealed: Any
reveal_type(Planet2.MERCURY._value_)  # revealed: Any
```

### `__new__` without `_value_` annotation

When `__new__` is defined but no explicit `_value_` annotation exists, member RHS values are passed
to `__new__`, but the method can assign `_value_` independently. In this case, `.value` falls back
to `Any`:

```py
from enum import Enum

class Connector(Enum):
    def __new__(cls, value: str, connector_id: int) -> "Connector":
        obj = object.__new__(cls)
        obj._value_ = value
        obj.connector_id = connector_id
        return obj

    GITHUB = ("github", 1)

reveal_type(Connector.GITHUB.value)  # revealed: Any
reveal_type(Connector.GITHUB._value_)  # revealed: Any
```

An explicit `_value_` annotation still takes precedence:

```py
from enum import Enum

class AnnotatedConnector(Enum):
    _value_: str

    def __new__(cls, value: str, connector_id: int = 0) -> "AnnotatedConnector":
        obj = object.__new__(cls)
        obj._value_ = value
        obj.connector_id = connector_id
        return obj

    GITHUB = "github"

reveal_type(AnnotatedConnector.GITHUB.value)  # revealed: str
reveal_type(AnnotatedConnector.GITHUB._value_)  # revealed: str
```

### Inherited `_value_` annotation

A `_value_` annotation on a parent enum is inherited by subclasses. Member values are validated
against the inherited annotation, and `.value` uses the declared type:

```py
from enum import Enum

class Base(Enum):
    _value_: int

class Child(Base):
    A = 1
    B = "not an int"  # error: [invalid-assignment]

reveal_type(Child.A.value)  # revealed: int
```

This also works through multiple levels of inheritance, where `_value_` is declared on an
intermediate class:

```py
from enum import Enum

class Grandparent(Enum):
    pass

class Parent(Grandparent):
    _value_: int

class Child(Parent):
    A = 1
    B = "not an int"  # error: [invalid-assignment]

reveal_type(Child.A.value)  # revealed: int
```

### Inherited `__init__`

A custom `__init__` on a parent enum is inherited by subclasses. Member values are validated against
the inherited `__init__` signature:

```py
from enum import Enum

class Base(Enum):
    def __init__(self, a: int, b: str):
        self._value_ = a

class Child(Base):
    A = (1, "foo")
    B = "should be checked against __init__"  # error: [invalid-assignment]

reveal_type(Child.A.value)  # revealed: Any
```

This also works through multiple levels of inheritance:

```py
from enum import Enum

class Grandparent(Enum):
    def __init__(self, a: int, b: str):
        self._value_ = a

class Parent(Grandparent):
    pass

class Child(Parent):
    A = (1, "foo")
    B = "bad"  # error: [invalid-assignment]

reveal_type(Child.A.value)  # revealed: Any
```

### Inherited `__new__`

A custom `__new__` on a parent enum is inherited by subclasses. Without an explicit `_value_`
annotation, subclass member values remain dynamic:

```py
from enum import Enum

class Base(Enum):
    def __new__(cls, value: str, connector_id: int) -> "Base":
        obj = object.__new__(cls)
        obj._value_ = value
        obj.connector_id = connector_id
        return obj

class Child(Base):
    GITHUB = ("github", 1)

reveal_type(Child.GITHUB.value)  # revealed: Any
reveal_type(Child.GITHUB._value_)  # revealed: Any
```

An explicit `_value_` annotation on the subclass still takes precedence:

```py
from enum import Enum

class Base(Enum):
    def __new__(cls, value: str, connector_id: int = 0) -> "Base":
        obj = object.__new__(cls)
        obj._value_ = value
        obj.connector_id = connector_id
        return obj

class Child(Base):
    _value_: str

    GITHUB = "github"

reveal_type(Child.GITHUB.value)  # revealed: str
reveal_type(Child.GITHUB._value_)  # revealed: str
```

Member values are still validated against the inherited `__new__` signature, even when `_value_` is
explicitly annotated:

```py
from enum import Enum

class Base(Enum):
    def __new__(cls, value: int, connector_id: int = 0) -> "Base":
        obj = object.__new__(cls)
        obj._value_ = value
        obj.connector_id = connector_id
        return obj

class Child(Base):
    _value_: str

    GITHUB = "github"  # error: [invalid-assignment]
```

### Non-member attributes with disallowed type

Methods, callables, descriptors (including properties), and nested classes that are defined in the
class are not treated as enum members:

```py
from enum import Enum
from ty_extensions import enum_members
from typing import Callable, Literal

def identity(x) -> int:
    return x

class Descriptor:
    def __get__(self, instance, owner):
        return 0

class Answer(Enum):
    YES = 1
    NO = 2

    def some_method(self) -> None: ...
    @staticmethod
    def some_static_method() -> None: ...
    @classmethod
    def some_class_method(cls) -> None: ...

    some_callable = lambda x: 0
    declared_callable: Callable[[int], int] = identity
    function_reference = identity

    some_descriptor = Descriptor()

    @property
    def some_property(self) -> str:
        return ""

    class NestedClass: ...

# revealed: tuple[Literal["YES"], Literal["NO"]]
reveal_type(enum_members(Answer))
```

### `enum.property`

Enum attributes that are defined using `enum.property` are not considered members:

```toml
[environment]
python-version = "3.11"
```

```py
from enum import Enum, property as enum_property
from typing import Any
from ty_extensions import enum_members

class Answer(Enum):
    YES = 1
    NO = 2

    @enum_property
    def some_property(self) -> str:
        return "property value"

# revealed: tuple[Literal["YES"], Literal["NO"]]
reveal_type(enum_members(Answer))
```

Enum attributes defined using `enum.property` take precedence over generated attributes.

```py
from enum import Enum, property as enum_property

class Choices(Enum):
    A = 1
    B = 2

    @enum_property
    def value(self) -> Any: ...

# TODO: This should be `Any` - overridden by `@enum_property`
reveal_type(Choices.A.value)  # revealed: Literal[1]
```

### `types.DynamicClassAttribute`

Attributes defined using `types.DynamicClassAttribute` are not considered members:

```py
from enum import Enum
from ty_extensions import enum_members
from types import DynamicClassAttribute

class Answer(Enum):
    YES = 1
    NO = 2

    @DynamicClassAttribute
    def dynamic_property(self) -> str:
        return "dynamic value"

# revealed: tuple[Literal["YES"], Literal["NO"]]
reveal_type(enum_members(Answer))
```

### In stubs

Stubs can optionally use `...` for the actual value:

```pyi
from enum import Enum
from ty_extensions import enum_members
from typing import cast

class Color(Enum):
    RED = ...
    GREEN = cast(int, ...)
    BLUE = 3

# revealed: tuple[Literal["RED"], Literal["GREEN"], Literal["BLUE"]]
reveal_type(enum_members(Color))
```

### Aliases

Enum members can have aliases, which are not considered separate members:

```py
from enum import Enum
from ty_extensions import enum_members

class Answer(Enum):
    YES = 1
    NO = 2

    DEFINITELY = YES

# revealed: tuple[Literal["YES"], Literal["NO"]]
reveal_type(enum_members(Answer))

reveal_type(Answer.DEFINITELY)  # revealed: Literal[Answer.YES]
```

If a value is duplicated, we also treat that as an alias:

```py
from enum import Enum

class Color(Enum):
    RED = 1
    GREEN = 2

    red = 1
    green = 2

# revealed: tuple[Literal["RED"], Literal["GREEN"]]
reveal_type(enum_members(Color))

# revealed: Literal[Color.RED]
reveal_type(Color.red)
```

Multiple aliases to the same member are also supported. This is a regression test for
<https://github.com/astral-sh/ty/issues/1293>:

```py
from ty_extensions import enum_members

class ManyAliases(Enum):
    real_member = "real_member"
    alias1 = "real_member"
    alias2 = "real_member"
    alias3 = "real_member"

    other_member = "other_real_member"

# revealed: tuple[Literal["real_member"], Literal["other_member"]]
reveal_type(enum_members(ManyAliases))

reveal_type(ManyAliases.real_member)  # revealed: Literal[ManyAliases.real_member]
reveal_type(ManyAliases.alias1)  # revealed: Literal[ManyAliases.real_member]
reveal_type(ManyAliases.alias2)  # revealed: Literal[ManyAliases.real_member]
reveal_type(ManyAliases.alias3)  # revealed: Literal[ManyAliases.real_member]

reveal_type(ManyAliases.real_member.value)  # revealed: Literal["real_member"]
reveal_type(ManyAliases.real_member.name)  # revealed: Literal["real_member"]

reveal_type(ManyAliases.alias1.value)  # revealed: Literal["real_member"]
reveal_type(ManyAliases.alias1.name)  # revealed: Literal["real_member"]

reveal_type(ManyAliases.alias2.value)  # revealed: Literal["real_member"]
reveal_type(ManyAliases.alias2.name)  # revealed: Literal["real_member"]

reveal_type(ManyAliases.alias3.value)  # revealed: Literal["real_member"]
reveal_type(ManyAliases.alias3.name)  # revealed: Literal["real_member"]
```

`auto()` still uses the preceding concrete value even when `1` and `True` compare equal:

```py
from enum import Enum, auto
from ty_extensions import enum_members

class IntThenTrue(Enum):
    A = 1
    B = True
    C = auto()

# revealed: tuple[Literal["A"], Literal["B"], Literal["C"]]
reveal_type(enum_members(IntThenTrue))

reveal_type(IntThenTrue.C.value)  # revealed: Literal[2]
```

Functional enums also detect duplicate-value aliases in both dict and list-of-tuples forms:

```py
from enum import Enum
from ty_extensions import enum_members

DictAlias = Enum("DictAlias", {"A": 1, "B": 1})

# revealed: tuple[Literal["A"]]
reveal_type(enum_members(DictAlias))

# single-member enum is a singleton, so member access resolves to the instance type
reveal_type(DictAlias.A)  # revealed: DictAlias
reveal_type(DictAlias.B)  # revealed: DictAlias

PairsAlias = Enum("PairsAlias", [("A", 1), ("B", 1)])

# revealed: tuple[Literal["A"]]
reveal_type(enum_members(PairsAlias))

reveal_type(PairsAlias.A)  # revealed: PairsAlias
reveal_type(PairsAlias.B)  # revealed: PairsAlias
```

### Using `auto()`

```toml
[environment]
python-version = "3.11"
```

```py
from enum import Enum, auto
from ty_extensions import enum_members

class Answer(Enum):
    YES = auto()
    NO = auto()

# revealed: tuple[Literal["YES"], Literal["NO"]]
reveal_type(enum_members(Answer))

reveal_type(Answer.YES.value)  # revealed: Literal[1]
reveal_type(Answer.NO.value)  # revealed: Literal[2]

class SingleMember(Enum):
    SINGLE = auto()

reveal_type(SingleMember.SINGLE.value)  # revealed: Literal[1]
```

Usages of `auto()` can be combined with manual value assignments:

```py
class Mixed(Enum):
    MANUAL_1 = -1
    AUTO_1 = auto()
    MANUAL_2 = -2
    AUTO_2 = auto()

reveal_type(Mixed.MANUAL_1.value)  # revealed: Literal[-1]
reveal_type(Mixed.AUTO_1.value)  # revealed: Literal[1]
reveal_type(Mixed.MANUAL_2.value)  # revealed: Literal[-2]
reveal_type(Mixed.AUTO_2.value)  # revealed: Literal[2]
```

If `auto()` follows a non-literal value, the generated value widens to `int` since the previous
value isn't known at type-check time:

```py
def f(n: int):
    class StaticDynamic(Enum):
        A = n
        B = auto()

    reveal_type(StaticDynamic.A.value)  # revealed: int
    reveal_type(StaticDynamic.B.value)  # revealed: int

    Dynamic = Enum("Dynamic", {"A": n, "B": auto()})

    reveal_type(Dynamic.A.value)  # revealed: int
    reveal_type(Dynamic.B.value)  # revealed: int
```

Bool literals are still concrete predecessors for `auto()`:

```py
class AfterFalse(Enum):
    A = False
    B = auto()

reveal_type(AfterFalse.B.value)  # revealed: Literal[1]

class AfterTrue(Enum):
    A = True
    B = auto()

reveal_type(AfterTrue.B.value)  # revealed: Literal[2]
```

When using `auto()` with `StrEnum`, the value is the lowercase name of the member:

```py
from enum import StrEnum, auto

class Answer(StrEnum):
    YES = auto()
    NO = auto()

reveal_type(Answer.YES.value)  # revealed: Literal["yes"]
reveal_type(Answer.NO.value)  # revealed: Literal["no"]

class SingleMember(StrEnum):
    SINGLE = auto()

reveal_type(SingleMember.SINGLE.value)  # revealed: Literal["single"]
```

Using `auto()` with `IntEnum` also works as expected. `IntEnum` declares `_value_: int` in typeshed,
so `.value` is typed as `int` rather than a precise literal:

```py
from enum import IntEnum, auto

class Answer(IntEnum):
    YES = auto()
    NO = auto()

reveal_type(Answer.YES.value)  # revealed: int
reveal_type(Answer.NO.value)  # revealed: int
```

As does using `auto()` for other enums that use `int` as a mixin:

```py
from enum import Enum, auto

class Answer(int, Enum):
    YES = auto()
    NO = auto()

reveal_type(Answer.YES.value)  # revealed: Literal[1]
reveal_type(Answer.NO.value)  # revealed: Literal[2]
```

It's [hard to predict](https://github.com/astral-sh/ruff/pull/20541#discussion_r2381878613) what the
effect of using `auto()` will be for an arbitrary non-integer mixin, so for anything that isn't a
`StrEnum` and has a non-`int` mixin, we simply fallback to typeshed's annotation of `Any` for the
`value` property:

```python
from enum import Enum, auto

class A(str, Enum):
    X = auto()
    Y = auto()

reveal_type(A.X.value)  # revealed: Any

class B(bytes, Enum):
    X = auto()
    Y = auto()

reveal_type(B.X.value)  # revealed: Any

class C(tuple, Enum):
    X = auto()
    Y = auto()

reveal_type(C.X.value)  # revealed: Any

class D(float, Enum):
    X = auto()
    Y = auto()

reveal_type(D.X.value)  # revealed: Any
```

Combining aliases with `auto()`:

```py
from enum import Enum, auto

class Answer(Enum):
    YES = auto()
    NO = auto()

    DEFINITELY = YES

# TODO: This should ideally be `tuple[Literal["YES"], Literal["NO"]]`
# revealed: tuple[Literal["YES"], Literal["NO"], Literal["DEFINITELY"]]
reveal_type(enum_members(Answer))
```

`auto()` values are computed at runtime by the enum metaclass, so we skip validation against both
`_value_` annotations and custom `__init__` signatures:

```py
from enum import Enum, auto

class WithValue(Enum):
    _value_: int
    A = auto()
    B = auto()

reveal_type(WithValue.A.value)  # revealed: int

class WithInit(Enum):
    def __init__(self, mass: float, radius: float):
        self.mass = mass
        self.radius = radius

    MERCURY = (3.303e23, 2.4397e6)
    AUTO = auto()

reveal_type(WithInit.MERCURY.value)  # revealed: Any
```

### `member` and `nonmember`

```toml
[environment]
python-version = "3.11"
```

```py
from enum import Enum, auto, member, nonmember
from ty_extensions import enum_members

class Answer(Enum):
    YES = member(1)
    NO = member(2)
    OTHER = nonmember(17)

# revealed: tuple[Literal["YES"], Literal["NO"]]
reveal_type(enum_members(Answer))

# `nonmember` attributes are unwrapped to the inner value type when accessed.
# revealed: int
reveal_type(Answer.OTHER)
```

`member` can also be used as a decorator:

```py
from enum import Enum, member
from ty_extensions import enum_members

class Answer(Enum):
    yes = member(1)
    no = member(2)

    @member
    def maybe(self) -> None:
        return

# revealed: tuple[Literal["yes"], Literal["no"], Literal["maybe"]]
reveal_type(enum_members(Answer))
```

### Dunder and class-private names

An attribute with a name beginning with a double underscore is treated as a non-member. This
includes both [class-private names] (not ending in `__`) and dunder names (ending in `__`).
CPython's enum metaclass excludes all such names from membership:

```py
from enum import Enum, IntEnum
from ty_extensions import enum_members

class Answer(Enum):
    YES = 1
    NO = 2

    __private_member = 3
    __maybe__ = 4

# revealed: tuple[Literal["YES"], Literal["NO"]]
reveal_type(enum_members(Answer))
```

Setting `__module__` (a common pattern to control `repr()` and `pickle` behavior) does not make it
an enum member, even when the value type differs from the enum's value type:

```py
class ExitCode(IntEnum):
    OK = 0
    ERROR = 1

    __module__ = "my_package"  # no error, not a member

# revealed: tuple[Literal["OK"], Literal["ERROR"]]
reveal_type(enum_members(ExitCode))
```

### Ignored names

An enum class can define a class symbol named `_ignore_`. This can be a string containing a
whitespace-delimited list of names:

```py
from enum import Enum
from ty_extensions import enum_members

class Answer(Enum):
    _ignore_ = "IGNORED _other_ignored       also_ignored"

    YES = 1
    NO = 2

    IGNORED = 3
    _other_ignored = "test"
    also_ignored = "test2"

# revealed: tuple[Literal["YES"], Literal["NO"]]
reveal_type(enum_members(Answer))
```

`_ignore_` can also be a list of names:

```py
class Answer2(Enum):
    _ignore_ = ["MAYBE", "_other"]

    YES = 1
    NO = 2

    MAYBE = 3
    _other = "test"

# TODO: This should be `tuple[Literal["YES"], Literal["NO"]]`
# revealed: tuple[Literal["YES"], Literal["NO"], Literal["MAYBE"], Literal["_other"]]
reveal_type(enum_members(Answer2))
```

### Special names

Make sure that special names like `name` and `value` can be used for enum members (without
conflicting with `Enum.name` and `Enum.value`):

```py
from enum import Enum
from ty_extensions import enum_members

class Answer(Enum):
    name = 1
    value = 2

# revealed: tuple[Literal["name"], Literal["value"]]
reveal_type(enum_members(Answer))

reveal_type(Answer.name)  # revealed: Literal[Answer.name]
reveal_type(Answer.value)  # revealed: Literal[Answer.value]
```

## Iterating over enum members

```py
from enum import Enum

class Color(Enum):
    RED = 1
    GREEN = 2
    BLUE = 3

for color in Color:
    reveal_type(color)  # revealed: Color

# TODO: Should be `list[Color]`
reveal_type(list(Color))  # revealed: list[Unknown]
```

## Methods / non-member attributes

Methods and non-member attributes defined in the enum class can be accessed on enum members:

```py
from enum import Enum

class Answer(Enum):
    YES = 1
    NO = 2

    def is_yes(self) -> bool:
        return self == Answer.YES

    constant: int

reveal_type(Answer.YES.is_yes())  # revealed: bool
reveal_type(Answer.YES.constant)  # revealed: int

class MyEnum(Enum):
    def some_method(self) -> None:
        pass

class MyAnswer(MyEnum):
    YES = 1
    NO = 2

reveal_type(MyAnswer.YES.some_method())  # revealed: None
```

## Accessing enum members from enum members / instances

```py
from enum import Enum

class Answer(Enum):
    YES = 1
    NO = 2

reveal_type(Answer.YES.NO)  # revealed: Literal[Answer.NO]

def _(answer: Answer) -> None:
    reveal_type(answer.YES)  # revealed: Literal[Answer.YES]
    reveal_type(answer.NO)  # revealed: Literal[Answer.NO]
```

## Accessing enum members from `type[…]`

```py
from enum import Enum

class Answer(Enum):
    YES = 1
    NO = 2

def _(answer: type[Answer]) -> None:
    reveal_type(answer.YES)  # revealed: Literal[Answer.YES]
    reveal_type(answer.NO)  # revealed: Literal[Answer.NO]
```

## Calling enum variants

```py
from enum import Enum
from typing import Callable
import sys

class Printer(Enum):
    STDOUT = 1
    STDERR = 2

    def __call__(self, msg: str) -> None:
        if self == Printer.STDOUT:
            print(msg)
        elif self == Printer.STDERR:
            print(msg, file=sys.stderr)

Printer.STDOUT("Hello, world!")
Printer.STDERR("An error occurred!")

callable: Callable[[str], None] = Printer.STDOUT
callable("Hello again!")
callable = Printer.STDERR
callable("Another error!")
```

## Special attributes on enum members

### `name` and `_name_`

```py
from enum import Enum
from typing import Literal

class Color(Enum):
    RED = 1
    GREEN = 2
    BLUE = 3

reveal_type(Color.RED._name_)  # revealed: Literal["RED"]

def _(red_or_blue: Literal[Color.RED, Color.BLUE]):
    reveal_type(red_or_blue.name)  # revealed: Literal["RED", "BLUE"]

def _(color: Color):
    reveal_type(color.name)  # revealed: Literal["RED", "GREEN", "BLUE"]
```

### `value` and `_value_`

```toml
[environment]
python-version = "3.11"
```

```py
from enum import Enum, StrEnum
from typing import Literal

class Color(Enum):
    RED = 1
    GREEN = 2
    BLUE = 3

reveal_type(Color.RED.value)  # revealed: Literal[1]
reveal_type(Color.RED._value_)  # revealed: Literal[1]

reveal_type(Color.GREEN.value)  # revealed: Literal[2]
reveal_type(Color.GREEN._value_)  # revealed: Literal[2]

def _(color: Color):
    reveal_type(color.value)  # revealed: Literal[1, 2, 3]

class Answer(StrEnum):
    YES = "yes"
    NO = "no"

reveal_type(Answer.YES.value)  # revealed: Literal["yes"]
reveal_type(Answer.YES._value_)  # revealed: Literal["yes"]

reveal_type(Answer.NO.value)  # revealed: Literal["no"]
reveal_type(Answer.NO._value_)  # revealed: Literal["no"]

def _(answer: Answer):
    reveal_type(answer.value)  # revealed: Literal["yes", "no"]
```

## Properties of enum types

### Implicitly final

An enum with one or more defined members cannot be subclassed. They are implicitly "final".

```py
from enum import Enum

class Color(Enum):
    RED = 1
    GREEN = 2
    BLUE = 3

# error: [subclass-of-final-class] "Class `ExtendedColor` cannot inherit from final class `Color`"
class ExtendedColor(Color):
    YELLOW = 4

def f(color: Color):
    if isinstance(color, int):
        reveal_type(color)  # revealed: Never
```

An `Enum` subclass without any defined members can be subclassed:

```py
from enum import Enum
from ty_extensions import enum_members

class MyEnum(Enum):
    def some_method(self) -> None:
        pass

class Answer(MyEnum):
    YES = 1
    NO = 2

# revealed: tuple[Literal["YES"], Literal["NO"]]
reveal_type(enum_members(Answer))
```

### Meta-type

```py
from enum import Enum

class Answer(Enum):
    YES = 1
    NO = 2

reveal_type(type(Answer.YES))  # revealed: <class 'Answer'>

class NoMembers(Enum): ...

def _(answer: Answer, no_members: NoMembers):
    reveal_type(type(answer))  # revealed: <class 'Answer'>
    reveal_type(type(no_members))  # revealed: type[NoMembers]
```

### Cyclic references

```py
from enum import Enum
from typing import Literal
from ty_extensions import enum_members

class Answer(Enum):
    YES = 1
    NO = 2

    @classmethod
    def yes(cls) -> "Literal[Answer.YES]":
        return Answer.YES

# revealed: tuple[Literal["YES"], Literal["NO"]]
reveal_type(enum_members(Answer))
```

## Custom enum types

Enum classes can also be defined using a subclass of `enum.Enum` or any class that uses
`enum.EnumType` (or a subclass thereof) as a metaclass. `enum.EnumType` was called `enum.EnumMeta`
prior to Python 3.11.

### Subclasses of `Enum`

```py
from enum import Enum, EnumMeta

class CustomEnumSubclass(Enum):
    def custom_method(self) -> int:
        return 0

class EnumWithCustomEnumSubclass(CustomEnumSubclass):
    NO = 0
    YES = 1

reveal_type(EnumWithCustomEnumSubclass.NO)  # revealed: Literal[EnumWithCustomEnumSubclass.NO]
reveal_type(EnumWithCustomEnumSubclass.NO.custom_method())  # revealed: int
```

### Enums with (subclasses of) `EnumMeta` as metaclass

```toml
[environment]
python-version = "3.9"
```

```py
from enum import Enum, EnumMeta

class EnumWithEnumMetaMetaclass(metaclass=EnumMeta):
    # Using `EnumMeta` as a metaclass without inheriting `Enum` requires an `__init__`
    # method that will accept member values (TODO we could catch the lack of this):
    def __init__(self, val): ...
    NO = 0
    YES = 1

reveal_type(EnumWithEnumMetaMetaclass.NO)  # revealed: Literal[EnumWithEnumMetaMetaclass.NO]
reveal_type(EnumWithEnumMetaMetaclass(0))  # revealed: EnumWithEnumMetaMetaclass

class SubclassOfEnumMeta(EnumMeta): ...

class EnumWithSubclassOfEnumMetaMetaclass(metaclass=SubclassOfEnumMeta):
    def __init__(self, val): ...
    NO = 0
    YES = 1

reveal_type(EnumWithSubclassOfEnumMetaMetaclass.NO)  # revealed: Literal[EnumWithSubclassOfEnumMetaMetaclass.NO]

# Attributes `.value` and `.name` can *not* be accessed on members of these enums:

# error: [unresolved-attribute]
EnumWithSubclassOfEnumMetaMetaclass.NO.value
# error: [unresolved-attribute]
EnumWithSubclassOfEnumMetaMetaclass.NO.name

# But the internal underscore attributes are available:

reveal_type(EnumWithSubclassOfEnumMetaMetaclass.NO._value_)  # revealed: Any
reveal_type(EnumWithSubclassOfEnumMetaMetaclass.NO._name_)  # revealed: Literal["NO"]

def _(x: EnumWithSubclassOfEnumMetaMetaclass):
    # error: [unresolved-attribute]
    x.value
    # error: [unresolved-attribute]
    x.name
    reveal_type(x._value_)  # revealed: Any
    reveal_type(x._name_)  # revealed: Literal["NO", "YES"]
```

Open `EnumMeta`-based classes still reject ordinary calls until they are finalized with members:

```py
from enum import EnumMeta

class Meta(EnumMeta): ...
class Empty(metaclass=Meta): ...

# error: [too-many-positional-arguments]
Empty(1)
```

### Enums with (subclasses of) `EnumType` as metaclass

In Python 3.11, the meta-type was renamed to `EnumType`.

```toml
[environment]
python-version = "3.11"
```

On Python 3.11+, open `EnumMeta`-based classes also accept the functional-enum calling convention,
though the inferred result is still imprecise:

```py
from enum import EnumMeta

class Meta(EnumMeta): ...
class Empty(metaclass=Meta): ...

# TODO: runtime MRO suggests this should be closer to `type[Empty]`.
reveal_type(Empty("Dynamic", {"X": 1}))  # revealed: type[Enum]
```

```py
from enum import Enum, EnumType

class EnumWithEnumMetaMetaclass(metaclass=EnumType):
    def __init__(self, val): ...
    NO = 0
    YES = 1

reveal_type(EnumWithEnumMetaMetaclass.NO)  # revealed: Literal[EnumWithEnumMetaMetaclass.NO]

class SubclassOfEnumMeta(EnumType): ...

class EnumWithSubclassOfEnumMetaMetaclass(metaclass=SubclassOfEnumMeta):
    def __init__(self, val): ...
    NO = 0
    YES = 1

reveal_type(EnumWithSubclassOfEnumMetaMetaclass.NO)  # revealed: Literal[EnumWithSubclassOfEnumMetaMetaclass.NO]

# Attributes `.value` and `.name` can *not* be accessed on members of these enums:

# error: [unresolved-attribute]
EnumWithSubclassOfEnumMetaMetaclass.NO.value
# error: [unresolved-attribute]
EnumWithSubclassOfEnumMetaMetaclass.NO.name

# But the internal underscore attributes are available:

reveal_type(EnumWithSubclassOfEnumMetaMetaclass.NO._value_)  # revealed: Any
reveal_type(EnumWithSubclassOfEnumMetaMetaclass.NO._name_)  # revealed: Literal["NO"]

def _(x: EnumWithSubclassOfEnumMetaMetaclass):
    # error: [unresolved-attribute]
    x.value
    # error: [unresolved-attribute]
    x.name
    reveal_type(x._value_)  # revealed: Any
    reveal_type(x._name_)  # revealed: Literal["NO", "YES"]
```

## Function syntax

### String names (positional)

```py
from enum import Enum
from ty_extensions import enum_members

Color = Enum("Color", "RED GREEN BLUE")

# revealed: tuple[Literal["RED"], Literal["GREEN"], Literal["BLUE"]]
reveal_type(enum_members(Color))

Color = Enum("Color", "RED, GREEN, BLUE")

# revealed: tuple[Literal["RED"], Literal["GREEN"], Literal["BLUE"]]
reveal_type(enum_members(Color))
```

### String names (keyword)

```py
from enum import Enum
from ty_extensions import enum_members

Color = Enum("Color", names="RED GREEN BLUE")

# revealed: tuple[Literal["RED"], Literal["GREEN"], Literal["BLUE"]]
reveal_type(enum_members(Color))
```

### Name mismatch diagnostics

The name passed to `Enum` must match the variable it is assigned to:

```py
from enum import Enum

GoodMatch1 = Enum("GoodMatch1", "A B")  # fine

name = "GoodMatch2"
GoodMatch2 = Enum(name, "A B")  # also fine
```

If there is a mitmatch, we emit the following diagnostic:

```py
# snapshot: mismatched-type-name
Mismatch = Enum("WrongName", "A B")
```

```snapshot
warning[mismatched-type-name]: The name passed to `Enum` must match the variable it is assigned to
 --> src/mdtest_snippet.py:8:17
  |
8 | Mismatch = Enum("WrongName", "A B")
  |                 ^^^^^^^^^^^ Expected "Mismatch", got "WrongName"
  |
```

If the name is not a string literal, we also emit a diagnostic:

```py
def f(name: str) -> None:
    # snapshot: mismatched-type-name
    DynamicMismatch = Enum(name, "A B")
```

```snapshot
warning[mismatched-type-name]: The name passed to `Enum` must match the variable it is assigned to
  --> src/mdtest_snippet.py:11:28
   |
11 |     DynamicMismatch = Enum(name, "A B")
   |                            ^^^^ Expected "DynamicMismatch", got variable of type `str`
   |
```

### List/tuple of tuples

```py
from enum import Enum
from ty_extensions import enum_members

Color = Enum("Color", [("RED", 1), ("GREEN", 2), ("BLUE", 3)])

# revealed: tuple[Literal["RED"], Literal["GREEN"], Literal["BLUE"]]
reveal_type(enum_members(Color))

Color = Enum("Color", (("RED", 1), ("GREEN", 2), ("BLUE", 3)))

# revealed: tuple[Literal["RED"], Literal["GREEN"], Literal["BLUE"]]
reveal_type(enum_members(Color))
```

### List of strings

```py
from enum import Enum
from ty_extensions import enum_members

Color = Enum("Color", ["RED", "GREEN", "BLUE"])

# revealed: tuple[Literal["RED"], Literal["GREEN"], Literal["BLUE"]]
reveal_type(enum_members(Color))
```

### Dict mapping

```py
from enum import Enum
from ty_extensions import enum_members

Color = Enum("Color", {"RED": 1, "GREEN": 2, "BLUE": 3})

# revealed: tuple[Literal["RED"], Literal["GREEN"], Literal["BLUE"]]
reveal_type(enum_members(Color))

reveal_type(Color.RED.value)  # revealed: Literal[1]
reveal_type(Color.GREEN.value)  # revealed: Literal[2]
reveal_type(Color.BLUE.value)  # revealed: Literal[3]
```

### Dict mapping with `auto()`

```py
from enum import Enum, auto
from ty_extensions import enum_members

Color = Enum("Color", {"RED": auto(), "GREEN": auto(), "BLUE": auto()})

# revealed: tuple[Literal["RED"], Literal["GREEN"], Literal["BLUE"]]
reveal_type(enum_members(Color))

reveal_type(Color.RED.value)  # revealed: Literal[1]
reveal_type(Color.GREEN.value)  # revealed: Literal[2]
reveal_type(Color.BLUE.value)  # revealed: Literal[3]
```

When mixing explicit values with `auto()` in a dict, the auto value is derived from the previous
member's value, not from `start + index`:

```py
from enum import Enum, auto
from ty_extensions import enum_members

Mixed = Enum("Mixed", {"A": 10, "B": auto(), "C": auto()})

# revealed: tuple[Literal["A"], Literal["B"], Literal["C"]]
reveal_type(enum_members(Mixed))

reveal_type(Mixed.A.value)  # revealed: Literal[10]
reveal_type(Mixed.B.value)  # revealed: Literal[11]
reveal_type(Mixed.C.value)  # revealed: Literal[12]
```

This also applies when the previous value is a bool literal:

```py
from enum import Enum, auto

AfterFalse = Enum("AfterFalse", {"A": False, "B": auto()})
reveal_type(AfterFalse.B.value)  # revealed: Literal[1]

AfterTrue = Enum("AfterTrue", {"A": True, "B": auto()})
reveal_type(AfterTrue.B.value)  # revealed: Literal[2]
```

### `auto()` in tuple/list entries

`auto()` should also expand in tuple/list entry forms of the functional syntax:

```py
from enum import Enum, Flag, auto

Color = Enum("Color", [("RED", auto()), ("GREEN", auto())])

reveal_type(Color.RED.value)  # revealed: Literal[1]
reveal_type(Color.GREEN.value)  # revealed: Literal[2]

Perm = Flag("Perm", (("READ", auto()), ("WRITE", auto())))

reveal_type(Perm.READ.value)  # revealed: Literal[1]
reveal_type(Perm.WRITE.value)  # revealed: Literal[2]
```

Explicit-value forms should ignore `start`, just like static enums do:

```py
from enum import Enum, Flag, auto

Color = Enum("Color", [("RED", auto()), ("GREEN", auto())], start=3)

reveal_type(Color.RED.value)  # revealed: Literal[1]
reveal_type(Color.GREEN.value)  # revealed: Literal[2]

Mapped = Enum("Mapped", {"RED": auto(), "GREEN": auto()}, start=3)

reveal_type(Mapped.RED.value)  # revealed: Literal[1]
reveal_type(Mapped.GREEN.value)  # revealed: Literal[2]

Perm = Flag("Perm", (("READ", auto()), ("WRITE", auto())), start=3)

reveal_type(Perm.READ.value)  # revealed: Literal[1]
reveal_type(Perm.WRITE.value)  # revealed: Literal[2]
```

### Duplicate member names

Duplicate member names raise `TypeError` at runtime. We degrade to unknown members rather than
synthesizing a broken enum.

```py
from enum import Enum
from ty_extensions import enum_members

E1 = Enum("E1", "A A")
reveal_type(enum_members(E1))  # revealed: Unknown

E2 = Enum("E2", ["A", "A"])
reveal_type(enum_members(E2))  # revealed: Unknown

E3 = Enum("E3", [("A", 1), ("A", 2)])
reveal_type(enum_members(E3))  # revealed: Unknown
```

### Unknown members: inherited attribute access

When members are unknown, own member access returns `Unknown`, but inherited attributes from the
enum base class should still resolve through the MRO.

```py
from enum import Enum
from ty_extensions import enum_members

def f(
    names: list[str],
    labels: str,
    pairs: tuple[tuple[str, int], ...],
    mapping: dict[str, int],
    name: str,
    key: str,
) -> None:
    E1 = Enum("E1", names)
    E2 = Enum("E2", labels)
    E3 = Enum("E3", pairs)
    E4 = Enum("E4", mapping)
    E5 = Enum("E5", ["A", name])
    E6 = Enum("E6", [(name, 1)])
    E7 = Enum("E7", {key: 1})

    reveal_type(enum_members(E1))  # revealed: Unknown
    reveal_type(enum_members(E2))  # revealed: Unknown
    reveal_type(enum_members(E3))  # revealed: Unknown
    reveal_type(enum_members(E4))  # revealed: Unknown
    reveal_type(enum_members(E5))  # revealed: Unknown
    reveal_type(enum_members(E6))  # revealed: Unknown
    reveal_type(enum_members(E7))  # revealed: Unknown

    # Inherited class attributes resolve from Enum base.
    reveal_type(E1.__members__)  # revealed: MappingProxyType[str, E1]

    # But own member access is unknown.
    reveal_type(E1.FOO)  # revealed: Unknown
```

### Too many positional args

`Enum(value, names, *, ...)` only accepts two positional args at runtime.

```py
from enum import Enum
from ty_extensions import enum_members

# error: [too-many-positional-arguments]
Color = Enum("Color", "RED", "GREEN", "BLUE")

reveal_type(enum_members(Color))  # revealed: Unknown
```

### Duplicate positional and keyword arguments

Passing the same functional-enum parameter both positionally and by keyword should still report the
usual duplicate-argument diagnostic:

```py
from enum import Enum
from ty_extensions import enum_members

# error: [parameter-already-assigned]
Color = Enum("Color", "RED", names="BLUE")

reveal_type(enum_members(Color))  # revealed: Unknown
```

### No positional args

```py
from enum import Enum

# This is invalid at runtime but should not panic.
Color = Enum()

reveal_type(Color)  # revealed: Enum
```

### Missing `names` argument

```py
from enum import Enum

# This is invalid at runtime but should not panic.
Enum("Color")  # error: [missing-argument]

# This is invalid at runtime but should not panic.
Enum(value="Color")  # error: [missing-argument]

# error: [missing-argument]
# error: [invalid-argument-type]
Enum(123)

# error: [missing-argument]
# error: [invalid-argument-type]
Enum(value=123)

# error: [missing-argument]
# error: [invalid-argument-type]
Enum("Color", start="0")

# error: [missing-argument]
# error: [invalid-argument-type]
Enum("Color", type=1)

# error: [missing-argument]
# error: [unknown-argument]
Enum("Color", bad_kwarg=True)
```

### Non-literal name

Non-literal names should still be recognized as creating an enum class.

```py
from enum import Enum

def make_enum(name: str) -> type[Enum]:
    # error: [mismatched-type-name]
    result = Enum(name.title(), "RED BLUE", module=__name__)
    reveal_type(result)  # revealed: type[Enum]
    return result

def validate_other_args(name: str) -> None:
    # error: [invalid-argument-type]
    Enum(name, "RED", start="0")

    # error: [invalid-argument-type]
    Enum(name, "RED", type=1)
```

### Non-string name

```py
from enum import Enum

# error: [invalid-argument-type]
Color = Enum(123, "RED GREEN BLUE")
```

### Unknown keyword arguments

```py
from enum import Enum

# error: [unknown-argument]
Color = Enum("Color", "RED GREEN BLUE", bad_kwarg=True)
```

### Definitely invalid `names` arguments

Functional enums should still reject obviously invalid `names` values:

```py
from enum import Enum
from ty_extensions import enum_members

# error: [invalid-argument-type]
Color = Enum("Color", 123)

reveal_type(enum_members(Color))  # revealed: Unknown
```

Empty functional enums are valid, even though they have no members:

```py
from enum import Enum
from ty_extensions import enum_members

EmptyFromString = Enum("EmptyFromString", "")
EmptyFromList = Enum("EmptyFromList", [])
EmptyFromDict = Enum("EmptyFromDict", {})

reveal_type(enum_members(EmptyFromString))  # revealed: tuple[()]
reveal_type(enum_members(EmptyFromList))  # revealed: tuple[()]
reveal_type(enum_members(EmptyFromDict))  # revealed: tuple[()]

class ExtendedEmpty(EmptyFromString):
    A = 1

# revealed: tuple[Literal["A"]]
reveal_type(enum_members(ExtendedEmpty))
```

Literal list/tuple/dict inputs that use unpacking are rejected:

```py
from enum import Enum

names: list[str] = ["B"]
pairs: list[tuple[str, int]] = [("B", 2)]
more: dict[str, int] = {"B": 2}
bad_keys: dict[int, int] = {1: 2}

# error: [invalid-argument-type]
Enum("FromNames", ["A", *names])

# error: [invalid-argument-type]
Enum("FromPairs", [("A", 1), *pairs])

# error: [invalid-argument-type]
Enum("FromMapping", {"A": 1, **more})

# error: [invalid-argument-type]
Enum("BadDoubleStar", {**bad_keys})
```

### Keyword argument type validation

Functional enum construction should still preserve overload-based argument validation:

```py
from enum import Enum

# error: [invalid-argument-type]
Color = Enum("Color", "RED", start="0")

reveal_type(Color.RED.value)  # revealed: Literal[1]
```

### `boundary` keyword (Python 3.11+)

#### Available on 3.11+

```toml
[environment]
python-version = "3.11"
```

```py
from enum import Flag

Perm = Flag("Perm", "READ WRITE EXECUTE", boundary=None)
```

#### Rejected before 3.11

```toml
[environment]
python-version = "3.10"
```

```py
from enum import Flag

# error: [unknown-argument]
Perm = Flag("Perm", "READ WRITE EXECUTE", boundary=None)
```

### StrEnum function syntax

```toml
[environment]
python-version = "3.11"
```

```py
from enum import StrEnum
from ty_extensions import enum_members

Color = StrEnum("Color", "RED GREEN BLUE")

# revealed: tuple[Literal["RED"], Literal["GREEN"], Literal["BLUE"]]
reveal_type(enum_members(Color))

reveal_type(Color.RED.value)  # revealed: Literal["red"]
reveal_type(Color.GREEN.value)  # revealed: Literal["green"]
reveal_type(Color.BLUE.value)  # revealed: Literal["blue"]
```

### Custom start value

```py
from enum import Enum, Flag

Color = Enum("Color", "RED GREEN BLUE", start=0)

reveal_type(Color.RED.value)  # revealed: Literal[0]
reveal_type(Color.GREEN.value)  # revealed: Literal[1]
reveal_type(Color.BLUE.value)  # revealed: Literal[2]

Perm = Flag("Perm", "READ WRITE EXECUTE", start=3)

reveal_type(Perm.READ.value)  # revealed: Literal[3]
reveal_type(Perm.WRITE.value)  # revealed: Literal[4]
reveal_type(Perm.EXECUTE.value)  # revealed: Literal[8]
```

Non-literal integer `start` values should widen member values to `int` rather than pretending the
default `start=1` was used:

```py
from enum import Enum, Flag

def make(n: int) -> None:
    Color = Enum("Color", "RED GREEN", start=n)

    reveal_type(Color.RED.value)  # revealed: int
    reveal_type(Color.GREEN.value)  # revealed: int

    Perm = Flag("Perm", "READ WRITE", start=n)

    reveal_type(Perm.READ.value)  # revealed: int
    reveal_type(Perm.WRITE.value)  # revealed: int
```

### Type mixin

```py
from enum import Enum, auto
from ty_extensions import enum_members

Http = Enum("Http", "OK NOT_FOUND", type=int)

reveal_type(Http.OK)  # revealed: Literal[Http.OK]
reveal_type(Http.OK.value)  # revealed: Literal[1]
reveal_type(Http.NOT_FOUND.value)  # revealed: Literal[2]

# revealed: tuple[Literal["OK"], Literal["NOT_FOUND"]]
reveal_type(enum_members(Http))

StringyNames = Enum("StringyNames", "A B", type=str)
BytesyNames = Enum("BytesyNames", "A B", type=bytes)
FloatyNames = Enum("FloatyNames", "A B", type=float)

reveal_type(StringyNames.A.value)  # revealed: Literal["1"]
reveal_type(StringyNames.B.value)  # revealed: Literal["2"]
reveal_type(BytesyNames.A.value)  # revealed: bytes
reveal_type(BytesyNames.B.value)  # revealed: bytes
reveal_type(FloatyNames.A.value)  # revealed: float
reveal_type(FloatyNames.B.value)  # revealed: float

# revealed: tuple[Literal["A"], Literal["B"]]
reveal_type(enum_members(StringyNames))
# revealed: tuple[Literal["A"], Literal["B"]]
reveal_type(enum_members(BytesyNames))
# revealed: tuple[Literal["A"], Literal["B"]]
reveal_type(enum_members(FloatyNames))

Parsed = Enum("Parsed", {"A": "1"}, type=int)
Stringy = Enum("Stringy", {"A": "1", "B": auto()}, type=str)

reveal_type(enum_members(Parsed))  # revealed: Unknown
reveal_type(enum_members(Stringy))  # revealed: Unknown

class Prefixed(str):
    pass

CustomNames = Enum("CustomNames", "A B", type=Prefixed)
Custom = Enum("Custom", {"A": "1"}, type=Prefixed)

reveal_type(enum_members(CustomNames))  # revealed: Unknown
reveal_type(enum_members(Custom))  # revealed: Unknown
```

Functional enums should still validate `type=` arguments eagerly, both for obvious non-types and for
bases that are structurally invalid to combine with `Enum`:

```py
from enum import Enum
from typing import TypedDict
from ty_extensions import reveal_mro

# error: [invalid-argument-type]
BadType = Enum("BadType", "RED", type=1)

# error: [invalid-argument-type]
BadStringType = Enum("BadStringType", "RED", type="Mixin")

TD = TypedDict("TD", {"x": int})

# error: [invalid-base]
BadBase = Enum("BadBase", "RED", type=TD)

reveal_mro(BadBase)  # revealed: (<class 'BadBase'>, <class 'Enum'>, <class 'object'>)
```

Mixins that are incompatible with the enum base should still report an error and avoid exposing a
precise member set:

```py
from enum import IntEnum, IntFlag
from ty_extensions import enum_members

# error: [invalid-base]
BadIntEnum = IntEnum("BadIntEnum", "RED", type=str)

# error: [invalid-base]
BadIntFlag = IntFlag("BadIntFlag", "RED", type=float)

reveal_type(enum_members(BadIntEnum))  # revealed: Unknown
reveal_type(enum_members(BadIntFlag))  # revealed: Unknown
```

Functional enums with a `type=` mixin should also have the same MRO as the equivalent static enum
class:

```py
from enum import Enum
from ty_extensions import reveal_mro

Http = Enum("Http", "OK NOT_FOUND", type=int)

reveal_mro(Http)  # revealed: (<class 'Http'>, <class 'int'>, <class 'Enum'>, <class 'object'>)

class StaticHttp(int, Enum):
    OK = 1
    NOT_FOUND = 2

reveal_mro(StaticHttp)  # revealed: (<class 'StaticHttp'>, <class 'int'>, <class 'Enum'>, <class 'object'>)
```

### IntEnum function syntax

```py
from enum import IntEnum
from ty_extensions import enum_members

Color = IntEnum("Color", "RED GREEN BLUE")

# revealed: tuple[Literal["RED"], Literal["GREEN"], Literal["BLUE"]]
reveal_type(enum_members(Color))
```

### Flag function syntax

```py
from enum import Flag
from ty_extensions import enum_members

Perm = Flag("Perm", "READ WRITE EXECUTE")

# revealed: tuple[Literal["READ"], Literal["WRITE"], Literal["EXECUTE"]]
reveal_type(enum_members(Perm))

reveal_type(Perm.READ.value)  # revealed: Literal[1]
reveal_type(Perm.WRITE.value)  # revealed: Literal[2]
reveal_type(Perm.EXECUTE.value)  # revealed: Literal[4]
```

### IntFlag function syntax

```py
from enum import IntFlag
from ty_extensions import enum_members

Perm = IntFlag("Perm", "READ WRITE EXECUTE")

# revealed: tuple[Literal["READ"], Literal["WRITE"], Literal["EXECUTE"]]
reveal_type(enum_members(Perm))

reveal_type(Perm.READ.value)  # revealed: Literal[1]
reveal_type(Perm.WRITE.value)  # revealed: Literal[2]
reveal_type(Perm.EXECUTE.value)  # revealed: Literal[4]
```

### Large start value (overflow guard)

Values that would overflow `i64` should gracefully widen to `int`.

```py
from enum import Enum, Flag

Big = Enum("Big", "A B", start=9223372036854775807)

reveal_type(Big.A.value)  # revealed: Literal[9223372036854775807]
reveal_type(Big.B.value)  # revealed: int

BigFlag = Flag("BigFlag", "X Y", start=4611686018427387904)

reveal_type(BigFlag.X.value)  # revealed: Literal[4611686018427387904]
reveal_type(BigFlag.Y.value)  # revealed: int
```

### Accessing members from instances

```py
from enum import Enum

Answer = Enum("Answer", "YES NO")

reveal_type(Answer.YES.NO)  # revealed: Literal[Answer.NO]

def _(answer: Answer) -> None:
    reveal_type(answer.YES)  # revealed: Literal[Answer.YES]
    reveal_type(answer.NO)  # revealed: Literal[Answer.NO]
```

### Accessing members from `type[…]`

```py
from enum import Enum

Answer = Enum("Answer", "YES NO")

def _(answer: type[Answer]) -> None:
    reveal_type(answer.YES)  # revealed: Literal[Answer.YES]
    reveal_type(answer.NO)  # revealed: Literal[Answer.NO]
```

### Implicitly final

Functional enums with members should also be implicitly final:

```py
from enum import Enum

Color = Enum("Color", "RED GREEN BLUE")

# error: [subclass-of-final-class]
class ExtendedColor(Color):
    YELLOW = 4
```

### Meta-type

```py
from enum import Enum

Answer = Enum("Answer", "YES NO")

reveal_type(type(Answer.YES))  # revealed: <class 'Answer'>

def _(answer: Answer):
    reveal_type(type(answer))  # revealed: <class 'Answer'>
```

## Exhaustiveness checking

## `if` statements

```py
from enum import Enum
from typing_extensions import assert_never

class Color(Enum):
    RED = 1
    GREEN = 2
    BLUE = 3

def color_name(color: Color) -> str:
    if color is Color.RED:
        return "Red"
    elif color is Color.GREEN:
        return "Green"
    elif color is Color.BLUE:
        return "Blue"
    else:
        assert_never(color)

# No `invalid-return-type` error here because the implicit `else` branch is detected as unreachable:
def color_name_without_assertion(color: Color) -> str:
    if color is Color.RED:
        return "Red"
    elif color is Color.GREEN:
        return "Green"
    elif color is Color.BLUE:
        return "Blue"

def color_name_misses_one_variant(color: Color) -> str:
    if color is Color.RED:
        return "Red"
    elif color is Color.GREEN:
        return "Green"
    else:
        assert_never(color)  # error: [type-assertion-failure] "Type `Literal[Color.BLUE]` is not equivalent to `Never`"

class Singleton(Enum):
    VALUE = 1

def singleton_check(value: Singleton) -> str:
    if value is Singleton.VALUE:
        return "Singleton value"
    else:
        assert_never(value)
```

## `match` statements

```toml
[environment]
python-version = "3.10"
```

```py
from enum import Enum
from typing_extensions import assert_never

class Color(Enum):
    RED = 1
    GREEN = 2
    BLUE = 3

def color_name(color: Color) -> str:
    match color:
        case Color.RED:
            return "Red"
        case Color.GREEN:
            return "Green"
        case Color.BLUE:
            return "Blue"
        case _:
            assert_never(color)

def color_name_without_assertion(color: Color) -> str:
    match color:
        case Color.RED:
            return "Red"
        case Color.GREEN:
            return "Green"
        case Color.BLUE:
            return "Blue"

def color_name_misses_one_variant(color: Color) -> str:
    match color:
        case Color.RED:
            return "Red"
        case Color.GREEN:
            return "Green"
        case _:
            assert_never(color)  # error: [type-assertion-failure] "Type `Literal[Color.BLUE]` is not equivalent to `Never`"

class Singleton(Enum):
    VALUE = 1

def singleton_check(value: Singleton) -> str:
    match value:
        case Singleton.VALUE:
            return "Singleton value"
        case _:
            assert_never(value)
```

## `if` statements (function syntax)

```py
from enum import Enum
from typing_extensions import assert_never

Color = Enum("Color", "RED GREEN BLUE")

def color_name(color: Color) -> str:
    if color is Color.RED:
        return "Red"
    elif color is Color.GREEN:
        return "Green"
    elif color is Color.BLUE:
        return "Blue"
    else:
        assert_never(color)

def color_name_without_assertion(color: Color) -> str:
    if color is Color.RED:
        return "Red"
    elif color is Color.GREEN:
        return "Green"
    elif color is Color.BLUE:
        return "Blue"

def color_name_misses_one_variant(color: Color) -> str:
    if color is Color.RED:
        return "Red"
    elif color is Color.GREEN:
        return "Green"
    else:
        assert_never(color)  # error: [type-assertion-failure] "Type `Literal[Color.BLUE]` is not equivalent to `Never`"
```

## `match` statements (function syntax)

TODO: `match` exhaustiveness does not yet work for functional enums. The pattern matching narrowing
path does not resolve functional enum members the same way `is` comparisons do.

```toml
[environment]
python-version = "3.10"
```

```py
from enum import Enum
from typing_extensions import assert_never

Color = Enum("Color", "RED GREEN BLUE")

# TODO: `assert_never` should not fire here (exhaustive match).
def color_name(color: Color) -> str:
    match color:
        case Color.RED:
            return "Red"
        case Color.GREEN:
            return "Green"
        case Color.BLUE:
            return "Blue"
        case _:
            assert_never(color)  # error: [type-assertion-failure]

# TODO: This should ideally emit `Literal[Color.BLUE]` in the assertion, not `Color`.
def color_name_misses_one_variant(color: Color) -> str:
    match color:
        case Color.RED:
            return "Red"
        case Color.GREEN:
            return "Green"
        case _:
            assert_never(color)  # error: [type-assertion-failure] "Type `Color` is not equivalent to `Never`"
```

## `__eq__` and `__ne__`

### No `__eq__` or `__ne__` overrides

```py
from enum import Enum

class Color(Enum):
    RED = 1
    GREEN = 2

reveal_type(Color.RED == Color.RED)  # revealed: Literal[True]
reveal_type(Color.RED != Color.RED)  # revealed: Literal[False]
```

### Overridden `__eq__`

```py
from enum import Enum

class Color(Enum):
    RED = 1
    GREEN = 2

    def __eq__(self, other: object) -> bool:
        return False

reveal_type(Color.RED == Color.RED)  # revealed: bool
```

### Overridden `__ne__`

```py
from enum import Enum

class Color(Enum):
    RED = 1
    GREEN = 2

    def __ne__(self, other: object) -> bool:
        return False

reveal_type(Color.RED != Color.RED)  # revealed: bool
```

## Generic enums are invalid

Enum classes cannot be generic. Python does not support generic enums, and attempting to create one
will result in a `TypeError` at runtime.

### PEP 695 syntax

Using PEP 695 type parameters on an enum is invalid:

```toml
[environment]
python-version = "3.12"
```

```py
from enum import Enum

# error: [invalid-generic-enum] "Enum class `E` cannot be generic"
class E[T](Enum):
    A = 1
    B = 2
```

### Legacy `Generic` base class

Inheriting from both `Enum` and `Generic[T]` is also invalid:

```py
from enum import Enum
from typing import Generic, TypeVar

T = TypeVar("T")

# error: [invalid-generic-enum] "Enum class `F` cannot be generic"
class F(Enum, Generic[T]):
    A = 1
    B = 2
```

### Swapped order (`Generic` first)

The order of bases doesn't matter; it's still invalid:

```py
from enum import Enum
from typing import Generic, TypeVar

T = TypeVar("T")

# error: [invalid-generic-enum] "Enum class `G` cannot be generic"
class G(Generic[T], Enum):
    A = 1
    B = 2
```

### Enum subclasses

Subclasses of enum base classes also cannot be generic:

```toml
[environment]
python-version = "3.12"
```

```py
from enum import Enum, IntEnum
from typing import Generic, TypeVar

T = TypeVar("T")

# error: [invalid-generic-enum] "Enum class `MyIntEnum` cannot be generic"
class MyIntEnum[T](IntEnum):
    A = 1

# error: [invalid-generic-enum] "Enum class `MyFlagEnum` cannot be generic"
class MyFlagEnum(IntEnum, Generic[T]):
    A = 1
```

### Custom enum base class

Even with custom enum subclasses that don't have members, they cannot be made generic:

```toml
[environment]
python-version = "3.12"
```

```py
from enum import Enum
from typing import Generic, TypeVar

T = TypeVar("T")

class MyEnumBase(Enum):
    def some_method(self) -> None: ...

# error: [invalid-generic-enum] "Enum class `MyEnum` cannot be generic"
class MyEnum[T](MyEnumBase):
    A = 1
```

## Constructor signature

```toml
[environment]
python-version = "3.11"
```

The constructor of an enum takes a single `value` argument and returns the enum member corresponding
to that value:

```py
from enum import Enum, IntEnum, StrEnum
from ty_extensions import into_regular_callable

class Color(Enum):
    RED = 1
    BLUE = 2

# revealed: (value: object) -> Color
reveal_type(into_regular_callable(Color))

class Priority(IntEnum):
    HIGH = 1
    LOW = 2

# revealed: (value: int) -> Priority
reveal_type(into_regular_callable(Priority))

class Answer(StrEnum):
    YES = "yes"
    NO = "no"

# revealed: (value: str) -> Answer
reveal_type(into_regular_callable(Answer))
```

The signature of `Enum`, `IntEnum`, and `StrEnum` is defined by `EnumMeta.__call__`, which allows
dynamic construction of enums using the functional syntax:

```py
from enum import Enum, IntEnum, StrEnum
from ty_extensions import into_regular_callable

# revealed: Overload[[_EnumMemberT](value: Any, names: None = None) -> _EnumMemberT, (value: str, names: Iterable[Iterable[str | Any]], *, module: str | None = None, qualname: str | None = None, type: type | None = None, start: int = 1, boundary: FlagBoundary | None = None) -> type[Enum]]
reveal_type(into_regular_callable(Enum))

# revealed: Overload[[_EnumMemberT](value: Any, names: None = None) -> _EnumMemberT, (value: str, names: Iterable[Iterable[str | Any]], *, module: str | None = None, qualname: str | None = None, type: type | None = None, start: int = 1, boundary: FlagBoundary | None = None) -> type[Enum]]
reveal_type(into_regular_callable(IntEnum))

# revealed: Overload[[_EnumMemberT](value: Any, names: None = None) -> _EnumMemberT, (value: str, names: Iterable[Iterable[str | Any]], *, module: str | None = None, qualname: str | None = None, type: type | None = None, start: int = 1, boundary: FlagBoundary | None = None) -> type[Enum]]
reveal_type(into_regular_callable(StrEnum))
```

## References

- Typing spec: <https://typing.python.org/en/latest/spec/enums.html>
- Documentation: <https://docs.python.org/3/library/enum.html>

[class-private names]: https://docs.python.org/3/reference/lexical_analysis.html#reserved-classes-of-identifiers
[typing spec]: https://typing.python.org/en/latest/spec/enums.html#enum-members
