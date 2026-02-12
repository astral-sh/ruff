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
        case _ if reveal_type(x):  # revealed: int | Literal["foo", b"bar"]
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

## Narrowing is preserved when a terminal branch prevents a path from flowing through

When one branch of a `match` statement is terminal (e.g. contains `raise`), narrowing from the
non-terminal branches is preserved after the merge point.

```py
class A: ...
class B: ...
class C: ...

def _(x: A | B | C):
    match x:
        case A():
            pass
        case B():
            pass
        case _:
            raise ValueError()

    reveal_type(x)  # revealed: B | (A & ~B)
```

Reassignment in non-terminal branches is also preserved when the default branch is terminal:

```py
def _(number_of_periods: int | None, interval: str):
    match interval:
        case "monthly":
            if number_of_periods is None:
                number_of_periods = 1
        case "daily":
            if number_of_periods is None:
                number_of_periods = 30
        case _:
            raise ValueError("unsupported interval")

    reveal_type(number_of_periods)  # revealed: int
```

## Narrowing tagged unions of tuples

Narrow unions of tuples based on literal tag elements in `match` statements:

```py
from typing import Literal

class A: ...
class B: ...
class C: ...

def _(x: tuple[Literal["tag1"], A] | tuple[Literal["tag2"], B, C]):
    match x[0]:
        case "tag1":
            reveal_type(x)  # revealed: tuple[Literal["tag1"], A]
            reveal_type(x[1])  # revealed: A
        case "tag2":
            reveal_type(x)  # revealed: tuple[Literal["tag2"], B, C]
            reveal_type(x[1])  # revealed: B
            reveal_type(x[2])  # revealed: C
        case _:
            reveal_type(x)  # revealed: Never

# With int literals
def _(x: tuple[Literal[1], A] | tuple[Literal[2], B]):
    match x[0]:
        case 1:
            reveal_type(x)  # revealed: tuple[Literal[1], A]
        case 2:
            reveal_type(x)  # revealed: tuple[Literal[2], B]
        case _:
            reveal_type(x)  # revealed: Never

# With bytes literals
def _(x: tuple[Literal[b"a"], A] | tuple[Literal[b"b"], B]):
    match x[0]:
        case b"a":
            reveal_type(x)  # revealed: tuple[Literal[b"a"], A]
        case b"b":
            reveal_type(x)  # revealed: tuple[Literal[b"b"], B]
        case _:
            reveal_type(x)  # revealed: Never

# Using index 1 instead of 0
def _(x: tuple[A, Literal["tag1"]] | tuple[B, Literal["tag2"]]):
    match x[1]:
        case "tag1":
            reveal_type(x)  # revealed: tuple[A, Literal["tag1"]]
        case "tag2":
            reveal_type(x)  # revealed: tuple[B, Literal["tag2"]]
        case _:
            reveal_type(x)  # revealed: Never
```

Narrowing is restricted to `Literal` tag elements:

```py
def _(x: tuple[Literal["tag1"], A] | tuple[str, B]):
    match x[0]:
        case "tag1":
            # Can't narrow because second tuple has `str` (not literal) at index 0
            reveal_type(x)  # revealed: tuple[Literal["tag1"], A] | tuple[str, B]
        case _:
            # But we *can* narrow with inequality
            reveal_type(x)  # revealed: tuple[str, B]
```

and it is also restricted to `match` patterns that solely consist of value patterns:

```py
class Config:
    MODE: str = "default"

def _(u: tuple[Literal["foo"], int] | tuple[Literal["bar"], str]):
    match u[0]:
        case Config.MODE | "foo":
            # Config.mode has type `str` (not a literal), which could match
            # any string value at runtime. We cannot narrow based on "foo" alone
            # because the actual match might have been against Config.mode.
            reveal_type(u)  # revealed: tuple[Literal["foo"], int] | tuple[Literal["bar"], str]
        case "bar":
            # Since the previous case could match any string, this case can
            # still narrow to `tuple[Literal["bar"], str]` when `u[0]` equals "bar".
            reveal_type(u)  # revealed: tuple[Literal["bar"], str]
```
