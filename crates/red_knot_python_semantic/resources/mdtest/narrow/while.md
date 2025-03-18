# Narrowing in `while` loops

We only make sure that narrowing works for `while` loops in general, we do not exhaustively test all
narrowing forms here, as they are covered in other tests.

Note how type narrowing works subtly different from `if` ... `else`, because the negated constraint
is retained after the loop.

## Basic `while` loop

```py
def next_item() -> int | None:
    return 1

x = next_item()

while x is not None:
    reveal_type(x)  # revealed: int
    x = next_item()

reveal_type(x)  # revealed: None
```

## `while` loop with `else`

```py
def next_item() -> int | None:
    return 1

x = next_item()

while x is not None:
    reveal_type(x)  # revealed: int
    x = next_item()
else:
    reveal_type(x)  # revealed: None

reveal_type(x)  # revealed: None
```

## Nested `while` loops

```py
from typing import Literal

def next_item() -> Literal[1, 2, 3]:
    raise NotImplementedError

x = next_item()

while x != 1:
    reveal_type(x)  # revealed: Literal[2, 3]

    while x != 2:
        # TODO: this should be Literal[1, 3]; Literal[3] is only correct
        # in the first loop iteration
        reveal_type(x)  # revealed: Literal[3]
        x = next_item()

    x = next_item()
```
