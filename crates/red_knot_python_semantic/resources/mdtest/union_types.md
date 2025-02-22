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

`Never` is an empty set, a type with no inhabitants. Its presence in a union is always redundant,
and so we eagerly simplify it away. `NoReturn` is equivalent to `Never`.

```py
from typing_extensions import Never, NoReturn

def never(u1: int | Never, u2: int | Never | str) -> None:
    reveal_type(u1)  # revealed: int
    reveal_type(u2)  # revealed:  int | str

def noreturn(u1: int | NoReturn, u2: int | NoReturn | str) -> None:
    reveal_type(u1)  # revealed: int
    reveal_type(u2)  # revealed:  int | str
```

## `object` subsumes everything

Unions with `object` can be simplified to `object`:

```py
from typing_extensions import Never, Any

def _(
    u1: int | object,
    u2: object | int,
    u3: Any | object,
    u4: object | Any,
    u5: object | Never,
    u6: Never | object,
    u7: int | str | object | bytes | Any,
) -> None:
    reveal_type(u1)  # revealed: object
    reveal_type(u2)  # revealed: object
    reveal_type(u3)  # revealed: object
    reveal_type(u4)  # revealed: object
    reveal_type(u5)  # revealed: object
    reveal_type(u6)  # revealed: object
    reveal_type(u7)  # revealed: object
```

## Flattening of nested unions

```py
from typing import Literal

def _(
    u1: (int | str) | bytes,
    u2: int | (str | bytes),
    u3: int | (str | (bytes | bytearray)),
) -> None:
    reveal_type(u1)  # revealed: int | str | bytes
    reveal_type(u2)  # revealed: int | str | bytes
    reveal_type(u3)  # revealed: int | str | bytes | bytearray
```

## Simplification using subtyping

The type `S | T` can be simplified to `T` if `S` is a subtype of `T`:

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

The union `Literal[True] | Literal[False]` is exactly equivalent to `bool`:

```py
from typing import Literal

def _(
    u1: Literal[True, False],
    u2: bool | Literal[True],
    u3: Literal[True] | bool,
    u4: Literal[True] | Literal[True, 17],
    u5: Literal[True, False, True, 17],
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
    reveal_type(u1)  # revealed: Unknown | str
    reveal_type(u2)  # revealed: str | Unknown
```

## Collapse multiple `Unknown`s

Since `Unknown` is a gradual type, it is not a subtype of anything, but multiple `Unknown`s in a
union are still redundant:

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

def _(u1: int | Unknown | bool) -> None:
    reveal_type(u1)  # revealed: int | Unknown
```

## Union of intersections

We can simplify unions of intersections:

```py
from knot_extensions import Intersection, Not

class P: ...
class Q: ...

def _(
    i1: Intersection[P, Q] | Intersection[P, Q],
    i2: Intersection[P, Q] | Intersection[Q, P],
) -> None:
    reveal_type(i1)  # revealed: P & Q
    reveal_type(i2)  # revealed: P & Q
```
