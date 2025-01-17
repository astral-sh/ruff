# `typing.ClassVar`

[`typing.ClassVar`] is a type qualifier that is used to indicate that a class variable may not be
set on instances. For more details, see the [typing spec].

This test mostly makes sure that we "see" the type qualifier in an annotation. For more details on
the semantics, see the [test on attributes](../attributes.md) test.

## Basic

```py
from typing import ClassVar, Annotated

class C:
    x: ClassVar[int] = 1
    y: Annotated[ClassVar[int], "the annotation for y"] = 1
    z: "ClassVar[int]" = 1

reveal_type(C.x)  # revealed: int
reveal_type(C.y)  # revealed: int
reveal_type(C.z)  # revealed: int

c = C()

# error: [invalid-attribute-access]
c.x = 2
# error: [invalid-attribute-access]
c.y = 2
# error: [invalid-attribute-access]
c.z = 2
```

## Too many arguments

```py
class C:
    # error: [invalid-type-form] "Special form `typing.ClassVar` expected exactly one type parameter"
    x: ClassVar[int, str] = 1
```

## Used outside of a class

```py
# TODO: this should be an error
x: ClassVar[int] = 1
```

[typing spec]: https://typing.readthedocs.io/en/latest/spec/class-compat.html#classvar
[`typing.classvar`]: https://docs.python.org/3/library/typing.html#typing.ClassVar
