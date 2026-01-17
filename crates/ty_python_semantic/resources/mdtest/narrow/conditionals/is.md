# Narrowing for `is` conditionals

## `is None`

```py
def _(flag: bool):
    x = None if flag else 1

    if x is None:
        reveal_type(x)  # revealed: None
    else:
        reveal_type(x)  # revealed: Literal[1]

    reveal_type(x)  # revealed: None | Literal[1]
```

## `is` for other types

```py
def _(flag: bool):
    class A: ...
    x = A()
    y = x if flag else None

    if y is x:
        reveal_type(y)  # revealed: A
    else:
        reveal_type(y)  # revealed: A | None

    reveal_type(y)  # revealed: A | None
```

## `is` in chained comparisons

```py
def _(x_flag: bool, y_flag: bool):
    x = True if x_flag else False
    y = True if y_flag else False

    reveal_type(x)  # revealed: bool
    reveal_type(y)  # revealed: bool

    if y is x is False:  # Interpreted as `(y is x) and (x is False)`
        reveal_type(x)  # revealed: Literal[False]
        reveal_type(y)  # revealed: bool
    else:
        # The negation of the clause above is (y is not x) or (x is not False)
        # So we can't narrow the type of x or y here, because each arm of the `or` could be true
        reveal_type(x)  # revealed: bool
        reveal_type(y)  # revealed: bool
```

## `is` in elif clause

```py
def _(flag1: bool, flag2: bool):
    x = None if flag1 else (1 if flag2 else True)

    reveal_type(x)  # revealed: None | Literal[1, True]
    if x is None:
        reveal_type(x)  # revealed: None
    elif x is True:
        reveal_type(x)  # revealed: Literal[True]
    else:
        reveal_type(x)  # revealed: Literal[1]
```

## `is` for enums

```py
from enum import Enum

class Answer(Enum):
    NO = 0
    YES = 1

def _(answer: Answer):
    if answer is Answer.NO:
        reveal_type(answer)  # revealed: Literal[Answer.NO]
    else:
        reveal_type(answer)  # revealed: Literal[Answer.YES]

class Single(Enum):
    VALUE = 1

def _(x: Single | int):
    if x is Single.VALUE:
        reveal_type(x)  # revealed: Single
    else:
        reveal_type(x)  # revealed: int
```

## `is` for `EllipsisType` (Python 3.10+)

```toml
[environment]
python-version = "3.10"
```

```py
from types import EllipsisType

def _(x: int | EllipsisType):
    if x is ...:
        reveal_type(x)  # revealed: EllipsisType
    else:
        reveal_type(x)  # revealed: int
```

## `is` for `EllipsisType` (Python 3.9 and below)

```toml
[environment]
python-version = "3.9"
```

```py
def _(flag: bool):
    x = ... if flag else 42

    reveal_type(x)  # revealed: ellipsis | Literal[42]

    if x is ...:
        reveal_type(x)  # revealed: ellipsis
    else:
        reveal_type(x)  # revealed: Literal[42]
```

## Assignment expressions

```py
from typing import Literal

def f() -> Literal[1, 2] | None: ...

if (x := f()) is None:
    reveal_type(x)  # revealed: None
else:
    reveal_type(x)  # revealed: Literal[1, 2]
```

## `is` where the other operand is a call expression

```py
from typing import Literal, final

def foo() -> Literal[42]:
    return 42

def f(x: object):
    if x is foo():
        reveal_type(x)  # revealed: Literal[42]
    else:
        reveal_type(x)  # revealed: object

    if x is not foo():
        reveal_type(x)  # revealed: object
    else:
        reveal_type(x)  # revealed: Literal[42]

    if foo() is x:
        reveal_type(x)  # revealed: Literal[42]
    else:
        reveal_type(x)  # revealed: object

    if foo() is not x:
        reveal_type(x)  # revealed: object
    else:
        reveal_type(x)  # revealed: Literal[42]

def bar() -> int:
    return 42

def g(x: object):
    if x is bar():
        reveal_type(x)  # revealed: int
    else:
        reveal_type(x)  # revealed: object

    if x is not bar():
        reveal_type(x)  # revealed: object
    else:
        reveal_type(x)  # revealed: int

@final
class FinalClass: ...

def baz() -> FinalClass:
    return FinalClass()

def h(x: object):
    if x is baz():
        reveal_type(x)  # revealed: FinalClass
    else:
        reveal_type(x)  # revealed: object

    if x is not baz():
        reveal_type(x)  # revealed: object
    else:
        reveal_type(x)  # revealed: FinalClass

def spam() -> None:
    return None

def h(x: object):
    if x is spam():
        reveal_type(x)  # revealed: None
    else:
        # `else` narrowing can occur because `spam()` returns a singleton type
        reveal_type(x)  # revealed: ~None

    if x is not spam():
        reveal_type(x)  # revealed: ~None
    else:
        reveal_type(x)  # revealed: None
```
