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

- v0.0.105: Instantiating a legacy `TypeVar` with exactly one constraint passed to the constructor
    now triggers `invalid-type-variable-constraints`. In earlier versions of ty, it caused
    `invalid-legacy-type-variable` to be emitted.
