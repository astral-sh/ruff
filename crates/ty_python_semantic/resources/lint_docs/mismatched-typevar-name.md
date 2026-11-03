## What it does

Checks whether the name passed to `TypeVar` differs from the assigned variable.

## Why is this bad?

The name argument identifies the type variable in generic signatures. A mismatch can make a
signature refer to a type variable that appears nowhere in the source.

## Examples

```python
from typing import TypeVar

T = TypeVar("U")  # error: [mismatched-typevar-name]
```
