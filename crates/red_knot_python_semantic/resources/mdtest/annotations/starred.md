# Starred expression annotations

Type annotations for `*args` can be starred expressions themselves:

```py
from typing_extensions import TypeVarTuple

Ts = TypeVarTuple("Ts")

def append_int(*args: *Ts) -> tuple[*Ts, int]:
    # TODO: should show some representation of the variadic generic type
    reveal_type(args)  # revealed: @Todo

    return (*args, 1)

# TODO should be tuple[Literal[True], Literal["a"], int]
reveal_type(append_int(True, "a"))  # revealed: @Todo
```
