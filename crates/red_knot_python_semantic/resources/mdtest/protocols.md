# Protocols

We do not support protocols yet, but to avoid false positives, we *partially* support some known
protocols.

## `typing.SupportsIndex`

```py
from typing import SupportsIndex, Literal

def _(some_int: int, some_literal_int: Literal[1], some_indexable: SupportsIndex):
    a: SupportsIndex = some_int
    b: SupportsIndex = some_literal_int
    c: SupportsIndex = some_indexable
```
