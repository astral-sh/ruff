# `typing.ClassVar`

[`typing.ClassVar`] is a type qualifier that is used to indicate that a class variable may not be
written to from instances of that class.

This test makes sure that we discover the type qualifier while inferring types from an annotation.
For more details on the semantics of pure class variables, see [this test](../attributes.md).

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

## Conflicting type qualifiers

We currently ignore conflicting qualifiers and simply union them, which is more conservative than
intersecting them. This means that we consider `a` to be a `ClassVar` here:

```py
from typing import ClassVar

def flag() -> bool:
    return True

class C:
    if flag():
        a: ClassVar[int] = 1
    else:
        a: str

reveal_type(C.a)  # revealed: int | str

c = C()

# error: [invalid-attribute-access]
c.a = 2
```

and similarly here:

```py
class Base:
    a: ClassVar[int] = 1

class Derived(Base):
    if flag():
        a: int

reveal_type(Derived.a)  # revealed: int

d = Derived()

# error: [invalid-attribute-access]
d.a = 2
```

## Too many arguments

```py
from typing import ClassVar

class C:
    # error: [invalid-type-form] "Type qualifier `typing.ClassVar` expects exactly one type parameter"
    x: ClassVar[int, str] = 1
```

## Illegal `ClassVar` in type expression

```py
from typing import ClassVar

class C:
    # error: [invalid-type-form] "Type qualifier `typing.ClassVar` is not allowed in type expressions (only in annotation expressions)"
    x: ClassVar | int

    # error: [invalid-type-form] "Type qualifier `typing.ClassVar` is not allowed in type expressions (only in annotation expressions)"
    y: int | ClassVar[str]
```

## Used outside of a class

```py
from typing import ClassVar

# TODO: this should be an error
x: ClassVar[int] = 1
```

[`typing.classvar`]: https://docs.python.org/3/library/typing.html#typing.ClassVar
