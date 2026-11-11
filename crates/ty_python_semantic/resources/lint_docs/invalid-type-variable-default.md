## What it does

Checks for [type variables] whose default type is not compatible with the type variable's bound or
constraints, or whose default references an out-of-scope type variable.

## Why is this bad?

If a type variable has a bound, the default must be assignable to that bound (see: [bound rules]).
If a type variable has constraints, the default must be one of the constraints (see:
[constraint rules]).

## Examples

```toml
[environment]
python-version = "3.13"
```

```python
from typing import Generic, TypeVar

T = TypeVar("T", bound=str, default=int)  # error: [invalid-type-variable-default]
U = TypeVar("U", int, str, default=bytes)  # error: [invalid-type-variable-default]
```

A class cannot supply a default from a type variable it never binds. Here, `T2` defaults to `T1`,
but `OnlyT2` has no type parameter named `T1`. Include `T1` before `T2` in `Generic[T1, T2]`, or
give `T2` a default that does not depend on `T1`.

```python
T1 = TypeVar("T1", default=int)
T2 = TypeVar("T2", default=T1)


class OnlyT2(Generic[T2]): ...  # error: [invalid-type-variable-default]
```

## Rule history

- v0.0.102: Split out `invalid-type-parameter-order`. This rule now also covers class type parameter
    defaults that reference out-of-scope type variables, previously reported by
    `invalid-generic-class`.

[bound rules]: https://typing.python.org/en/latest/spec/generics.html#bound-rules
[constraint rules]: https://typing.python.org/en/latest/spec/generics.html#constraint-rules
[type variables]: https://docs.python.org/3/library/typing.html#typing.TypeVar
