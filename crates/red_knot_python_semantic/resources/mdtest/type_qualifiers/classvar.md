# `typing.ClassVar`

[`typing.ClassVar`] is a type qualifier that is used to indicate that a class variable may not be
set on instances.

This test mostly makes sure that we "see" the type qualifier in an annotation. For more details on
the semantics, see the [test on attributes](../attributes.md) test.

## Basic

```py
from typing import ClassVar, Annotated

class C:
    a: ClassVar[int] = 1
    b: Annotated[ClassVar[int], "the annotation for b"] = 1
    c: ClassVar[Annotated[int, "the annotation for c"]] = 1
    d: ClassVar = 1
    e: "ClassVar[int]" = 1

reveal_type(C.a)  # revealed: int
reveal_type(C.b)  # revealed: int
reveal_type(C.c)  # revealed: int
# TODO: should be Unknown | Literal[1]
reveal_type(C.d)  # revealed: Unknown
reveal_type(C.e)  # revealed: int

c = C()

# error: [invalid-attribute-access]
c.a = 2
# error: [invalid-attribute-access]
c.b = 2
# error: [invalid-attribute-access]
c.c = 2
# error: [invalid-attribute-access]
c.d = 2
# error: [invalid-attribute-access]
c.e = 2
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

[`typing.classvar`]: https://docs.python.org/3/library/typing.html#typing.ClassVar
