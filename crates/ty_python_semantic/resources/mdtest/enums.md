# Enums

## Basic

```py
from enum import Enum

class Color(Enum):
    RED = 1
    GREEN = 2
    BLUE = 3

reveal_type(Color.RED)  # revealed: @Todo(Attribute access on enum classes)
reveal_type(Color.RED.value)  # revealed: @Todo(Attribute access on enum classes)

# TODO: Should be `Color` or `Literal[Color.RED]`
reveal_type(Color["RED"])  # revealed: Unknown

# TODO: Should be `Color` or `Literal[Color.RED]`
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

Attributes on the enum class that are declared are not considered members of the enum. Similarly,
methods are not considered members of the enum:

```py
from enum import Enum
from ty_extensions import enum_members

class Answer(Enum):
    YES = 1
    NO = 2

    non_member_1: int

    # TODO: this could be considered an error:
    non_member_1: str = "some value"

    def some_method(self) -> None: ...
    @property
    def some_property(self) -> str:
        return ""

    class Nested: ...

# revealed: tuple[Literal["YES"], Literal["NO"]]
reveal_type(enum_members(Answer))
```

### Non-member attributes with disallowed type

Some attributes are not considered members based on their type:

```py
from enum import Enum
from ty_extensions import enum_members

def identity(x) -> int:
    return x

class Answer(Enum):
    YES = 1
    NO = 2

    non_member = lambda x: 0
    non_member_2 = identity

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

### Private names

An attribute with a private name (beginning with, but not ending in, a double underscore) is treated
as a non-member:

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
space-delimited list of names:

```py
from enum import Enum
from ty_extensions import enum_members

class Answer(Enum):
    _ignore_ = "MAYBE _other"

    YES = 1
    NO = 2

    MAYBE = 3
    _other = "test"

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

## Iterating over enum members

```py
from enum import Enum

class Color(Enum):
    RED = 1
    GREEN = 2
    BLUE = 3

for color in Color:
    # TODO: Should be `Color`
    reveal_type(color)  # revealed: Unknown

# TODO: Should be `list[Color]`
reveal_type(list(Color))  # revealed: list[Unknown]
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

# TODO: This should emit an error
class ExtendedColor(Color):
    YELLOW = 4
```

## Custom enum types

To do: <https://typing.python.org/en/latest/spec/enums.html#enum-definition>

## Function syntax

To do: <https://typing.python.org/en/latest/spec/enums.html#enum-definition>

## Exhaustiveness checking

To do

## References

- Typing spec: <https://typing.python.org/en/latest/spec/enums.html>
- Documentation: <https://docs.python.org/3/library/enum.html>
