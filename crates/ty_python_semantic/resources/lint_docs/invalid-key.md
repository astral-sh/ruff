## What it does

Checks for subscript accesses with invalid keys and `TypedDict` construction with an unknown key.

Before 0.0.97, this rule also reported string keys whose literal value ty could not determine. To
retain a customized severity for both cases, set `non-literal-typed-dict-key` to the same level as
`invalid-key`. For an inline ignore, replace `invalid-key` with `non-literal-typed-dict-key` if the
diagnostic moved. List both rules if the line reports both.

## Why is this bad?

Reading a missing key raises `KeyError`. Assigning to an undeclared key can silently add it to the
dictionary.

Creating a `TypedDict` with an unknown key is likely a mistake. A `TypedDict` declared with
`closed=True` prohibits extra keys.

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
