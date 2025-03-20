# NewType

Currently, red-knot doesn't support `typing.NewType` in type annotations.

## Valid forms

```py
from typing_extensions import NewType
from types import GenericAlias

A = NewType("A", int)
B = GenericAlias(A, ())

def _(
    a: A,
    b: B,
):
    reveal_type(a)  # revealed: @Todo(Support for `typing.NewType` instances in type expressions)
    reveal_type(b)  # revealed: @Todo(Support for `typing.GenericAlias` instances in type expressions)
```
