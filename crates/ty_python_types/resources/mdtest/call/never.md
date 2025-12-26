# Never is callable

The type `Never` is callable with an arbitrary set of arguments. The result is always `Never`.

```py
from typing_extensions import Never

def f(never: Never):
    reveal_type(never())  # revealed: Never
    reveal_type(never(1))  # revealed: Never
    reveal_type(never(1, "a", never, x=None))  # revealed: Never
```
