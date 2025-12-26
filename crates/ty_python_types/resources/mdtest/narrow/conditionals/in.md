# Narrowing for `in` conditionals

## `in` for tuples

```py
def _(x: int):
    if x in (1, 2, 3):
        reveal_type(x)  # revealed: int
    else:
        reveal_type(x)  # revealed: int
```

```py
def _(x: str):
    if x in ("a", "b", "c"):
        reveal_type(x)  # revealed: str
    else:
        reveal_type(x)  # revealed: str
```

```py
from typing import Literal

def _(x: Literal[1, 2, "a", "b", False, b"abc"]):
    if x in (1,):
        reveal_type(x)  # revealed: Literal[1]
    elif x in (2, "a"):
        reveal_type(x)  # revealed: Literal[2, "a"]
    elif x in (b"abc",):
        reveal_type(x)  # revealed: Literal[b"abc"]
    elif x not in (3,):
        reveal_type(x)  # revealed: Literal["b", False]
    else:
        reveal_type(x)  # revealed: Never
```

```py
def _(x: Literal["a", "b", "c", 1]):
    if x in ("a", "b", "c", 2):
        reveal_type(x)  # revealed: Literal["a", "b", "c"]
    else:
        reveal_type(x)  # revealed: Literal[1]
```

## `in` for `str` and literal strings

```py
def _(x: str):
    if x in "abc":
        reveal_type(x)  # revealed: str
    else:
        reveal_type(x)  # revealed: str
```

```py
from typing import Literal

def _(x: Literal["a", "b", "c", "d"]):
    if x in "abc":
        reveal_type(x)  # revealed: Literal["a", "b", "c"]
    else:
        reveal_type(x)  # revealed: Literal["d"]
```

```py
def _(x: Literal["a", "b", "c", "e"]):
    if x in "abcd":
        reveal_type(x)  # revealed: Literal["a", "b", "c"]
    else:
        reveal_type(x)  # revealed: Literal["e"]
```

```py
def _(x: Literal[1, "a", "b", "c", "d"]):
    # error: [unsupported-operator]
    if x in "abc":
        reveal_type(x)  # revealed: Literal["a", "b", "c"]
    else:
        reveal_type(x)  # revealed: Literal[1, "d"]
```

## Assignment expressions

```py
from typing import Literal

def f() -> Literal[1, 2, 3]:
    return 1

if (x := f()) in (1,):
    reveal_type(x)  # revealed: Literal[1]
else:
    reveal_type(x)  # revealed: Literal[2, 3]
```

## Union with `Literal`, `None` and `int`

```py
from typing import Literal

def test(x: Literal["a", "b", "c"] | None | int = None):
    if x in ("a", "b"):
        # int is included because custom __eq__ methods could make
        # an int equal to "a" or "b", so we can't eliminate it
        reveal_type(x)  # revealed: Literal["a", "b"] | int
    else:
        reveal_type(x)  # revealed: Literal["c"] | None | int
```

## Direct `not in` conditional

```py
from typing import Literal

def test(x: Literal["a", "b", "c"] | None | int = None):
    if x not in ("a", "c"):
        # int is included because custom __eq__ methods could make
        # an int equal to "a" or "c", so we can't eliminate it
        reveal_type(x)  # revealed: Literal["b"] | None | int
    else:
        reveal_type(x)  # revealed: Literal["a", "c"] | int
```

## bool

```py
def _(x: bool):
    if x in (True,):
        reveal_type(x)  # revealed: Literal[True]
    else:
        reveal_type(x)  # revealed: Literal[False]

def _(x: bool | str):
    if x in (False,):
        # `str` remains due to possible custom __eq__ methods on a subclass
        reveal_type(x)  # revealed: Literal[False] | str
    else:
        reveal_type(x)  # revealed: Literal[True] | str
```

## LiteralString

```py
from typing_extensions import LiteralString

def _(x: LiteralString):
    if x in ("a", "b", "c"):
        reveal_type(x)  # revealed: Literal["a", "b", "c"]
    else:
        reveal_type(x)  # revealed: LiteralString & ~Literal["a"] & ~Literal["b"] & ~Literal["c"]

def _(x: LiteralString | int):
    if x in ("a", "b", "c"):
        reveal_type(x)  # revealed: Literal["a", "b", "c"] | int
    else:
        reveal_type(x)  # revealed: (LiteralString & ~Literal["a"] & ~Literal["b"] & ~Literal["c"]) | int
```

## enums

```py
from enum import Enum

class Color(Enum):
    RED = "red"
    GREEN = "green"
    BLUE = "blue"

def _(x: Color):
    if x in (Color.RED, Color.GREEN):
        reveal_type(x)  # revealed: Literal[Color.RED, Color.GREEN]
    else:
        reveal_type(x)  # revealed: Literal[Color.BLUE]
```

## Union with enum and `int`

```py
from enum import Enum

class Status(Enum):
    PENDING = 1
    APPROVED = 2
    REJECTED = 3

def test(x: Status | int):
    if x in (Status.PENDING, Status.APPROVED):
        # int is included because custom __eq__ methods could make
        # an int equal to Status.PENDING or Status.APPROVED, so we can't eliminate it
        reveal_type(x)  # revealed: Literal[Status.PENDING, Status.APPROVED] | int
    else:
        reveal_type(x)  # revealed: Literal[Status.REJECTED] | int
```

## Union with tuple and `Literal`

We assume that tuple subclasses don't override `tuple.__eq__`, which only returns True for other
tuples. So they are excluded from the narrowed type when disjoint from the RHS values.

```py
from typing import Literal

def test(x: Literal["none", "auto", "required"] | tuple[list[str], Literal["auto", "required"]]):
    if x in ("auto", "required"):
        # tuple type is excluded because it's disjoint from the string literals
        reveal_type(x)  # revealed: Literal["auto", "required"]
    else:
        # tuple type remains in the else branch
        reveal_type(x)  # revealed: Literal["none"] | tuple[list[str], Literal["auto", "required"]]
```
