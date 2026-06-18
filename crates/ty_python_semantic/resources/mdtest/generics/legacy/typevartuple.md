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
