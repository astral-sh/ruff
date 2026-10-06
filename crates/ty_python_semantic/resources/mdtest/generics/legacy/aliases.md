# Generic type aliases: legacy syntax

## Ambiguous overload returns through generic aliases

Aliases are expanded when comparing the structure of ambiguous return types. Recursive aliases
retain their outer structure, but recursive references fall back to `Unknown`, even when each
reference changes the alias's type arguments.

```py
from typing import Any, TypeVar, overload

T = TypeVar("T")
Wrapped = dict[str, list[T]]
Growing = tuple[T, list["Growing[list[T]]"]]

@overload
def wrapped(value: int) -> Wrapped[int]: ...
@overload
def wrapped(value: str) -> Wrapped[str]: ...
def wrapped(value): ...
@overload
def growing(value: int) -> Growing[int]: ...
@overload
def growing(value: str) -> Growing[str]: ...
def growing(value): ...
def _(value: Any):
    reveal_type(wrapped(value))  # revealed: dict[str, list[Unknown]]
    reveal_type(growing(value))  # revealed: tuple[Unknown, list[Unknown]]
```

## Negated recursive alias specializations

A recursive alias can specialize to `object`, whose negation is `Never`.

```py
from typing import TypeVar
from typing_extensions import Never, assert_type
from ty_extensions import Not

T = TypeVar("T")
A = T | list["A[T]"]

def f(x: Not[A[object]]):
    assert_type(x, Never)
```

Specialization also preserves double-negation elimination when the alias body is recursive.

```py
B = Not[list["B[T]"] | T]

def identity(x: T) -> T:
    return x

def g(x: Not[B[int]]) -> list[B[int]] | int:
    assert_type(x, list[B[int]] | int)
    return identity(x)  # no diagnostic
```

## Recursive aliases in restricted complements

A negated recursive alias can share an intersection with a positive constraint. The result keeps the
exclusion: since `Any` can materialize differently in each union member, the two members do not
simplify to `list[Any]`.

```py
from typing import Any, TypeVar
from typing_extensions import TypeAliasType
from ty_extensions import Intersection, Not

T = TypeVar("T")
Without = TypeAliasType("Without", Intersection[list[Any], Not[T]], type_params=(T,))
A = TypeAliasType("A", "list[Without[A] | A]")

def f(x: A):
    reveal_type(x[0])  # revealed: (list[Any] & ~A) | list[Without[A] | A]
```

## Recursive complements after specialization

Specializing the positive constraint to `object` removes it. The remaining complement and the
recursive alias together cover every object.

```py
from typing import TypeVar
from typing_extensions import TypeAliasType, assert_type
from ty_extensions import Intersection, Not

T = TypeVar("T")
U = TypeVar("U")
Without = TypeAliasType("Without", Intersection[T, Not[U]], type_params=(T, U))
A = TypeAliasType("A", "list[Without[object, A] | A]")

def f(x: A):
    assert_type(x[0], object)
```
