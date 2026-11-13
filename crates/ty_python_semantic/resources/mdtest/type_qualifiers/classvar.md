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
reveal_type(C.d)  # revealed: int
reveal_type(C.e)  # revealed: int
reveal_type(C.f)  # revealed: int

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

For a new class variable with no type argument, the initializer determines which values can be
assigned through the class.

```py
C.d = 2  # ok
C.d = "two"  # error: [invalid-assignment]
```

A `None` initializer widens as it does for other mutable class attributes. With no initializer, the
type is unknown.

```py
class Defaults:
    placeholder: ClassVar = None
    uninitialized: ClassVar

reveal_type(Defaults.placeholder)  # revealed: None | Unknown
reveal_type(Defaults.uninitialized)  # revealed: Unknown
```

## Inferring the type from a generic call

A bare `ClassVar` lets a generic call infer its return type from the receiver. The inferred type is
also visible outside the class body.

```py
from typing import ClassVar
from ty_extensions._internal import Unknown

class C:
    header: ClassVar = bytes.fromhex("fd")
    reveal_type(header)  # revealed: bytes

    other: ClassVar[Unknown]
    other = b""

reveal_type(C.header)  # revealed: bytes
reveal_type(C.other)  # revealed: Unknown
```

Inside the subclass, both assignments narrow the variable to `bytes`. Repeating the bare qualifier
on `other` preserves its inherited type of `Unknown` outside the class body.

```py
class Child(C):
    header = bytes.fromhex("ab")
    reveal_type(header)  # revealed: bytes

    other: ClassVar = "data".encode()
    reveal_type(other)  # revealed: bytes

reveal_type(Child.header)  # revealed: bytes
reveal_type(Child.other)  # revealed: Unknown
```

A subclass can replace the default with another value of the inferred type. An incompatible
assignment is rejected, including through an intermediate subclass.

```py
class TextHeader(Child):
    header = "header"  # error: [invalid-assignment]

reveal_type(TextHeader.header)  # revealed: bytes
TextHeader().header = b"new header"  # error: [invalid-attribute-access]
```

## Overriding a mutable class variable

The inferred public type also constrains subclass annotations. For a mutable class variable, an
override must support the reads and writes allowed by the base class.

```py
from typing import ClassVar

class Base:
    count: ClassVar = 0
    items: ClassVar = [1]

class Compatible(Base):
    count = 1  # ok
    items = []
    reveal_type(items)  # revealed: list[int]

class Redeclared(Compatible):
    count: ClassVar[int] = 2  # ok
    items: ClassVar = []
    reveal_type(items)  # revealed: list[int]

class Incompatible(Base):
    count: ClassVar = "one"  # error: [invalid-assignment]

class Narrower(Base):
    count: ClassVar[bool] = True  # error: [incompatible-mutable-override]
```

Without an initializer the base supplies only the qualifier. A subclass can supply the first value.

```py
class Uninitialized:
    items: ClassVar

class Initialized(Uninitialized):
    items = [1]

class Grandchild(Initialized):
    items = []
    reveal_type(items)  # revealed: list[int]
```

## Repeating `ClassVar` in a subclass

A subclass can repeat the qualifier while keeping the inherited value type.

```py
from typing import ClassVar

class Message:
    group_by: ClassVar[tuple[str, ...]] = ()

class Record(Message):
    group_by: ClassVar = ("record",)

    # Inside the class body, we can use the locally inferred type.
    reveal_type(group_by)  # revealed: tuple[Literal["record"]]

# But externally, the inherited declared type governs the attribute.
reveal_type(Record.group_by)  # revealed: tuple[str, ...]
reveal_type(Record().group_by)  # revealed: tuple[str, ...]
Record.group_by = ()  # ok
Record().group_by = ()  # error: [invalid-attribute-access]
```

The inherited type constrains the initializer.

```py
class InvalidDefault(Message):
    group_by: ClassVar = (1,)  # snapshot: invalid-assignment
```

```snapshot
error[invalid-assignment]: Object of type `tuple[Literal[1]]` is not assignable to `tuple[str, ...]`
  --> src/mdtest_snippet.py:18:26
   |
18 |     group_by: ClassVar = (1,)  # snapshot: invalid-assignment
   |               --------   ^^^^ Incompatible value of type `tuple[Literal[1]]`
   |               |
   |               Class variable inherits type `tuple[str, ...]`
```

An explicit type argument redeclares the variable and must support all writes allowed by the base
class.

```py
class Narrower(Message):
    group_by: ClassVar[tuple[str]] = ("record",)  # error: [incompatible-mutable-override]
```

## From stubs

This is a regression test for a bug where we did not properly keep track of type qualifiers when
accessed from stub files.

`module.pyi`:

```pyi
from typing import ClassVar

class C:
    a: ClassVar[int]
    untyped: ClassVar
    placeholder: ClassVar = ...
```

`main.py`:

```py
from module import C

c = C()
c.a = 2  # error: [invalid-attribute-access]
reveal_type(C.a)  # revealed: int
reveal_type(C.untyped)  # revealed: Unknown
reveal_type(C.placeholder)  # revealed: Unknown
C.untyped = 2  # ok
c.untyped = 2  # error: [invalid-attribute-access]

class Child(C):
    untyped = 1
    placeholder = "ready"

reveal_type(Child.untyped)  # revealed: int
reveal_type(Child.placeholder)  # revealed: str
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

## Conditional type argument

A bare `ClassVar` can appear on one branch and an explicit type on another. A read through the class
includes the value inferred on the bare branch and the type declared on the other.

```py
from typing import ClassVar

def flag() -> bool:
    return True

class C:
    if flag():
        x: ClassVar = "a"
    else:
        x: ClassVar[int] = 0

reveal_type(C.x)  # revealed: Literal["a"] | int
C.x = 3.14  # error: [invalid-assignment]

class Child(C):
    x = "a"  # ok

reveal_type(Child.x)  # revealed: Literal["a"] | int
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

## `ClassVar` cannot contain non-self type variables

`ClassVar` cannot include type variables at any level of nesting.

```toml
[environment]
python-version = "3.12"
```

```py
from typing import ClassVar, TypeVar, ParamSpec, Generic

T = TypeVar("T")
P = ParamSpec("P")

class C(Generic[T, P]):
    # error: [invalid-type-form] "`ClassVar` cannot contain type variables"
    a: ClassVar[T]

    # error: [invalid-type-form] "`ClassVar` cannot contain type variables"
    b: ClassVar[list[T]]

    # error: [invalid-type-form] "`ClassVar` cannot contain type variables"
    c: ClassVar[int | T]

    # error: [invalid-type-form] "Bare ParamSpec `P` is not valid in this context"
    d: ClassVar[P]

    # No error: no type variables
    e: ClassVar[int] = 1

# PEP 695 syntax
class D[T]:
    # error: [invalid-type-form] "`ClassVar` cannot contain type variables"
    x: ClassVar[T]

    # error: [invalid-type-form] "`ClassVar` cannot contain type variables"
    y: ClassVar[dict[str, T]]
```

## `ClassVar` can contain `Self`

`Self` is allowed inside `ClassVar`.

```toml
[environment]
python-version = "3.11"
```

```py
from typing import ClassVar, Self

class Base:
    all_instances: ClassVar[list[Self]]

    def method(self):
        reveal_type(self.all_instances)  # revealed: list[Self@method]

    @classmethod
    def cls_method(cls):
        reveal_type(cls.all_instances)  # revealed: list[Self@cls_method]

reveal_type(Base.all_instances)  # revealed: list[Base]

class Sub(Base): ...

reveal_type(Sub.all_instances)  # revealed: list[Sub]
```

Assignments through class objects should bind `Self` when writing a `ClassVar`, matching read-side
behavior. This remains permissive for `type[Base]` values even though `ClassVar[Self]` in non-final
classes is unsound.

```py
from typing import ClassVar, Self, TypeVar

class Saved:
    latest: ClassVar[Self]

    def save(self) -> None:
        type(self).latest = self

Saved.latest = Saved()

reveal_type(Saved.latest)  # revealed: Saved

class SavedSub(Saved): ...

reveal_type(SavedSub.latest)  # revealed: SavedSub

SavedSub.latest = SavedSub()

SavedSub.latest = Saved()  # error: [invalid-assignment]

def store_saved(cls: type[Saved]) -> None:
    cls.latest = Saved()

T = TypeVar("T", bound=Saved)

def store_generic(cls: type[T], value: T) -> None:
    cls.latest = value
```

Assignments through gradual class objects remain permissive.

```py
from typing import Any, ClassVar, reveal_type

class DynamicSaved:
    count: ClassVar[int]

def store_any(cls: type[Any], value: Any) -> None:
    cls.count = value
    reveal_type(cls.count)  # revealed: Any
```

## Assignments through generic aliases

Assignments through generic aliases still resolve class variables.

```py
from typing import ClassVar, Generic, TypeVar
from typing_extensions import reveal_type

T = TypeVar("T")

class Box(Generic[T]):
    count: ClassVar[int]

Box[int].count = 1
reveal_type(Box[int].count)  # revealed: int
```

## Combining `ClassVar` and `Final` in normal classes

An attribute on a class body that is annotated as `Final` is implicitly treated as a class variable.
The error message is different, but these attributes cannot be written to from instances of the
class:

```py
from typing import Final

class C:
    a: Final[int] = 1

reveal_type(C.a)  # revealed: int

c = C()
c.a = 2  # error: [invalid-assignment] "Cannot assign to final attribute `a` on type `C`"
```

In this sense, it is redundant to combine `ClassVar` and `Final`. We issue a warning in these cases:

```py
from typing import Annotated, ClassVar

class D:
    # error: [redundant-final-classvar] "Combining `ClassVar` and `Final` is redundant"
    a: ClassVar[Final[int]] = 1

    # error: [redundant-final-classvar] "Combining `ClassVar` and `Final` is redundant"
    b: Final[ClassVar[int]] = 1

    # error: [redundant-final-classvar] "Combining `ClassVar` and `Final` is redundant"
    c: Final[ClassVar] = 1

    # error: [redundant-final-classvar] "Combining `ClassVar` and `Final` is redundant"
    d: Annotated[Final[ClassVar[int]], "metadata"] = 1

    # error: [redundant-final-classvar] "Combining `ClassVar` and `Final` is redundant"
    e: ClassVar[Final] = 1

    # error: [redundant-final-classvar] "Combining `ClassVar` and `Final` is redundant"
    f: Annotated[Final[Annotated[Annotated[ClassVar[int], "a"], "b"]], "c"] = 1

reveal_type(D.a)  # revealed: int
reveal_type(D.b)  # revealed: int
reveal_type(D.c)  # revealed: Literal[1]
reveal_type(D.d)  # revealed: int
reveal_type(D.e)  # revealed: Literal[1]
reveal_type(D.f)  # revealed: int

d = D()
d.a = 2  # error: [invalid-attribute-access] "Cannot assign to ClassVar `a` from an instance of type `D`"
d.b = 2  # error: [invalid-attribute-access] "Cannot assign to ClassVar `b` from an instance of type `D`"
d.c = 2  # error: [invalid-attribute-access] "Cannot assign to ClassVar `c` from an instance of type `D`"
d.d = 2  # error: [invalid-attribute-access] "Cannot assign to ClassVar `d` from an instance of type `D`"
d.e = 2  # error: [invalid-attribute-access] "Cannot assign to ClassVar `e` from an instance of type `D`"
d.f = 2  # error: [invalid-attribute-access] "Cannot assign to ClassVar `f` from an instance of type `D`"
```

## Combining `ClassVar` and `Final` in dataclasses

In dataclasses, `ClassVar[Final[int]]` has a distinct meaning from `Final[int]`. The former is a
final class variable, the latter is a final instance attribute. The warning is therefore not emitted
when combining `ClassVar[Final[...]]` in dataclasses:

```py
from dataclasses import dataclass
from typing import ClassVar, Final

@dataclass
class D:
    # No warning:
    class_attr: ClassVar[Final[int]] = 1

    instance_attr: Final[int] = 1
```

Note that `class_attr` does not appear in the signature of `__init__`:

```py
# revealed: (self: D, instance_attr: int = 1) -> None
reveal_type(D.__init__)
```

```py
def _(d: D):
    reveal_type(d.class_attr)  # revealed: int
    reveal_type(d.instance_attr)  # revealed: int

    d.class_attr = 2  # error: [invalid-attribute-access]
```

The reverse direction `Final[ClassVar[...]]` is not recognized by the runtime implementation of
dataclasses. We could consider emitting a warning in these cases, but for now, we treat is just like
`ClassVar[Final[...]]` and allow it in dataclasses:

```py
from dataclasses import dataclass

@dataclass
class E:
    class_attr: Final[ClassVar[int]] = 1

# revealed: (self: E) -> None
reveal_type(E.__init__)

def _(e: E):
    reveal_type(e.class_attr)  # revealed: int

    e.class_attr = 2  # error: [invalid-attribute-access]
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
from typing import ClassVar, TypedDict
from ty_extensions._internal import reveal_mro

# error: [invalid-type-form] "`ClassVar` is only allowed in class bodies"
x: ClassVar[int] = 1

class C:
    def __init__(self) -> None:
        # error: [invalid-type-form] "`ClassVar` annotations are not allowed for non-name targets"
        self.x: ClassVar[int] = 1

        # error: [invalid-type-form] "`ClassVar` is only allowed in class bodies"
        y: ClassVar[int] = 1

# error: [invalid-type-form] "Type qualifier `typing.ClassVar` is not allowed in parameter annotations"
def f(x: ClassVar[int]) -> None:
    pass

# error: [invalid-type-form] "Type qualifier `typing.ClassVar` is not allowed in parameter annotations"
def f[T](x: ClassVar[T]) -> T:
    return x

# error: [invalid-type-form] "Type qualifier `typing.ClassVar` is not allowed in return type annotations"
def f() -> ClassVar[int]:
    return 1

# error: [invalid-type-form] "Type qualifier `typing.ClassVar` is not allowed in return type annotations"
def f[T](x: T) -> ClassVar[T]:
    return x

# TODO: this should be an error
class Foo(ClassVar[tuple[int]]): ...

# TODO: Show `Unknown` instead of `@Todo` type in the MRO; or ignore `ClassVar` and show the MRO as if `ClassVar` was not there
# revealed: (<class 'Foo'>, @Todo(Inference of subscript on special form), <class 'object'>)
reveal_mro(Foo)

class Foo(TypedDict):
    # error: [invalid-type-form] "`ClassVar` is not allowed in TypedDict fields"
    x: ClassVar[int]
    # error: [invalid-type-form] "`ClassVar` is not allowed in TypedDict fields"
    y: ClassVar
```

[`typing.classvar`]: https://docs.python.org/3/library/typing.html#typing.ClassVar
