## What it does

Checks for constrained [type variables] with only one constraint,
or that those constraints reference type variables.

## Why is this bad?

A constrained type variable must have at least two constraints.

## Examples

```python
from typing import TypeVar

T = TypeVar('T', str)  # invalid constrained TypeVar

I = TypeVar('I', bound=int)
U = TypeVar('U', list[I], int)  # invalid constrained TypeVar
```

Use instead:

```python
T = TypeVar('T', str, int)  # valid constrained TypeVar

# or

T = TypeVar('T', bound=str)  # valid bound TypeVar

U = TypeVar('U', list[int], int)  # valid constrained Type
```

[type variables]: https://docs.python.org/3/library/typing.html#typing.TypeVar
