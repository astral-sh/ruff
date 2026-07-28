## What it does

Detects statements other than annotated declarations in `TypedDict` class bodies.

## Why is this bad?

`TypedDict` class bodies aren't allowed to contain any other types of statements. For
example, method definitions and field values aren't allowed. None of these will be
available on "instances of the `TypedDict`" at runtime (as `dict` is the runtime class of
all "`TypedDict` instances").

## Example

```python
from typing import TypedDict


class Foo(TypedDict):
    def bar(self):  # error: [invalid-typed-dict-statement]
        pass
```
