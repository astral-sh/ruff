# Unsupported type qualifiers

## Not yet fully supported

Several type qualifiers are unsupported by ty currently. However, we also don't emit false-positive
errors if you use one in an annotation:

```py
from typing_extensions import Final, ReadOnly, TypedDict

X: Final = 42
Y: Final[int] = 42

class Bar(TypedDict):
    z: ReadOnly[bytes]
```

## Type expressions

One thing that is supported is error messages for using type qualifiers in type expressions.

```py
from typing_extensions import Final, ClassVar, Required, NotRequired, ReadOnly

def _(
    # error: [invalid-type-form] "Type qualifier `typing.Final` is not allowed in type expressions (only in annotation expressions)"
    a: Final | int,
    # error: [invalid-type-form] "Type qualifier `typing.ClassVar` is not allowed in type expressions (only in annotation expressions)"
    b: ClassVar | int,
    # error: [invalid-type-form] "Type qualifier `typing.ReadOnly` is not allowed in type expressions (only in annotation expressions, and only with exactly one argument)"
    c: ReadOnly | int,
) -> None:
    reveal_type(a)  # revealed: Unknown | int
    reveal_type(b)  # revealed: Unknown | int
    reveal_type(c)  # revealed: Unknown | int
```

## Inheritance

You can't inherit from a type qualifier.

```py
from typing_extensions import Final, ClassVar, Required, NotRequired, ReadOnly

class A(Final): ...  # error: [invalid-base]
class B(ClassVar): ...  # error: [invalid-base]
class C(ReadOnly): ...  # error: [invalid-base]
```
