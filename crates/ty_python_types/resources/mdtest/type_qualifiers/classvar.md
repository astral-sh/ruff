# `typing.ClassVar`

[`typing.ClassVar`] is a type qualifier that is used to indicate that a class variable may not be
written to from instances of that class.

This test makes sure that we discover the type qualifier while inferring types from an annotation.
For more details on the semantics of pure class variables, see [this test](../attributes.md).

## Basic

```py
import typing
from typing import ClassVar, Annotated

class C:
    a: ClassVar[int] = 1
    b: Annotated[ClassVar[int], "the annotation for b"] = 1
    c: ClassVar[Annotated[int, "the annotation for c"]] = 1
    d: ClassVar = 1
    e: "ClassVar[int]" = 1
    f: typing.ClassVar = 1

reveal_type(C.a)  # revealed: int
reveal_type(C.b)  # revealed: int
reveal_type(C.c)  # revealed: int
reveal_type(C.d)  # revealed: Unknown | Literal[1]
reveal_type(C.e)  # revealed: int
reveal_type(C.f)  # revealed: Unknown | Literal[1]

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
# error: [invalid-attribute-access]
c.f = 3
```

## From stubs

This is a regression test for a bug where we did not properly keep track of type qualifiers when
accessed from stub files.

`module.pyi`:

```pyi
from typing import ClassVar

class C:
    a: ClassVar[int]
```

`main.py`:

```py
from module import C

c = C()
c.a = 2  # error: [invalid-attribute-access]
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

## Too many arguments

```py
from typing import ClassVar

class C:
    # error: [invalid-type-form] "Type qualifier `typing.ClassVar` expected exactly 1 argument, got 2"
    x: ClassVar[int, str] = 1
```

## Trailing comma creates a tuple

A trailing comma in a subscript creates a single-element tuple. We need to handle this gracefully
and emit a proper error rather than crashing (see
[ty#1793](https://github.com/astral-sh/ty/issues/1793)).

```py
from typing import ClassVar

class C:
    # error: [invalid-type-form] "Tuple literals are not allowed in this context in a type expression: Did you mean `tuple[()]`?"
    x: ClassVar[(),]

# error: [invalid-attribute-access] "Cannot assign to ClassVar `x` from an instance of type `C`"
C().x = 42
reveal_type(C.x)  # revealed: Unknown
```

This also applies when the trailing comma is inside the brackets (see
[ty#1768](https://github.com/astral-sh/ty/issues/1768)):

```py
from typing import ClassVar

class D:
    # A trailing comma here doesn't change the meaning; it's still one argument.
    a: ClassVar[int,] = 1

reveal_type(D.a)  # revealed: int
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

## Illegal positions

```toml
[environment]
python-version = "3.12"
```

```py
from typing import ClassVar
from ty_extensions import reveal_mro

# error: [invalid-type-form] "`ClassVar` annotations are only allowed in class-body scopes"
x: ClassVar[int] = 1

class C:
    def __init__(self) -> None:
        # error: [invalid-type-form] "`ClassVar` annotations are not allowed for non-name targets"
        self.x: ClassVar[int] = 1

        # error: [invalid-type-form] "`ClassVar` annotations are only allowed in class-body scopes"
        y: ClassVar[int] = 1

# error: [invalid-type-form] "`ClassVar` is not allowed in function parameter annotations"
def f(x: ClassVar[int]) -> None:
    pass

# error: [invalid-type-form] "`ClassVar` is not allowed in function parameter annotations"
def f[T](x: ClassVar[T]) -> T:
    return x

# error: [invalid-type-form] "`ClassVar` is not allowed in function return type annotations"
def f() -> ClassVar[int]:
    return 1

# error: [invalid-type-form] "`ClassVar` is not allowed in function return type annotations"
def f[T](x: T) -> ClassVar[T]:
    return x

# TODO: this should be an error
class Foo(ClassVar[tuple[int]]): ...

# TODO: Show `Unknown` instead of `@Todo` type in the MRO; or ignore `ClassVar` and show the MRO as if `ClassVar` was not there
# revealed: (<class 'Foo'>, @Todo(Inference of subscript on special form), <class 'object'>)
reveal_mro(Foo)
```

[`typing.classvar`]: https://docs.python.org/3/library/typing.html#typing.ClassVar
