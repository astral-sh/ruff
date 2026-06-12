## What it does

Checks for dataclass definitions where required fields are defined after
fields with default values.

## Why is this bad?

In dataclasses, all required fields (fields without default values) must be
defined before fields with default values. This is a Python requirement that
will raise a `TypeError` at runtime if violated.

## Example

```python
from dataclasses import dataclass


@dataclass
class Example:
    x: int = 1  # Field with default value
    # Required field after field with default
    y: str  # error
```
