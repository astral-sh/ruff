## What it does

Checks for the creation of invalid legacy `TypeVar`s

## Why is this bad?

There are several requirements that you must follow when creating a legacy `TypeVar`.

## Examples

```python
from typing import TypeVar

T = TypeVar("T")  # okay
T = TypeVar("T")  # error: "Cannot redefine `T` as a type variable"


# TypeVar must be immediately assigned to a variable
# error
def f(t: TypeVar("U")): ...  # ty: ignore[invalid-type-form]
```

## References

- [Typing spec: Generics](https://typing.python.org/en/latest/spec/generics.html#introduction)

## Rule history

- v0.0.102: A `TypeVar` call with exactly one constraint now triggers
    `invalid-type-variable-constraints`. Use that selector in rule settings and ignore comments for
    this error.
