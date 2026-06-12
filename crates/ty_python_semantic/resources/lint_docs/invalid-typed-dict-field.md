## What it does

Detects invalid `TypedDict` field declarations.

## Why is this bad?

`TypedDict` subclasses cannot redefine inherited fields incompatibly. Doing so breaks the
subtype guarantees that `TypedDict` inheritance is meant to preserve.

## Example

```python
from typing import TypedDict


class Base(TypedDict):
    x: int


class Child(Base):
    x: str  # error: [invalid-typed-dict-field]
```
