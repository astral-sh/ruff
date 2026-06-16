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
from typing_extensions import NotRequired


class Person(TypedDict):
    name: NotRequired[str]
    age: NotRequired[int]


alice = Person(name="Alice", age=30)
# KeyError: 'height'
alice["height"]  # error

# error
bob: Person = {"nickname": "Bob", "age": 30}  # typo!

# error
carol = Person(name="Carol", aeg=25)  # typo!
```
