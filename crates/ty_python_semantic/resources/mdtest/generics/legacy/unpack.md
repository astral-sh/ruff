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
