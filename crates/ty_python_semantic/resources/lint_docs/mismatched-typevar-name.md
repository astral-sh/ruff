## What it does

Checks whether the name passed to `TypeVar` differs from the assigned variable.

## Why is this bad?

A mismatched name can be a typo and can make a `TypeVar`'s runtime representation confusing.

## Examples

```python
from typing import TypeVar

T = TypeVar("U")  # error: [mismatched-typevar-name] "Expected "T", got "U""
```
