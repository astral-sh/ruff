## What it does

Checks whether the name passed to `TypeAliasType` differs from the assigned variable, or if ty
cannot determine the value of a string passed to the `name` argument.

## Why is this bad?

A mismatched name can be a typo and can make a type alias's runtime representation confusing.

## Examples

```python
from typing_extensions import TypeAliasType

# error: "Expected "UserId", got "Id""
UserId = TypeAliasType("Id", int)


def f(x: str):
    # error: "Cannot verify that a value of type `str` matches "Id""
    Id = TypeAliasType(x, int)
```

## Rule history

- v0.0.108: Split out from `mismatched-type-name`.
