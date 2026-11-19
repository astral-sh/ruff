## What it does

Checks whether the name passed to the functional form of `TypedDict` differs from the assigned
variable, or if ty cannot determine the value of a string passed to the `typename` argument.

## Why is this bad?

A mismatched name can be a typo and can make a `TypedDict`'s runtime representation confusing.

## Examples

```python
from typing import TypedDict

# error: "Expected "Movie", got "Film""
Movie = TypedDict("Film", {"title": str})


def f(x: str):
    # error: "Cannot verify that a value of type `str` matches "Film""
    Film = TypedDict(x, {"title": str})
```

## Rule history

- v0.0.108: Split out from `mismatched-type-name`.
