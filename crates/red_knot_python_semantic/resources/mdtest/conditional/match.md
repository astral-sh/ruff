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

## Value match

A value pattern matches based on equality: the first `case` branch here will be taken if `subject`
is equal to `2`, even if `subject` is not an instance of `int`. We can't know whether `C` here has a
custom `__eq__` implementation that might cause it to compare equal to `2`, so we have to consider
the possibility that the `case` branch might be taken even though the type `C` is disjoint from the
type `Literal[2]`.

This leads us to infer `Literal[1, 3]` as the type of `y` after the `match` statement, rather than
`Literal[1]`:

```py
from typing import final

@final
class C:
    pass

def _(subject: C):
    y = 1
    match subject:
        case 2:
            y = 3
    reveal_type(y)  # revealed: Literal[1, 3]
```

## Class match

A `case` branch with a class pattern is taken if the subject is an instance of the given class, and
all subpatterns in the class pattern match.

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

def _(target: FooSub):
    y = 1

    match target:
        case Baz():
            y = 2
        case Bar():
            y = 3
        case Foo():
            y = 4

    reveal_type(y)  # revealed: Literal[3, 4]

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

Singleton patterns are matched based on identity, not equality comparisons or `isinstance()` checks.

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

    # TODO: with exhaustiveness checking, this should be Literal[2, 3]
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

    # TODO: with exhaustiveness checking, this should be Literal[2, 3]
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

    # TODO: with exhaustiveness checking, this should be Literal[2, 4]
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

A `|` pattern matches if any of the subpatterns match.

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
