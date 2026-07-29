## What it does

Checks for type variables that are used in a scope where they are not bound
to any enclosing generic context.

## Why is this bad?

Using a type variable outside of a scope that binds it has no well-defined meaning.

## Examples

```python
from typing import TypeVar, Generic

T = TypeVar("T")
S = TypeVar("S")

# unbound type variable in module scope
x: T  # error


class C(Generic[T]):
    # S is not in this class's generic context
    x: list[S] = []  # error
```

## References

- [Typing spec: Scoping rules for type variables](https://typing.python.org/en/latest/spec/generics.html#scoping-rules-for-type-variables)
