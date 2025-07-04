# `typing.Final`

[`typing.Final`] is a type qualifier that is used to indicate that a symbol may not be reassigned in
any scope. Final names declared in class scopes cannot be overridden in subclasses.

## Basic type inference

`mod.py`:

```py
from typing import Final, Annotated

FINAL_A: Final[int] = 1
FINAL_B: Annotated[Final[int], "the annotation for FINAL_B"] = 1
FINAL_C: Final[Annotated[int, "the annotation for FINAL_C"]] = 1
FINAL_D: Final = 1
FINAL_E: "Final[int]" = 1
FINAL_F: Final[int]
FINAL_F = 1

reveal_type(FINAL_A)  # revealed: Literal[1]
reveal_type(FINAL_B)  # revealed: Literal[1]
reveal_type(FINAL_C)  # revealed: Literal[1]
reveal_type(FINAL_D)  # revealed: Literal[1]
reveal_type(FINAL_E)  # revealed: Literal[1]

def nonlocal_uses():
    reveal_type(FINAL_A)  # revealed: Literal[1]
    reveal_type(FINAL_B)  # revealed: Literal[1]
    reveal_type(FINAL_C)  # revealed: Literal[1]
    reveal_type(FINAL_D)  # revealed: Literal[1]
    reveal_type(FINAL_E)  # revealed: Literal[1]
```

Imported types:

```py
from mod import FINAL_A, FINAL_B, FINAL_C, FINAL_D, FINAL_E, FINAL_F

reveal_type(FINAL_A)  # revealed: Literal[1]
reveal_type(FINAL_B)  # revealed: Literal[1]
reveal_type(FINAL_C)  # revealed: Literal[1]
reveal_type(FINAL_D)  # revealed: Literal[1]
reveal_type(FINAL_E)  # revealed: Literal[1]
reveal_type(FINAL_F)  # revealed: Literal[1]
```

## Not modifiable

```py
from typing import Final, Annotated

FINAL_A: Final[int] = 1
FINAL_B: Annotated[Final[int], "the annotation for FINAL_B"] = 1
FINAL_C: Final[Annotated[int, "the annotation for FINAL_C"]] = 1
FINAL_D: Final = 1
FINAL_E: "Final[int]" = 1
FINAL_F: Final[int]
FINAL_F = 1

# TODO: all of these should be errors
FINAL_A = 2
FINAL_B = 2
FINAL_C = 2
FINAL_D = 2
FINAL_E = 2
FINAL_F = 2
```

## Too many arguments

```py
from typing import Final

class C:
    # error: [invalid-type-form] "Type qualifier `typing.Final` expected exactly 1 argument, got 2"
    x: Final[int, str] = 1
```

## Illegal `Final` in type expression

```py
from typing import Final

class C:
    # error: [invalid-type-form] "Type qualifier `typing.Final` is not allowed in type expressions (only in annotation expressions)"
    x: Final | int

    # error: [invalid-type-form] "Type qualifier `typing.Final` is not allowed in type expressions (only in annotation expressions)"
    y: int | Final[str]
```

## No assignment

```py
from typing import Final

DECLARED_THEN_BOUND: Final[int]
DECLARED_THEN_BOUND = 1
```

## No assignment for bare `Final`

```py
from typing import Final

# TODO: This should be an error
NO_RHS: Final
```

[`typing.final`]: https://docs.python.org/3/library/typing.html#typing.Final
