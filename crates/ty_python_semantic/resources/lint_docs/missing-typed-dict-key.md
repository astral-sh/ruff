## What it does

Detects missing required keys in `TypedDict` constructor calls.

## Why is this bad?

`TypedDict` requires all non-optional keys to be provided during construction.
Missing items can lead to a `KeyError` at runtime.

## Example

```python
from typing import TypedDict


class Person(TypedDict):
    name: str
    age: int


alice: Person = {"name": "Alice"}  # missing required key 'age'

alice["age"]  # KeyError
```
