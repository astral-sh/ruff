## What it does

Checks for constrained [type variables] with only one constraint,
or that those constraints reference type variables.

## Why is this bad?

A constrained type variable must have at least two constraints.

## Examples

```toml
[environment]
python-version = "3.12"
```

```python
from typing import TypeVar

I = TypeVar("I", bound=int)
# constraint references `I`
S = TypeVar("S", list[I], int)  # error


# a constrained type variable needs at least two constraints
def f[T: (int,)](): ...  # error
```

Use instead:

```python
from typing import TypeVar

U = TypeVar("U", str, int)  # valid constrained TypeVar

# or

T = TypeVar("T", bound=str)  # valid bound TypeVar

V = TypeVar("V", list[int], int)  # valid constrained Type
```

[type variables]: https://docs.python.org/3/library/typing.html#typing.TypeVar
