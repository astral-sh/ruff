## What it does

Checks for dataclasses with invalid frozen inheritance:

- A frozen dataclass cannot inherit from a non-frozen dataclass.
- A non-frozen dataclass cannot inherit from a frozen dataclass.

## Why is this bad?

Python raises a `TypeError` at runtime when either of these inheritance
patterns occurs.

## Example

```python
from dataclasses import dataclass


@dataclass
class Base:
    x: int


@dataclass(frozen=True)
class Child(Base):  # error
    y: int


@dataclass(frozen=True)
class FrozenBase:
    x: int


@dataclass
class NonFrozenChild(FrozenBase):  # error
    y: int
```
