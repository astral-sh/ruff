# Narrowing for `match` statements

```toml
[environment]
python-version = "3.10"
```

## Single `match` pattern

```py
def _(flag: bool):
    x = None if flag else 1

    reveal_type(x)  # revealed: None | Literal[1]

    y = 0

    match x:
        case None:
            y = x

    reveal_type(y)  # revealed: Literal[0] | None
```

## Class patterns

```py
def get_object() -> object:
    return object()

class A: ...
class B: ...

x = get_object()

reveal_type(x)  # revealed: object

match x:
    case A():
        reveal_type(x)  # revealed: A
    case B():
        reveal_type(x)  # revealed: B & ~A

reveal_type(x)  # revealed: object
```

## Class pattern with guard

```py
def get_object() -> object:
    return object()

class A:
    def y() -> int:
        return 1

class B: ...

x = get_object()

reveal_type(x)  # revealed: object

match x:
    case A() if reveal_type(x):  # revealed: A
        pass
    case B() if reveal_type(x):  # revealed: B
        pass

reveal_type(x)  # revealed: object
```

## Class patterns with generic classes

```toml
[environment]
python-version = "3.12"
```

```py
from typing import assert_never

class Covariant[T]:
    def get(self) -> T:
        raise NotImplementedError

def f(x: Covariant[int]):
    match x:
        case Covariant():
            reveal_type(x)  # revealed: Covariant[int]
        case _:
            reveal_type(x)  # revealed: Never
            assert_never(x)
```

## Class patterns with generic `@final` classes

These work the same as non-`@final` classes.

```toml
[environment]
python-version = "3.12"
```

```py
from typing import assert_never, final

@final
class Covariant[T]:
    def get(self) -> T:
        raise NotImplementedError

def f(x: Covariant[int]):
    match x:
        case Covariant():
            reveal_type(x)  # revealed: Covariant[int]
        case _:
            reveal_type(x)  # revealed: Never
            assert_never(x)
```

## Class patterns where the class pattern does not resolve to a class

In general this does not allow for narrowing, but we make an exception for `Any`. This is to support
[real ecosystem code](https://github.com/jax-ml/jax/blob/d2ce04b6c3d03ae18b145965b8b8b92e09e8009c/jax/_src/pallas/mosaic_gpu/lowering.py#L3372-L3387)
found in `jax`.

```py
from typing import Any

X = Any

def f(obj: object):
    match obj:
        case int():
            reveal_type(obj)  # revealed: int
        case X():
            reveal_type(obj)  # revealed: Any & ~int

def g(obj: object, Y: Any):
    match obj:
        case int():
            reveal_type(obj)  # revealed: int
        case Y():
            reveal_type(obj)  # revealed: Any & ~int
```

## Value patterns

Value patterns are evaluated by equality, which is overridable. Therefore successfully matching on
one can only give us information where we know how the subject type implements equality.

Consider the following example.

```py
from typing import Literal

def _(x: Literal["foo"] | int):
    match x:
        case "foo":
            reveal_type(x)  # revealed: Literal["foo"] | int

    match x:
        case "bar":
            reveal_type(x)  # revealed: int
```

In the first `match`'s `case "foo"` all we know is `x == "foo"`. `x` could be an instance of an
arbitrary `int` subclass with an arbitrary `__eq__`, so we can't actually narrow to
`Literal["foo"]`.

In the second `match`'s `case "bar"` we know `x == "bar"`. As discussed above, this isn't enough to
rule out `int`, but we know that `"foo" == "bar"` is false so we can eliminate `Literal["foo"]`.

More examples follow.

```py
from typing import Literal

class C:
    pass

def _(x: Literal["foo", "bar", 42, b"foo"] | bool | complex):
    match x:
        case "foo":
            reveal_type(x)  # revealed: Literal["foo"] | int | float | complex
        case 42:
            reveal_type(x)  # revealed: int | float | complex
        case 6.0:
            reveal_type(x)  # revealed: Literal["bar", b"foo"] | (int & ~Literal[42]) | float | complex
        case 1j:
            reveal_type(x)  # revealed: Literal["bar", b"foo"] | (int & ~Literal[42]) | float | complex
        case b"foo":
            reveal_type(x)  # revealed: (int & ~Literal[42]) | Literal[b"foo"] | float | complex
        case _:
            reveal_type(x)  # revealed: Literal["bar"] | (int & ~Literal[42]) | float | complex
```

## Value patterns with guard

```py
from typing import Literal

class C:
    pass

def _(x: Literal["foo", b"bar"] | int):
    match x:
        case "foo" if reveal_type(x):  # revealed: Literal["foo"] | int
            pass
        case b"bar" if reveal_type(x):  # revealed: Literal[b"bar"] | int
            pass
        case 42 if reveal_type(x):  # revealed: int
            pass
```

## Or patterns

```py
from typing import Literal
from enum import Enum

class Color(Enum):
    RED = 1
    GREEN = 2
    BLUE = 3

def _(color: Color):
    match color:
        case Color.RED | Color.GREEN:
            reveal_type(color)  # revealed: Literal[Color.RED, Color.GREEN]
        case Color.BLUE:
            reveal_type(color)  # revealed: Literal[Color.BLUE]

    match color:
        case Color.RED | Color.GREEN | Color.BLUE:
            reveal_type(color)  # revealed: Color

    match color:
        case Color.RED:
            reveal_type(color)  # revealed: Literal[Color.RED]
        case _:
            reveal_type(color)  # revealed: Literal[Color.GREEN, Color.BLUE]

class A: ...
class B: ...
class C: ...

def _(x: A | B | C):
    match x:
        case A() | B():
            reveal_type(x)  # revealed: A | B
        case C():
            reveal_type(x)  # revealed: C & ~A & ~B
        case _:
            reveal_type(x)  # revealed: Never

    match x:
        case A() | B() | C():
            reveal_type(x)  # revealed: A | B | C
        case _:
            reveal_type(x)  # revealed: Never

    match x:
        case A():
            reveal_type(x)  # revealed: A
        case _:
            reveal_type(x)  # revealed: (B & ~A) | (C & ~A)
```

## Or patterns with guard

```py
from typing import Literal

def _(x: Literal["foo", b"bar"] | int):
    match x:
        case "foo" | 42 if reveal_type(x):  # revealed: Literal["foo"] | int
            pass
        case b"bar" if reveal_type(x):  # revealed: Literal[b"bar"] | int
            pass
        case _ if reveal_type(x):  # revealed: Literal["foo", b"bar"] | int
            pass
```

## Narrowing due to guard

```py
def get_object() -> object:
    return object()

x = get_object()

reveal_type(x)  # revealed: object

match x:
    case str() | float() if type(x) is str:
        reveal_type(x)  #  revealed: str
    case "foo" | 42 | None if isinstance(x, int):
        reveal_type(x)  #  revealed: int
    case False if x:
        reveal_type(x)  #  revealed: Never
    case "foo" if x := "bar":
        reveal_type(x)  # revealed: Literal["bar"]

reveal_type(x)  # revealed: object
```

## Guard and reveal_type in guard

```py
def get_object() -> object:
    return object()

x = get_object()

reveal_type(x)  # revealed: object

match x:
    case str() | float() if type(x) is str and reveal_type(x):  # revealed: str
        pass
    case "foo" | 42 | None if isinstance(x, int) and reveal_type(x):  #  revealed: int
        pass
    case False if x and reveal_type(x):  #  revealed: Never
        pass
    case "foo" if (x := "bar") and reveal_type(x):  #  revealed: Literal["bar"]
        pass

reveal_type(x)  # revealed: object
```

## Narrowing on `Self` in `match` statements

When performing narrowing on `self` inside methods on enums, we take into account that `Self` might
refer to a subtype of the enum class, like `Literal[Answer.YES]`. This is why we do not simplify
`Self & ~Literal[Answer.YES]` to `Literal[Answer.NO, Answer.MAYBE]`. Otherwise, we wouldn't be able
to return `self` in the `assert_yes` method below:

```py
from enum import Enum
from typing_extensions import Self, assert_never

class Answer(Enum):
    NO = 0
    YES = 1
    MAYBE = 2

    def is_yes(self) -> bool:
        reveal_type(self)  # revealed: Self@is_yes

        match self:
            case Answer.YES:
                reveal_type(self)  # revealed: Self@is_yes
                return True
            case Answer.NO | Answer.MAYBE:
                reveal_type(self)  # revealed: Self@is_yes & ~Literal[Answer.YES]
                return False
            case _:
                assert_never(self)  # no error

    def assert_yes(self) -> Self:
        reveal_type(self)  # revealed: Self@assert_yes

        match self:
            case Answer.YES:
                reveal_type(self)  # revealed: Self@assert_yes
                return self
            case _:
                reveal_type(self)  # revealed: Self@assert_yes & ~Literal[Answer.YES]
                raise ValueError("Answer is not YES")

Answer.YES.is_yes()

try:
    reveal_type(Answer.MAYBE.assert_yes())  # revealed: Literal[Answer.MAYBE]
except ValueError:
    pass
```

## Sequence patterns

Sequence patterns narrow tuple element types based on the patterns matched against each element.

```py
def _(subj: tuple[int | str, int | str]):
    match subj:
        case (x, str()):
            reveal_type(subj)  # revealed: tuple[int | str, str]
        case (int(), y):
            reveal_type(subj)  # revealed: tuple[int, int | str]

def _(subj: tuple[int | str, int | str]):
    match subj:
        case (int(), str()):
            reveal_type(subj)  # revealed: tuple[int, str]

def _(subj: tuple[int | str | None, int | str | None]):
    match subj:
        case (None, _):
            reveal_type(subj)  # revealed: tuple[None, int | str | None]
        case (_, None):
            reveal_type(subj)  # revealed: tuple[int | str | None, None]
```

## Sequence patterns with nested tuples

```py
def _(subj: tuple[tuple[int | str, int], int | str]):
    match subj:
        case ((str(), _), _):
            # The inner tuple is narrowed by intersecting with the pattern's constraint
            reveal_type(subj)  # revealed: tuple[tuple[int | str, int] & tuple[str, object], int | str]
```

## Sequence patterns with or patterns

```py
def _(subj: tuple[int | str | bytes, int | str]):
    match subj:
        case (int() | str(), _):
            reveal_type(subj)  # revealed: tuple[int | str, int | str]
```

## Sequence patterns with wildcards

Wildcards (`_`) and name patterns don't narrow the element type.

```py
def _(subj: tuple[int | str, int | str]):
    match subj:
        case (_, _):
            reveal_type(subj)  # revealed: tuple[int | str, int | str]

def _(subj: tuple[int | str, int | str]):
    match subj:
        case (x, y):
            reveal_type(subj)  # revealed: tuple[int | str, int | str]
```

## Sequence pattern negative narrowing

Negative narrowing for sequence patterns is not currently supported. When a sequence pattern doesn't
match, subsequent cases see the original type.

```py
def _(subj: tuple[int | str, int | str]):
    match subj:
        case (int(), int()):
            reveal_type(subj)  # revealed: tuple[int, int]
        case _:
            reveal_type(subj)  # revealed: tuple[int | str, int | str]
```

## Sequence pattern exhaustiveness

When a sequence pattern exhaustively matches all possible tuple values, subsequent cases should be
unreachable (`Never`).

```py
def _(subj: tuple[int, str]):
    match subj:
        case (int(), str()):
            reveal_type(subj)  # revealed: tuple[int, str]
        case _:
            reveal_type(subj)  # revealed: Never
```

## Sequence patterns with homogeneous tuples

Sequence patterns on homogeneous tuples narrow to a fixed-length tuple with the specified length.

```py
def _(subj: tuple[int | str, ...]):
    match subj:
        case (x, str()):
            reveal_type(subj)  # revealed: tuple[int | str, str]

def _(subj: tuple[int | str, ...]):
    match subj:
        case (int(), int(), y):
            reveal_type(subj)  # revealed: tuple[int, int, int | str]
```
