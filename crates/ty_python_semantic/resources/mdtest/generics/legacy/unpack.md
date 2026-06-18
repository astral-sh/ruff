# Legacy `typing.Unpack`

```toml
environment.python-version = "3.13"
```

## Validation

```py
from typing import TypeVarTuple, Unpack

Ts = TypeVarTuple("Ts")

def valid(*args: Unpack[Ts]) -> None: ...
def invalid(*args: Unpack[int]) -> None: ...  # error: [invalid-type-form]
```

## Explicit specialization

```py
from typing import Generic, TypeVarTuple, Unpack

Ts = TypeVarTuple("Ts")

class Array(Generic[Unpack[Ts]]):
    value: tuple[Unpack[Ts]]

reveal_type(Array[()]().value)  # revealed: tuple[()]
reveal_type(Array[int, str]().value)  # revealed: tuple[int, str]
reveal_type(Array[Unpack[tuple[int, ...]]]().value)  # revealed: tuple[int, ...]
```

## Calls use a gradual pack

```py
from typing import TypeVarTuple, Unpack

Ts = TypeVarTuple("Ts")

def collect(*args: Unpack[Ts]) -> tuple[Unpack[Ts]]:
    return args

reveal_type(collect())  # revealed: tuple[Unknown, ...]
reveal_type(collect(1, "a"))  # revealed: tuple[Unknown, ...]
```
