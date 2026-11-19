## What it does

Checks whether the name passed to `NewType` differs from the assigned variable, or if ty cannot
determine the value of a string passed to the `name` argument.

## Why is this bad?

The name argument sets the newtype's `__name__`. A typo can make the type appear under an unexpected
name in diagnostics.

## Examples

```python
from typing import NewType

# error: "Expected "UserId", got "Id""
UserId = NewType("Id", int)


def f(x: str):
    # error: "Cannot verify that a value of type `str` matches "Id""
    Id = NewType(x, int)
```

## Rule history

- v0.0.108: Split out from `mismatched-type-name`.
