# Starred expression annotations

```toml
[environment]
python-version = "3.11"
```

Type annotations for `*args` can be starred expressions themselves:

```py
from typing_extensions import TypeVarTuple

Ts = TypeVarTuple("Ts")

def append_int(*args: *Ts) -> tuple[*Ts, int]:
    reveal_type(args)  # revealed: @Todo(PEP 646)

    return (*args, 1)

# TODO should be tuple[Literal[True], Literal["a"], int]
reveal_type(append_int(True, "a"))  # revealed: @Todo(PEP 646)
```
