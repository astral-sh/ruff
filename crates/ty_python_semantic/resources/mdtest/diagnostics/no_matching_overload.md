# No matching overload diagnostics

<!-- snapshot-diagnostics -->

## Calls to overloaded functions

```py
from typing import overload

@overload
def f(x: int) -> int: ...
@overload
def f(x: str) -> str: ...
def f(x: int | str) -> int | str:
    return x

f(b"foo")  # error: [no-matching-overload]
```
