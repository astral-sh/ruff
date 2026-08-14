# Starred expression annotations

```toml
[environment]
python-version = "3.11"
```

An unpacked type variable tuple keeps the types of positional arguments passed to `*args`.

```py
from typing_extensions import TypeVarTuple

Ts = TypeVarTuple("Ts")

def append_int(*args: *Ts) -> tuple[*Ts, int]:
    reveal_type(args)  # revealed: tuple[*Ts@append_int]

    return (*args, 1)

reveal_type(append_int(True, "a"))  # revealed: tuple[Literal[True], Literal["a"], int]
reveal_type(append_int())  # revealed: tuple[int]
```

A concrete starred tuple checks its fixed first argument, remaining argument types, and arity.

```py
def first_arg_int(*args: *tuple[int, *tuple[str, ...]]): ...

first_arg_int(42, "42", "42")  # fine
# error: [invalid-argument-type] "Argument to function `first_arg_int` is incorrect: Expected `int`"
first_arg_int("not an int", "42", "42")
# error: [invalid-argument-type] "Argument to function `first_arg_int` is incorrect: Expected `str`, found `Literal[56]`"
first_arg_int(56, "42", 56)
# error: [missing-argument] "No argument provided for required parameter `*args` of function `first_arg_int`"
first_arg_int()
```
