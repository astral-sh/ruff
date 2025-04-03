# Pattern matching

## With wildcard

```py
def _(target: int):
    match target:
        case 1:
            y = 2
        case _:
            y = 3

    reveal_type(y)  # revealed: Literal[2, 3]
```

## Without wildcard

```py
def _(target: int):
    match target:
        case 1:
            y = 2
        case 2:
            y = 3

    # revealed: Literal[2, 3]
    # error: [possibly-unresolved-reference]
    reveal_type(y)
```

## Basic match

```py
def _(target: int):
    y = 1
    y = 2

    match target:
        case 1:
            y = 3
        case 2:
            y = 4

    reveal_type(y)  # revealed: Literal[2, 3, 4]
```

## Class match

We have to take into account custom equality implementations

```py
from typing import final

@final
class C:
    pass

def _(subject: C):
    y = 1
    match subject:
        case 1:
            y = 2
    reveal_type(y)  # revealed: Literal[1, 2]
```

```py
from typing import final

class Foo:
    pass

class FooSub(Foo):
    pass

class Bar:
    pass

@final
class Baz:
    pass

def _(target: FooSub):
    y = 1

    match target:
        case Baz():
            y = 2
        case Foo():
            y = 3
        case Bar():
            y = 4

    reveal_type(y)  # revealed: Literal[3]

def _(target: FooSub | str):
    y = 1

    match target:
        case Baz():
            y = 2
        case Foo():
            y = 3
        case Bar():
            y = 4

    reveal_type(y)  # revealed: Literal[1, 3, 4]
```

## Singleton match

```py
from typing import Literal

def _(target: Literal[True, False]):
    y = 1

    match target:
        case True:
            y = 2
        case False:
            y = 3
        case None:
            y = 4

    # TODO: with exhaustivity checking, this should be Literal[2, 3]
    reveal_type(y)  # revealed: Literal[1, 2, 3]

def _(target: bool):
    y = 1

    match target:
        case True:
            y = 2
        case False:
            y = 3
        case None:
            y = 4

    # TODO: with exhaustivity checking, this should be Literal[2, 3]
    reveal_type(y)  # revealed: Literal[1, 2, 3]

def _(target: None):
    y = 1

    match target:
        case True:
            y = 2
        case False:
            y = 3
        case None:
            y = 4

    reveal_type(y)  # revealed: Literal[4]

def _(target: None | Literal[True]):
    y = 1

    match target:
        case True:
            y = 2
        case False:
            y = 3
        case None:
            y = 4

    # TODO: with exhaustivity checking, this should be Literal[2, 4]
    reveal_type(y)  # revealed: Literal[1, 2, 4]

# bool is an int subclass
def _(target: int):
    y = 1

    match target:
        case True:
            y = 2
        case False:
            y = 3
        case None:
            y = 4

    reveal_type(y)  # revealed: Literal[1, 2, 3]

def _(target: str):
    y = 1

    match target:
        case True:
            y = 2
        case False:
            y = 3
        case None:
            y = 4

    reveal_type(y)  # revealed: Literal[1]
```

## Or match

```py
from typing import Literal, final

def _(target: Literal["foo", "baz"]):
    y = 1

    match target:
        case "foo" | "bar":
            y = 2
        case "baz":
            y = 3

    # TODO: with exhaustiveness, this should be Literal[2, 3]
    reveal_type(y)  # revealed: Literal[1, 2, 3]

def _(target: None):
    y = 1

    match target:
        case None | 3:
            y = 2
        case "foo" | 4 | True:
            y = 3

    reveal_type(y)  # revealed: Literal[2]

@final
class Baz:
    pass

def _(target: int | None | float):
    y = 1

    match target:
        case None | 3:
            y = 2
        case Baz():
            y = 3

    reveal_type(y)  # revealed: Literal[1, 2]

def _(target: None | str):
    y = 1

    match target:
        case Baz() | True | False:
            y = 2
        case int():
            y = 3

    reveal_type(y)  # revealed: Literal[1, 3]
```

## Guard with object that implements `__bool__` incorrectly

```py
class NotBoolable:
    __bool__: int = 3

def _(target: int, flag: NotBoolable):
    y = 1
    match target:
        # error: [unsupported-bool-conversion] "Boolean conversion is unsupported for type `NotBoolable`; its `__bool__` method isn't callable"
        case 1 if flag:
            y = 2
        case 2:
            y = 3

    reveal_type(y)  # revealed: Literal[1, 2, 3]
```
