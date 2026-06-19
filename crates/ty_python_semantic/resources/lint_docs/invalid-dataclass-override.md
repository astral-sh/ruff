## What it does

Checks for dataclass definitions that have both `frozen=True` and a custom `__setattr__` or
`__delattr__` method defined.

## Why is this bad?

Frozen dataclasses synthesize `__setattr__` and `__delattr__` methods which raise a
`FrozenInstanceError` to emulate immutability.

Overriding either of these methods raises a runtime error.

## Examples

```python
from dataclasses import dataclass


@dataclass(frozen=True)
class A:
    def __setattr__(self, name: str, value: object) -> None: ...  # error
```
