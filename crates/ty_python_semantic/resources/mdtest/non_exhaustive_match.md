# Non-exhaustive match statements

The `non-exhaustive-match` rule detects `match` statements that may not handle every possible
subject value. It can alert developers when, for example, a newly added enum member would otherwise
go unhandled.

```toml
[environment]
python-version = "3.11"
```

## Diagnostic

```py
from typing import Literal

def describe(value: Literal["red", "green"]) -> None:
    match value:  # TODO: should error
        case "red":
            pass
```

## Concise diagnostics

For a finite subject, the concise message lists missing values when ty can enumerate them.
Otherwise, it describes the remaining type, or the subject type if it is dynamic.

```py
from enum import Enum
from typing import Any, Literal

def one_value(value: Literal[1, 2]) -> None:
    # TODO: should error with "Match is not exhaustive: value `2` is not covered"
    match value:
        case 1:
            pass

def none(value: Literal[1] | None) -> None:
    # TODO: should error with "Match is not exhaustive: `None` is not covered"
    match value:
        case 1:
            pass

def several_values(value: Literal[1, 2, 3]) -> None:
    # TODO: should error with "Match is not exhaustive: values `2` and `3` are not covered"
    match value:
        case 1:
            pass

def many_values(value: Literal[0, 1, 2, 3, 4]) -> None:
    # TODO: should error with "Match is not exhaustive: values `1`, `2`, `3` and 1 more are not covered"
    match value:
        case 0:
            pass

class Color(Enum):
    RED = 1
    BLUE = 2

def enum(value: Color) -> None:
    # TODO: should error with "Match is not exhaustive: enum variant `Color.BLUE` is not covered"
    match value:
        case Color.RED:
            pass

def open_type(value: int | str) -> None:
    # TODO: should error with "Match is not exhaustive: objects of type `str` are not covered"
    match value:
        case int():
            pass

def dynamic(value: Any) -> None:
    # TODO: should error with "Match is not exhaustive: subject has type `Any`"
    match value:
        case 1:
            pass

def unknown(value) -> None:
    # TODO: should error with "Match is not exhaustive: subject has type `Unknown`"
    match value:
        case 1:
            pass
```

## Enum diagnostics

The diagnostic lists at most three missing members and points to their definitions.

```py
from enum import Enum

class Direction(Enum):
    NORTH = 1
    SOUTH = 2
    EAST = 3
    WEST = 4
    UP = 5

def describe(value: Direction) -> None:
    # TODO: should error: Match is not exhaustive: enum variants `Direction.SOUTH`, `Direction.EAST`, `Direction.WEST` and 1 more are not covered
    # The definition annotations should name `SOUTH`, `EAST` and `WEST`.
    match value:
        case Direction.NORTH:
            pass
```

## Verbose enum diagnostics

With `--verbose`, the diagnostic lists all missing members.

```toml
verbose = true

[environment]
python-version = "3.11"
```

```py
from enum import Enum

class Direction(Enum):
    NORTH = 1
    SOUTH = 2
    EAST = 3
    WEST = 4
    UP = 5

def describe(value: Direction) -> None:
    # TODO: should error: Match is not exhaustive: enum variants `Direction.SOUTH`, `Direction.EAST`, `Direction.WEST` and `Direction.UP` are not covered
    # The definition annotations should also name `SOUTH`, `EAST`, `WEST` and `UP`.
    match value:
        case Direction.NORTH:
            pass
```

## A union of members of the same enum

The diagnostic’s member annotations use unqualified names when the subject contains only members of
one enum. For example, the primary message names `Color.BLUE`, while the annotation at its
definition names `BLUE`.

```py
from enum import Enum
from typing import Literal

class Color(Enum):
    RED = 1
    BLUE = 2
    GREEN = 3
    YELLOW = 4

def describe(value: Literal[Color.RED, Color.BLUE, Color.GREEN]) -> None:
    match value:  # TODO: should error
        case Color.RED:
            pass
```

## An enum subject narrowed by excluding a member

```py
from enum import Enum

class Color(Enum):
    RED = 1
    BLUE = 2
    GREEN = 3
    YELLOW = 4

def describe(value: Color) -> None:
    if value is not Color.RED:
        match value:  # TODO: should error
            case Color.BLUE:
                pass
```

## An enum subject narrowed by truthiness

Members whose truthiness ty cannot determine remain possible missing values.

```py
from enum import Enum

class Color(Enum):
    RED = 0
    BLUE = 1
    GREEN = 2

    def __bool__(self) -> bool:
        return self.value != 0

def describe(value: Color) -> None:
    if value:
        # TODO: should error: Match is not exhaustive: enum variants `Color.RED` and `Color.GREEN` are not covered
        match value:
            case Color.BLUE:
                pass
```

Previously excluded members stay excluded after truthiness narrowing.

```py
def previously_excluded(value: Color) -> None:
    if value is not Color.RED and value:
        # TODO: should error with "enum variant `Color.GREEN` is not covered"
        match value:
            case Color.BLUE:
                pass
```

The remaining enum members are also listed when the subject is a union.

```py
from typing import Literal

def mixed_union(value: Color | Literal["", "stop"]) -> None:
    if value:
        # TODO: should error with "enum variants `Color.RED` and `Color.GREEN` are not covered"
        match value:
            case Color.BLUE | "stop":
                pass
```

## Enum intersections with additional constraints

When an enum's members cover all its instances, intersecting it with another type cannot add values
outside that enum. Ty retains a member unless it can prove the member cannot satisfy the other
constraints. Here, `__bool__` returns `bool`, so ty cannot determine its result for an individual
member.

```py
from enum import Enum
from typing import Any, TypeVar

from ty_extensions import AlwaysTruthy, Intersection

class Color(Enum):
    RED = 1
    BLUE = 2
    GREEN = 3

    def __bool__(self) -> bool:
        return self.value != 1

def truthy(value: Intersection[Color, AlwaysTruthy]) -> None:
    # TODO: should error: Match is not exhaustive: enum variants `Color.RED` and `Color.GREEN` are not covered
    match value:
        case Color.BLUE:
            pass
```

```py
def gradual(value: Intersection[Color, Any]) -> None:
    if value is not Color.RED:
        # TODO: should error with "enum variant `Color.GREEN` is not covered"
        match value:
            case Color.BLUE:
                pass
```

When intersection arms overlap, the diagnostic lists each missing member only once.

```py
T = TypeVar("T")
U = TypeVar("U")

def overlapping(value: Intersection[Color, T] | Intersection[Color, U]) -> None:
    # TODO: should error with "enum variants `Color.RED` and `Color.GREEN` are not covered"
    match value:
        case Color.BLUE:
            pass
```

## An intersection without a finite component

The pattern `1` also matches `True`, because numeric literal patterns compare by equality.

```py
from ty_extensions import AlwaysTruthy, Intersection

def open_type(value: Intersection[int, AlwaysTruthy]) -> None:
    # TODO: should error and report the uncovered type as
    # `int & AlwaysTruthy & ~Literal[1] & ~Literal[True]`.
    match value:
        case 1:
            pass
```

## Type variables and enums

The diagnostic lists members allowed by a type variable's bound, even though a particular
specialization may admit fewer of them.

```py
from enum import Enum
from typing import Literal, TypeVar

class Color(Enum):
    RED = 1
    BLUE = 2
    GREEN = 3

T = TypeVar("T", bound=Color)

def incomplete(value: T) -> None:
    match value:  # TODO: should error
        case Color.RED:
            pass
```

An earlier exclusion still removes a member from the diagnostic.

```py
def narrowed(value: T) -> None:
    if value is not Color.GREEN:
        # TODO: should error with "enum variant `Color.BLUE` is not covered"
        match value:
            case Color.RED:
                pass

def complete(value: T) -> None:
    match value:  # no diagnostic
        case Color.RED | Color.BLUE | Color.GREEN:
            pass
```

Type-variable constraints can also supply a finite set of possibilities.

```py
class Shape(Enum):
    CIRCLE = 1
    SQUARE = 2

Constrained = TypeVar("Constrained", Color, Shape)

def constrained(value: Constrained) -> None:
    # TODO: should error with "enum variants `Color.BLUE`, `Color.GREEN` and `Shape.SQUARE` are not covered"
    match value:
        case Color.RED | Shape.CIRCLE:
            pass
```

A type variable can also have a bound consisting of enum literals.

```py
Bounded = TypeVar("Bounded", bound=Literal[Color.RED, Color.GREEN])

def bounded(value: Bounded) -> None:
    match value:  # TODO: should error
        case Color.RED:
            pass
```

An unbounded type variable, or one whose bound includes an open type, is not finite.

```py
Unbounded = TypeVar("Unbounded")
Wide = TypeVar("Wide", bound=Color | str)

def unbounded(value: Unbounded) -> None:
    match value:  # TODO: should error
        case Color.RED:
            pass
```

```py
def wide(value: Wide) -> None:
    match value:  # TODO: should error
        case Color.RED:
            pass
```

## NewTypes of an enum

```py
from enum import Enum
from typing import NewType

class Color(Enum):
    RED = 1
    BLUE = 2
    GREEN = 3

ColorId = NewType("ColorId", Color)
NestedColorId = NewType("NestedColorId", ColorId)

def incomplete(value: ColorId) -> None:
    match value:  # TODO: should error
        case Color.RED:
            pass
```

```py
def nested(value: NestedColorId) -> None:
    if value is not Color.GREEN:
        # TODO: should error with "enum variant `Color.BLUE` is not covered"
        match value:
            case Color.RED:
                pass

def complete(value: ColorId) -> None:
    match value:  # no diagnostic
        case Color.RED | Color.BLUE | Color.GREEN:
            pass
```

## Type variables and NewTypes of flags

Flags can have values formed by combining members, so their members do not exhaust the possible
values.

```py
from enum import Flag
from typing import NewType, TypeVar

class Permission(Flag):
    READ = 1
    WRITE = 2

T = TypeVar("T", bound=Permission)
PermissionId = NewType("PermissionId", Permission)

def type_variable(value: T) -> None:
    match value:  # TODO: should error and explain that flags can have unnamed combinations
        case Permission.READ:
            pass
```

```py
def newtype(value: PermissionId) -> None:
    match value:  # TODO: should error and explain that flags can have unnamed combinations
        case Permission.READ:
            pass
```

## Type aliases

The recursive alias `Tree` includes an open `list` alternative, so its values cannot be enumerated.

```toml
[environment]
python-version = "3.12"
```

```py
from enum import Enum
from typing import Literal

class Color(Enum):
    RED = 1
    BLUE = 2

type Identity[T] = T
type Nested = Identity[Identity[Color]]
type Mixed = Color | Literal[7]
type Tree = Color | list[Tree]

def nested(value: Nested) -> None:
    # TODO: should error with "enum variant `Color.BLUE` is not covered"
    match value:
        case Color.RED:
            pass

def mixed(value: Mixed) -> None:
    # TODO: should error with "values `Color.BLUE` and `7` are not covered"
    match value:
        case Color.RED:
            pass

def bounded[T: Nested](value: T) -> None:
    # TODO: should error with "enum variant `Color.BLUE` is not covered"
    match value:
        case Color.RED:
            pass
```

```py
def recursive(value: Tree) -> None:
    match value:  # TODO: should error
        case Color.RED:
            pass
```

```py
def recursive_bound[T: Tree](value: T) -> None:
    match value:  # TODO: should error
        case Color.RED:
            pass
```

## Enum and literal unions

```py
from enum import Enum
from typing import Literal

class Color(Enum):
    RED = 1
    BLUE = 2

def describe(value: Color | Literal["stop"]) -> None:
    match value:  # TODO: should error
        case Color.RED:
            pass
```

## Literal unions

```py
from typing import Literal

def describe(value: Literal["red", "green", "blue"]) -> None:
    match value:  # TODO: should error
        case "red":
            pass
```

## Escaped string literals

```py
from typing import Literal

def describe(value: Literal["handled", "\x1b", "\u200b", "\U000e0001"]) -> None:
    # TODO: should error, displaying the missing values as `"\x1b"`, `"\u200b"` and
    # `"\U000e0001"` in both the diagnostic and the suggested `case`.
    match value:
        case "handled":
            pass
```

```py
def line_break(value: Literal["handled", "\n"]) -> None:
    # TODO: should error, displaying the missing value as `"\n"` in both the diagnostic
    # and the suggested `case`.
    match value:
        case "handled":
            pass
```

## Byte literals

```py
from typing import Literal

def describe(value: Literal[b"red", b"blue"]) -> None:
    match value:  # TODO: should error
        case b"red":
            pass
```

## Boolean literals

Ty treats a `Literal[True, False]` subject as `bool` and reports the uncovered type. For a union of
a boolean literal and another literal, it lists the uncovered value.

```py
from typing import Literal

def describe(value: Literal[True, False]) -> None:
    match value:  # TODO: should error
        case True:
            pass
```

```py
def mixed(value: Literal[True, "stop"]) -> None:
    match value:  # TODO: should error
        case "stop":
            pass
```

## `LiteralString`

```py
from typing import LiteralString

def describe(value: LiteralString) -> None:
    match value:  # TODO: should error
        case "red":
            pass
```

## More than three missing literal values

```py
from typing import Literal

def describe(value: Literal["red", "green", "blue", "white", "black"]) -> None:
    # TODO: should error: Match is not exhaustive: values `"green"`, `"blue"`, `"white"` and 1 more are not covered
    # The suggested branch should be `case "green" | "blue" | "white" | "black"`.
    match value:
        case "red":
            pass
```

## Five missing literal values

```py
from typing import Literal

def describe(value: Literal[0, 1, 2, 3, 4, 5]) -> None:
    # TODO: should error: Match is not exhaustive: values `1`, `2`, `3` and 2 more are not covered
    # The suggested branch should be `case 1 | 2 | 3 | 4 | 5`.
    match value:
        case 0:
            pass
```

## More than five missing literal values

```py
from typing import Literal

def describe(value: Literal[0, 1, 2, 3, 4, 5, 6]) -> None:
    # TODO: should error: Match is not exhaustive: values `1`, `2`, `3` and 3 more are not covered
    # The suggested branch should be `case _`.
    match value:
        case 0:
            pass
```

## Functional enum

```py
from enum import Enum

Color = Enum("Color", "RED GREEN BLUE")

def describe(value: Color) -> None:
    match value:  # TODO: should error
        case Color.RED:
            pass
```

## Multiple functional enums

The diagnostic groups missing members by enum definition in its annotations: `Color.GREEN` and
`Color.BLUE` appear together on the `Color` definition, while `Direction.SOUTH` appears on the
`Direction` definition.

```py
from enum import Enum

Color = Enum("Color", "RED GREEN BLUE")
Direction = Enum("Direction", "NORTH SOUTH")

def describe(value: Color | Direction) -> None:
    match value:  # TODO: should error
        case Color.RED | Direction.NORTH:
            pass
```

## Functional enum with a keyword argument

```py
from enum import Enum

Color = Enum("Color", names={"RED": 1, "BLUE": 2})

def describe(value: Color) -> None:
    match value:  # TODO: should error
        case Color.RED:
            pass
```

## A missing `case None`

```py
from typing import Literal

def describe(value: Literal["red"] | None) -> None:
    match value:  # TODO: should error
        case "red":
            pass
```

Same example, but in a union with a non-`Literal` type:

```py
def describe(value: int | None):
    match value:  # TODO: should error
        case int():
            pass
```

## Enum members defined in another file

The suggested pattern uses the name by which the enum was imported.

`colors.py`:

```py
from enum import Enum

class Color(Enum):
    RED = 1
    BLUE = 2
```

```py
from colors import Color as Hue

def describe(value: Hue) -> None:
    # TODO: should error and suggest `case Hue.BLUE`.
    match value:
        case Hue.RED:
            pass
```

## An enum import alias without an enum `case`

`colors.py`:

```py
from enum import Enum

class Color(Enum):
    RED = 1
    BLUE = 2
```

```py
from typing import Literal
from colors import Color as Hue

def describe(value: Hue | Literal["stop"]) -> None:
    # TODO: should error and suggest `case Hue.RED | Hue.BLUE`.
    match value:
        case "stop":
            pass
```

## A module import alias without an enum `case`

`colors.py`:

```py
from enum import Enum

class Color(Enum):
    RED = 1
    BLUE = 2
```

```py
from typing import Literal
import colors as palette

def describe(value: palette.Color | Literal["stop"]) -> None:
    # TODO: should error and suggest `case palette.Color.RED | palette.Color.BLUE`.
    match value:
        case "stop":
            pass
```

## An enum name rebound in a guard

`colors.py`:

```py
from enum import Enum

class Color(Enum):
    RED = 1
    BLUE = 2
```

```py
from typing import Literal
from colors import Color as Hue

def describe(value: Hue | Literal["stop"]) -> None:
    global Hue
    # TODO: should error and suggest `case _`.
    match value:
        case Hue.RED if Hue := None:  # ty: ignore[unresolved-attribute]
            pass
        case "stop":
            pass
```

## An enum on a module rebound before a match

`colors.py`:

```py
from enum import Enum

class Color(Enum):
    RED = 1
    BLUE = 2
```

```py
from typing import Literal
import colors as palette

def describe(value: palette.Color | Literal["stop"]) -> None:
    palette.Color = None  # ty: ignore[invalid-assignment]
    # TODO: should error and suggest `case _`.
    match value:
        case "stop":
            pass
```

## An enum on a module rebound by a guard function

`colors.py`:

```py
from enum import Enum

class Color(Enum):
    RED = 1
    BLUE = 2
```

```py
from typing import Literal
import colors as palette

def rebind() -> bool:
    palette.Color = None  # ty: ignore[invalid-assignment]
    return False

def describe(value: palette.Color | Literal["stop"]) -> None:
    # TODO: should error and suggest `case _`.
    match value:
        case palette.Color.RED if rebind():
            pass
        case "stop":
            pass
```

## A type-only enum import

`colors.py`:

```py
from enum import Enum

class Color(Enum):
    RED = 1
    BLUE = 2
```

```py
from __future__ import annotations
from typing import TYPE_CHECKING, Literal

if TYPE_CHECKING:
    from colors import Color as Hue

def describe(value: Hue | Literal["stop"]) -> None:
    # TODO: should error and suggest `case _`.
    match value:
        case "stop":
            pass
```

## A shadowed enum import

`colors.py`:

```py
from enum import Enum

class Color(Enum):
    RED = 1
    BLUE = 2
```

```py
from typing import Literal
from colors import Color as Hue

def describe(value: Hue | Literal["stop"], Hue: int) -> None:
    # TODO: should error and suggest `case _`.
    match value:
        case "stop":
            pass
```

## An enum defined in the same file

```py
from enum import Enum
from typing import Literal

class Color(Enum):
    RED = 1
    BLUE = 2

def describe(value: Color | Literal["stop"]) -> None:
    # TODO: should error and suggest `case Color.RED | Color.BLUE`.
    match value:
        case "stop":
            pass
```

## An enum name shadowed by a parameter

```py
from enum import Enum
from typing import Literal

class Color(Enum):
    RED = 1
    BLUE = 2

def describe(value: Color | Literal["stop"], Color: int) -> None:
    # TODO: should error and suggest `case _`.
    match value:
        case "stop":
            pass
```

## An enum defined after a match

Ty suggests a wildcard when the enum is defined later in the file, even if it might be available
when the function is called.

```py
from __future__ import annotations
from enum import Enum
from typing import Literal

def describe(value: Color | Literal["stop"]) -> None:
    # TODO: should error and suggest `case _`.
    match value:
        case "stop":
            pass

class Color(Enum):
    RED = 1
    BLUE = 2
```

## An enum defined in a function

```py
from enum import Enum
from typing import Literal, cast

def describe(value: object) -> None:
    class Color(Enum):
        RED = 1
        BLUE = 2

    narrowed = cast(Color | Literal["stop"], value)
    # TODO: should error and suggest `case Color.RED | Color.BLUE`.
    match narrowed:
        case "stop":
            pass
```

## Enums with the same name

`first.py`:

```py
from enum import Enum

class Color(Enum):
    RED = 1
    BLUE = 2
```

`second.py`:

```py
from enum import Enum

class Color(Enum):
    RED = 1
    BLUE = 2
```

```py
import first
import second

def describe(value: first.Color | second.Color) -> None:
    # TODO: should error and suggest `case first.Color.BLUE | second.Color.BLUE`.
    match value:
        case first.Color.RED | second.Color.RED:
            pass
```

## Flag instances

Flag members do not necessarily exhaust the possible flag values. The diagnostic explains that
`enum.Flag` can have unnamed combinations of members.

```py
from enum import Flag

class Permission(Flag):
    READ = 1
    WRITE = 2

def describe(value: Permission) -> None:
    match value:  # TODO: should error
        case Permission.READ:
            pass
```

The `|` pattern matches either named member, but not a combination of them.

```py
def all_members(value: Permission) -> None:
    match value:  # TODO: should error
        case Permission.READ | Permission.WRITE:
            pass
```

## Literal flag members

```py
from enum import Flag
from typing import Literal

class Permission(Flag):
    READ = 1
    WRITE = 2

def describe(value: Literal[Permission.READ, Permission.WRITE]) -> None:
    match value:  # TODO: should error
        case Permission.READ:
            pass
```

## Flag instances in a union

```py
from enum import IntFlag

class Permission(IntFlag):
    READ = 1
    WRITE = 2

def unhandled_flag_and_string(value: Permission | str | bytes) -> None:
    match value:  # TODO: should error
        case bytes():
            pass
```

When the flag is already covered, the diagnostic does not include the explanation about flags.

```py
def unhandled_string(value: Permission | str) -> None:
    match value:  # TODO: should error
        case Permission():
            pass
```

## A union containing an open type

```py
from enum import Enum

class Color(Enum):
    RED = 1
    BLUE = 2

def describe(value: Color | str) -> None:
    match value:  # TODO: should error
        case Color.RED:
            pass
```

## Suggested wildcard `case`

```py
def describe(value: int | str) -> None:
    # TODO: should error and suggest `case _` after the existing `case`.
    match value:
        case int():
            pass  # Kept with the existing `case`.
```

## Nested match with trailing comments

```py
from typing import Literal

def describe(value: Literal[1, 2], enabled: bool) -> None:
    if enabled:
        # TODO: should error and suggest `case 2` after the trailing comment.
        match value:
            case 1:
                pass
                # This comment belongs to the existing `case`.
```

## Single-line `case` body

```py
from typing import Literal

def describe(value: Literal["red", "green"]) -> None:
    # fmt: off
    # TODO: should error and suggest `case "green"` after the existing `case`.
    match value:
        case "red": pass  # Kept with the existing `case`.
    # fmt: on
```

## Literal patterns

```py
from typing import Literal

def incomplete(value: Literal["red", "green", "blue"]) -> None:
    # TODO: should error with "Match is not exhaustive: value `"blue"` is not covered"
    match value:
        case "red" | "green":
            pass

def complete(value: Literal["red", "green", "blue"]) -> None:
    match value:  # no diagnostic
        case "red" | "green":
            pass
        case "blue":
            pass

def booleans(value: bool) -> None:
    match value:  # no diagnostic
        case True:
            pass
        case False:
            pass
```

Numeric literal patterns compare by equality, so `0 | 1` also matches `False` and `True`.

```py
def boolean_values_as_integers(value: bool) -> None:
    match value:  # no diagnostic
        case 0 | 1:
            pass

def wildcard(value: str) -> None:
    match value:  # no diagnostic
        case "red":
            pass
        case _:
            pass

def capture(value: str) -> None:
    match value:  # no diagnostic
        case captured:
            print(captured)
```

## Enum members

Aliases of enum members are not listed as separate missing values.

```py
from enum import Enum

class Color(Enum):
    RED = 1
    CRIMSON = 1
    GREEN = 2
    BLUE = 3

def incomplete(value: Color) -> None:
    match value:  # TODO: should error
        case Color.RED:
            pass
```

```py
def complete(value: Color) -> None:
    match value:  # no diagnostic
        case Color.RED | Color.GREEN:
            pass
        case Color.BLUE:
            pass

def alias(value: Color) -> None:
    match value:  # no diagnostic
        case Color.CRIMSON | Color.GREEN | Color.BLUE:
            pass
```

## Class patterns and unions

```py
def complete(value: int | str) -> None:
    match value:  # no diagnostic
        case int() | str():
            pass

def already_narrowed(value: int | str) -> None:
    if isinstance(value, int):
        match value:  # no diagnostic
            case int():
                pass
```

The pattern `1` also matches `True`, because numeric literal patterns compare by equality.

```py
def open_type(value: int) -> None:
    # TODO: should error with "Match is not exhaustive: objects of type `int & ~Literal[1] & ~Literal[True]` are not covered"
    match value:
        case 1:
            pass
```

## Tuple patterns

```py
def incomplete(value: tuple[bool, bool]) -> None:
    # TODO: should error with "Match is not exhaustive: objects of type `tuple[Literal[False], Literal[False]]` are not covered"
    match value:
        case (True, _):
            pass
        case (False, True):
            pass

def complete(value: tuple[bool, bool]) -> None:
    match value:  # no diagnostic
        case (True, _):
            pass
        case (False, True):
            pass
        case (False, False):
            pass

def expression_subject(first: bool, second: bool) -> None:
    match (first, second):  # no diagnostic
        case (True, _):
            pass
        case (False, _):
            pass
```

```py
def expression_incomplete(first: bool, second: bool) -> None:
    match (first, second):  # TODO: should error
        case (True, _):
            pass
```

## Guards

An enum reference such as `Color` can be rebound by a guard before the suggested `case` is reached,
causing that `case` to refer to a different object. Ty therefore suggests a wildcard even when the
guard shown does not rebind `Color`. Non-enum literal patterns do not resolve such a name.

```py
from enum import Enum
from typing import Literal

class Color(Enum):
    RED = 1
    BLUE = 2

def guarded_enum(value: Color, enabled: bool) -> None:
    # TODO: should error and suggest `case _`.
    match value:
        case Color.RED if enabled:
            pass
```

```py
def guarded_literal(value: Literal[1, 2], enabled: bool) -> None:
    # TODO: should error and suggest `case 1 | 2`.
    match value:
        case 1 if enabled:
            pass
```

```py
def guarded(value: Literal[1, 2], flag: bool) -> None:
    match value:  # TODO: should error with "`1` is not covered"
        case 1 if flag:
            pass
        case 2:
            pass

    match value:  # no diagnostic
        case 1 if flag:
            pass
        case 1 | 2:
            pass

def true_guard(value: Literal[1, 2]) -> None:
    match value:  # no diagnostic
        case 1 if True:
            pass
        case 2:
            pass

def false_guard(value: Literal[1, 2]) -> None:
    match value:  # TODO: should error with "`1` is not covered"
        case 1 if False:
            pass
        case 2:
            pass

def guarded_wildcard(value: str, flag: bool) -> None:
    match value:  # TODO: should error
        case _ if flag:
            pass

def true_guarded_wildcard(value: str) -> None:
    match value:  # no diagnostic
        case _ if True:
            pass

def subject_guard(value: Literal[1, 2]) -> None:
    match value:  # no diagnostic
        case 1 if value == 1:
            pass
        case 2:
            pass

def capture_guard(value: Literal[1, 2]) -> None:
    match value:  # no diagnostic
        case 1 as captured if captured == 1:
            pass
        case 2:
            pass

def final_capture_guard(value: Literal[1, 2]) -> None:
    match value:  # no diagnostic
        case 1:
            pass
        case 2 as captured if captured == 2:
            pass

def final_ambiguous_guard(value: Literal[1, 2], flag: bool) -> None:
    match value:  # TODO: should error with "`2` is not covered"
        case 1:
            pass
        case 2 if flag:
            pass

# The guard remains true for a string even when the subject changes between iterations.
def changing_subject(value: int | str, again: bool) -> None:
    while again:
        match value:  # no diagnostic
            case str() as captured if captured is not None:
                value = 1
            case int():
                value = 0
```

## Uninhabited and dynamic subjects

```py
from typing import Any, Never

def uninhabited(value: Never) -> None:
    match value:  # no diagnostic
        case 1:
            pass

def dynamic(value: Any) -> None:
    match value:  # no diagnostic
        case _:
            pass

def unreachable(value: int) -> None:
    if False:
        match value:  # no diagnostic
            case 1:
                pass
```
