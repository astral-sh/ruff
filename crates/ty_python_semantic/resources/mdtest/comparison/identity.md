# Identity comparisons

## Basic comparisons

```py
from typing_extensions import TypeAliasType

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

## Repeated identity comparisons after narrowing `Unknown`

Once `value is None` has succeeded, the value can only be the `None` singleton even when its
original type is `Unknown`.

```py
from ty_extensions import Unknown

def f(value: Unknown) -> None:
    if value is None:
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

## Recursive type aliases

Checking identity for a recursive alias must terminate instead of repeatedly expanding the alias.

```toml
[environment]
python-version = "3.12"
```

```py
type Recursive = Recursive | int

def f(value: Recursive) -> None:
    reveal_type(value is 1)  # revealed: bool
```
