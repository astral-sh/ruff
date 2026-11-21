## What it does

Checks for the creation of invalid `NewType`s

## Why is this bad?

There are several requirements that you must follow when creating a `NewType`.

## Examples

```python
from typing import NewType

Foo = NewType("Foo", int)  # okay
# The first argument to `NewType` must be a string
Bar = NewType(42, int)  # error
# invalid base for `typing.NewType`
Baz = NewType("Baz", int | str)  # error
```
