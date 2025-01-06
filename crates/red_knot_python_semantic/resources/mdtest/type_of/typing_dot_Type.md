# `typing.Type`

## Annotation

`typing.Type` can be used interchangeably with `type`:

```py
from typing import Type

class A: ...

def _(c: Type, d: Type[A]):
    reveal_type(c)  # revealed: type
    reveal_type(d)  # revealed: type[A]
    c = d  # fine
    d = c  # fine
```

## Inheritance

Inheriting from `Type` results in a MRO with `builtins.type` and `typing.Generic`. `Type` itself is
not a class.

```py
from typing import Type

class C(Type): ...

# Runtime value: `(C, type, typing.Generic, object)`
# TODO: Add `Generic` to the MRO
reveal_type(C.__mro__)  # revealed: tuple[Literal[C], Literal[type], Literal[object]]
```
