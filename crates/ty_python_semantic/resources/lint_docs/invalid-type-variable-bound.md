## What it does

Checks for [type variables][type variable] whose bounds reference type variables.

## Why is this bad?

The bound of a type variable must be a concrete type.

## Examples

```toml
[environment]
python-version = "3.12"
```

```python
from typing import TypeVar

# error: [invalid-type-variable-bound]
RecursiveT = TypeVar("RecursiveT", bound=list["RecursiveT"])
U = TypeVar("U")
# error: [invalid-type-variable-bound]
BoundT = TypeVar("BoundT", bound=U)


def f[T: list[T]](): ...  # error: [invalid-type-variable-bound]
def g[U, T: U](): ...  # error: [invalid-type-variable-bound]
```

[type variable]: https://docs.python.org/3/library/typing.html#typing.TypeVar
