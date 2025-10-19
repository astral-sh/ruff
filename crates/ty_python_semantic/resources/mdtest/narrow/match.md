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
