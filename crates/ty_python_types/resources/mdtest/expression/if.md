# If expression

## Union

```py
def _(flag: bool):
    reveal_type(1 if flag else 2)  # revealed: Literal[1, 2]
```

## Statically known conditions in if-expressions

```py
reveal_type(1 if True else 2)  # revealed: Literal[1]
reveal_type(1 if "not empty" else 2)  # revealed: Literal[1]
reveal_type(1 if (1,) else 2)  # revealed: Literal[1]
reveal_type(1 if 1 else 2)  # revealed: Literal[1]

reveal_type(1 if False else 2)  # revealed: Literal[2]
reveal_type(1 if None else 2)  # revealed: Literal[2]
reveal_type(1 if "" else 2)  # revealed: Literal[2]
reveal_type(1 if 0 else 2)  # revealed: Literal[2]
```

## Leaked Narrowing Constraint

(issue #14588)

The test inside an if expression should not affect code outside of the expression.

```py
from typing import Literal

def _(flag: bool):
    x: Literal[42, "hello"] = 42 if flag else "hello"

    reveal_type(x)  # revealed: Literal[42, "hello"]

    _ = ... if isinstance(x, str) else ...

    reveal_type(x)  # revealed: Literal[42, "hello"]
```
