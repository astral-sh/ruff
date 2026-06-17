## What it does

Checks for type guard functions without
a first non-self-like non-keyword-only non-variadic parameter.

## Why is this bad?

Type narrowing functions must accept at least one positional argument
(non-static methods must accept another in addition to `self`/`cls`).

Extra parameters/arguments are allowed but do not affect narrowing.

## Examples

```toml
[environment]
python-version = "3.13"
```

```python
from typing import TypeIs


# no parameter
def f() -> TypeIs[int]:  # error
    return True


# no positional arguments allowed
def f(*, v: object) -> TypeIs[int]:  # error
    return True


# expected variadic arguments
def f(*args: object) -> TypeIs[int]:  # error
    return True


class C:
    # only positional argument is `self`
    def f(self) -> TypeIs[int]:  # error
        return True
```
