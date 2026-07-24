# Identity comparisons

## Basic comparisons

```py
from typing_extensions import TypeAliasType

reveal_type(False is False)  # revealed: Literal[True]
reveal_type(False is True)  # revealed: Literal[False]
reveal_type(1 is True)  # revealed: Literal[False]
reveal_type(... is ...)  # revealed: Literal[True]
reveal_type(NotImplemented is NotImplemented)  # revealed: Literal[True]

# two occurences of the same literal `1` do not necessarily share the
# same memory address, as `1` is not a singleton (but they also *might*!)
reveal_type(1 is 1)  # revealed: bool

# but two different integer literals definitely don't share the same memory address
reveal_type(1 is 2)  # revealed: Literal[False]

class A: ...

def _(a1: A, a2: A, o: object):
    n1 = None
    n2 = None

    reveal_type(a1 is a1)  # revealed: bool
    reveal_type(a1 is a2)  # revealed: bool

    reveal_type(n1 is n1)  # revealed: Literal[True]
    reveal_type(n1 is n2)  # revealed: Literal[True]

    reveal_type(a1 is n1)  # revealed: Literal[False]
    reveal_type(n1 is a1)  # revealed: Literal[False]

    reveal_type(a1 is o)  # revealed: bool
    reveal_type(n1 is o)  # revealed: bool

    reveal_type(a1 is not a1)  # revealed: bool
    reveal_type(a1 is not a2)  # revealed: bool

    reveal_type(n1 is not n1)  # revealed: Literal[False]
    reveal_type(n1 is not n2)  # revealed: Literal[False]

    reveal_type(a1 is not n1)  # revealed: Literal[True]
    reveal_type(n1 is not a1)  # revealed: Literal[True]

    reveal_type(a1 is not o)  # revealed: bool
    reveal_type(n1 is not o)  # revealed: bool

def _(a1: TypeAliasType, a2: TypeAliasType):
    reveal_type(a1 is a2)  # revealed: bool
    reveal_type(a1 is not a2)  # revealed: bool

reveal_type(list[int] is list[int])  # revealed: bool
reveal_type(list[int] is not list[int])  # revealed: bool
```

## Identity comparisons with NewTypes

Two variables cannot share the same memory address if they have disjoint nominal-instance backing
types:

```py
def f(x: str, y: int):
    reveal_type(x is y)  # revealed: Literal[False]
    reveal_type(x is not y)  # revealed: Literal[True]
```

but simple disjointness is not enough -- these two `NewType`s are disjoint, yet `B(True)` shares the
same memory address as `C(True)`. Disjointness of the nominal-instance types *backing* the `NewType`
is the necessary precondition:

```py
from typing import NewType, Literal
from ty_extensions._internal import is_disjoint_from

B = NewType("B", bool)
C = NewType("C", bool)

reveal_type(is_disjoint_from(B, C))  # revealed: ConstraintSet[Literal[True]]
reveal_type(is_disjoint_from(B, Literal[True]))  # revealed: ConstraintSet[Literal[True]]

def f(x: B, y: C):
    reveal_type(x is y)  # revealed: bool
    reveal_type(x is not y)  # revealed: bool
    reveal_type(x is True)  # revealed: bool
    reveal_type(x is False)  # revealed: bool
    reveal_type(x is not True)  # revealed: bool
    reveal_type(x is not False)  # revealed: bool
```

Nonetheless, if the NewType's nominal backing type is disjoint from another type, `Literal` boolean
types can still be inferred as a result:

```py
from typing import NewType, Literal

N = NewType("N", str)
O = NewType("O", int)

def f(x: N, y: int, z: O):
    reveal_type(x is y)  # revealed: Literal[False]
    reveal_type(x is not y)  # revealed: Literal[True]
    reveal_type(x is z)  # revealed: Literal[False]
    reveal_type(x is not z)  # revealed: Literal[True]
```

## Identity comparisons see through type aliases

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Literal

type SoTrue = Literal[True]
type SoFalse = Literal[False]

def f(x: SoTrue, y: SoFalse):
    reveal_type(x is True)  # revealed: Literal[True]
    reveal_type(x is False)  # revealed: Literal[False]
    reveal_type(x is y)  # revealed: Literal[False]
    reveal_type(x is not y)  # revealed: Literal[True]
```

## Repeated identity comparisons after narrowing `Unknown`

Once `value is None` has succeeded, the value can only be the `None` singleton even when its
original type is `Unknown`.

```py
from ty_extensions import Unknown

def f(value: Unknown) -> None:
    if value is None:
        reveal_type(value)  # revealed: Unknown & None
        reveal_type(value is not None)  # revealed: Literal[False]
```

## Identity comparisons for the same constrained `TypeVar`

All occurrences of the same constrained `TypeVar` use the same constraint. Here, each constraint
contains only one object, so two values with that `TypeVar` must be identical. This remains true
when one occurrence appears through a type alias.

```toml
[environment]
python-version = "3.12"
```

```py
from types import EllipsisType
from typing import TypeVar

T = TypeVar("T", None, EllipsisType)

def f(left: T, right: T) -> None:
    reveal_type(left is right)  # revealed: Literal[True]

type Alias[X] = X

def aliased(left: Alias[T], right: T) -> None:
    reveal_type(left is right)  # revealed: Literal[True]
```
