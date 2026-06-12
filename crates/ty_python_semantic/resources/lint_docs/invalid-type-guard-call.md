## What it does
Checks for type guard function calls without a valid target.

## Why is this bad?
The first non-keyword non-variadic argument to a type guard function
is its target and must map to a symbol.

Starred (`is_str(*a)`), literal (`is_str(42)`) and other non-symbol-like
expressions are invalid as narrowing targets.

## Examples
```python
from typing import TypeIs

def is_int(value: object = object()) -> TypeIs[int]:
    return isinstance(value, int)

is_int()  # Error: no positional narrowing target

is_int(value=1)  # Error: narrowing target passed by keyword
```
