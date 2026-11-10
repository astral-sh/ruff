## What it does

Checks for type guard function calls without a valid target.

## Why is this bad?

The first non-keyword non-variadic argument to a type guard function
is its target and must map to a symbol.

Starred (`is_str(*a)`), literal (`is_str(42)`) and other non-symbol-like
expressions are invalid as narrowing targets.

## Examples

```toml
[environment]
python-version = "3.13"
```

```python
from typing import TypeIs


def is_int(value: object = object()) -> TypeIs[int]:
    return isinstance(value, int)


# no positional narrowing target
is_int()  # error

# narrowing target passed by keyword
is_int(value=1)  # error
```
