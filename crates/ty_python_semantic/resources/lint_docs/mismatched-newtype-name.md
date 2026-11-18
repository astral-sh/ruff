## What it does

Checks whether the name passed to `NewType` differs from the assigned variable.

## Why is this bad?

The name argument sets the newtype's `__name__`. A typo can make the type appear under an
unexpected name in diagnostics.

## Examples

```python
from typing import NewType

UserId = NewType("Id", int)  # error: [mismatched-newtype-name] "Expected "UserId", got "Id""
```
