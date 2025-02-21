# Truthiness

## Literals

```py
from typing_extensions import Literal, LiteralString
from knot_extensions import AlwaysFalsy, AlwaysTruthy

def _(
    a: Literal[1],
    b: Literal[-1],
    c: Literal["foo"],
    d: tuple[Literal[0]],
    e: Literal[1, 2],
    f: AlwaysTruthy,
):
    reveal_type(bool(a))  # revealed: Literal[True]
    reveal_type(bool(b))  # revealed: Literal[True]
    reveal_type(bool(c))  # revealed: Literal[True]
    reveal_type(bool(d))  # revealed: Literal[True]
    reveal_type(bool(e))  # revealed: Literal[True]
    reveal_type(bool(f))  # revealed: Literal[True]

def _(
    a: tuple[()],
    b: Literal[0],
    c: Literal[""],
    d: Literal[b""],
    e: Literal[0, 0],
    f: AlwaysFalsy,
):
    reveal_type(bool(a))  # revealed: Literal[False]
    reveal_type(bool(b))  # revealed: Literal[False]
    reveal_type(bool(c))  # revealed: Literal[False]
    reveal_type(bool(d))  # revealed: Literal[False]
    reveal_type(bool(e))  # revealed: Literal[False]
    reveal_type(bool(f))  # revealed: Literal[False]

def _(
    a: str,
    b: Literal[1, 0],
    c: str | Literal[0],
    d: str | Literal[1],
):
    reveal_type(bool(a))  # revealed: bool
    reveal_type(bool(b))  # revealed: bool
    reveal_type(bool(c))  # revealed: bool
    reveal_type(bool(d))  # revealed: bool
```

## Instances

Checks that we don't get into a cycle if someone sets their `__bool__` method to the `bool` builtin:

### __bool__ is bool

```py
class BoolIsBool:
    __bool__ = bool

reveal_type(bool(BoolIsBool()))  # revealed: bool
```

### Conditional __bool__ method

```py
def flag() -> bool:
    return True

class Boom:
    if flag():
        __bool__ = bool
    else:
        __bool__ = int

reveal_type(bool(Boom()))  # revealed: bool
```

### Possibly unbound __bool__ method

```py
from typing import Literal

def flag() -> bool:
    return True

class PossiblyUnboundTrue:
    if flag():
        def __bool__(self) -> Literal[True]:
            return True

reveal_type(bool(PossiblyUnboundTrue()))  # revealed: bool
```
