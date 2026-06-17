# Narrowing for `in` conditionals

## `in` for tuples

```py
def _(x: int):
    if x in (1, 2, 3):
        reveal_type(x)  # revealed: int
    else:
        reveal_type(x)  # revealed: int & ~Literal[1] & ~Literal[True] & ~Literal[2] & ~Literal[3]
```

```py
def _(x: str):
    if x in ("a", "b", "c"):
        reveal_type(x)  # revealed: str
    else:
        reveal_type(x)  # revealed: str & ~Literal["a"] & ~Literal["b"] & ~Literal["c"]
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
        reveal_type(x)  # revealed: str & ~Literal["a"] & ~Literal["b"] & ~Literal["c"]
```

```py
from typing import Literal, TypeVar

T = TypeVar("T", Literal["a"], Literal["d"])

def _(x: Literal["a", "b", "c", "d"]):
    if x in "abc":
        reveal_type(x)  # revealed: Literal["a", "b", "c"]
    else:
        reveal_type(x)  # revealed: Literal["d"]

def substring(x: Literal["", "ab", "z"]):
    if x in "abc":
        reveal_type(x)  # revealed: Literal["", "ab"]
    else:
        reveal_type(x)  # revealed: Literal["z"]

def constrained_substring(x: T):
    if x in "abc":
        reveal_type(x)  # revealed: T@constrained_substring & Literal["a"]
    else:
        reveal_type(x)  # revealed: T@constrained_substring & Literal["d"]
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

def empty_string(x: str):
    if x in "":
        reveal_type(x)  # revealed: str

def empty_bytes(x: bytes):
    if x in b"":
        reveal_type(x)  # revealed: bytes
```

## Byte containment

`bytes` and `bytearray` accept byte subsequences and objects implementing `__index__`, not only the
integers described by their iteration type. We therefore leave the subject unchanged in the positive
branch:

```py
from typing import Literal, final

@final
class ByteSubstring(bytes): ...

@final
class ByteIndex:
    def __index__(self) -> int:
        return 97

def bytes_subsequence(value: ByteSubstring | Literal[97]) -> None:
    if value in b"abc":
        reveal_type(value)  # revealed: ByteSubstring | Literal[97]
    else:
        reveal_type(value)  # revealed: ByteSubstring | Literal[97]

def bytes_index(value: ByteIndex | Literal[97], values: bytes) -> None:
    if value in values:
        reveal_type(value)  # revealed: ByteIndex | Literal[97]
    else:
        reveal_type(value)  # revealed: ByteIndex | Literal[97]

def bytes_union_container(
    value: Literal[b"a", 97],
    values: bytes | tuple[int, ...],
) -> None:
    if value in values:
        reveal_type(value)  # revealed: Literal[b"a", 97]
    else:
        reveal_type(value)  # revealed: Literal[b"a", 97]

def bytearray_index(value: ByteIndex | Literal[97], values: bytearray) -> None:
    if value in values:
        reveal_type(value)  # revealed: ByteIndex | Literal[97]
    else:
        reveal_type(value)  # revealed: ByteIndex | Literal[97]

def bytearray_subsequence(value: Literal[b"a", 97], values: bytearray) -> None:
    if value in values:
        reveal_type(value)  # revealed: Literal[b"a", 97]
    else:
        reveal_type(value)  # revealed: Literal[b"a", 97]
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

def broad_element_type(x: str | None, values: dict[str, int]):
    if x in values:
        reveal_type(x)  # revealed: str | None
    else:
        reveal_type(x)  # revealed: str | None

def broad_element_type_with_unknown(values: dict[str, int]):
    x = [None][0]
    if x in values:
        reveal_type(x)  # revealed: None | Unknown
    else:
        reveal_type(x)  # revealed: None | Unknown
```

## Correlated constrained type variables

Membership in a tuple containing a constrained type variable can preserve the relationship between
that type variable and a broader subject type. In the first example, a successful membership test
proves that `x` has the same enum-literal constraint as `y`, so returning `x` as `T` is valid.
Equality compatibility alone is not enough to establish that relationship: `AlwaysEqual` can compare
equal to every `U`, but it does not become a `U`, so the return in the second example remains
invalid.

```py
from enum import Enum
from typing import Literal, TypeVar

class E(Enum):
    A = 1
    B = 2

T = TypeVar("T", Literal[E.A], Literal[E.B])

def correlated_typevar(x: E, y: T) -> T:
    if x in (y,):
        reveal_type(x)  # revealed: T@correlated_typevar
        return x
    return y

U = TypeVar("U", int, str)

class AlwaysEqual:
    def __eq__(self, other: object) -> Literal[True]:
        return True

def unrelated_typevar(x: AlwaysEqual, y: U) -> U:
    if x in (y,):
        reveal_type(x)  # revealed: AlwaysEqual
        # error: [invalid-return-type] "Return type does not match returned value: expected `U@unrelated_typevar`, found `AlwaysEqual`"
        return x
    return y
```

## Direct `not in` conditional

```py
from typing import Any, Literal, TypeVar

T = TypeVar("T", Literal[1], Literal[2])

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

def broad_dict_element(x: str | None, values: dict[str, int]) -> None:
    if x not in values:
        reveal_type(x)  # revealed: str | None
    else:
        reveal_type(x)  # revealed: str | None

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

def equality_equivalent_union_slot(
    x: Literal[0, False, 2],
    values: tuple[Literal[0, False]],
) -> None:
    if x not in values:
        reveal_type(x)  # revealed: Literal[2]
    else:
        reveal_type(x)  # revealed: Literal[0, False]

def correlated_typevar(x: T | None, y: T) -> None:
    if x not in (y,):
        reveal_type(x)  # revealed: None

def tuple_with_any_slot(x: str | None, missing: Any) -> None:
    if x not in (missing, None):
        reveal_type(x)  # revealed: str
    else:
        reveal_type(x)  # revealed: str | None

def constrained_typevar_slot(x: T | None, y: T) -> None:
    if x not in (y,):
        reveal_type(x)  # revealed: None

def local_literal_rhs(x: str | None) -> None:
    unavailable = [None, ""]
    if x not in unavailable:
        # TODO: This should narrow to `str` if we can prove that the local
        # literal collection has not been mutated or aliased before the test.
        reveal_type(x)  # revealed: str | None
    else:
        reveal_type(x)  # revealed: str | None

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
        reveal_type(x)  # revealed: int | Literal["missing"]

def custom_equality(x: AlwaysEqual | Literal[1]):
    if x in (1,):
        reveal_type(x)  # revealed: AlwaysEqual | Literal[1]

def empty_tuple(x: Payload | Literal["missing"], values: tuple[()]):
    if x in values:
        reveal_type(x)  # revealed: Never
```

## Custom containment methods

A custom `__contains__` method can return `True` for values that the class would never produce
during iteration. We therefore leave the subject type unchanged:

```py
from collections.abc import Iterator
from typing import Literal, TypedDict

class Payload(TypedDict):
    value: int

class ContainsEverything:
    def __iter__(self) -> Iterator[Literal["missing"]]:
        yield "missing"

    def __contains__(self, value: object) -> bool:
        return True

def custom_contains(x: Payload | Literal["missing"], values: ContainsEverything):
    if x in values:
        reveal_type(x)  # revealed: Payload | Literal["missing"]

def custom_contains_literal_domain(
    x: Literal["present", "missing"],
    values: ContainsEverything,
):
    if x in values:
        reveal_type(x)  # revealed: Literal["present", "missing"]
```

## Final and non-final iterable classes

An instance of a non-final class might be an instance of a subclass that defines `__contains__`. For
a final class without `__contains__`, we know that membership falls back to iteration, so we can use
the iterable element type to narrow the subject. A final class with its own `__contains__` method
remains conservative:

```py
from collections.abc import Iterator
from typing import Literal, TypedDict, final

class Payload(TypedDict):
    value: int

class IteratesMissing:
    def __iter__(self) -> Iterator[Literal["missing"]]:
        yield "missing"

def non_final_iterable(x: Payload | Literal["missing"], values: IteratesMissing):
    if x in values:
        reveal_type(x)  # revealed: Payload | Literal["missing"]

@final
class FinalIterable:
    def __iter__(self) -> Iterator[Literal["missing"]]:
        yield "missing"

def final_iterable(x: Payload | Literal["missing"], values: FinalIterable):
    if x in values:
        reveal_type(x)  # revealed: Literal["missing"]

@final
class FinalContainsEverything:
    def __iter__(self) -> Iterator[Literal["missing"]]:
        yield "missing"

    def __contains__(self, value: object) -> bool:
        return True

def final_custom_contains(
    x: Payload | Literal["missing"],
    values: FinalContainsEverything,
):
    if x in values:
        reveal_type(x)  # revealed: Payload | Literal["missing"]
```

## Exact containers inside assignment expressions

An assignment expression does not change the containment behavior of the value it wraps. An exact
tuple display can therefore still remove union members that cannot compare equal to any item:

```py
from typing import Literal, final

@final
class Token: ...

def assignment_expression(value: Token | Literal[1]) -> None:
    if value in (items := (1,)):
        reveal_type(value)  # revealed: Literal[1]
```

## Wrapped types with known containment

A type variable uses its upper bound or constraints, and a `NewType` uses its concrete base. Broad
union members can be removed when every possible container has known containment behavior:

```py
from collections.abc import Iterator
from typing import Literal, NewType, TypeVar, final

@final
class Token: ...

@final
class FinalIterable:
    def __iter__(self) -> Iterator[Literal[1]]:
        yield 1

BoundFinalIterable = TypeVar("BoundFinalIterable", bound=FinalIterable)

def bounded_final_iterable(
    value: Token | Literal[1],
    values: BoundFinalIterable,
) -> None:
    if value in values:
        reveal_type(value)  # revealed: Literal[1]

WrappedFinalIterable = NewType("WrappedFinalIterable", FinalIterable)

def wrapped_final_iterable(
    value: Token | Literal[1],
    values: WrappedFinalIterable,
) -> None:
    if value in values:
        reveal_type(value)  # revealed: Literal[1]

BoundTuple = TypeVar("BoundTuple", bound=tuple[Literal[1], ...])

def bounded_tuple(
    value: Token | Literal[1],
    values: BoundTuple,
) -> None:
    if value in values:
        reveal_type(value)  # revealed: Literal[1]

ConstrainedTuple = TypeVar(
    "ConstrainedTuple",
    tuple[Literal[1], ...],
    tuple[Literal[1]],
)

def constrained_tuple(
    value: Token | Literal[1],
    values: ConstrainedTuple,
) -> None:
    if value in values:
        reveal_type(value)  # revealed: Literal[1]

class OpenIterable:
    def __iter__(self) -> Iterator[Literal[1]]:
        yield 1

MixedContainers = TypeVar("MixedContainers", FinalIterable, OpenIterable)

def mixed_constraints(
    value: Token | Literal[1],
    values: MixedContainers,
) -> None:
    if value in values:
        reveal_type(value)  # revealed: Token | Literal[1]

def range_membership(value: Token | Literal[1], values: range) -> None:
    if value in values:
        reveal_type(value)  # revealed: Token | Literal[1]
```

## Built-in containment with overridden iteration

`list.__contains__` searches the values stored in the list. Overriding `__iter__` does not change
which values membership can find:

```py
from collections.abc import Iterator
from typing import Literal, TypedDict, final

class Payload(TypedDict):
    value: int

@final
class OverridesBuiltinIteration(list[object]):
    def __iter__(self) -> Iterator[Literal["missing"]]:
        yield "missing"

def inherited_builtin_contains(
    x: Payload | Literal["missing"],
    values: OverridesBuiltinIteration,
):
    if x in values:
        reveal_type(x)  # revealed: Payload | Literal["missing"]
```

## Custom containment methods on tuple subclasses

A tuple subclass can override `__contains__`, so its tuple element type does not necessarily
describe containment:

```py
from typing import Literal, TypedDict

class Payload(TypedDict):
    value: int

class ContainsEverything(tuple[Literal["missing"], ...]):
    def __contains__(self, value: object) -> bool:
        return True

def custom_tuple_contains(x: Payload | Literal["missing"], values: ContainsEverything):
    if x in values:
        reveal_type(x)  # revealed: Payload | Literal["missing"]

class ContainsNothing(tuple[Literal[1]]):
    def __contains__(self, value: object) -> bool:
        return False

def custom_tuple_not_in(x: Literal[1], values: ContainsNothing):
    if x not in values:
        reveal_type(x)  # revealed: Literal[1]
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
from typing import Literal

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

When the container's element type is a union of enum literals, membership narrows to that union.
Without the annotation, the tuple's elements are widened to `Color`, so the comprehension remains
`list[Color]`:

```py
SelectedColor = Literal[Color.RED, Color.GREEN]
SELECTED_COLORS: tuple[SelectedColor, ...] = (Color.RED, Color.GREEN)

def selected_colors(colors: list[Color]) -> list[SelectedColor]:
    result: list[SelectedColor] = []
    result.extend([color for color in colors if color in SELECTED_COLORS])
    return result

def _(colors: list[Color]):
    inline = [color for color in colors if color in (Color.RED, Color.GREEN)]
    reveal_type(inline)  # revealed: list[Color]
```

An enum that can have additional runtime members can still be narrowed by a membership test against
an explicit member. The other branch excludes that member without assuming that the declared members
are exhaustive.

```py
from enum import Enum, EnumMeta

class InjectingEnumMeta(EnumMeta):
    def __new__(metacls, name, bases, namespace, **kwargs):
        namespace["INJECTED"] = 2
        return super().__new__(metacls, name, bases, namespace, **kwargs)

class InjectedEnum(Enum, metaclass=InjectingEnumMeta):
    ONLY = 1

def _(value: InjectedEnum):
    if value in (InjectedEnum.ONLY,):
        reveal_type(value)  # revealed: Literal[InjectedEnum.ONLY]
    else:
        reveal_type(value)  # revealed: InjectedEnum & ~Literal[InjectedEnum.ONLY]
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

A built-in tuple cannot compare equal to a string literal, so the tuple arm is excluded from the
narrowed type.

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
