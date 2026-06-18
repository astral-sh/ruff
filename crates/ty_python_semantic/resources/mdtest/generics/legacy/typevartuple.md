# Legacy `TypeVarTuple`

```toml
environment.python-version = "3.13"
```

## Definition and validation

```py
from typing import TypeVarTuple, Unpack

Ts = TypeVarTuple("Ts")

reveal_type(type(Ts))  # revealed: <class 'TypeVarTuple'>
reveal_type(Ts)  # revealed: TypeVarTuple
reveal_type(Ts.__name__)  # revealed: Literal["Ts"]

def bare(value: Ts) -> None: ...  # error: [invalid-type-form]
def invalid_unpack(value: Unpack[int]) -> None: ...  # error: [invalid-type-form]
```

## Explicit specialization

```py
from typing import Generic, TypeVar, TypeVarTuple

T = TypeVar("T")
U = TypeVar("U")
Ts = TypeVarTuple("Ts")

class Simple(Generic[*Ts]):
    value: tuple[*Ts]

class Between(Generic[T, *Ts, U]):
    value: tuple[T, *Ts, U]

reveal_type(Simple[()]().value)  # revealed: tuple[()]
reveal_type(Simple[int, str]().value)  # revealed: tuple[int, str]
reveal_type(Simple[*tuple[int, ...]]().value)  # revealed: tuple[int, ...]
reveal_type(Between[int, bool, bytes, str]().value)  # revealed: tuple[int, bool, bytes, str]
reveal_type(Between[int, *tuple[bool, ...], str]().value)  # revealed: tuple[int, *tuple[bool, ...], str]
reveal_type(Between().value)  # revealed: tuple[Unknown, *tuple[Unknown, ...], Unknown]
```

## Unsolved packs in generic inheritance

Distinct unsolved packs are gradual during assignability checks.

```py
from __future__ import annotations

from typing import Any, Generic, TypeVarTuple, Unpack

Ts = TypeVarTuple("Ts", default=Unpack[tuple[Any, ...]])

class Base(Generic[Unpack[Ts]]):
    def __init__(self, other: Base) -> None: ...

class Derived(Base[Unpack[Ts]]):
    pass

def construct(value: Base) -> None:
    Derived(value)
```

## Constrained type variables

An unspecialized variadic constraint satisfies the corresponding method's `Self` upper bound.

```py
from typing import Any, Generic, TypeVar, TypeVarTuple, Unpack

Ts = TypeVarTuple("Ts")
Us = TypeVarTuple("Us")

class C(Generic[Unpack[Ts]]):
    def method(self) -> None: ...

class D(Generic[Unpack[Us]]):
    def method(self) -> None: ...

T = TypeVar("T", C[Unpack[tuple[Any, ...]]], D[Unpack[tuple[Any, ...]]])

class Interface(Generic[T]):
    def call(self, value: T) -> None:
        # TODO: Remove the temporary constrained-Self fallback once TypeVarTuple solving is
        # implemented.
        value.method()
```
