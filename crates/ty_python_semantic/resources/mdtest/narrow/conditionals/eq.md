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
from typing import Any

def _(x: Any | None, y: Any | None):
    if x != 1:
        reveal_type(x)  # revealed: (Any & ~Literal[1]) | None
    if y == 1:
        reveal_type(y)  # revealed: Any & ~None
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
        # TODO could be `Literal["foo"] | Any`
        reveal_type(t)  # revealed: LiteralString | Any
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

## Large fallthrough conditional

Non-terminal arms in a large equality ladder can leave many paths to the statement after the
conditional. Narrowing through those paths should not enumerate the shared tails.

```py
from typing import Any, Literal, TypeGuard

Value = Literal[
    "f00",
    "v01",
    "v02",
    "v03",
    "v04",
    "v05",
    "v06",
    "v07",
    "f08",
    "v09",
    "v10",
    "v11",
    "v12",
    "v13",
    "v14",
    "v15",
    "f16",
    "v17",
    "v18",
    "v19",
    "v20",
    "v21",
    "v22",
    "v23",
    "f24",
    "v25",
    "v26",
    "v27",
    "v28",
    "v29",
    "v30",
    "v31",
    "f32",
    "v33",
    "v34",
    "v35",
    "f36",
    "v37",
    "v38",
    "v39",
]

def keep_value(value: object) -> TypeGuard[Value]:
    return True

def _(value: Value | Any) -> None:
    if value == "f00" and keep_value(value):
        pass
    elif value == "v01":
        return
    elif value == "v02":
        return
    elif value == "v03":
        return
    elif value == "v04":
        return
    elif value == "v05":
        return
    elif value == "v06":
        return
    elif value == "v07":
        return
    elif value == "f08":
        pass
    elif value == "v09":
        return
    elif value == "v10":
        return
    elif value == "v11":
        return
    elif value == "v12":
        return
    elif value == "v13":
        return
    elif value == "v14":
        return
    elif value == "v15":
        return
    elif value == "f16":
        pass
    elif value == "v17":
        return
    elif value == "v18":
        return
    elif value == "v19":
        return
    elif value == "v20":
        return
    elif value == "v21":
        return
    elif value == "v22":
        return
    elif value == "v23":
        return
    elif value == "f24":
        pass
    elif value == "v25":
        return
    elif value == "v26":
        return
    elif value == "v27":
        return
    elif value == "v28":
        return
    elif value == "v29":
        return
    elif value == "v30":
        return
    elif value == "v31":
        return
    elif value == "f32":
        pass
    elif value == "v33":
        return
    elif value == "v34":
        return
    elif value == "v35":
        return
    elif value == "f36":
        pass
    elif value == "v37":
        return
    elif value == "v38":
        return
    elif value == "v39":
        return

    repr(value)
```
