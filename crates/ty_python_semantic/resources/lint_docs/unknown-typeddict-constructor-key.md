## What it does

Reports keys that a `TypedDict` constructor or a dictionary literal used to initialize a `TypedDict`
cannot accept. This includes unpacked mappings that may supply unknown keys.

## Why is this bad?

An unexpected key in a constructor often means a field name was misspelled. Even an implicitly open
`TypedDict` rejects an explicitly supplied key it does not declare. A `TypedDict` declared with
`closed=True` also rejects a string key that might introduce an extra item.

## Examples

```python
from typing import TypedDict
from typing_extensions import NotRequired


class Person(TypedDict):
    name: NotRequired[str]
    age: NotRequired[int]


# error
bob: Person = {"nickname": "Bob", "age": 30}

# error
carol = Person(name="Carol", aeg=25)  # typo!
```
