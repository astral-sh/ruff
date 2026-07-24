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
    reveal_type(args)  # revealed: tuple[*Ts@append_int]

    return (*args, 1)

reveal_type(append_int(True, "a"))  # revealed: tuple[Literal[True], Literal["a"], int]
reveal_type(append_int())  # revealed: tuple[int]

def first_arg_int(*args: *tuple[int, *tuple[str, ...]]): ...

first_arg_int(42, "42", "42")  # fine
# error: [invalid-argument-type]
first_arg_int("not an int", "42", "42")
# error: [invalid-argument-type]
first_arg_int(56, "42", 56)
# error: [invalid-argument-type]
first_arg_int()
```
