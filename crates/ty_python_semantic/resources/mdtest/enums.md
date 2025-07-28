# Enums

## Basic

```py
from enum import Enum

class Color(Enum):
    RED = 1
    GREEN = 2
    BLUE = 3

reveal_type(Color.RED)  # revealed: Literal[Color.RED]
# TODO: This could be `Literal[1]`
reveal_type(Color.RED.value)  # revealed: Any

# TODO: Should be `Color` or `Literal[Color.RED]`
reveal_type(Color["RED"])  # revealed: Unknown

# TODO: Could be `Literal[Color.RED]` to be more precise
reveal_type(Color(1))  # revealed: Color

reveal_type(Color.RED in Color)  # revealed: bool
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

### Declared non-member attributes

Attributes on the enum class that are declared are not considered members of the enum:

```py
from enum import Enum
from ty_extensions import enum_members

class Answer(Enum):
    YES = 1
    NO = 2

    non_member_1: int

    # TODO: this could be considered an error:
    non_member_1: str = "some value"

# revealed: tuple[Literal["YES"], Literal["NO"]]
reveal_type(enum_members(Answer))
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

### Using `auto()`

```py
from enum import Enum, auto
from ty_extensions import enum_members

class Answer(Enum):
    YES = auto()
    NO = auto()

# revealed: tuple[Literal["YES"], Literal["NO"]]
reveal_type(enum_members(Answer))
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

### Class-private names

An attribute with a [class-private name] (beginning with, but not ending in, a double underscore) is
treated as a non-member:

```py
from enum import Enum
from ty_extensions import enum_members

class Answer(Enum):
    YES = 1
    NO = 2

    __private_member = 3
    __maybe__ = 4

# revealed: tuple[Literal["YES"], Literal["NO"], Literal["__maybe__"]]
reveal_type(enum_members(Answer))
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
    constant: int = 1

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
    NO = 0
    YES = 1

reveal_type(EnumWithEnumMetaMetaclass.NO)  # revealed: Literal[EnumWithEnumMetaMetaclass.NO]

class SubclassOfEnumMeta(EnumMeta): ...

class EnumWithSubclassOfEnumMetaMetaclass(metaclass=SubclassOfEnumMeta):
    NO = 0
    YES = 1

reveal_type(EnumWithSubclassOfEnumMetaMetaclass.NO)  # revealed: Literal[EnumWithSubclassOfEnumMetaMetaclass.NO]

# Attributes like `.value` can *not* be accessed on members of these enums:
# error: [unresolved-attribute]
EnumWithSubclassOfEnumMetaMetaclass.NO.value
```

### Enums with (subclasses of) `EnumType` as metaclass

```toml
[environment]
python-version = "3.11"
```

```py
from enum import Enum, EnumType

class EnumWithEnumMetaMetaclass(metaclass=EnumType):
    NO = 0
    YES = 1

reveal_type(EnumWithEnumMetaMetaclass.NO)  # revealed: Literal[EnumWithEnumMetaMetaclass.NO]

class SubclassOfEnumMeta(EnumType): ...

class EnumWithSubclassOfEnumMetaMetaclass(metaclass=SubclassOfEnumMeta):
    NO = 0
    YES = 1

reveal_type(EnumWithSubclassOfEnumMetaMetaclass.NO)  # revealed: Literal[EnumWithSubclassOfEnumMetaMetaclass.NO]

# error: [unresolved-attribute]
EnumWithSubclassOfEnumMetaMetaclass.NO.value
```

## Function syntax

To do: <https://typing.python.org/en/latest/spec/enums.html#enum-definition>

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
        assert_never(color)  # error: [type-assertion-failure] "Argument does not have asserted type `Never`"

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
            assert_never(color)  # error: [type-assertion-failure] "Argument does not have asserted type `Never`"

class Singleton(Enum):
    VALUE = 1

def singleton_check(value: Singleton) -> str:
    match value:
        case Singleton.VALUE:
            return "Singleton value"
        case _:
            assert_never(value)
```

## References

- Typing spec: <https://typing.python.org/en/latest/spec/enums.html>
- Documentation: <https://docs.python.org/3/library/enum.html>

[class-private name]: https://docs.python.org/3/reference/lexical_analysis.html#reserved-classes-of-identifiers
