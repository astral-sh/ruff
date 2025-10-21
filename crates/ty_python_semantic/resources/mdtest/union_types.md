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

## Enum literals

```py
from enum import Enum
from typing import Literal, Any
from ty_extensions import Intersection

class Color(Enum):
    RED = "red"
    GREEN = "green"
    BLUE = "blue"

def _(
    u1: Literal[Color.RED, Color.GREEN],
    u2: Color | Literal[Color.RED],
    u3: Literal[Color.RED] | Color,
    u4: Literal[Color.RED] | Literal[Color.RED, Color.GREEN],
    u5: Literal[Color.RED, Color.GREEN, Color.BLUE],
    u6: Literal[Color.RED] | Literal[Color.GREEN] | Literal[Color.BLUE],
) -> None:
    reveal_type(u1)  # revealed: Literal[Color.RED, Color.GREEN]
    reveal_type(u2)  # revealed: Color
    reveal_type(u3)  # revealed: Color
    reveal_type(u4)  # revealed: Literal[Color.RED, Color.GREEN]
    reveal_type(u5)  # revealed: Color
    reveal_type(u6)  # revealed: Color

def _(
    u1: Intersection[Literal[Color.RED], Any] | Literal[Color.RED],
    u2: Literal[Color.RED] | Intersection[Literal[Color.RED], Any],
):
    reveal_type(u1)  # revealed: Literal[Color.RED]
    reveal_type(u2)  # revealed: Literal[Color.RED]
```

## Do not erase `Unknown`

```py
from ty_extensions import Unknown

def _(u1: Unknown | str, u2: str | Unknown) -> None:
    reveal_type(u1)  # revealed: Unknown | str
    reveal_type(u2)  # revealed: str | Unknown
```

## Collapse multiple `Unknown`s

Since `Unknown` is a gradual type, it is not a subtype of anything, but multiple `Unknown`s in a
union are still redundant:

```py
from ty_extensions import Unknown

def _(u1: Unknown | Unknown | str, u2: Unknown | str | Unknown, u3: str | Unknown | Unknown) -> None:
    reveal_type(u1)  # revealed: Unknown | str
    reveal_type(u2)  # revealed: Unknown | str
    reveal_type(u3)  # revealed: str | Unknown
```

## Subsume multiple elements

Simplifications still apply when `Unknown` is present.

```py
from ty_extensions import Unknown

def _(u1: int | Unknown | bool) -> None:
    reveal_type(u1)  # revealed: int | Unknown
```

## Union of intersections

We can simplify unions of intersections:

```py
from ty_extensions import Intersection, Not

class P: ...
class Q: ...

def _(
    i1: Intersection[P, Q] | Intersection[P, Q],
    i2: Intersection[P, Q] | Intersection[Q, P],
) -> None:
    reveal_type(i1)  # revealed: P & Q
    reveal_type(i2)  # revealed: P & Q
```

## Unions of literals with `AlwaysTruthy` and `AlwaysFalsy`

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Literal, Union
from ty_extensions import AlwaysTruthy, AlwaysFalsy, is_equivalent_to, static_assert

type strings = Literal["foo", ""]
type ints = Literal[0, 1]
type bytes = Literal[b"foo", b""]

def _(
    strings_or_truthy: strings | AlwaysTruthy,
    truthy_or_strings: AlwaysTruthy | strings,
    strings_or_falsy: strings | AlwaysFalsy,
    falsy_or_strings: AlwaysFalsy | strings,
    ints_or_truthy: ints | AlwaysTruthy,
    truthy_or_ints: AlwaysTruthy | ints,
    ints_or_falsy: ints | AlwaysFalsy,
    falsy_or_ints: AlwaysFalsy | ints,
    bytes_or_truthy: bytes | AlwaysTruthy,
    truthy_or_bytes: AlwaysTruthy | bytes,
    bytes_or_falsy: bytes | AlwaysFalsy,
    falsy_or_bytes: AlwaysFalsy | bytes,
):
    reveal_type(strings_or_truthy)  # revealed: Literal[""] | AlwaysTruthy
    reveal_type(truthy_or_strings)  # revealed: AlwaysTruthy | Literal[""]

    reveal_type(strings_or_falsy)  # revealed: Literal["foo"] | AlwaysFalsy
    reveal_type(falsy_or_strings)  # revealed: AlwaysFalsy | Literal["foo"]

    reveal_type(ints_or_truthy)  # revealed: Literal[0] | AlwaysTruthy
    reveal_type(truthy_or_ints)  # revealed: AlwaysTruthy | Literal[0]

    reveal_type(ints_or_falsy)  # revealed: Literal[1] | AlwaysFalsy
    reveal_type(falsy_or_ints)  # revealed: AlwaysFalsy | Literal[1]

    reveal_type(bytes_or_truthy)  # revealed: Literal[b""] | AlwaysTruthy
    reveal_type(truthy_or_bytes)  # revealed: AlwaysTruthy | Literal[b""]

    reveal_type(bytes_or_falsy)  # revealed: Literal[b"foo"] | AlwaysFalsy
    reveal_type(falsy_or_bytes)  # revealed: AlwaysFalsy | Literal[b"foo"]

type SA = Union[Literal[""], AlwaysTruthy, Literal["foo"]]
static_assert(is_equivalent_to(SA, Literal[""] | AlwaysTruthy))

type SD = Union[Literal[""], AlwaysTruthy, Literal["foo"], AlwaysFalsy, AlwaysTruthy, int]
static_assert(is_equivalent_to(SD, AlwaysTruthy | AlwaysFalsy | int))

type BA = Union[Literal[b""], AlwaysTruthy, Literal[b"foo"]]
static_assert(is_equivalent_to(BA, Literal[b""] | AlwaysTruthy))

type BD = Union[Literal[b""], AlwaysTruthy, Literal[b"foo"], AlwaysFalsy, AlwaysTruthy, int]
static_assert(is_equivalent_to(BD, AlwaysTruthy | AlwaysFalsy | int))

type IA = Union[Literal[0], AlwaysTruthy, Literal[1]]
static_assert(is_equivalent_to(IA, Literal[0] | AlwaysTruthy))

type ID = Union[Literal[0], AlwaysTruthy, Literal[1], AlwaysFalsy, AlwaysTruthy, str]
static_assert(is_equivalent_to(ID, AlwaysTruthy | AlwaysFalsy | str))
```

## Unions with intersections of literals and Any

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Any, Literal
from ty_extensions import Intersection

type SA = Literal[""]
type SB = Intersection[Literal[""], Any]
type SC = SA | SB
type SD = SB | SA

def _(c: SC, d: SD):
    reveal_type(c)  # revealed: Literal[""]
    reveal_type(d)  # revealed: Literal[""]

type IA = Literal[0]
type IB = Intersection[Literal[0], Any]
type IC = IA | IB
type ID = IB | IA

def _(c: IC, d: ID):
    reveal_type(c)  # revealed: Literal[0]
    reveal_type(d)  # revealed: Literal[0]

type BA = Literal[b""]
type BB = Intersection[Literal[b""], Any]
type BC = BA | BB
type BD = BB | BA

def _(c: BC, d: BD):
    reveal_type(c)  # revealed: Literal[b""]
    reveal_type(d)  # revealed: Literal[b""]
```

## Unions of tuples

A union of a fixed-length tuple and a variable-length tuple must be collapsed to the variable-length
element, never to the fixed-length element (`tuple[()] | tuple[Any, ...]` -> `tuple[Any, ...]`, not
`tuple[()]`).

```py
from typing import Any

def f(
    a: tuple[()] | tuple[int, ...],
    b: tuple[int, ...] | tuple[()],
    c: tuple[int] | tuple[str, ...],
    d: tuple[str, ...] | tuple[int],
    e: tuple[()] | tuple[Any, ...],
    f: tuple[Any, ...] | tuple[()],
    g: tuple[Any, ...] | tuple[Any | str, ...],
    h: tuple[Any | str, ...] | tuple[Any, ...],
):
    reveal_type(a)  # revealed: tuple[int, ...]
    reveal_type(b)  # revealed: tuple[int, ...]
    reveal_type(c)  # revealed: tuple[int] | tuple[str, ...]
    reveal_type(d)  # revealed: tuple[str, ...] | tuple[int]
    reveal_type(e)  # revealed: tuple[Any, ...]
    reveal_type(f)  # revealed: tuple[Any, ...]
    reveal_type(g)  # revealed: tuple[Any | str, ...]
    reveal_type(h)  # revealed: tuple[Any | str, ...]
```

## Unions of other generic containers

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Any

class Bivariant[T]: ...

class Covariant[T]:
    def get(self) -> T:
        raise NotImplementedError

class Contravariant[T]:
    def receive(self, input: T) -> None: ...

class Invariant[T]:
    mutable_attribute: T

def _(
    a: Bivariant[Any] | Bivariant[Any | str],
    b: Bivariant[Any | str] | Bivariant[Any],
    c: Covariant[Any] | Covariant[Any | str],
    d: Covariant[Any | str] | Covariant[Any],
    e: Contravariant[Any | str] | Contravariant[Any],
    f: Contravariant[Any] | Contravariant[Any | str],
    g: Invariant[Any] | Invariant[Any | str],
    h: Invariant[Any | str] | Invariant[Any],
):
    reveal_type(a)  # revealed: Bivariant[Any]
    reveal_type(b)  # revealed: Bivariant[Any | str]
    reveal_type(c)  # revealed: Covariant[Any | str]
    reveal_type(d)  # revealed: Covariant[Any | str]
    reveal_type(e)  # revealed: Contravariant[Any]
    reveal_type(f)  # revealed: Contravariant[Any]
    reveal_type(g)  # revealed: Invariant[Any] | Invariant[Any | str]
    reveal_type(h)  # revealed: Invariant[Any | str] | Invariant[Any]
```
