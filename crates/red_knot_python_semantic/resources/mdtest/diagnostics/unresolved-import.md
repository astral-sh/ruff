# Unresolved import diagnostics

<!-- snapshot-diagnostics -->

`libs/utils.py`:

```py
def add(a: int, b: int) -> int:
    a + b
```

```py
from utils import add  # error: [unresolved-import]

stat = add(10, 15)
```
