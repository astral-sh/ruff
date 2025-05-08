# Narrowing for `!=` conditionals

## `x != None`

```py
def _(flag: bool):
    x = None if flag else 1

    if x != None:
        reveal_type(x)  # revealed: Literal[1]
    else:
        reveal_type(x)  # revealed: None
```

## `!=` for other singleton types

```py
def _(flag: bool):
    x = True if flag else False

    if x != False:
        reveal_type(x)  # revealed: Literal[True]
    else:
        reveal_type(x)  # revealed: Literal[False]
```

## `x != y` where `y` is of literal type

```py
def _(flag: bool):
    x = 1 if flag else 2

    if x != 1:
        reveal_type(x)  # revealed: Literal[2]
```

## `x != y` where `y` is a single-valued type

```py
def _(flag: bool):
    class A: ...
    class B: ...
    C = A if flag else B

    if C != A:
        reveal_type(C)  # revealed: <class 'B'>
    else:
        reveal_type(C)  # revealed: <class 'A'>
```

## `x != y` where `y` has multiple single-valued options

```py
def _(flag1: bool, flag2: bool):
    x = 1 if flag1 else 2
    y = 2 if flag2 else 3

    if x != y:
        reveal_type(x)  # revealed: Literal[1, 2]
    else:
        reveal_type(x)  # revealed: Literal[2]
```

## `!=` for non-single-valued types

Only single-valued types should narrow the type:

```py
def _(flag: bool, a: int, y: int):
    x = a if flag else None

    if x != y:
        reveal_type(x)  # revealed: int | None
```

## Mix of single-valued and non-single-valued types

```py
def _(flag1: bool, flag2: bool, a: int):
    x = 1 if flag1 else 2
    y = 2 if flag2 else a

    if x != y:
        reveal_type(x)  # revealed: Literal[1, 2]
    else:
        reveal_type(x)  # revealed: Literal[1, 2]
```

## Assignment expressions

```py
from typing import Literal

def f() -> Literal[1, 2, 3]:
    return 1

if (x := f()) != 1:
    reveal_type(x)  # revealed: Literal[2, 3]
else:
    reveal_type(x)  # revealed: Literal[1]
```

## Union with `Any`

```py
from typing import Any

def _(x: Any | None, y: Any | None):
    if x != 1:
        reveal_type(x)  # revealed: (Any & ~Literal[1]) | None
    if y == 1:
        reveal_type(y)  # revealed: Any & ~None
```

## Booleans and integers

```py
from typing import Literal

def _(b: bool, i: Literal[1, 2]):
    if b == 1:
        reveal_type(b)  # revealed: Literal[True]
    else:
        reveal_type(b)  # revealed: Literal[False]

    if b == 6:
        reveal_type(b)  # revealed: Never
    else:
        reveal_type(b)  # revealed: bool

    if b == 0:
        reveal_type(b)  # revealed: Literal[False]
    else:
        reveal_type(b)  # revealed: Literal[True]

    if i == True:
        reveal_type(i)  # revealed: Literal[1]
    else:
        reveal_type(i)  # revealed: Literal[2]
```

## Narrowing `LiteralString` in union

```py
from typing_extensions import Literal, LiteralString, Any

def _(s: LiteralString | None, t: LiteralString | Any):
    if s == "foo":
        reveal_type(s)  # revealed: Literal["foo"]

    if s == 1:
        reveal_type(s)  # revealed: Never

    if t == "foo":
        # TODO could be `Literal["foo"] | Any`
        reveal_type(t)  # revealed: LiteralString | Any
```
