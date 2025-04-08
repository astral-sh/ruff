# `typing.Callable`

```py
from typing import Callable

def _(c: Callable[[], int]):
    reveal_type(c())  # revealed: int

def _(c: Callable[[int, str], int]):
    reveal_type(c(1, "a"))  # revealed: int

    # error: [invalid-argument-type] "Object of type `Literal["a"]` cannot be assigned to parameter 1; expected type `int`"
    # error: [invalid-argument-type] "Object of type `Literal[1]` cannot be assigned to parameter 2; expected type `str`"
    reveal_type(c("a", 1))  # revealed: int
```

The `Callable` annotation can only be used to describe positional-only parameters.

```py
def _(c: Callable[[int, str], None]):
    # error: [unknown-argument] "Argument `a` does not match any known parameter"
    # error: [unknown-argument] "Argument `b` does not match any known parameter"
    # error: [missing-argument] "No arguments provided for required parameters 1, 2"
    reveal_type(c(a=1, b="b"))  # revealed: None
```

If the annotation uses a gradual form (`...`) for the parameter list, then it can accept any kind of
parameter with any type.

```py
def _(c: Callable[..., int]):
    reveal_type(c())  # revealed: int
    reveal_type(c(1))  # revealed: int
    reveal_type(c(1, "str", False, a=[1, 2], b=(3, 4)))  # revealed: int
```

An invalid `Callable` form can accept any parameters and will return `Unknown`.

```py
# error: [invalid-type-form]
def _(c: Callable[42, str]):
    reveal_type(c())  # revealed: Unknown
```
