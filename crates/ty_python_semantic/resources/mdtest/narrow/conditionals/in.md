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

## `in` for PEP 695 aliases

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Literal, assert_never

type Foo = Literal["a", "b", "c", "d"]

def _(x: Foo):
    if x in ("a", "b"):
        reveal_type(x)  # revealed: Literal["a", "b"]
    else:
        reveal_type(x)  # revealed: Literal["c", "d"]

def _(x: Foo) -> str:
    if x in ("a", "b"):
        return "AB"
    match x:
        case "c":
            return "C"
        case "d":
            return "D"
        case _ as never:
            assert_never(never)
```

## `in` for mixed PEP 695 aliases

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Literal

type Foo = Literal["a", "b", "c"] | int

def _(x: Foo):
    if x in ("a", "b"):
        reveal_type(x)  # revealed: Literal["a", "b"] | int
    else:
        reveal_type(x)  # revealed: Literal["c"] | int

def _(x: Foo):
    if x not in ("a", "c"):
        reveal_type(x)  # revealed: Literal["b"] | int
    else:
        reveal_type(x)  # revealed: Literal["a", "c"] | int
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
from typing import Any, Literal

def test(x: Literal["a", "b", "c"] | None | int = None):
    if x not in ("a", "c"):
        # int is included because custom __eq__ methods could make
        # an int equal to "a" or "c", so we can't eliminate it
        reveal_type(x)  # revealed: Literal["b"] | None | int
    else:
        reveal_type(x)  # revealed: Literal["a", "c"] | int

def broad_set_element(x: Literal[1, 2], values: set[int]) -> None:
    if x not in values:
        reveal_type(x)  # revealed: Literal[1, 2]
    else:
        reveal_type(x)  # revealed: Literal[1, 2]

def union_tuple_slot(x: Literal[1, 2], values: tuple[Literal[1, 2]]) -> None:
    if x not in values:
        reveal_type(x)  # revealed: Literal[1, 2]
    else:
        reveal_type(x)  # revealed: Literal[1, 2]

def union_tuple_slot_with_exact_value(
    x: Literal[1, 2, 3],
    values: tuple[Literal[1, 2], Literal[3]],
) -> None:
    if x not in values:
        reveal_type(x)  # revealed: Literal[1, 2]
    else:
        reveal_type(x)  # revealed: Literal[1, 2, 3]

def tuple_with_any_slot(x: str | None, missing: Any) -> None:
    if x not in (missing, None):
        reveal_type(x)  # revealed: str
    else:
        reveal_type(x)  # revealed: str | None

def local_literal_rhs(x: str | None) -> None:
    unavailable = [None, ""]
    if x not in unavailable:
        # TODO: This should narrow to `str` if we can prove that the local
        # literal collection has not been mutated or aliased before the test.
        reveal_type(x)  # revealed: str | None
    else:
        reveal_type(x)  # revealed: None | str

def mutable_global_rhs(x: str | None, unavailable: set[str | None]) -> None:
    if x not in unavailable:
        reveal_type(x)  # revealed: str | None
    else:
        reveal_type(x)  # revealed: str | None
```

## Membership and equality

When containment is known to compare items using equality, we can remove a union member that cannot
compare equal to any item in the container. A `TypedDict` cannot compare equal to a string, and a
final class with the default identity-based equality cannot compare equal to an integer. We retain
types such as `int` and classes with custom equality when they might still match an item.

```py
from typing import Literal, TypedDict, final

class Payload(TypedDict):
    value: int

@final
class Token: ...

@final
class AlwaysEqual:
    def __eq__(self, other: object) -> bool:
        return True

def typed_dict(x: Payload | Literal["missing"]):
    if x in ("missing",):
        reveal_type(x)  # revealed: Literal["missing"]

def default_equality(x: Token | Literal[1]):
    if x in (1,):
        reveal_type(x)  # revealed: Literal[1]

def overlapping_union_member(x: int | Literal["missing"]):
    if x in ("missing", 1):
        reveal_type(x)  # revealed: Literal["missing"] | int

def custom_equality(x: AlwaysEqual | Literal[1]):
    if x in (1,):
        reveal_type(x)  # revealed: Literal[1] | AlwaysEqual

def empty_tuple(x: Payload | Literal["missing"], values: tuple[()]):
    if x in values:
        reveal_type(x)  # revealed: Never
```

## Custom containment methods

Python uses `__contains__` when a class defines it. The method can return `True` for values that the
class would never produce during iteration, but membership tests are currently narrowed from the
iterable element type. The result below is therefore too narrow and documents a known limitation:

```py
from typing import Literal, TypedDict

class Payload(TypedDict):
    value: int

class ContainsEverything(tuple[Literal["missing"], ...]):
    def __contains__(self, value: object) -> bool:
        return True

def custom_contains(x: Payload | Literal["missing"], values: ContainsEverything):
    if x in values:
        # TODO: `x` can still be `Payload` because `values.__contains__` always returns `True`.
        reveal_type(x)  # revealed: Literal["missing"]
```

## No present-key narrowing without a `TypedDict`

We only synthesize a key-access protocol for string membership tests on right-hand-side values that
include a `TypedDict`. Other membership tests can mean substring or element containment instead:

```py
from typing import Literal

def f(x: Literal["abc", "def"]):
    if "a" in x:
        # `x` could also be validly narrowed to `Literal["abc"]` here:
        reveal_type(x)  # revealed: Literal["abc", "def"]
    else:
        # `x` could also be validly narrowed to `Literal["def"]` here:
        reveal_type(x)  # revealed: Literal["abc", "def"]

    if "a" not in x:
        # `x` could also be validly narrowed to `Literal["def"]` here:
        reveal_type(x)  # revealed: Literal["abc", "def"]
    else:
        # `x` could also be validly narrowed to `Literal["abc"]` here:
        reveal_type(x)  # revealed: Literal["abc", "def"]
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

def after_excluding_red(x: Color):
    if x is Color.RED:
        return
    if x in (Color.GREEN,):
        reveal_type(x)  # revealed: Literal[Color.GREEN]
    else:
        reveal_type(x)  # revealed: Literal[Color.BLUE]

def after_excluding_red_mixed(x: Color | int):
    if x is Color.RED:
        return
    if x in (Color.GREEN,):
        reveal_type(x)  # revealed: Literal[Color.GREEN] | int
    else:
        reveal_type(x)  # revealed: Literal[Color.BLUE] | int
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
