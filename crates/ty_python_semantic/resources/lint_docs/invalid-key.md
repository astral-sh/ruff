## What it does
Checks for subscript accesses with invalid keys and `TypedDict` construction with an
unknown key.

## Why is this bad?
Subscripting with an invalid key will raise a `KeyError` at runtime.

Creating a `TypedDict` with an unknown key is likely a mistake; if the `TypedDict` is
`closed=true` it also violates the expectations of the type.

## Examples
```python
from typing import TypedDict

class Person(TypedDict):
    name: str
    age: int

alice = Person(name="Alice", age=30)
alice["height"]  # KeyError: 'height'

bob: Person = { "nickname": "Bob", "age": 30 }  # typo!

carol = Person(name="Carol", aeg=25)  # typo!
```
