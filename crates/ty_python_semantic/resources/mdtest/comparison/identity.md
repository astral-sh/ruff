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

## Same constrained `TypeVar`

Every occurrence of the same constrained `TypeVar` has the same specialization, including when an
occurrence appears through a type alias. If every constraint is a singleton, two values with that
`TypeVar` must therefore be the same object.

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
    reveal_type(left is not right)  # revealed: Literal[False]

type Alias[X] = X

def aliased(left: Alias[T], right: T) -> None:
    reveal_type(left is right)  # revealed: Literal[True]
    reveal_type(left is not right)  # revealed: Literal[False]
```

## Recursive type aliases

Projecting a recursive alias for an identity comparison must stop when it encounters the alias
again.

```toml
[environment]
python-version = "3.12"
```

```py
type Recursive = Recursive | int

def f(value: Recursive) -> None:
    reveal_type(value is 1)  # revealed: bool
```
