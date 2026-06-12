## What it does
Checks for [type variables] whose bounds reference type variables.

## Why is this bad?
The bound of a type variable must be a concrete type.

## Examples
```python
T = TypeVar('T', bound=list['T'])  # error: [invalid-type-variable-bound]
U = TypeVar('U')
T = TypeVar('T', bound=U)  # error: [invalid-type-variable-bound]

def f[T: list[T]](): ...  # error: [invalid-type-variable-bound]
def g[U, T: U](): ...  # error: [invalid-type-variable-bound]
```

[type variable]: https://docs.python.org/3/library/typing.html#typing.TypeVar
