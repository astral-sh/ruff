## What it does

Checks for [type variables] whose default type is not compatible with
the type variable's bound or constraints.

## Why is this bad?

If a type variable has a bound, the default must be assignable to that
bound (see: [bound rules]). If a type variable has constraints, the default
must be one of the constraints (see: [constraint rules]).

## Examples

```toml
[environment]
python-version = "3.13"
```

```python
from typing import TypeVar

T = TypeVar("T", bound=str, default=int)  # error: [invalid-type-variable-default]
U = TypeVar("U", int, str, default=bytes)  # error: [invalid-type-variable-default]
```

[bound rules]: https://typing.python.org/en/latest/spec/generics.html#bound-rules
[constraint rules]: https://typing.python.org/en/latest/spec/generics.html#constraint-rules
[type variables]: https://docs.python.org/3/library/typing.html#typing.TypeVar
