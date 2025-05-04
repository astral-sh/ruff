# Cycles

## Function signature

Deferred annotations can result in cycles in resolving a function signature:

```py
from __future__ import annotations

# error: [invalid-type-form]
def f(x: f):
    pass

reveal_type(f)  # revealed: def f(x: Unknown) -> Unknown
```
