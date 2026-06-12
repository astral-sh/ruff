## What it does
Checks for the creation of invalid generic classes

## Why is this bad?
There are several requirements that you must follow when defining a generic class.
Many of these result in `TypeError` being raised at runtime if they are violated.

## Examples
```python
from typing_extensions import Generic, TypeVar

T = TypeVar("T")
U = TypeVar("U", default=int)

# error: class uses both PEP-695 syntax and legacy syntax
class C[U](Generic[T]): ...

# error: type parameter with default comes before type parameter without default
class D(Generic[U, T]): ...
```

## References
- [Typing spec: Generics](https://typing.python.org/en/latest/spec/generics.html#introduction)
