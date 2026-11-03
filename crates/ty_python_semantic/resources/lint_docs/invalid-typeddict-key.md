## What it does

Checks for invalid keys when accessing or assigning items on a `TypedDict`.

## Why is this bad?

Reading a missing key raises `KeyError`. Assigning to an undeclared key can silently add it to the
dictionary.

## Example

```python
from typing import TypedDict
from typing_extensions import NotRequired


class Person(TypedDict):
    name: NotRequired[str]
    age: NotRequired[int]


alice = Person(name="Alice", age=30)
# KeyError: 'height'
alice["height"]  # error
```

## See also

- `non-literal-typeddict-key` reports subscripts where ty cannot narrow the string key to a literal
    or a union of literals.
