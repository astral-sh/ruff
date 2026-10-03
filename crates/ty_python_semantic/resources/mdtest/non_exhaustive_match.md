# Non-exhaustive match statements

```toml
[environment]
python-version = "3.11"

[rules]
non-exhaustive-match = "error"
```

## Diagnostic

```py
from typing import Literal

def describe(value: Literal["red", "green"]) -> None:
    match value:  # snapshot: non-exhaustive-match
        case "red":
            pass
```

```snapshot
error[non-exhaustive-match]: Match is not exhaustive: value `"green"` is not covered
 --> src/mdtest_snippet.py:4:11
  |
4 |     match value:  # snapshot: non-exhaustive-match
  |           ^^^^^ Subject has type `Literal["red", "green"]`
help: Add a `case` branch for the remaining values
  |
5 |         case "red":
  -             pass
6 +             pass
7 +         case "green":
8 +             raise NotImplementedError("TODO")
  |
note: This is a display-only fix and is likely to be incorrect
```

## Concise diagnostics

For a finite subject, the concise message lists missing values when ty can enumerate them.
Otherwise, it describes the remaining type, or the subject type if it is dynamic.

```py
from enum import Enum
from typing import Any, Literal

def one_value(value: Literal[1, 2]) -> None:
    # error: [non-exhaustive-match] "Match is not exhaustive: value `2` is not covered"
    match value:
        case 1:
            pass

def none(value: Literal[1] | None) -> None:
    # error: [non-exhaustive-match] "Match is not exhaustive: `None` is not covered"
    match value:
        case 1:
            pass

def several_values(value: Literal[1, 2, 3]) -> None:
    # error: [non-exhaustive-match] "Match is not exhaustive: values `2` and `3` are not covered"
    match value:
        case 1:
            pass

def many_values(value: Literal[0, 1, 2, 3, 4]) -> None:
    # error: [non-exhaustive-match] "Match is not exhaustive: values `1`, `2`, `3` and 1 more are not covered"
    match value:
        case 0:
            pass

class Color(Enum):
    RED = 1
    BLUE = 2

def enum(value: Color) -> None:
    # error: [non-exhaustive-match] "Match is not exhaustive: enum variant `Color.BLUE` is not covered"
    match value:
        case Color.RED:
            pass

def open_type(value: int | str) -> None:
    # error: [non-exhaustive-match] "Match is not exhaustive: objects of type `str` are not covered"
    match value:
        case int():
            pass

def dynamic(value: Any) -> None:
    # error: [non-exhaustive-match] "Match is not exhaustive: subject has type `Any`"
    match value:
        case 1:
            pass

def unknown(value) -> None:
    # error: [non-exhaustive-match] "Match is not exhaustive: subject has type `Unknown`"
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
    match value:  # snapshot: non-exhaustive-match
        case Direction.NORTH:
            pass
```

```snapshot
error[non-exhaustive-match]: Match is not exhaustive: enum variants `Direction.SOUTH`, `Direction.EAST`, `Direction.WEST` and 1 more are not covered
  --> src/mdtest_snippet.py:11:11
   |
11 |     match value:  # snapshot: non-exhaustive-match
   |           ^^^^^ Subject has type `Direction`
   |
  ::: src/mdtest_snippet.py:5:5
   |
 5 |     SOUTH = 2
   |     ----- enum variant `SOUTH` is not covered
 6 |     EAST = 3
   |     ---- enum variant `EAST` is not covered
 7 |     WEST = 4
   |     ---- enum variant `WEST` is not covered
info: Use `--verbose` to see all 4 uncovered values
help: Add a `case` branch for the remaining values
   |
12 |         case Direction.NORTH:
   -             pass
13 +             pass
14 +         case Direction.SOUTH | Direction.EAST | Direction.WEST | Direction.UP:
15 +             raise NotImplementedError("TODO")
   |
note: This is a display-only fix and is likely to be incorrect
```

## Verbose enum diagnostics

With `--verbose`, the diagnostic lists all missing members.

```toml
verbose = true

[environment]
python-version = "3.11"

[rules]
non-exhaustive-match = "error"
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
    match value:  # snapshot: non-exhaustive-match
        case Direction.NORTH:
            pass
```

```snapshot
error[non-exhaustive-match]: Match is not exhaustive: enum variants `Direction.SOUTH`, `Direction.EAST`, `Direction.WEST` and `Direction.UP` are not covered
  --> src/mdtest_snippet.py:11:11
   |
 5 |     SOUTH = 2
   |     ----- enum variant `SOUTH` is not covered
 6 |     EAST = 3
   |     ---- enum variant `EAST` is not covered
 7 |     WEST = 4
   |     ---- enum variant `WEST` is not covered
 8 |     UP = 5
   |     -- enum variant `UP` is not covered
 9 |
10 | def describe(value: Direction) -> None:
11 |     match value:  # snapshot: non-exhaustive-match
   |           ^^^^^ Subject has type `Direction`
help: Add a `case` branch for the remaining values
info: rule `non-exhaustive-match` was selected in the configuration file
   |
12 |         case Direction.NORTH:
   -             pass
13 +             pass
14 +         case Direction.SOUTH | Direction.EAST | Direction.WEST | Direction.UP:
15 +             raise NotImplementedError("TODO")
   |
note: This is a display-only fix and is likely to be incorrect
```

## A union of members of the same enum

The member annotations use unqualified names when the subject contains only members of one enum.

```py
from enum import Enum
from typing import Literal

class Color(Enum):
    RED = 1
    BLUE = 2
    GREEN = 3
    YELLOW = 4

def describe(value: Literal[Color.RED, Color.BLUE, Color.GREEN]) -> None:
    match value:  # snapshot: non-exhaustive-match
        case Color.RED:
            pass
```

```snapshot
error[non-exhaustive-match]: Match is not exhaustive: enum variants `Color.BLUE` and `Color.GREEN` are not covered
  --> src/mdtest_snippet.py:11:11
   |
11 |     match value:  # snapshot: non-exhaustive-match
   |           ^^^^^ Subject has type `Literal[Color.RED, Color.BLUE, Color.GREEN]`
   |
  ::: src/mdtest_snippet.py:6:5
   |
 6 |     BLUE = 2
   |     ---- enum variant `BLUE` is not covered
 7 |     GREEN = 3
   |     ----- enum variant `GREEN` is not covered
help: Add a `case` branch for the remaining values
   |
12 |         case Color.RED:
   -             pass
13 +             pass
14 +         case Color.BLUE | Color.GREEN:
15 +             raise NotImplementedError("TODO")
   |
note: This is a display-only fix and is likely to be incorrect
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
        match value:  # snapshot: non-exhaustive-match
            case Color.BLUE:
                pass
```

```snapshot
error[non-exhaustive-match]: Match is not exhaustive: enum variants `Color.GREEN` and `Color.YELLOW` are not covered
  --> src/mdtest_snippet.py:11:15
   |
 6 |     GREEN = 3
   |     ----- enum variant `GREEN` is not covered
 7 |     YELLOW = 4
   |     ------ enum variant `YELLOW` is not covered
 8 |
 9 | def describe(value: Color) -> None:
10 |     if value is not Color.RED:
11 |         match value:  # snapshot: non-exhaustive-match
   |               ^^^^^ Subject has type `Literal[Color.BLUE, Color.GREEN, Color.YELLOW]`
help: Add a `case` branch for the remaining values
   |
12 |             case Color.BLUE:
   -                 pass
13 +                 pass
14 +             case Color.GREEN | Color.YELLOW:
15 +                 raise NotImplementedError("TODO")
   |
note: This is a display-only fix and is likely to be incorrect
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
        match value:  # snapshot: non-exhaustive-match
            case Color.BLUE:
                pass
```

```snapshot
error[non-exhaustive-match]: Match is not exhaustive: enum variants `Color.RED` and `Color.GREEN` are not covered
  --> src/mdtest_snippet.py:13:15
   |
13 |         match value:  # snapshot: non-exhaustive-match
   |               ^^^^^ Subject has type `Color & ~AlwaysFalsy`
   |
  ::: src/mdtest_snippet.py:4:5
   |
 4 |     RED = 0
   |     --- enum variant `RED` is not covered
 5 |     BLUE = 1
 6 |     GREEN = 2
   |     ----- enum variant `GREEN` is not covered
help: Add a `case` branch for the remaining values
   |
15 |                 pass
16 +             case Color.RED | Color.GREEN:
17 +                 raise NotImplementedError("TODO")
18 | def previously_excluded(value: Color) -> None:
   |
note: This is a display-only fix and is likely to be incorrect
```

Previously excluded members stay excluded after truthiness narrowing.

```py
def previously_excluded(value: Color) -> None:
    if value is not Color.RED and value:
        # error: [non-exhaustive-match] "enum variant `Color.GREEN` is not covered"
        match value:
            case Color.BLUE:
                pass
```

The remaining enum members are also listed when the subject is a union.

```py
from typing import Literal

def mixed_union(value: Color | Literal["", "stop"]) -> None:
    if value:
        # error: [non-exhaustive-match] "enum variants `Color.RED` and `Color.GREEN` are not covered"
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
    match value:  # snapshot: non-exhaustive-match
        case Color.BLUE:
            pass
```

```snapshot
error[non-exhaustive-match]: Match is not exhaustive: enum variants `Color.RED` and `Color.GREEN` are not covered
  --> src/mdtest_snippet.py:15:11
   |
15 |     match value:  # snapshot: non-exhaustive-match
   |           ^^^^^ Subject has type `Color & AlwaysTruthy`
   |
  ::: src/mdtest_snippet.py:7:5
   |
 7 |     RED = 1
   |     --- enum variant `RED` is not covered
 8 |     BLUE = 2
 9 |     GREEN = 3
   |     ----- enum variant `GREEN` is not covered
help: Add a `case` branch for the remaining values
   |
17 |             pass
18 +         case Color.RED | Color.GREEN:
19 +             raise NotImplementedError("TODO")
20 | def gradual(value: Intersection[Color, Any]) -> None:
   |
note: This is a display-only fix and is likely to be incorrect
```

```py
def gradual(value: Intersection[Color, Any]) -> None:
    if value is not Color.RED:
        # error: [non-exhaustive-match] "enum variant `Color.GREEN` is not covered"
        match value:
            case Color.BLUE:
                pass
```

When intersection arms overlap, the diagnostic lists each missing member only once.

```py
T = TypeVar("T")
U = TypeVar("U")

def overlapping(value: Intersection[Color, T] | Intersection[Color, U]) -> None:
    # error: [non-exhaustive-match] "enum variants `Color.RED` and `Color.GREEN` are not covered"
    match value:
        case Color.BLUE:
            pass
```

## An intersection without a finite component

The pattern `1` also matches `True`, because numeric literal patterns compare by equality.

```py
from ty_extensions import AlwaysTruthy, Intersection

def open_type(value: Intersection[int, AlwaysTruthy]) -> None:
    match value:  # snapshot: non-exhaustive-match
        case 1:
            pass
```

```snapshot
error[non-exhaustive-match]: Match is not exhaustive: objects of type `int & AlwaysTruthy & ~Literal[1] & ~Literal[True]` are not covered
 --> src/mdtest_snippet.py:4:11
  |
4 |     match value:  # snapshot: non-exhaustive-match
  |           ^^^^^ Subject has type `int & AlwaysTruthy`
help: Add a `case` branch for the remaining values
  |
5 |         case 1:
  -             pass
6 +             pass
7 +         case _:
8 +             raise NotImplementedError("TODO")
  |
note: This is a display-only fix and is likely to be incorrect
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
    match value:  # snapshot: non-exhaustive-match
        case Color.RED:
            pass
```

```snapshot
error[non-exhaustive-match]: Match is not exhaustive: enum variants `Color.BLUE` and `Color.GREEN` are not covered
  --> src/mdtest_snippet.py:12:11
   |
12 |     match value:  # snapshot: non-exhaustive-match
   |           ^^^^^ Subject has type `T@incomplete`
   |
  ::: src/mdtest_snippet.py:6:5
   |
 6 |     BLUE = 2
   |     ---- enum variant `Color.BLUE` is not covered
 7 |     GREEN = 3
   |     ----- enum variant `Color.GREEN` is not covered
help: Add a `case` branch for the remaining values
   |
14 |             pass
15 +         case Color.BLUE | Color.GREEN:
16 +             raise NotImplementedError("TODO")
17 | def narrowed(value: T) -> None:
   |
note: This is a display-only fix and is likely to be incorrect
```

An earlier exclusion still removes a member from the diagnostic.

```py
def narrowed(value: T) -> None:
    if value is not Color.GREEN:
        # error: [non-exhaustive-match] "enum variant `Color.BLUE` is not covered"
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
    # error: [non-exhaustive-match] "enum variants `Color.BLUE`, `Color.GREEN` and `Shape.SQUARE` are not covered"
    match value:
        case Color.RED | Shape.CIRCLE:
            pass
```

A type variable can also have a bound consisting of enum literals.

```py
Bounded = TypeVar("Bounded", bound=Literal[Color.RED, Color.GREEN])

def bounded(value: Bounded) -> None:
    match value:  # snapshot: non-exhaustive-match
        case Color.RED:
            pass
```

```snapshot
error[non-exhaustive-match]: Match is not exhaustive: enum variant `Color.GREEN` is not covered
  --> src/mdtest_snippet.py:40:11
   |
40 |     match value:  # snapshot: non-exhaustive-match
   |           ^^^^^ Subject has type `Bounded@bounded`
   |
  ::: src/mdtest_snippet.py:7:5
   |
 7 |     GREEN = 3
   |     ----- enum variant `Color.GREEN` is not covered
help: Add a `case` branch for the remaining values
   |
42 |             pass
43 +         case Color.GREEN:
44 +             raise NotImplementedError("TODO")
45 | Unbounded = TypeVar("Unbounded")
   |
note: This is a display-only fix and is likely to be incorrect
```

An unbounded type variable, or one whose bound includes an open type, is not finite.

```py
Unbounded = TypeVar("Unbounded")
Wide = TypeVar("Wide", bound=Color | str)

def unbounded(value: Unbounded) -> None:
    match value:  # snapshot: non-exhaustive-match
        case Color.RED:
            pass
```

```snapshot
error[non-exhaustive-match]: Match is not exhaustive: objects of type `Unbounded@unbounded & ~Literal[Color.RED]` are not covered
  --> src/mdtest_snippet.py:47:11
   |
47 |     match value:  # snapshot: non-exhaustive-match
   |           ^^^^^ Subject has type `Unbounded@unbounded`
help: Add a `case` branch for the remaining values
   |
49 |             pass
50 +         case _:
51 +             raise NotImplementedError("TODO")
52 | def wide(value: Wide) -> None:
   |
note: This is a display-only fix and is likely to be incorrect
```

```py
def wide(value: Wide) -> None:
    match value:  # snapshot: non-exhaustive-match
        case Color.RED:
            pass
```

```snapshot
error[non-exhaustive-match]: Match is not exhaustive: objects of type `Wide@wide & ~Literal[Color.RED]` are not covered
  --> src/mdtest_snippet.py:51:11
   |
51 |     match value:  # snapshot: non-exhaustive-match
   |           ^^^^^ Subject has type `Wide@wide`
help: Add a `case` branch for the remaining values
   |
52 |         case Color.RED:
   -             pass
53 +             pass
54 +         case _:
55 +             raise NotImplementedError("TODO")
   |
note: This is a display-only fix and is likely to be incorrect
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
    match value:  # snapshot: non-exhaustive-match
        case Color.RED:
            pass
```

```snapshot
error[non-exhaustive-match]: Match is not exhaustive: enum variants `Color.BLUE` and `Color.GREEN` are not covered
  --> src/mdtest_snippet.py:13:11
   |
13 |     match value:  # snapshot: non-exhaustive-match
   |           ^^^^^ Subject has type `ColorId`
   |
  ::: src/mdtest_snippet.py:6:5
   |
 6 |     BLUE = 2
   |     ---- enum variant `BLUE` is not covered
 7 |     GREEN = 3
   |     ----- enum variant `GREEN` is not covered
help: Add a `case` branch for the remaining values
   |
15 |             pass
16 +         case Color.BLUE | Color.GREEN:
17 +             raise NotImplementedError("TODO")
18 | def nested(value: NestedColorId) -> None:
   |
note: This is a display-only fix and is likely to be incorrect
```

```py
def nested(value: NestedColorId) -> None:
    if value is not Color.GREEN:
        # error: [non-exhaustive-match] "enum variant `Color.BLUE` is not covered"
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
    match value:  # snapshot: non-exhaustive-match
        case Permission.READ:
            pass
```

```snapshot
error[non-exhaustive-match]: Match is not exhaustive: objects of type `T@type_variable & ~Literal[Permission.READ]` are not covered
  --> src/mdtest_snippet.py:12:11
   |
12 |     match value:  # snapshot: non-exhaustive-match
   |           ^^^^^ Subject has type `T@type_variable`
info: `enum.Flag` can have unnamed combinations of members
info: See https://docs.python.org/3/howto/enum.html#combining-members-of-flag
help: Add a `case` branch for the remaining values
   |
14 |             pass
15 +         case _:
16 +             raise NotImplementedError("TODO")
17 | def newtype(value: PermissionId) -> None:
   |
note: This is a display-only fix and is likely to be incorrect
```

```py
def newtype(value: PermissionId) -> None:
    match value:  # snapshot: non-exhaustive-match
        case Permission.READ:
            pass
```

```snapshot
error[non-exhaustive-match]: Match is not exhaustive: objects of type `PermissionId & ~Literal[Permission.READ]` are not covered
  --> src/mdtest_snippet.py:16:11
   |
16 |     match value:  # snapshot: non-exhaustive-match
   |           ^^^^^ Subject has type `PermissionId`
info: `enum.Flag` can have unnamed combinations of members
info: See https://docs.python.org/3/howto/enum.html#combining-members-of-flag
help: Add a `case` branch for the remaining values
   |
17 |         case Permission.READ:
   -             pass
18 +             pass
19 +         case _:
20 +             raise NotImplementedError("TODO")
   |
note: This is a display-only fix and is likely to be incorrect
```

## Type aliases

The recursive alias `Tree` includes an open `list` alternative, so its values cannot be enumerated.

```toml
[environment]
python-version = "3.12"

[rules]
non-exhaustive-match = "error"
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
    # error: [non-exhaustive-match] "enum variant `Color.BLUE` is not covered"
    match value:
        case Color.RED:
            pass

def mixed(value: Mixed) -> None:
    # error: [non-exhaustive-match] "values `Color.BLUE` and `7` are not covered"
    match value:
        case Color.RED:
            pass

def bounded[T: Nested](value: T) -> None:
    # error: [non-exhaustive-match] "enum variant `Color.BLUE` is not covered"
    match value:
        case Color.RED:
            pass
```

```py
def recursive(value: Tree) -> None:
    match value:  # snapshot: non-exhaustive-match
        case Color.RED:
            pass
```

```snapshot
error[non-exhaustive-match]: Match is not exhaustive: objects of type `Literal[Color.BLUE] | list[Tree]` are not covered
  --> src/mdtest_snippet.py:31:11
   |
31 |     match value:  # snapshot: non-exhaustive-match
   |           ^^^^^ Subject has type `Tree`
help: Add a `case` branch for the remaining values
   |
33 |             pass
34 +         case _:
35 +             raise NotImplementedError("TODO")
36 | def recursive_bound[T: Tree](value: T) -> None:
   |
note: This is a display-only fix and is likely to be incorrect
```

```py
def recursive_bound[T: Tree](value: T) -> None:
    match value:  # snapshot: non-exhaustive-match
        case Color.RED:
            pass
```

```snapshot
error[non-exhaustive-match]: Match is not exhaustive: objects of type `T@recursive_bound & ~Literal[Color.RED]` are not covered
  --> src/mdtest_snippet.py:35:11
   |
35 |     match value:  # snapshot: non-exhaustive-match
   |           ^^^^^ Subject has type `T@recursive_bound`
help: Add a `case` branch for the remaining values
   |
36 |         case Color.RED:
   -             pass
37 +             pass
38 +         case _:
39 +             raise NotImplementedError("TODO")
   |
note: This is a display-only fix and is likely to be incorrect
```

## Enum and literal unions

```py
from enum import Enum
from typing import Literal

class Color(Enum):
    RED = 1
    BLUE = 2

def describe(value: Color | Literal["stop"]) -> None:
    match value:  # snapshot: non-exhaustive-match
        case Color.RED:
            pass
```

```snapshot
error[non-exhaustive-match]: Match is not exhaustive: values `Color.BLUE` and `"stop"` are not covered
 --> src/mdtest_snippet.py:9:11
  |
6 |     BLUE = 2
  |     ---- enum variant `Color.BLUE` is not covered
7 |
8 | def describe(value: Color | Literal["stop"]) -> None:
9 |     match value:  # snapshot: non-exhaustive-match
  |           ^^^^^ Subject has type `Color | Literal["stop"]`
help: Add a `case` branch for the remaining values
   |
10 |         case Color.RED:
   -             pass
11 +             pass
12 +         case Color.BLUE | "stop":
13 +             raise NotImplementedError("TODO")
   |
note: This is a display-only fix and is likely to be incorrect
```

## Literal unions

```py
from typing import Literal

def describe(value: Literal["red", "green", "blue"]) -> None:
    match value:  # snapshot: non-exhaustive-match
        case "red":
            pass
```

```snapshot
error[non-exhaustive-match]: Match is not exhaustive: values `"green"` and `"blue"` are not covered
 --> src/mdtest_snippet.py:4:11
  |
4 |     match value:  # snapshot: non-exhaustive-match
  |           ^^^^^ Subject has type `Literal["red", "green", "blue"]`
help: Add a `case` branch for the remaining values
  |
5 |         case "red":
  -             pass
6 +             pass
7 +         case "green" | "blue":
8 +             raise NotImplementedError("TODO")
  |
note: This is a display-only fix and is likely to be incorrect
```

## Escaped string literals

```py
from typing import Literal

def describe(value: Literal["handled", "\x1b", "\u200b", "\U000e0001"]) -> None:
    # error: [non-exhaustive-match] "Match is not exhaustive: values `"\x1b"`, `"\u200b"` and `"\U000e0001"` are not covered"
    match value:
        case "handled":
            pass
```

```py
def line_break(value: Literal["handled", "\n"]) -> None:
    match value:  # snapshot: non-exhaustive-match
        case "handled":
            pass
```

```snapshot
error[non-exhaustive-match]: Match is not exhaustive: value `"\n"` is not covered
 --> src/mdtest_snippet.py:9:11
  |
9 |     match value:  # snapshot: non-exhaustive-match
  |           ^^^^^ Subject has type `Literal["handled", "\n"]`
help: Add a `case` branch for the remaining values
   |
10 |         case "handled":
   -             pass
11 +             pass
12 +         case "\n":
13 +             raise NotImplementedError("TODO")
   |
note: This is a display-only fix and is likely to be incorrect
```

## Byte literals

```py
from typing import Literal

def describe(value: Literal[b"red", b"blue"]) -> None:
    match value:  # snapshot: non-exhaustive-match
        case b"red":
            pass
```

```snapshot
error[non-exhaustive-match]: Match is not exhaustive: value `b"blue"` is not covered
 --> src/mdtest_snippet.py:4:11
  |
4 |     match value:  # snapshot: non-exhaustive-match
  |           ^^^^^ Subject has type `Literal[b"red", b"blue"]`
help: Add a `case` branch for the remaining values
  |
5 |         case b"red":
  -             pass
6 +             pass
7 +         case b"blue":
8 +             raise NotImplementedError("TODO")
  |
note: This is a display-only fix and is likely to be incorrect
```

## Boolean literals

Ty treats a `Literal[True, False]` subject as `bool` and reports the uncovered type. For a union of
a boolean literal and another literal, it lists the uncovered value.

```py
from typing import Literal

def describe(value: Literal[True, False]) -> None:
    match value:  # snapshot: non-exhaustive-match
        case True:
            pass
```

```snapshot
error[non-exhaustive-match]: Match is not exhaustive: objects of type `Literal[False]` are not covered
 --> src/mdtest_snippet.py:4:11
  |
4 |     match value:  # snapshot: non-exhaustive-match
  |           ^^^^^ Subject has type `bool`
help: Add a `case` branch for the remaining values
  |
6 |             pass
7 +         case False:
8 +             raise NotImplementedError("TODO")
9 | def mixed(value: Literal[True, "stop"]) -> None:
  |
note: This is a display-only fix and is likely to be incorrect
```

```py
def mixed(value: Literal[True, "stop"]) -> None:
    match value:  # snapshot: non-exhaustive-match
        case "stop":
            pass
```

```snapshot
error[non-exhaustive-match]: Match is not exhaustive: value `True` is not covered
 --> src/mdtest_snippet.py:8:11
  |
8 |     match value:  # snapshot: non-exhaustive-match
  |           ^^^^^ Subject has type `Literal[True, "stop"]`
help: Add a `case` branch for the remaining values
   |
9  |         case "stop":
   -             pass
10 +             pass
11 +         case True:
12 +             raise NotImplementedError("TODO")
   |
note: This is a display-only fix and is likely to be incorrect
```

## `LiteralString`

```py
from typing import LiteralString

def describe(value: LiteralString) -> None:
    match value:  # snapshot: non-exhaustive-match
        case "red":
            pass
```

```snapshot
error[non-exhaustive-match]: Match is not exhaustive: objects of type `LiteralString & ~Literal["red"]` are not covered
 --> src/mdtest_snippet.py:4:11
  |
4 |     match value:  # snapshot: non-exhaustive-match
  |           ^^^^^ Subject has type `LiteralString`
help: Add a `case` branch for the remaining values
  |
5 |         case "red":
  -             pass
6 +             pass
7 +         case _:
8 +             raise NotImplementedError("TODO")
  |
note: This is a display-only fix and is likely to be incorrect
```

## More than three missing literal values

```py
from typing import Literal

def describe(value: Literal["red", "green", "blue", "white", "black"]) -> None:
    match value:  # snapshot: non-exhaustive-match
        case "red":
            pass
```

```snapshot
error[non-exhaustive-match]: Match is not exhaustive: values `"green"`, `"blue"`, `"white"` and 1 more are not covered
 --> src/mdtest_snippet.py:4:11
  |
4 |     match value:  # snapshot: non-exhaustive-match
  |           ^^^^^ Subject has type `Literal["red", "green", "blue", "white", "black"]`
info: Use `--verbose` to see all 4 uncovered values
help: Add a `case` branch for the remaining values
  |
5 |         case "red":
  -             pass
6 +             pass
7 +         case "green" | "blue" | "white" | "black":
8 +             raise NotImplementedError("TODO")
  |
note: This is a display-only fix and is likely to be incorrect
```

## Five missing literal values

```py
from typing import Literal

def describe(value: Literal[0, 1, 2, 3, 4, 5]) -> None:
    match value:  # snapshot: non-exhaustive-match
        case 0:
            pass
```

```snapshot
error[non-exhaustive-match]: Match is not exhaustive: values `1`, `2`, `3` and 2 more are not covered
 --> src/mdtest_snippet.py:4:11
  |
4 |     match value:  # snapshot: non-exhaustive-match
  |           ^^^^^ Subject has type `Literal[0, 1, 2, 3, 4, 5]`
info: Use `--verbose` to see all 5 uncovered values
help: Add a `case` branch for the remaining values
  |
5 |         case 0:
  -             pass
6 +             pass
7 +         case 1 | 2 | 3 | 4 | 5:
8 +             raise NotImplementedError("TODO")
  |
note: This is a display-only fix and is likely to be incorrect
```

## More than five missing literal values

```py
from typing import Literal

def describe(value: Literal[0, 1, 2, 3, 4, 5, 6]) -> None:
    match value:  # snapshot: non-exhaustive-match
        case 0:
            pass
```

```snapshot
error[non-exhaustive-match]: Match is not exhaustive: values `1`, `2`, `3` and 3 more are not covered
 --> src/mdtest_snippet.py:4:11
  |
4 |     match value:  # snapshot: non-exhaustive-match
  |           ^^^^^ Subject has type `Literal[0, 1, 2, 3, 4, 5, 6]`
info: Use `--verbose` to see all 6 uncovered values
help: Add a `case` branch for the remaining values
  |
5 |         case 0:
  -             pass
6 +             pass
7 +         case _:
8 +             raise NotImplementedError("TODO")
  |
note: This is a display-only fix and is likely to be incorrect
```

## Functional enum

```py
from enum import Enum

Color = Enum("Color", "RED GREEN BLUE")

def describe(value: Color) -> None:
    match value:  # snapshot: non-exhaustive-match
        case Color.RED:
            pass
```

```snapshot
error[non-exhaustive-match]: Match is not exhaustive: enum variants `Color.GREEN` and `Color.BLUE` are not covered
 --> src/mdtest_snippet.py:6:11
  |
3 | Color = Enum("Color", "RED GREEN BLUE")
  |                       ---------------- Enum variants `GREEN` and `BLUE` are not covered
4 |
5 | def describe(value: Color) -> None:
6 |     match value:  # snapshot: non-exhaustive-match
  |           ^^^^^ Subject has type `Color`
help: Add a `case` branch for the remaining values
   |
7  |         case Color.RED:
   -             pass
8  +             pass
9  +         case Color.GREEN | Color.BLUE:
10 +             raise NotImplementedError("TODO")
   |
note: This is a display-only fix and is likely to be incorrect
```

## Multiple functional enums

The diagnostic groups missing members by enum definition in its annotations.

```py
from enum import Enum

Color = Enum("Color", "RED GREEN BLUE")
Direction = Enum("Direction", "NORTH SOUTH")

def describe(value: Color | Direction) -> None:
    match value:  # snapshot: non-exhaustive-match
        case Color.RED | Direction.NORTH:
            pass
```

```snapshot
error[non-exhaustive-match]: Match is not exhaustive: enum variants `Color.GREEN`, `Color.BLUE` and `Direction.SOUTH` are not covered
 --> src/mdtest_snippet.py:7:11
  |
3 | Color = Enum("Color", "RED GREEN BLUE")
  |                       ---------------- Enum variants `Color.GREEN` and `Color.BLUE` are not covered
4 | Direction = Enum("Direction", "NORTH SOUTH")
  |                               ------------- Enum variant `Direction.SOUTH` is not covered
5 |
6 | def describe(value: Color | Direction) -> None:
7 |     match value:  # snapshot: non-exhaustive-match
  |           ^^^^^ Subject has type `Color | Direction`
help: Add a `case` branch for the remaining values
   |
8  |         case Color.RED | Direction.NORTH:
   -             pass
9  +             pass
10 +         case Color.GREEN | Color.BLUE | Direction.SOUTH:
11 +             raise NotImplementedError("TODO")
   |
note: This is a display-only fix and is likely to be incorrect
```

## Functional enum with a keyword argument

```py
from enum import Enum

Color = Enum("Color", names={"RED": 1, "BLUE": 2})

def describe(value: Color) -> None:
    match value:  # snapshot: non-exhaustive-match
        case Color.RED:
            pass
```

```snapshot
error[non-exhaustive-match]: Match is not exhaustive: enum variant `Color.BLUE` is not covered
 --> src/mdtest_snippet.py:6:11
  |
3 | Color = Enum("Color", names={"RED": 1, "BLUE": 2})
  |                             --------------------- Enum variant `BLUE` is not covered
4 |
5 | def describe(value: Color) -> None:
6 |     match value:  # snapshot: non-exhaustive-match
  |           ^^^^^ Subject has type `Color`
help: Add a `case` branch for the remaining values
   |
7  |         case Color.RED:
   -             pass
8  +             pass
9  +         case Color.BLUE:
10 +             raise NotImplementedError("TODO")
   |
note: This is a display-only fix and is likely to be incorrect
```

## A missing `None` case

```py
from typing import Literal

def describe(value: Literal["red"] | None) -> None:
    match value:  # snapshot: non-exhaustive-match
        case "red":
            pass
```

```snapshot
error[non-exhaustive-match]: Match is not exhaustive: `None` is not covered
 --> src/mdtest_snippet.py:4:11
  |
4 |     match value:  # snapshot: non-exhaustive-match
  |           ^^^^^ Subject has type `Literal["red"] | None`
help: Add a `case` branch for the remaining values
  |
5 |         case "red":
  -             pass
6 +             pass
7 +         case None:
8 +             raise NotImplementedError("TODO")
  |
note: This is a display-only fix and is likely to be incorrect
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
    match value:  # snapshot: non-exhaustive-match
        case Hue.RED:
            pass
```

```snapshot
error[non-exhaustive-match]: Match is not exhaustive: enum variant `Color.BLUE` is not covered
 --> src/mdtest_snippet.py:4:11
  |
4 |     match value:  # snapshot: non-exhaustive-match
  |           ^^^^^ Subject has type `Color`
  |
 ::: src/colors.py:5:5
  |
5 |     BLUE = 2
  |     ---- enum variant `BLUE` is not covered
help: Add a `case` branch for the remaining values
  |
5 |         case Hue.RED:
  -             pass
6 +             pass
7 +         case Hue.BLUE:
8 +             raise NotImplementedError("TODO")
  |
note: This is a display-only fix and is likely to be incorrect
```

## An enum import alias without an enum case

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
    match value:  # snapshot: non-exhaustive-match
        case "stop":
            pass
```

```snapshot
error[non-exhaustive-match]: Match is not exhaustive: enum variants `Color.RED` and `Color.BLUE` are not covered
 --> src/mdtest_snippet.py:5:11
  |
5 |     match value:  # snapshot: non-exhaustive-match
  |           ^^^^^ Subject has type `Color | Literal["stop"]`
  |
 ::: src/colors.py:4:5
  |
4 |     RED = 1
  |     --- enum variant `Color.RED` is not covered
5 |     BLUE = 2
  |     ---- enum variant `Color.BLUE` is not covered
help: Add a `case` branch for the remaining values
  |
6 |         case "stop":
  -             pass
7 +             pass
8 +         case Hue.RED | Hue.BLUE:
9 +             raise NotImplementedError("TODO")
  |
note: This is a display-only fix and is likely to be incorrect
```

## A module import alias without an enum case

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
    match value:  # snapshot: non-exhaustive-match
        case "stop":
            pass
```

```snapshot
error[non-exhaustive-match]: Match is not exhaustive: enum variants `Color.RED` and `Color.BLUE` are not covered
 --> src/mdtest_snippet.py:5:11
  |
5 |     match value:  # snapshot: non-exhaustive-match
  |           ^^^^^ Subject has type `Color | Literal["stop"]`
  |
 ::: src/colors.py:4:5
  |
4 |     RED = 1
  |     --- enum variant `Color.RED` is not covered
5 |     BLUE = 2
  |     ---- enum variant `Color.BLUE` is not covered
help: Add a `case` branch for the remaining values
  |
6 |         case "stop":
  -             pass
7 +             pass
8 +         case palette.Color.RED | palette.Color.BLUE:
9 +             raise NotImplementedError("TODO")
  |
note: This is a display-only fix and is likely to be incorrect
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
    match value:  # snapshot: non-exhaustive-match
        case Hue.RED if Hue := None:  # ty: ignore[unresolved-attribute]
            pass
        case "stop":
            pass
```

```snapshot
error[non-exhaustive-match]: Match is not exhaustive: enum variants `Color.RED` and `Color.BLUE` are not covered
 --> src/mdtest_snippet.py:6:11
  |
6 |     match value:  # snapshot: non-exhaustive-match
  |           ^^^^^ Subject has type `Color | Literal["stop"]`
  |
 ::: src/colors.py:4:5
  |
4 |     RED = 1
  |     --- enum variant `Color.RED` is not covered
5 |     BLUE = 2
  |     ---- enum variant `Color.BLUE` is not covered
help: Add a `case` branch for the remaining values
   |
9  |         case "stop":
   -             pass
10 +             pass
11 +         case _:
12 +             raise NotImplementedError("TODO")
   |
note: This is a display-only fix and is likely to be incorrect
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
    match value:  # snapshot: non-exhaustive-match
        case "stop":
            pass
```

```snapshot
error[non-exhaustive-match]: Match is not exhaustive: enum variants `Color.RED` and `Color.BLUE` are not covered
 --> src/mdtest_snippet.py:6:11
  |
6 |     match value:  # snapshot: non-exhaustive-match
  |           ^^^^^ Subject has type `Color | Literal["stop"]`
  |
 ::: src/colors.py:4:5
  |
4 |     RED = 1
  |     --- enum variant `Color.RED` is not covered
5 |     BLUE = 2
  |     ---- enum variant `Color.BLUE` is not covered
help: Add a `case` branch for the remaining values
   |
7  |         case "stop":
   -             pass
8  +             pass
9  +         case _:
10 +             raise NotImplementedError("TODO")
   |
note: This is a display-only fix and is likely to be incorrect
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
    match value:  # snapshot: non-exhaustive-match
        case palette.Color.RED if rebind():
            pass
        case "stop":
            pass
```

```snapshot
error[non-exhaustive-match]: Match is not exhaustive: enum variants `Color.RED` and `Color.BLUE` are not covered
 --> src/mdtest_snippet.py:9:11
  |
9 |     match value:  # snapshot: non-exhaustive-match
  |           ^^^^^ Subject has type `Color | Literal["stop"]`
  |
 ::: src/colors.py:4:5
  |
4 |     RED = 1
  |     --- enum variant `Color.RED` is not covered
5 |     BLUE = 2
  |     ---- enum variant `Color.BLUE` is not covered
help: Add a `case` branch for the remaining values
   |
12 |         case "stop":
   -             pass
13 +             pass
14 +         case _:
15 +             raise NotImplementedError("TODO")
   |
note: This is a display-only fix and is likely to be incorrect
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
    match value:  # snapshot: non-exhaustive-match
        case "stop":
            pass
```

```snapshot
error[non-exhaustive-match]: Match is not exhaustive: enum variants `Color.RED` and `Color.BLUE` are not covered
 --> src/mdtest_snippet.py:8:11
  |
8 |     match value:  # snapshot: non-exhaustive-match
  |           ^^^^^ Subject has type `Color | Literal["stop"]`
  |
 ::: src/colors.py:4:5
  |
4 |     RED = 1
  |     --- enum variant `Color.RED` is not covered
5 |     BLUE = 2
  |     ---- enum variant `Color.BLUE` is not covered
help: Add a `case` branch for the remaining values
   |
9  |         case "stop":
   -             pass
10 +             pass
11 +         case _:
12 +             raise NotImplementedError("TODO")
   |
note: This is a display-only fix and is likely to be incorrect
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
    match value:  # snapshot: non-exhaustive-match
        case "stop":
            pass
```

```snapshot
error[non-exhaustive-match]: Match is not exhaustive: enum variants `Color.RED` and `Color.BLUE` are not covered
 --> src/mdtest_snippet.py:5:11
  |
5 |     match value:  # snapshot: non-exhaustive-match
  |           ^^^^^ Subject has type `Color | Literal["stop"]`
  |
 ::: src/colors.py:4:5
  |
4 |     RED = 1
  |     --- enum variant `Color.RED` is not covered
5 |     BLUE = 2
  |     ---- enum variant `Color.BLUE` is not covered
help: Add a `case` branch for the remaining values
  |
6 |         case "stop":
  -             pass
7 +             pass
8 +         case _:
9 +             raise NotImplementedError("TODO")
  |
note: This is a display-only fix and is likely to be incorrect
```

## An enum defined in the same file

```py
from enum import Enum
from typing import Literal

class Color(Enum):
    RED = 1
    BLUE = 2

def describe(value: Color | Literal["stop"]) -> None:
    match value:  # snapshot: non-exhaustive-match
        case "stop":
            pass
```

```snapshot
error[non-exhaustive-match]: Match is not exhaustive: enum variants `Color.RED` and `Color.BLUE` are not covered
 --> src/mdtest_snippet.py:9:11
  |
5 |     RED = 1
  |     --- enum variant `Color.RED` is not covered
6 |     BLUE = 2
  |     ---- enum variant `Color.BLUE` is not covered
7 |
8 | def describe(value: Color | Literal["stop"]) -> None:
9 |     match value:  # snapshot: non-exhaustive-match
  |           ^^^^^ Subject has type `Color | Literal["stop"]`
help: Add a `case` branch for the remaining values
   |
10 |         case "stop":
   -             pass
11 +             pass
12 +         case Color.RED | Color.BLUE:
13 +             raise NotImplementedError("TODO")
   |
note: This is a display-only fix and is likely to be incorrect
```

## An enum name shadowed by a parameter

```py
from enum import Enum
from typing import Literal

class Color(Enum):
    RED = 1
    BLUE = 2

def describe(value: Color | Literal["stop"], Color: int) -> None:
    match value:  # snapshot: non-exhaustive-match
        case "stop":
            pass
```

```snapshot
error[non-exhaustive-match]: Match is not exhaustive: enum variants `Color.RED` and `Color.BLUE` are not covered
 --> src/mdtest_snippet.py:9:11
  |
5 |     RED = 1
  |     --- enum variant `Color.RED` is not covered
6 |     BLUE = 2
  |     ---- enum variant `Color.BLUE` is not covered
7 |
8 | def describe(value: Color | Literal["stop"], Color: int) -> None:
9 |     match value:  # snapshot: non-exhaustive-match
  |           ^^^^^ Subject has type `Color | Literal["stop"]`
help: Add a `case` branch for the remaining values
   |
10 |         case "stop":
   -             pass
11 +             pass
12 +         case _:
13 +             raise NotImplementedError("TODO")
   |
note: This is a display-only fix and is likely to be incorrect
```

## An enum defined after a match

Ty suggests a wildcard when the enum is defined later in the file, even if it might be available
when the function is called.

```py
from __future__ import annotations
from enum import Enum
from typing import Literal

def describe(value: Color | Literal["stop"]) -> None:
    match value:  # snapshot: non-exhaustive-match
        case "stop":
            pass

class Color(Enum):
    RED = 1
    BLUE = 2
```

```snapshot
error[non-exhaustive-match]: Match is not exhaustive: enum variants `Color.RED` and `Color.BLUE` are not covered
  --> src/mdtest_snippet.py:6:11
   |
 6 |     match value:  # snapshot: non-exhaustive-match
   |           ^^^^^ Subject has type `Color | Literal["stop"]`
   |
  ::: src/mdtest_snippet.py:11:5
   |
11 |     RED = 1
   |     --- enum variant `Color.RED` is not covered
12 |     BLUE = 2
   |     ---- enum variant `Color.BLUE` is not covered
help: Add a `case` branch for the remaining values
   |
8  |             pass
9  +         case _:
10 +             raise NotImplementedError("TODO")
11 |
   |
note: This is a display-only fix and is likely to be incorrect
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
    match narrowed:  # snapshot: non-exhaustive-match
        case "stop":
            pass
```

```snapshot
error[non-exhaustive-match]: Match is not exhaustive: enum variants `Color.RED` and `Color.BLUE` are not covered
  --> src/mdtest_snippet.py:10:11
   |
 6 |         RED = 1
   |         --- enum variant `Color.RED` is not covered
 7 |         BLUE = 2
   |         ---- enum variant `Color.BLUE` is not covered
 8 |
 9 |     narrowed = cast(Color | Literal["stop"], value)
10 |     match narrowed:  # snapshot: non-exhaustive-match
   |           ^^^^^^^^ Subject has type `Color | Literal["stop"]`
help: Add a `case` branch for the remaining values
   |
11 |         case "stop":
   -             pass
12 +             pass
13 +         case Color.RED | Color.BLUE:
14 +             raise NotImplementedError("TODO")
   |
note: This is a display-only fix and is likely to be incorrect
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
    match value:  # snapshot: non-exhaustive-match
        case first.Color.RED | second.Color.RED:
            pass
```

```snapshot
error[non-exhaustive-match]: Match is not exhaustive: enum variants `first.Color.BLUE` and `second.Color.BLUE` are not covered
 --> src/mdtest_snippet.py:5:11
  |
5 |     match value:  # snapshot: non-exhaustive-match
  |           ^^^^^ Subject has type `first.Color | second.Color`
  |
 ::: src/first.py:5:5
  |
5 |     BLUE = 2
  |     ---- enum variant `first.Color.BLUE` is not covered
  |
 ::: src/second.py:5:5
  |
5 |     BLUE = 2
  |     ---- enum variant `second.Color.BLUE` is not covered
help: Add a `case` branch for the remaining values
  |
6 |         case first.Color.RED | second.Color.RED:
  -             pass
7 +             pass
8 +         case first.Color.BLUE | second.Color.BLUE:
9 +             raise NotImplementedError("TODO")
  |
note: This is a display-only fix and is likely to be incorrect
```

## Flag instances

Flag members do not necessarily exhaust the possible flag values.

```py
from enum import Flag

class Permission(Flag):
    READ = 1
    WRITE = 2

def describe(value: Permission) -> None:
    match value:  # snapshot: non-exhaustive-match
        case Permission.READ:
            pass
```

```snapshot
error[non-exhaustive-match]: Match is not exhaustive: objects of type `Permission & ~Literal[Permission.READ]` are not covered
 --> src/mdtest_snippet.py:8:11
  |
8 |     match value:  # snapshot: non-exhaustive-match
  |           ^^^^^ Subject has type `Permission`
info: `enum.Flag` can have unnamed combinations of members
info: See https://docs.python.org/3/howto/enum.html#combining-members-of-flag
help: Add a `case` branch for the remaining values
   |
10 |             pass
11 +         case _:
12 +             raise NotImplementedError("TODO")
13 | def all_members(value: Permission) -> None:
   |
note: This is a display-only fix and is likely to be incorrect
```

The `|` pattern matches either named member, but not a combination of them.

```py
def all_members(value: Permission) -> None:
    match value:  # snapshot: non-exhaustive-match
        case Permission.READ | Permission.WRITE:
            pass
```

```snapshot
error[non-exhaustive-match]: Match is not exhaustive: objects of type `Permission & ~Literal[Permission.READ] & ~Literal[Permission.WRITE]` are not covered
  --> src/mdtest_snippet.py:12:11
   |
12 |     match value:  # snapshot: non-exhaustive-match
   |           ^^^^^ Subject has type `Permission`
info: `enum.Flag` can have unnamed combinations of members
info: See https://docs.python.org/3/howto/enum.html#combining-members-of-flag
help: Add a `case` branch for the remaining values
   |
13 |         case Permission.READ | Permission.WRITE:
   -             pass
14 +             pass
15 +         case _:
16 +             raise NotImplementedError("TODO")
   |
note: This is a display-only fix and is likely to be incorrect
```

## Literal flag members

```py
from enum import Flag
from typing import Literal

class Permission(Flag):
    READ = 1
    WRITE = 2

def describe(value: Literal[Permission.READ, Permission.WRITE]) -> None:
    match value:  # snapshot: non-exhaustive-match
        case Permission.READ:
            pass
```

```snapshot
error[non-exhaustive-match]: Match is not exhaustive: enum variant `Permission.WRITE` is not covered
 --> src/mdtest_snippet.py:9:11
  |
6 |     WRITE = 2
  |     ----- enum variant `WRITE` is not covered
7 |
8 | def describe(value: Literal[Permission.READ, Permission.WRITE]) -> None:
9 |     match value:  # snapshot: non-exhaustive-match
  |           ^^^^^ Subject has type `Literal[Permission.READ, Permission.WRITE]`
help: Add a `case` branch for the remaining values
   |
10 |         case Permission.READ:
   -             pass
11 +             pass
12 +         case Permission.WRITE:
13 +             raise NotImplementedError("TODO")
   |
note: This is a display-only fix and is likely to be incorrect
```

## Flag instances in a union

```py
from enum import IntFlag

class Permission(IntFlag):
    READ = 1
    WRITE = 2

def unhandled_flag_and_string(value: Permission | str | bytes) -> None:
    match value:  # snapshot: non-exhaustive-match
        case bytes():
            pass
```

```snapshot
error[non-exhaustive-match]: Match is not exhaustive: objects of type `Permission | str` are not covered
 --> src/mdtest_snippet.py:8:11
  |
8 |     match value:  # snapshot: non-exhaustive-match
  |           ^^^^^ Subject has type `Permission | str | bytes`
info: `enum.Flag` can have unnamed combinations of members
info: See https://docs.python.org/3/howto/enum.html#combining-members-of-flag
help: Add a `case` branch for the remaining values
   |
10 |             pass
11 +         case _:
12 +             raise NotImplementedError("TODO")
13 | def unhandled_string(value: Permission | str) -> None:
   |
note: This is a display-only fix and is likely to be incorrect
```

When the flag is already covered, the diagnostic does not include the explanation about flags.

```py
def unhandled_string(value: Permission | str) -> None:
    match value:  # snapshot: non-exhaustive-match
        case Permission():
            pass
```

```snapshot
error[non-exhaustive-match]: Match is not exhaustive: objects of type `str` are not covered
  --> src/mdtest_snippet.py:12:11
   |
12 |     match value:  # snapshot: non-exhaustive-match
   |           ^^^^^ Subject has type `Permission | str`
help: Add a `case` branch for the remaining values
   |
13 |         case Permission():
   -             pass
14 +             pass
15 +         case _:
16 +             raise NotImplementedError("TODO")
   |
note: This is a display-only fix and is likely to be incorrect
```

## A union containing an open type

```py
from enum import Enum

class Color(Enum):
    RED = 1
    BLUE = 2

def describe(value: Color | str) -> None:
    match value:  # snapshot: non-exhaustive-match
        case Color.RED:
            pass
```

```snapshot
error[non-exhaustive-match]: Match is not exhaustive: objects of type `Literal[Color.BLUE] | str` are not covered
 --> src/mdtest_snippet.py:8:11
  |
8 |     match value:  # snapshot: non-exhaustive-match
  |           ^^^^^ Subject has type `Color | str`
help: Add a `case` branch for the remaining values
   |
9  |         case Color.RED:
   -             pass
10 +             pass
11 +         case _:
12 +             raise NotImplementedError("TODO")
   |
note: This is a display-only fix and is likely to be incorrect
```

## Suggested wildcard case

```py
def describe(value: int | str) -> None:
    match value:  # snapshot: non-exhaustive-match
        case int():
            pass  # Kept with the existing case.
```

```snapshot
error[non-exhaustive-match]: Match is not exhaustive: objects of type `str` are not covered
 --> src/mdtest_snippet.py:2:11
  |
2 |     match value:  # snapshot: non-exhaustive-match
  |           ^^^^^ Subject has type `int | str`
help: Add a `case` branch for the remaining values
  |
3 |         case int():
  -             pass  # Kept with the existing case.
4 +             pass  # Kept with the existing case.
5 +         case _:
6 +             raise NotImplementedError("TODO")
  |
note: This is a display-only fix and is likely to be incorrect
```

## Nested match with trailing comments

```py
from typing import Literal

def describe(value: Literal[1, 2], enabled: bool) -> None:
    if enabled:
        match value:  # snapshot: non-exhaustive-match
            case 1:
                pass
                # This comment belongs to the existing case.
```

```snapshot
error[non-exhaustive-match]: Match is not exhaustive: value `2` is not covered
 --> src/mdtest_snippet.py:5:15
  |
5 |         match value:  # snapshot: non-exhaustive-match
  |               ^^^^^ Subject has type `Literal[1, 2]`
help: Add a `case` branch for the remaining values
   |
7  |                 pass
   -                 # This comment belongs to the existing case.
8  +                 # This comment belongs to the existing case.
9  +             case 2:
10 +                 raise NotImplementedError("TODO")
   |
note: This is a display-only fix and is likely to be incorrect
```

## Single-line case body

```py
from typing import Literal

def describe(value: Literal["red", "green"]) -> None:
    # fmt: off
    match value:  # snapshot: non-exhaustive-match
        case "red": pass  # Kept with the existing case.
    # fmt: on
```

```snapshot
error[non-exhaustive-match]: Match is not exhaustive: value `"green"` is not covered
 --> src/mdtest_snippet.py:5:11
  |
5 |     match value:  # snapshot: non-exhaustive-match
  |           ^^^^^ Subject has type `Literal["red", "green"]`
help: Add a `case` branch for the remaining values
  |
6 |         case "red": pass  # Kept with the existing case.
7 +         case "green":
8 +             raise NotImplementedError("TODO")
9 |     # fmt: on
  |
note: This is a display-only fix and is likely to be incorrect
```

## Literal patterns

```py
from typing import Literal

def incomplete(value: Literal["red", "green", "blue"]) -> None:
    # error: [non-exhaustive-match] "Match is not exhaustive: value `"blue"` is not covered"
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
    match value:  # snapshot: non-exhaustive-match
        case Color.RED:
            pass
```

```snapshot
error[non-exhaustive-match]: Match is not exhaustive: enum variants `Color.GREEN` and `Color.BLUE` are not covered
  --> src/mdtest_snippet.py:10:11
   |
 6 |     GREEN = 2
   |     ----- enum variant `GREEN` is not covered
 7 |     BLUE = 3
   |     ---- enum variant `BLUE` is not covered
 8 |
 9 | def incomplete(value: Color) -> None:
10 |     match value:  # snapshot: non-exhaustive-match
   |           ^^^^^ Subject has type `Color`
help: Add a `case` branch for the remaining values
   |
12 |             pass
13 +         case Color.GREEN | Color.BLUE:
14 +             raise NotImplementedError("TODO")
15 | def complete(value: Color) -> None:
   |
note: This is a display-only fix and is likely to be incorrect
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
    # error: [non-exhaustive-match] "Match is not exhaustive: objects of type `int & ~Literal[1] & ~Literal[True]` are not covered"
    match value:
        case 1:
            pass
```

## Tuple patterns

```py
def incomplete(value: tuple[bool, bool]) -> None:
    # error: [non-exhaustive-match] "Match is not exhaustive: objects of type `tuple[Literal[False], Literal[False]]` are not covered"
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
    match (first, second):  # snapshot: non-exhaustive-match
        case (True, _):
            pass
```

```snapshot
error[non-exhaustive-match]: Match is not exhaustive: objects of type `tuple[Literal[False], Literal[True]] | tuple[Literal[False], Literal[False]]` are not covered
  --> src/mdtest_snippet.py:25:11
   |
25 |     match (first, second):  # snapshot: non-exhaustive-match
   |           ^^^^^^^^^^^^^^^ Subject has type `tuple[bool, bool]`
help: Add a `case` branch for the remaining values
   |
26 |         case (True, _):
   -             pass
27 +             pass
28 +         case _:
29 +             raise NotImplementedError("TODO")
   |
note: This is a display-only fix and is likely to be incorrect
```

## Guards

An enum reference such as `Color` can be rebound by a guard before the suggested case is reached,
causing that case to refer to a different object. Ty therefore suggests a wildcard even when the
guard shown does not rebind `Color`. Non-enum literal patterns do not resolve such a name.

```py
from enum import Enum
from typing import Literal

class Color(Enum):
    RED = 1
    BLUE = 2

def guarded_enum(value: Color, enabled: bool) -> None:
    match value:  # snapshot: non-exhaustive-match
        case Color.RED if enabled:
            pass
```

```snapshot
error[non-exhaustive-match]: Match is not exhaustive: enum variants `Color.RED` and `Color.BLUE` are not covered
 --> src/mdtest_snippet.py:9:11
  |
5 |     RED = 1
  |     --- enum variant `RED` is not covered
6 |     BLUE = 2
  |     ---- enum variant `BLUE` is not covered
7 |
8 | def guarded_enum(value: Color, enabled: bool) -> None:
9 |     match value:  # snapshot: non-exhaustive-match
  |           ^^^^^ Subject has type `Color`
help: Add a `case` branch for the remaining values
   |
11 |             pass
12 +         case _:
13 +             raise NotImplementedError("TODO")
14 | def guarded_literal(value: Literal[1, 2], enabled: bool) -> None:
   |
note: This is a display-only fix and is likely to be incorrect
```

```py
def guarded_literal(value: Literal[1, 2], enabled: bool) -> None:
    match value:  # snapshot: non-exhaustive-match
        case 1 if enabled:
            pass
```

```snapshot
error[non-exhaustive-match]: Match is not exhaustive: values `1` and `2` are not covered
  --> src/mdtest_snippet.py:13:11
   |
13 |     match value:  # snapshot: non-exhaustive-match
   |           ^^^^^ Subject has type `Literal[1, 2]`
help: Add a `case` branch for the remaining values
   |
15 |             pass
16 +         case 1 | 2:
17 +             raise NotImplementedError("TODO")
18 | def guarded(value: Literal[1, 2], flag: bool) -> None:
   |
note: This is a display-only fix and is likely to be incorrect
```

```py
def guarded(value: Literal[1, 2], flag: bool) -> None:
    match value:  # error: [non-exhaustive-match] "`1` is not covered"
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
    match value:  # error: [non-exhaustive-match] "`1` is not covered"
        case 1 if False:
            pass
        case 2:
            pass

def guarded_wildcard(value: str, flag: bool) -> None:
    match value:  # error: [non-exhaustive-match]
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
    match value:  # error: [non-exhaustive-match] "`2` is not covered"
        case 1:
            pass
        case 2 if flag:
            pass

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

def suppressed(value: int) -> None:
    match value:  # ty: ignore[non-exhaustive-match]
        case 1:
            pass
```
