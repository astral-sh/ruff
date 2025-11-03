# NewType

Currently, ty doesn't support `typing.NewType` in type annotations.

## Valid forms

```py
from typing_extensions import NewType
from types import GenericAlias

X = GenericAlias(type, ())
A = NewType("A", int)
# TODO: typeshed for `typing.GenericAlias` uses `type` for the first argument. `NewType` should be special-cased
# to be compatible with `type`
# error: [invalid-argument-type] "Argument to function `__new__` is incorrect: Expected `type`, found `NewType`"
B = GenericAlias(A, ())

def _(
    a: A,
    b: B,
):
    reveal_type(a)  # revealed: @Todo(Support for `typing.NewType` instances in type expressions)
    reveal_type(b)  # revealed: @Todo(Support for `typing.GenericAlias` instances in type expressions)
```
