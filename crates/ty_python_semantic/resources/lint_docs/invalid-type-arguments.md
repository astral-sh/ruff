## What it does

Checks that explicit type arguments satisfy their type parameter's bounds and constraints. It also
checks whether arguments involving `ParamSpec` or `TypeVarTuple` are valid for their parameter.

## Why is this bad?

Providing type arguments that don't satisfy the type variable's bounds or constraints will lead to
incorrect type inference and may indicate a misunderstanding of the generic type's interface.

## Examples

Using legacy type variables:

```toml
[environment]
python-version = "3.12"
```

```python
from typing import Generic, TypeVar

T1 = TypeVar("T1", int, str)
T2 = TypeVar("T2", bound=int)


class Foo1(Generic[T1]): ...


class Foo2(Generic[T2]): ...


# bytes does not satisfy T1's constraints
Foo1[bytes]  # error
# str does not satisfy T2's bound
Foo2[str]  # error
```

## Rule history

- v0.0.102: Wrong counts of explicit type arguments now use `invalid-type-argument-count`.
