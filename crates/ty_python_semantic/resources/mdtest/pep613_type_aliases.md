# PEP 613 type aliases

We do not support PEP 613 type aliases yet. For now, just make sure that we don't panic:

```py
from typing import TypeAlias

RecursiveTuple: TypeAlias = tuple[int | "RecursiveTuple", str]

def _(rec: RecursiveTuple):
    reveal_type(rec)  # revealed: tuple[Divergent, str]

RecursiveHomogeneousTuple: TypeAlias = tuple[int | "RecursiveHomogeneousTuple", ...]

def _(rec: RecursiveHomogeneousTuple):
    reveal_type(rec)  # revealed: tuple[Divergent, ...]
```
