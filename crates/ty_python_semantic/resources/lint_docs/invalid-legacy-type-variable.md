## What it does
Checks for the creation of invalid legacy `TypeVar`s

## Why is this bad?
There are several requirements that you must follow when creating a legacy `TypeVar`.

## Examples
```python
from typing import TypeVar

T = TypeVar("T")  # okay
T = TypeVar("T")  # error: TypeVars should not be redefined

# error: TypeVar must be immediately assigned to a variable
def f(t: TypeVar("U")): ...
```

## References
- [Typing spec: Generics](https://typing.python.org/en/latest/spec/generics.html#introduction)
