## What it does

Checks for invalid keys when accessing or assigning items on a `TypedDict`.

This rule flags invalid keys such as a misspelled string literal or an integer. If the rejected key
is a string whose value is unknown, ty reports `non-literal-typed-dict-key` instead.

## Why is this bad?

Reading a missing key raises `KeyError`. Assigning to an undeclared key can silently add it to the
dictionary.

## Example

```python
from typing import TypedDict


class Person(TypedDict):
    name: str
    age: int


alice = Person(name="Alice", age=30)

# KeyError: 'height'
print(f"The height of Alice is {alice['height']}")  # error
```
