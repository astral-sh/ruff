# Union types

This test suite covers certain basic properties and simplification strategies for union types.

## Basic unions

```py
from typing import Literal

def _(u1: int | str, u2: Literal[0] | Literal[1]) -> None:
    reveal_type(u1)  # revealed: int | str
    reveal_type(u2)  # revealed: Literal[0, 1]
```

## Duplicate elements are collapsed

```py
def _(u1: int | int | str, u2: int | str | int) -> None:
    reveal_type(u1)  # revealed: int | str
    reveal_type(u2)  # revealed: int | str
```

## `Never` is removed

```py
from typing_extensions import Never

def _(u1: int | Never, u2: int | Never | str) -> None:
    reveal_type(u1)  # revealed: int
    reveal_type(u2)  # revealed:  int | str
```

## Flattening of nested unions

```py
from typing import Literal

def _(
    u1: (int | str) | bytes,
    u2: int | (str | bytes),
    u3: int | (str | (bytes | complex)),
) -> None:
    reveal_type(u1)  # revealed: int | str | bytes
    reveal_type(u2)  # revealed: int | str | bytes
    reveal_type(u3)  # revealed: int | str | bytes | complex
```

## Simplification using subtyping

Elements that are subtypes of other elements can be removed.

```py
from typing_extensions import Literal, LiteralString

def _(
    u1: str | LiteralString, u2: LiteralString | str, u3: Literal["a"] | str | LiteralString, u4: str | bytes | LiteralString
) -> None:
    reveal_type(u1)  # revealed: str
    reveal_type(u2)  # revealed: str
    reveal_type(u3)  # revealed: str
    reveal_type(u4)  # revealed: str | bytes
```

## Boolean literals

The Boolean literals `True` and `False` can be unioned to create `bool`.

```py
from typing import Literal

def _(
    u1: Literal[True] | Literal[False],
    u2: bool | Literal[True],
    u3: Literal[True] | bool,
    u4: Literal[True] | Literal[True] | Literal[17],
    u5: Literal[True] | Literal[True] | Literal[False] | Literal[17],
) -> None:
    reveal_type(u1)  # revealed: bool
    reveal_type(u2)  # revealed: bool
    reveal_type(u3)  # revealed: bool
    reveal_type(u4)  # revealed: Literal[True, 17]
    reveal_type(u5)  # revealed: bool | Literal[17]
```

## Do not erase `Unknown`

```py
from knot_extensions import Unknown

def _(u1: Unknown | str, u2: str | Unknown) -> None:
    reveal_type(u2)  # revealed: str | Unknown
    reveal_type(u1)  # revealed: Unknown | str
```

## Collapse multiple `Unknown`s

```py
from knot_extensions import Unknown

def _(u1: Unknown | Unknown | str, u2: Unknown | str | Unknown, u3: str | Unknown | Unknown) -> None:
    reveal_type(u1)  # revealed: Unknown | str
    reveal_type(u2)  # revealed: Unknown | str
    reveal_type(u3)  # revealed: str | Unknown
```

## Subsume multiple elements

Simplifications still apply when `Unknown` is present.

```py
from knot_extensions import Unknown

def _(u1: str | Unknown | int | object):
    reveal_type(u1)  # revealed: Unknown | object
```
