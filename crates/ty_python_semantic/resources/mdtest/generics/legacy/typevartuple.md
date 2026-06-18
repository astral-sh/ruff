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
