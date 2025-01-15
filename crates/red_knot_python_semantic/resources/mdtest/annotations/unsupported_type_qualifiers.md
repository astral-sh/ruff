# Unsupported type qualifiers

## Not yet supported

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
