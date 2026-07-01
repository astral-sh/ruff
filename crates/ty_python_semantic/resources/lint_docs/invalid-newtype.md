## What it does

Checks for the creation of invalid `NewType`s

## Why is this bad?

There are several requirements that you must follow when creating a `NewType`.

## Examples

```python
from typing import NewType


def get_name() -> str:
    return "name"


Foo = NewType("Foo", int)  # okay
# The first argument to `NewType` must be a string literal
Bar = NewType(get_name(), int)  # error
# invalid base for `typing.NewType`
Baz = NewType("Baz", int | str)  # error
```
