# Narrowing for `is not` conditionals

## `is not None`

The type guard removes `None` from the union type:

```py
from typing import Literal

def _(x: None | Literal[1]):
    if x is not None:
        reveal_type(x)  # revealed: Literal[1]
    else:
        reveal_type(x)  # revealed: None

    reveal_type(x)  # revealed: None | Literal[1]
```

## `None is not x` (reversed operands)

```py
from typing import Literal

def _(x: None | Literal[1]):
    if None is not x:
        reveal_type(x)  # revealed: Literal[1]
    else:
        reveal_type(x)  # revealed: None

    reveal_type(x)  # revealed: None | Literal[1]
```

This also works for other singleton types with reversed operands:

```py
def _(x: bool):
    if False is not x:
        reveal_type(x)  # revealed: Literal[True]
    else:
        reveal_type(x)  # revealed: Literal[False]
```

## `is not` for other singleton types

Boolean literals:

```py
def _(flag: bool):
    x = True if flag else False
    reveal_type(x)  # revealed: bool

    if x is not False:
        reveal_type(x)  # revealed: Literal[True]
    else:
        reveal_type(x)  # revealed: Literal[False]
```

Enum literals:

```py
from enum import Enum
from typing import Literal

class Answer(Enum):
    NO = 0
    YES = 1

def _(answer: Answer):
    if answer is not Answer.NO:
        reveal_type(answer)  # revealed: Literal[Answer.YES]
    else:
        reveal_type(answer)  # revealed: Literal[Answer.NO]

    reveal_type(answer)  # revealed: Answer

class Single(Enum):
    VALUE = 1

def _(x: Single | int):
    if x is not Single.VALUE:
        reveal_type(x)  # revealed: int
    else:
        reveal_type(x)  # revealed: Single

def _(x: list[int] | Literal[Answer.NO]):
    if x is not Answer.NO:
        reveal_type(x)  # revealed: list[int]
```

## `is not` for non-singleton types

Non-singleton types should *not* narrow the type: two instances of a non-singleton class may occupy
different addresses in memory even if they compare equal.

```py
x = 345
y = 345

if x is not y:
    reveal_type(x)  # revealed: Literal[345]
else:
    reveal_type(x)  # revealed: Literal[345]
```

## `is not` for other types

```py
class A: ...

def _(x: A, y: A | None):
    if y is not x:
        reveal_type(y)  # revealed: A | None
    else:
        reveal_type(y)  # revealed: A

    reveal_type(y)  # revealed: A | None
```

## `is not` in chained comparisons

The type guard removes `False` from the union type of the tested value only.

```py
def _(x_flag: bool, y_flag: bool):
    x = True if x_flag else False
    y = True if y_flag else False

    reveal_type(x)  # revealed: bool
    reveal_type(y)  # revealed: bool

    if y is not x is not False:  # Interpreted as `(y is not x) and (x is not False)`
        reveal_type(x)  # revealed: Literal[True]
        reveal_type(y)  # revealed: bool
    else:
        # The negation of the clause above is (y is x) or (x is False)
        # So we can't narrow the type of x or y here, because each arm of the `or` could be true

        reveal_type(x)  # revealed: bool
        reveal_type(y)  # revealed: bool
```

## `is not` with two narrowable operands

Both operands should be narrowed when both are narrowable expressions.

```py
def _(x: None, y: int | None):
    if x is not y:
        reveal_type(y)  # revealed: int
    if y is not x:
        reveal_type(y)  # revealed: int
```

## Assignment expressions

```py
def f() -> int | str | None: ...

if (x := f()) is not None:
    reveal_type(x)  # revealed: int | str
else:
    reveal_type(x)  # revealed: None
```
