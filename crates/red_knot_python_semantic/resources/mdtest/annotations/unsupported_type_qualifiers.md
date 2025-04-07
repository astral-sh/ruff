# Unsupported type qualifiers

## Not yet fully supported

Several type qualifiers are unsupported by red-knot currently. However, we also don't emit
false-positive errors if you use one in an annotation:

```py
from typing_extensions import Final, Required, NotRequired, ReadOnly, TypedDict

X: Final = 42
Y: Final[int] = 42

# TODO: `TypedDict` is actually valid as a base
# error: [invalid-base]
class Bar(TypedDict):
    x: Required[int]
    y: NotRequired[str]
    z: ReadOnly[bytes]
```

## Type expressions

One thing that is supported is error messages for using type qualifiers in type expressions.

```py
from typing_extensions import Final, ClassVar, Required, NotRequired, ReadOnly

def _(
    a: (
        Final  # error: [invalid-type-form] "Type qualifier `typing.Final` is not allowed in type expressions (only in annotation expressions)"
        | int
    ),
    b: (
        ClassVar  # error: [invalid-type-form] "Type qualifier `typing.ClassVar` is not allowed in type expressions (only in annotation expressions)"
        | int
    ),
    c: Required,  # error: [invalid-type-form] "Type qualifier `typing.Required` is not allowed in type expressions (only in annotation expressions, and only with exactly one argument)"
    d: NotRequired,  # error: [invalid-type-form] "Type qualifier `typing.NotRequired` is not allowed in type expressions (only in annotation expressions, and only with exactly one argument)"
    e: ReadOnly,  # error: [invalid-type-form] "Type qualifier `typing.ReadOnly` is not allowed in type expressions (only in annotation expressions, and only with exactly one argument)"
) -> None:
    reveal_type(a)  # revealed: Unknown | int
    reveal_type(b)  # revealed: Unknown | int
    reveal_type(c)  # revealed: Unknown
    reveal_type(d)  # revealed: Unknown
    reveal_type(e)  # revealed: Unknown
```

## Inheritance

You can't inherit from a type qualifier.

```py
from typing_extensions import Final, ClassVar, Required, NotRequired, ReadOnly

class A(Final): ...  # error: [invalid-base]
class B(ClassVar): ...  # error: [invalid-base]
class C(Required): ...  # error: [invalid-base]
class D(NotRequired): ...  # error: [invalid-base]
class E(ReadOnly): ...  # error: [invalid-base]
```
